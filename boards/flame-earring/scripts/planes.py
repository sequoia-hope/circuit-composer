#!/usr/bin/env python3
"""The planes: In1.Cu solid GND; In2.Cu three VLED pours in concentric bands, each the area
between two ring midlines, so every VLED rail via lands in its own zone's pour. Zones are
filled. In3 and In4 stay free for signals.

    planes.py BOARD -o OUT
"""
import argparse
import json
from pathlib import Path

import pcbnew
from shapely.geometry import Polygon

BOARD_DIR = Path(__file__).resolve().parent.parent
GAP = 0.3   # between VLED pours


def mm(v):
    return int(round(v * 1e6))


def zone(board, layer, net, poly, priority=0):
    z = pcbnew.ZONE(board)
    z.SetLayer(layer)
    z.SetNet(board.FindNet(net))
    out = z.Outline()
    out.NewOutline()
    for x, y in poly.exterior.coords[:-1]:
        out.Append(mm(x), mm(y))
    for hole in poly.interiors:
        out.NewHole()
        for x, y in hole.coords[:-1]:
            out.Append(mm(x), mm(y), -1, 0)
    z.SetMinThickness(mm(0.2))
    z.SetLocalClearance(mm(0.2))
    z.SetThermalReliefGap(mm(0.2))
    z.SetThermalReliefSpokeWidth(mm(0.25))
    z.SetPadConnection(pcbnew.ZONE_CONNECTION_FULL)
    z.SetAssignedPriority(priority)
    z.SetIsFilled(False)
    board.Add(z)
    return z


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    import sys
    sys.path.insert(0, str(Path(__file__).resolve().parent))
    import geometry as G
    geo = json.load(open(BOARD_DIR / "geometry.json"))
    b = pcbnew.LoadBoard(a.board)
    outline = Polygon(geo["outline"])
    zone(b, pcbnew.In1_Cu, "GND", outline)
    offs = [r["offset"] for r in geo["rings"]]
    mids = [(offs[i] + offs[i + 1]) / 2 for i in range(len(offs) - 1)]
    m01, m12 = mids[0], mids[1]
    band_l = outline.difference(outline.buffer(-(m01 - GAP / 2), join_style="round"))
    band_r = outline.buffer(-(m01 + GAP / 2), join_style="round").difference(outline.buffer(-(m12 - GAP / 2), join_style="round"))
    band_c = outline.buffer(-(m12 + GAP / 2), join_style="round")
    made = []
    for net, g in (("VLED_L", band_l), ("VLED_R", band_r), ("VLED_C", band_c)):
        parts = [g] if g.geom_type == "Polygon" else list(g.geoms)
        for p in parts:
            if p.area > 1.0:
                zone(b, pcbnew.In2_Cu, net, p.simplify(0.02))
                made.append((net, round(p.area, 1)))
    pcbnew.ZONE_FILLER(b).Fill(b.Zones())
    pcbnew.SaveBoard(a.output, b)
    print(json.dumps({"board": a.output, "zones": [("GND", "In1")] + made}))


if __name__ == "__main__":
    main()
