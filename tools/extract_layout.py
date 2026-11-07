#!/usr/bin/env python3
"""Lift a regulator's reference layout out of a vendor board, in the anchor's frame.

    extract_layout.py REFERENCE.kicad_pcb ROLES_JSON -o LAYOUT_JSON [--block-type buck] [--source ID]

ROLES_JSON names the reference board's parts and pins by role, as `apply_layout.py` takes them:
{"vin": "U1.49", "sw": "U1.48", "pgnd": "U1.47", "fb": "U1.50", "cin": "C6", "cout": "C7",
"inductor": "L1"}. What comes out is the layout `apply_layout.py` applies, with the copper the
reference draws rather than a generator's:

- **parts:** each role part's position and rotation relative to the anchor.
- **pours:** every zone on one of the block's nets (vin, sw, gnd, out) that lies within the
  anchor's and the parts' extent (plus 1 mm), with its layer, priority, clearance and minimum
  width. Board-wide planes are larger than that and stay behind.
- **vias:** on a block net, inside one of those pours of the same net (return and feed vias).
- **tracks:** on a block net, with each end on one of those pours or on a pad of the block on
  the same net. Tracks that pass through to the rest of the chip's pins stay behind.

Nets are named by role: `out` is the inductor's pad off the switch node, which may also be the
FB pin's net. Prints a summary.
"""
import argparse
import hashlib
import json
import math
import sys

import pcbnew

NM = 1e6


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("roles")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--block-type", default="buck")
    ap.add_argument("--source", help="the source id (sources.json) the layout cites")
    a = ap.parse_args()

    roles = json.load(open(a.roles))
    board = pcbnew.LoadBoard(a.board)
    anchor_ref = roles["vin"].split(".")[0]
    anchor = board.FindFootprintByReference(anchor_ref)
    pos, ang = anchor.GetPosition(), anchor.GetOrientationDegrees()
    s, c = math.sin(math.radians(ang)), math.cos(math.radians(ang))

    def local(p):
        """Board IU to the anchor frame (mm): the inverse of `apply_layout.to_board`."""
        dx, dy = (p.x - pos.x) / NM, (p.y - pos.y) / NM
        return [round(dx * c - dy * s, 4), round(dx * s + dy * c, 4)]

    def pad(ref_pin):
        ref, pin = ref_pin.split(".", 1)
        return board.FindFootprintByReference(ref).FindPadByNumber(pin)

    names = {role: pad(roles[k]).GetNetname() for role, k in (("vin", "vin"), ("sw", "sw"), ("gnd", "pgnd"), ("fb", "fb"))}
    ind = board.FindFootprintByReference(roles["inductor"])
    names["out"] = next(q.GetNetname() for q in ind.Pads() if q.GetNetname() != names["sw"])
    # One role per net; the output wins over FB when FB senses the output directly.
    role_of = {}
    for role in ("fb", "gnd", "vin", "sw", "out"):
        role_of[names[role]] = role

    part_roles = [r for r in roles if "." not in roles[r]]
    parts = [board.FindFootprintByReference(roles[r]) for r in part_roles]
    box = anchor.GetBoundingBox(False)
    for fp in parts:
        box.Merge(fp.GetBoundingBox(False))
    box.Inflate(int(1.0 * NM))
    block_pads = [p for fp in [anchor] + parts for p in fp.Pads() if p.GetNetname() in role_of and (fp is not anchor or any(roles[k] == f"{anchor_ref}.{p.GetNumber()}" for k in ("vin", "sw", "pgnd", "fb")))]

    pours, kept = [], []
    for z in board.Zones():
        if z.GetIsRuleArea() or z.GetNetname() not in role_of:
            continue
        bb = z.GetBoundingBox()
        if not (box.Contains(bb.GetOrigin()) and box.Contains(bb.GetEnd())):
            continue
        outline = z.Outline().Outline(0)
        pts = [local(outline.CPoint(i)) for i in range(outline.PointCount())]
        pours.append({"net": role_of[z.GetNetname()], "layer": board.GetLayerName(z.GetLayer()), "priority": z.GetAssignedPriority(),
                      "polygon": pts, "clearance": round(z.GetLocalClearance() / NM, 4) if z.GetLocalClearance() else None,
                      "min_width": round(z.GetMinThickness() / NM, 4), "connection": "solid" if z.GetPadConnection() == pcbnew.ZONE_CONNECTION_FULL else "thermal"})
        kept.append(z)

    def in_pour(p, net, layer=None):
        return any(z.GetNetname() == net and (layer is None or z.IsOnLayer(layer)) and z.Outline().Contains(p) for z in kept)

    def on_pad(p, net):
        return any(q.GetNetname() == net and q.HitTest(p) for q in block_pads)

    tracks, vias, via_objs = [], [], []
    for t in board.Tracks():
        if t.GetClass() == "PCB_VIA" and t.GetNetname() in role_of and in_pour(t.GetPosition(), t.GetNetname()):
            vias.append({"at": local(t.GetPosition()), "size": round(t.GetWidth(pcbnew.F_Cu) / NM, 4), "drill": round(t.GetDrill() / NM, 4), "net": role_of[t.GetNetname()]})
            via_objs.append(t)

    # Tracks go in by connected run (same net and layer, ends within 0.01 mm): a run is kept when
    # each of its free ends lands on a kept pour, a block pad or a kept via.
    segs = [t for t in board.Tracks() if t.GetClass() != "PCB_VIA" and t.GetNetname() in role_of]
    near = lambda p, q: abs(p.x - q.x) <= 10000 and abs(p.y - q.y) <= 10000
    joined = lambda t, u: t.GetNetname() == u.GetNetname() and t.GetLayer() == u.GetLayer() and any(near(p, q) for p in (t.GetStart(), t.GetEnd()) for q in (u.GetStart(), u.GetEnd()))
    seen, runs = set(), []
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
        if all(in_pour(p, net, t.GetLayer()) or on_pad(p, net) or any(v.GetNetname() == net and v.HitTest(p) for v in via_objs) for p in free):
            runs.append(run)
            tracks += [{"a": local(u.GetStart()), "b": local(u.GetEnd()), "width": round(u.GetWidth() / NM, 4), "layer": board.GetLayerName(u.GetLayer()), "net": role_of[net]} for u in run]

    # A via stays when it joins something of the block's: the ground plane (a gnd via), a kept
    # track, or kept pours on two layers. The reference's other vias lead off to the rest of the
    # chip (the core rail's DVDD feeds) and would dangle.
    kept_tracks = [u for run in runs for u in run]
    def joins(v):
        net = v.GetNetname()
        if role_of[net] == "gnd" or any(u.GetNetname() == net and (v.HitTest(u.GetStart()) or v.HitTest(u.GetEnd())) for u in kept_tracks):
            return True
        return len({z.GetLayer() for z in kept if z.GetNetname() == net and z.Outline().Contains(v.GetPosition())}) >= 2
    vias = [rec for rec, v in zip(vias, via_objs) if joins(v)]

    layout = {
        "anchor_footprint": anchor.GetFPIDAsString(),
        "block_type": a.block_type,
        "parts": {r: {"at": local(fp.GetPosition()), "rotation": round(fp.GetOrientationDegrees() - ang, 4)} for r, fp in zip(part_roles, parts)},
        "tracks": tracks, "vias": vias, "keepouts": [], "pours": pours,
        "reference": {"board": a.board.split("/")[-1], "sha256": hashlib.sha256(open(a.board, "rb").read()).hexdigest(), **({"source": a.source} if a.source else {}),
                      "nets": {role: names[role] for role in ("vin", "sw", "gnd", "fb", "out")}},
    }
    json.dump(layout, open(a.output, "w"), indent=1)
    print(json.dumps({"ok": True, "wrote": a.output, "parts": len(layout["parts"]), "pours": len(pours), "tracks": len(tracks), "vias": len(vias)}))


if __name__ == "__main__":
    main()
