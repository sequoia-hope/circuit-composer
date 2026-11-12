#!/usr/bin/env python3
"""Lift a reviewed cluster of parts out of a board as a rigid layout, in its anchor's frame.

    lift_layout.py BOARD --anchor U501 --parts Q501 Q502 C510 ... -o DIR/layout.json
                   [--roles ROLES_JSON] [--block-type NAME] [--source ID] [--no-copper]

For a block with fixed geometry and no generator (a GaN half-bridge cell, a clamp loop, a
decoupler field under a BGA), placed and reviewed once on a board: `ccomp floorplan` then moves
and turns it as one (`rigid` in the floorplan spec), and `apply_layout.py` (through
`prepare_route.py`) puts it and its copper on the composed board.

ROLES_JSON maps role names to the cluster's designators ({"hs_fet": "Q501", ...}); without it
each part's role is its designator. What comes out:

- **parts:** each member's position and rotation relative to the anchor, and `flipped` for one
  on the anchor's other side (a decoupler on the bottom under a top-side BGA).
- **tracks:** on a net joining two or more of the cluster's pads, by connected run (same net and
  layer), each run kept when every free end lands on a cluster pad, a kept via or a kept pour.
- **vias:** on such a net, within the cluster's extent, where a kept track ends, inside a kept
  pour or inside a cluster pad (in-pad and thermal vias).
- **pours:** zones on such a net lying within the cluster's extent (plus 1 mm).

Nets are named by a pad, as `ROLE.PIN` (`anchor.3` for the anchor's), so the layout applies on a
board whose nets are named otherwise. ROLES (the anchor and every member) go to `roles.json`
beside the layout, where `prepare_route.py` reads them. The anchor must be on the top side.
Prints one JSON result.
"""
import argparse
import hashlib
import json
import math
import sys
from pathlib import Path

import pcbnew

NM = 1e6


def fail(msg):
    sys.exit(json.dumps({"ok": False, "error": msg}))


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("--anchor", required=True)
    ap.add_argument("--parts", nargs="+", default=[])
    ap.add_argument("--roles", help="role -> designator; the parts are its values")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--block-type", default="cluster")
    ap.add_argument("--source", help="the source id (sources.json) the layout cites")
    ap.add_argument("--no-copper", action="store_true", help="parts only")
    a = ap.parse_args()

    board = pcbnew.LoadBoard(a.board)
    roles = json.load(open(a.roles)) if a.roles else {d: d for d in a.parts}
    if a.anchor in roles.values():
        fail(f"{a.anchor} is the anchor; list only the other parts")
    anchor = board.FindFootprintByReference(a.anchor)
    if anchor is None:
        fail(f"no part {a.anchor}")
    if anchor.IsFlipped():
        fail(f"{a.anchor} is on the bottom: lift from a board with the anchor on top")
    members = {}
    for role, d in roles.items():
        fp = board.FindFootprintByReference(d)
        if fp is None:
            fail(f"no part {d}")
        members[role] = fp
    pos, ang = anchor.GetPosition(), anchor.GetOrientationDegrees()
    s, c = math.sin(math.radians(ang)), math.cos(math.radians(ang))

    def local(p):
        """Board IU to the anchor frame (mm): the inverse of `apply_layout.to_board`."""
        dx, dy = (p.x - pos.x) / NM, (p.y - pos.y) / NM
        return [round(dx * c - dy * s, 4), round(dx * s + dy * c, 4)]

    cluster = [("anchor", anchor)] + list(members.items())
    pads = [(role, q) for role, fp in cluster for q in fp.Pads()]
    count = {}
    for _, q in pads:
        if q.GetNetCode() > 0:
            count[q.GetNetname()] = count.get(q.GetNetname(), 0) + 1
    # Each net the cluster joins, named by its first pad.
    name = {}
    for role, q in pads:
        n = q.GetNetname()
        if count.get(n, 0) >= 2 and n not in name:
            name[n] = f"{role}.{q.GetNumber()}"

    box = anchor.GetBoundingBox(False)
    for _, fp in members.items():
        box.Merge(fp.GetBoundingBox(False))
    box.Inflate(int(1.0 * NM))

    parts = {}
    for role, fp in members.items():
        parts[role] = {"at": local(fp.GetPosition()), "rotation": round((fp.GetOrientationDegrees() - ang + 180) % 360 - 180, 4)}
        if fp.IsFlipped():
            parts[role]["flipped"] = True

    tracks, vias, pours = [], [], []
    if not a.no_copper:
        kept = []
        for z in board.Zones():
            if z.GetIsRuleArea() or z.GetNetname() not in name:
                continue
            bb = z.GetBoundingBox()
            if not (box.Contains(bb.GetOrigin()) and box.Contains(bb.GetEnd())):
                continue
            for layer in z.GetLayerSet().CuStack():
                outline = z.Outline().Outline(0)
                pours.append({"net": name[z.GetNetname()], "layer": board.GetLayerName(layer), "priority": z.GetAssignedPriority(),
                              "polygon": [local(outline.CPoint(i)) for i in range(outline.PointCount())],
                              "clearance": round(z.GetLocalClearance() / NM, 4) if z.GetLocalClearance() else None,
                              "min_width": round(z.GetMinThickness() / NM, 4), "connection": "solid" if z.GetPadConnection() == pcbnew.ZONE_CONNECTION_FULL else "thermal"})
            kept.append(z)

        def in_pour(p, net, layer=None):
            return any(z.GetNetname() == net and (layer is None or z.IsOnLayer(layer)) and z.Outline().Contains(p) for z in kept)

        def on_pad(p, net):
            return any(q.GetNetname() == net and q.HitTest(p) for _, q in pads)

        within = lambda p: box.Contains(p)
        cand = [t for t in board.Tracks() if t.GetClass() == "PCB_VIA" and t.GetNetname() in name and within(t.GetPosition())]
        segs = [t for t in board.Tracks() if t.GetClass() != "PCB_VIA" and t.GetNetname() in name and within(t.GetStart()) and within(t.GetEnd())]
        near = lambda p, q: abs(p.x - q.x) <= 10000 and abs(p.y - q.y) <= 10000
        joined = lambda t, u: t.GetNetname() == u.GetNetname() and t.GetLayer() == u.GetLayer() and any(near(p, q) for p in (t.GetStart(), t.GetEnd()) for q in (u.GetStart(), u.GetEnd()))
        seen, kept_tracks = set(), []
        for i, t in enumerate(segs):
            if i in seen:
                continue
            run, stack = [], [i]
            seen.add(i)
            while stack:
                j = stack.pop()
                run.append(segs[j])
                for k, u in enumerate(segs):
                    if k not in seen and joined(segs[j], u):
                        seen.add(k)
                        stack.append(k)
            ends = [p for u in run for p in (u.GetStart(), u.GetEnd())]
            free = [p for p in ends if sum(near(p, q) for q in ends) == 1]
            net = t.GetNetname()
            if all(in_pour(p, net, t.GetLayer()) or on_pad(p, net) or any(v.GetNetname() == net and v.HitTest(p) for v in cand) for p in free):
                kept_tracks += run
        for u in kept_tracks:
            tracks.append({"a": local(u.GetStart()), "b": local(u.GetEnd()), "width": round(u.GetWidth() / NM, 4), "layer": board.GetLayerName(u.GetLayer()), "net": name[u.GetNetname()]})
        for v in cand:
            net, p = v.GetNetname(), v.GetPosition()
            if any(u.GetNetname() == net and (v.HitTest(u.GetStart()) or v.HitTest(u.GetEnd())) for u in kept_tracks) or in_pour(p, net) or on_pad(p, net):
                vias.append({"at": local(p), "size": round(v.GetWidth(pcbnew.F_Cu) / NM, 4), "drill": round(v.GetDrill() / NM, 4), "net": name[net]})

    layout = {
        "anchor_footprint": anchor.GetFPIDAsString(),
        "block_type": a.block_type,
        "parts": parts,
        "tracks": tracks, "vias": vias, "keepouts": [], "pours": pours,
        "reference": {"board": Path(a.board).name, "sha256": hashlib.sha256(open(a.board, "rb").read()).hexdigest(), **({"source": a.source} if a.source else {}),
                      "anchor": a.anchor, "nets": {v: k for k, v in name.items()}},
    }
    out = Path(a.output)
    out.write_text(json.dumps(layout, indent=1) + "\n")
    (out.parent / "roles.json").write_text(json.dumps({"anchor": a.anchor, **roles}) + "\n")
    print(json.dumps({"ok": True, "wrote": str(out), "roles": str(out.parent / "roles.json"), "parts": len(parts),
                      "flipped": sorted(r for r, p in parts.items() if p.get("flipped")), "tracks": len(tracks), "vias": len(vias), "pours": len(pours)}))


if __name__ == "__main__":
    main()
