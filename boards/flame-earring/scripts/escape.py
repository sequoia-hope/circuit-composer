#!/usr/bin/env python3
"""Escape pads the router cannot reach (a pad walled in by its neighbours' nets): for each
REF.PIN, the nearest via site free on every layer, a via there and a B.Cu stub from the pad,
both locked, before routing.

A site is free when the via clears every other net's copper on the front (the LED field: only
the via streets qualify), every other net's pad on the back, the board edge and every via, and
the stub from the pad's centre to it clears every other net's back pads.

    escape.py BOARD REF.PIN [...] -o OUT
"""
import argparse
import json
import math
import sys
from pathlib import Path

import pcbnew
from shapely.geometry import LineString, Point, Polygon
from shapely.ops import unary_union

sys.path.insert(0, str(Path(__file__).resolve().parent))
from stitch import CLEAR, DRILL, NM, VIA, V, mm, poly_of  # noqa: E402

BOARD_DIR = Path(__file__).resolve().parent.parent
STUB = 0.2


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("pads", nargs="+")
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    geo = json.load(open(BOARD_DIR / "geometry.json"))
    b = pcbnew.LoadBoard(a.board)
    fps = {f.GetReference(): f for f in b.GetFootprints()}
    edge = Polygon(geo["outline"]).buffer(-(VIA / 2 + 0.2))
    done = {}
    for spec in a.pads:
        ref, num = spec.split(".")
        pad = fps[ref].FindPadByNumber(num)
        net = pad.GetNetname()
        front, back = [], []
        for t in b.GetTracks():
            if t.GetNetname() == net:
                continue
            if t.Type() == pcbnew.PCB_VIA_T:
                front.append(Point(t.GetPosition().x / NM, t.GetPosition().y / NM).buffer(t.GetWidth(pcbnew.F_Cu) / NM / 2))
                back.append(front[-1])
            elif t.GetLayer() in (pcbnew.F_Cu, pcbnew.B_Cu):
                g = LineString([(t.GetStart().x / NM, t.GetStart().y / NM), (t.GetEnd().x / NM, t.GetEnd().y / NM)]).buffer(t.GetWidth() / NM / 2)
                (front if t.GetLayer() == pcbnew.F_Cu else back).append(g)
        for fp in b.GetFootprints():
            for p in fp.Pads():
                if p.GetNetname() == net and p.GetNetname():
                    continue
                if p.IsOnLayer(pcbnew.F_Cu):
                    front.append(poly_of(p.GetEffectivePolygon(pcbnew.F_Cu)))
                if p.IsOnLayer(pcbnew.B_Cu):
                    back.append(poly_of(p.GetEffectivePolygon(pcbnew.B_Cu)))
        # No via under any part's body on the back, its own included.
        courts = [poly_of(fp.GetCourtyard(pcbnew.B_CrtYd)) for fp in b.GetFootprints() if fp.IsFlipped() and fp.GetCourtyard(pcbnew.B_CrtYd).OutlineCount()]
        front_u, back_u, court_u = unary_union(front), unary_union(back), unary_union(courts)
        c = (pad.GetPosition().x / NM, pad.GetPosition().y / NM)
        best = None
        for i in range(-60, 61):
            for j in range(-60, 61):
                x, y = c[0] + i * 0.05, c[1] + j * 0.05
                d = math.hypot(x - c[0], y - c[1])
                if d < 0.45 or d > 3.0 or (best and d >= best[0]):
                    continue
                disc = Point(x, y).buffer(VIA / 2 + CLEAR)
                if not edge.contains(Point(x, y)) or disc.intersects(front_u) or disc.intersects(back_u) or Point(x, y).buffer(VIA / 2).intersects(court_u):
                    continue
                if LineString([c, (x, y)]).buffer(STUB / 2 + CLEAR).intersects(back_u):
                    continue
                best = (d, x, y)
        if not best:
            done[spec] = None
            continue
        _, x, y = best
        via = pcbnew.PCB_VIA(b)
        via.SetPosition(V(x, y))
        via.SetWidth(mm(VIA))
        via.SetDrill(mm(DRILL))
        via.SetNet(pad.GetNet())
        via.SetLocked(True)
        b.Add(via)
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(V(*c))
        t.SetEnd(V(x, y))
        t.SetWidth(mm(STUB))
        t.SetLayer(pcbnew.B_Cu)
        t.SetNet(pad.GetNet())
        t.SetLocked(True)
        b.Add(t)
        done[spec] = [round(x, 3), round(y, 3), round(best[0], 3)]
    pcbnew.SaveBoard(a.output, b)
    print(json.dumps({"board": a.output, "escaped": done}))


if __name__ == "__main__":
    main()
