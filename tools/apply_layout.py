#!/usr/bin/env python3
"""Apply a generated block layout (`ccomp gen ...` output) to a KiCad board with pcbnew.

    apply_layout.py BOARD GEN_JSON ROLES_JSON -o OUT [--fixture [--copper-layers N]]

ROLES_JSON maps roles to parts and pads, e.g. {"vin": "U1.49", ..., "cin": "C14"}. Parts are
placed in the anchor's frame and locked; tracks, vias and keepouts are added and locked; zones
are refilled. With --fixture the board is first reduced to a test bench: only the anchor and
the block's parts, no copper, and a solid GND plane on the first inner layer (on B.Cu for a
2-layer board), so the block is measured alone. --copper-layers sets the bench's layer count,
so a 4-layer bench can be made from a 2-layer reference board.

Prints one JSON object. pcbnew rotates and writes footprints correctly, which is why this step
is Python and not Rust.
"""
import argparse
import json
import math
import sys

import pcbnew

NM = 1e6


def to_board(anchor, p):
    """The anchor frame to board mm, as KiCad places footprint-local points."""
    a = anchor.GetOrientationDegrees()
    s, c = math.sin(math.radians(a)), math.cos(math.radians(a))
    x, y = p
    pos = anchor.GetPosition()
    return (pos.x / NM + x * c + y * s, pos.y / NM + y * c - x * s)


def v(p):
    return pcbnew.VECTOR2I(int(round(p[0] * NM)), int(round(p[1] * NM)))


def on_layer(zone, layer):
    """Put a zone on exactly one layer; ZONE.SetLayer alone can leave it on F.Cu in KiCad 9."""
    ls = pcbnew.LSET()
    ls.AddLayer(layer)
    zone.SetLayerSet(ls)


def pad_net(board, ref_pin):
    ref, pin = ref_pin.split(".", 1)
    fp = board.FindFootprintByReference(ref)
    if fp is None:
        sys.exit(json.dumps({"ok": False, "error": f"no part {ref}"}))
    pad = fp.FindPadByNumber(pin)
    if pad is None:
        sys.exit(json.dumps({"ok": False, "error": f"no pad {ref_pin}"}))
    return pad.GetNet()


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("gen")
    ap.add_argument("roles")
    ap.add_argument("-o", "--output", required=True)
    ap.add_argument("--fixture", action="store_true")
    ap.add_argument("--copper-layers", type=int, help="with --fixture: the bench's copper layer count")
    a = ap.parse_args()

    gen = json.load(open(a.gen))
    layout = gen.get("layout", gen)
    roles = json.load(open(a.roles))
    board = pcbnew.LoadBoard(a.board)
    anchor_ref = roles["vin"].split(".")[0]
    anchor = board.FindFootprintByReference(anchor_ref)
    if anchor.GetFPIDAsString() != layout["anchor_footprint"] and anchor.GetFPID().GetLibItemName() != layout["anchor_footprint"].split(":")[-1]:
        sys.exit(json.dumps({"ok": False, "error": f"{anchor_ref} is {anchor.GetFPIDAsString()}, layout is for {layout['anchor_footprint']}"}))
    nets = {"vin": pad_net(board, roles["vin"]), "sw": pad_net(board, roles["sw"]), "gnd": pad_net(board, roles["pgnd"]), "out": pad_net(board, roles["fb"])}
    block_refs = {anchor_ref} | {roles[r] for r in layout["parts"]}

    removed = {"footprints": 0, "tracks": 0, "zones": 0}
    if a.fixture:
        # Collect first, then Delete: iterating while removing upsets pcbnew's SWIG lists.
        doomed_fps = [fp for fp in board.Footprints() if fp.GetReference() not in block_refs]
        doomed_tracks = list(board.Tracks())
        doomed_zones = list(board.Zones())
        for fp in doomed_fps:
            board.Delete(fp)
        for t in doomed_tracks:
            board.Delete(t)
        for z in doomed_zones:
            board.Delete(z)
        removed = {"footprints": len(doomed_fps), "tracks": len(doomed_tracks), "zones": len(doomed_zones)}
        if a.copper_layers:
            board.SetCopperLayerCount(a.copper_layers)
        box = board.GetBoardEdgesBoundingBox()
        z = pcbnew.ZONE(board)
        on_layer(z, pcbnew.In1_Cu if board.GetCopperLayerCount() >= 4 else pcbnew.B_Cu)
        z.SetNet(nets["gnd"])
        z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)
        z.SetMinThickness(int(0.2 * NM))
        outline = z.Outline()
        outline.NewOutline()
        for x, y in [(box.GetLeft(), box.GetTop()), (box.GetRight(), box.GetTop()), (box.GetRight(), box.GetBottom()), (box.GetLeft(), box.GetBottom())]:
            outline.Append(x, y)
        z.SetZoneName("fixture GND plane")
        board.Add(z)

    for role, pose in layout["parts"].items():
        fp = board.FindFootprintByReference(roles[role])
        fp.SetOrientationDegrees(anchor.GetOrientationDegrees() + pose["rotation"])
        fp.SetPosition(v(to_board(anchor, pose["at"])))
        fp.SetLocked(True)
    anchor.SetLocked(True)

    layer_id = {board.GetLayerName(i): i for i in range(pcbnew.PCB_LAYER_ID_COUNT)}
    for t in layout["tracks"]:
        tr = pcbnew.PCB_TRACK(board)
        tr.SetStart(v(to_board(anchor, t["a"])))
        tr.SetEnd(v(to_board(anchor, t["b"])))
        tr.SetWidth(int(round(t["width"] * NM)))
        tr.SetLayer(layer_id[t["layer"]])
        tr.SetNet(nets[t["net"]])
        tr.SetLocked(True)
        board.Add(tr)
    for vi in layout["vias"]:
        via = pcbnew.PCB_VIA(board)
        via.SetPosition(v(to_board(anchor, vi["at"])))
        via.SetWidth(int(round(vi["size"] * NM)))
        via.SetDrill(int(round(vi["drill"] * NM)))
        via.SetViaType(pcbnew.VIATYPE_THROUGH)
        via.SetLayerPair(pcbnew.F_Cu, pcbnew.B_Cu)
        via.SetNet(nets[vi["net"]])
        via.SetLocked(True)
        board.Add(via)
    for k in layout["keepouts"]:
        z = pcbnew.ZONE(board)
        z.SetIsRuleArea(True)
        z.SetDoNotAllowCopperPour(True)
        z.SetDoNotAllowTracks(True)
        z.SetDoNotAllowVias(True)
        z.SetDoNotAllowPads(False)
        z.SetDoNotAllowFootprints(False)
        on_layer(z, layer_id[k["layer"]])
        z.SetZoneName("switch-node keepout")
        outline = z.Outline()
        outline.NewOutline()
        for p in k["polygon"]:
            q = v(to_board(anchor, p))
            outline.Append(q.x, q.y)
        z.SetLocked(True)
        board.Add(z)

    pcbnew.ZONE_FILLER(board).Fill(board.Zones())
    board.Save(a.output)
    print(json.dumps({"ok": True, "wrote": a.output, "fixture": a.fixture, "removed": removed,
                      "placed": {roles[r]: p for r, p in layout["parts"].items()},
                      "tracks": len(layout["tracks"]), "vias": len(layout["vias"]), "keepouts": len(layout["keepouts"])}))


if __name__ == "__main__":
    main()
