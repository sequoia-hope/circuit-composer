#!/usr/bin/env python3
"""Stitch the LED rails to the planes and place the LED bypass capacitors.

Two steps, either side of the floorplan:

    stitch.py reserve BOARD -o reserved.json     before: choose sites, as back-side keep-outs
    stitch.py place BOARD reserved.json -o OUT    after: put vias, caps and tracks there

For each gap between neighbouring LEDs on a ring there are two candidate via sites: where the
gap's middle meets the ring's GND rail (inside) and its VLED rail (outside). A site is usable
when its via clears every other net's copper on the front (pads, rails, data links), the board
edge, and every via already chosen; `place` also checks the back.

Reserving chooses, for each zone, a cap slot per bypass capacitor (circuit.json, C201 on) at
gaps spread evenly along the zone's rings: two vias and, on the back between them along the
ring, the capacitor. Then every rail piece gets at least one more via, and one every
STITCH_EVERY LEDs. The vias and capacitor courtyards become the floorplan's keep-outs.
"""
import argparse
import json
import math
from pathlib import Path

import pcbnew
from shapely.geometry import LineString, Point, Polygon, box
from shapely.ops import unary_union

HERE = Path(__file__).resolve().parent
BOARD_DIR = HERE.parent
VIA, DRILL, CLEAR = 0.4, 0.2, 0.127
STITCH_EVERY = 10   # the In1 / In2 planes carry the current between rail vias
CAP_TRACK = 0.25
CAP_HALF = (0.95, 0.5)
# Pads the router cannot reach once the back is full: each gets a via and a B.Cu stub reserved
# now. U5 (USBLC6) pin 5, VBUS, sits between the USB pair's pins 4 and 6.
ESCAPES = ["U5.5"]
STUB = 0.2   # a 0402's courtyard half-extents, along and across
NM = 1e6


def mm(v):
    return int(round(v * NM))


def V(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))


def poly_of(sh):
    return unary_union([Polygon([(sh.Outline(i).CPoint(j).x / NM, sh.Outline(i).CPoint(j).y / NM) for j in range(sh.Outline(i).PointCount())]) for i in range(sh.OutlineCount())])


def zone_of_ring(k):
    return "L" if k == 0 else "R" if k == 1 else "C"


def front_copper(b):
    """Front copper by net: F.Cu tracks and the pads of front-side parts."""
    front = {}
    for t in b.GetTracks():
        if t.GetLayer() == pcbnew.F_Cu and t.Type() == pcbnew.PCB_TRACE_T:
            front.setdefault(t.GetNetname(), []).append(LineString([(t.GetStart().x / NM, t.GetStart().y / NM), (t.GetEnd().x / NM, t.GetEnd().y / NM)]).buffer(t.GetWidth() / NM / 2))
    for fp in b.GetFootprints():
        for pad in fp.Pads():
            if pad.IsOnLayer(pcbnew.F_Cu):
                front.setdefault(pad.GetNetname() or f"~{fp.GetReference()}{pad.GetNumber()}", []).append(poly_of(pad.GetEffectivePolygon(pcbnew.F_Cu)))
    return {n: unary_union(g) for n, g in front.items()}


def back_occupied(b, skip):
    """The back's occupied area: courtyards and pads of bottom-side parts, through-holes."""
    out = []
    for fp in b.GetFootprints():
        if fp.GetReference() in skip:
            continue
        for pad in fp.Pads():
            if pad.GetAttribute() in (pcbnew.PAD_ATTRIB_PTH, pcbnew.PAD_ATTRIB_NPTH):
                bb = pad.GetBoundingBox()
                out.append(box(bb.GetLeft() / NM, bb.GetTop() / NM, bb.GetRight() / NM, bb.GetBottom() / NM).buffer(CLEAR))
        if fp.IsFlipped():
            cy = fp.GetCourtyard(pcbnew.B_CrtYd)
            if cy.OutlineCount():
                out.append(poly_of(cy))
            for pad in fp.Pads():
                out.append(poly_of(pad.GetEffectivePolygon(pcbnew.B_Cu)).buffer(CLEAR))
    return unary_union(out)


def gap_sites(geo):
    """Per ring, per gap between neighbouring LEDs: the GND and VLED via sites, the gap's
    middle and the ring's direction there."""
    sites = []
    for ring in geo["rings"]:
        k = ring["ring"]
        leds = ring["leds"]
        ins = unary_union([LineString(p) for p in ring["rails"]["inside"]]) if ring["rails"]["inside"] else None
        outs = unary_union([LineString(p) for p in ring["rails"]["outside"]]) if ring["rails"]["outside"] else None
        for j in range(len(leds) - 1):
            p, q = leds[j], leds[j + 1]
            if math.hypot(p["x"] - q["x"], p["y"] - q["y"]) > 3.0:
                continue
            mid = ((p["x"] + q["x"]) / 2, (p["y"] + q["y"]) / 2)
            tl = math.hypot(q["x"] - p["x"], q["y"] - p["y"])
            t = ((q["x"] - p["x"]) / tl, (q["y"] - p["y"]) / tl)
            s = {"ring": k, "zone": zone_of_ring(k), "gap": j, "mid": mid, "t": t, "gnd": None, "vled": None}
            for key, line in (("gnd", ins), ("vled", outs)):
                if line is not None:
                    pt = line.interpolate(line.project(Point(mid)))
                    if pt.distance(Point(mid)) < 0.95:
                        s[key] = (round(pt.x, 4), round(pt.y, 4))
            sites.append(s)
    return sites


def cap_court(mid, t):
    """The bypass capacitor's courtyard: centred on the gap, along the ring."""
    (cx, cy), (tx, ty) = mid, t
    nx, ny = -ty, tx
    a, c = CAP_HALF
    return Polygon([(cx + u * a * tx + v * c * nx, cy + u * a * ty + v * c * ny) for u, v in ((-1, -1), (1, -1), (1, 1), (-1, 1))])


def reserve(a):
    geo = json.load(open(BOARD_DIR / "geometry.json"))
    cir = json.load(open(BOARD_DIR / "circuit.json"))
    b = pcbnew.LoadBoard(a.board)
    front = front_copper(b)
    # Fixed back parts (J1, SW2, BT1, the MCU cluster) and through-holes are known already.
    keep = set(json.load(open(BOARD_DIR / "floorplan/spec.json"))["keep"])
    fixed_back = back_occupied(b, skip={f.GetReference() for f in b.GetFootprints() if f.GetReference() not in keep})
    edge = Polygon(geo["outline"]).buffer(-(VIA / 2 + 0.2))
    chosen = []

    def usable(pt, net, extra=()):
        if pt is None or not edge.contains(Point(pt)):
            return False
        disc = Point(pt).buffer(VIA / 2)
        if any(n != net and disc.distance(g) < CLEAR for n, g in front.items()):
            return False
        if disc.buffer(CLEAR).intersects(fixed_back):
            return False
        return all(math.hypot(x - pt[0], y - pt[1]) >= VIA + 0.2 for x, y, _ in list(chosen) + list(extra))

    sites = gap_sites(geo)
    caps = sorted((r for r in cir["components"] if r.startswith("C") and r[1:].isdigit() and int(r[1:]) >= 201), key=lambda r: int(r[1:]))
    zone_caps = {}
    for r in caps:
        zone_caps.setdefault(cir["components"][r]["pins"]["1"].split("_")[-1], []).append(r)
    slots, courts = [], []
    for z, refs in zone_caps.items():
        cands = [s for s in sites if s["zone"] == z and s["gnd"] and s["vled"]]
        # Walk the zone's gaps from evenly spaced starting points, each taking the next usable.
        for n, r in enumerate(refs):
            start = int(n * len(cands) / len(refs))
            for i in list(range(start, len(cands))) + list(range(0, start)):
                s = cands[i]
                court = cap_court(s["mid"], s["t"])
                if court.intersects(fixed_back) or any(court.intersects(c) for c in courts) or not Polygon(geo["outline"]).buffer(-0.3).contains(court):
                    continue
                if not usable(s["gnd"], "GND") or not usable(s["vled"], f"VLED_{z}", extra=[(*s["gnd"], "GND")]):
                    continue
                chosen += [(*s["gnd"], "GND"), (*s["vled"], f"VLED_{z}")]
                courts.append(court)
                slots.append({"ref": r, "zone": z, "mid": s["mid"], "t": s["t"], "gnd": s["gnd"], "vled": s["vled"]})
                cands = cands[:i] + cands[i + 1:]
                break
    placed_caps = {s["ref"] for s in slots}
    # Every rail piece: at least one via, then one per STITCH_EVERY LEDs.
    missing = []
    for ring in geo["rings"]:
        z = zone_of_ring(ring["ring"])
        for side, net, key in (("inside", "GND", "gnd"), ("outside", f"VLED_{z}", "vled")):
            ring_sites = [s for s in sites if s["ring"] == ring["ring"] and s[key]]
            for piece in ring["rails"][side]:
                ls = LineString(piece)
                on = [s for s in ring_sites if ls.distance(Point(s[key])) < 0.05]
                have = [i for i, s in enumerate(on) if any(math.hypot(x - s[key][0], y - s[key][1]) < 1e-6 for x, y, _ in chosen)]
                last = have[0] if have else -10 ** 6
                for i, s in enumerate(on):
                    if (not have or i - last >= STITCH_EVERY) and usable(s[key], net):
                        chosen.append((*s[key], net))
                        have.append(i)
                        last = i
                if not have:
                    # No gap on this piece (it lies between keep-outs): anywhere along it will do.
                    for i in range(int(ls.length / 0.05) + 1):
                        pt = ls.interpolate(i * 0.05)
                        if usable((round(pt.x, 4), round(pt.y, 4)), net):
                            chosen.append((round(pt.x, 4), round(pt.y, 4), net))
                            have.append(-1)
                            break
                if not have:
                    missing.append(f"ring {ring['ring']} {side}: a piece of {round(ls.length, 2)} mm has no usable via site")
    # Escapes: the nearest via site to the pad, free on the front and of every fixed part on the
    # back (courtyards included), with a stub corridor clear of other nets' back pads.
    fps = {f.GetReference(): f for f in b.GetFootprints()}
    courts = unary_union([poly_of(fp.GetCourtyard(pcbnew.B_CrtYd)) for fp in fps.values() if fp.IsFlipped() and fp.GetReference() in keep and fp.GetCourtyard(pcbnew.B_CrtYd).OutlineCount()])
    escapes = []
    for spec in ESCAPES:
        ref, num = spec.split(".")
        pad = fps[ref].FindPadByNumber(num)
        net = pad.GetNetname()
        others = unary_union([poly_of(p.GetEffectivePolygon(pcbnew.B_Cu)) for fp in fps.values() for p in fp.Pads() if p.IsOnLayer(pcbnew.B_Cu) and p.GetNetname() != net])
        c = (pad.GetPosition().x / NM, pad.GetPosition().y / NM)
        best = None
        for i in range(-60, 61):
            for j in range(-60, 61):
                pt = (round(c[0] + i * 0.05, 4), round(c[1] + j * 0.05, 4))
                d = math.dist(pt, c)
                if d < 0.45 or d > 3.0 or (best and d >= best[0]):
                    continue
                if not usable(pt, net) or Point(pt).buffer(VIA / 2 + CLEAR).intersects(courts) or Point(pt).buffer(VIA / 2 + CLEAR).intersects(others):
                    continue
                if LineString([c, pt]).buffer(STUB / 2 + CLEAR).intersects(others):
                    continue
                best = (d, pt)
        if best:
            chosen.append((*best[1], net))
            escapes.append({"pad": spec, "net": net, "from": [round(c[0], 4), round(c[1], 4)], "via": list(best[1])})
        else:
            missing.append(f"escape {spec}: no via site within 3 mm")
    keepouts = [{"name": f"via{i}", "rect": [x - VIA / 2 - CLEAR, y - VIA / 2 - CLEAR, x + VIA / 2 + CLEAR, y + VIA / 2 + CLEAR], "side": "back"} for i, (x, y, _) in enumerate(chosen)]
    keepouts += [{"name": f"cap_{s['ref']}", "rect": list(cap_court(s["mid"], s["t"]).bounds), "side": "back"} for s in slots]
    for e in escapes:
        (x0, y0), (x1, y1) = e["from"], e["via"]
        keepouts.append({"name": f"escape_{e['pad']}", "rect": [min(x0, x1) - 0.25, min(y0, y1) - 0.25, max(x0, x1) + 0.25, max(y0, y1) + 0.25], "side": "back"})
    out = {"vias": [{"x": x, "y": y, "net": n} for x, y, n in chosen], "caps": slots, "keepouts": keepouts, "escapes": escapes,
           "caps_unplaced": [r for r in caps if r not in placed_caps], "rail_pieces_without_via": missing}
    json.dump(out, open(a.output, "w"), indent=1)
    print(json.dumps({"vias": len(chosen), "cap_slots": len(slots), "escapes": escapes, "caps_unplaced": out["caps_unplaced"], "rail_pieces_without_via": missing}))


def place(a):
    b = pcbnew.LoadBoard(a.board)
    res = json.load(open(a.reserved))
    fps = {f.GetReference(): f for f in b.GetFootprints()}
    back = back_occupied(b, skip={s["ref"] for s in res["caps"]})
    clashes = []

    def add_via(x, y, net):
        if Point(x, y).buffer(VIA / 2).buffer(CLEAR).intersects(back):
            clashes.append(f"via {net} at ({x:.2f}, {y:.2f})")
        v = pcbnew.PCB_VIA(b)
        v.SetPosition(V(x, y))
        v.SetWidth(mm(VIA))
        v.SetDrill(mm(DRILL))
        v.SetNet(b.FindNet(net))
        v.SetLocked(True)
        b.Add(v)

    def track(p, q, net):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(V(*p))
        t.SetEnd(V(*q))
        t.SetWidth(mm(CAP_TRACK))
        t.SetLayer(pcbnew.B_Cu)
        t.SetNet(b.FindNet(net))
        t.SetLocked(True)
        b.Add(t)

    for v in res["vias"]:
        add_via(v["x"], v["y"], v["net"])
    for e in res.get("escapes", []):
        t = pcbnew.PCB_TRACK(b)
        t.SetStart(V(*e["from"]))
        t.SetEnd(V(*e["via"]))
        t.SetWidth(mm(STUB))
        t.SetLayer(pcbnew.B_Cu)
        t.SetNet(b.FindNet(e["net"]))
        t.SetLocked(True)
        b.Add(t)
    for s in res["caps"]:
        fp = fps[s["ref"]]
        ang = math.degrees(math.atan2(-s["t"][1], s["t"][0]))
        fp.SetPosition(V(*s["mid"]))
        if not fp.IsFlipped():
            fp.Flip(fp.GetPosition(), pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
        # Of the two ways round, the one that puts pad 1 (VLED) nearer the VLED via.
        def reach(turn):
            fp.SetOrientationDegrees(ang + turn)
            p1, p2 = fp.FindPadByNumber("1").GetPosition(), fp.FindPadByNumber("2").GetPosition()
            return math.hypot(p1.x / NM - s["vled"][0], p1.y / NM - s["vled"][1]) + math.hypot(p2.x / NM - s["gnd"][0], p2.y / NM - s["gnd"][1])
        fp.SetOrientationDegrees(ang + min((0, 180), key=reach))
        p1, p2 = fp.FindPadByNumber("1").GetPosition(), fp.FindPadByNumber("2").GetPosition()
        track((p1.x / NM, p1.y / NM), tuple(s["vled"]), f"VLED_{s['zone']}")
        track((p2.x / NM, p2.y / NM), tuple(s["gnd"]), "GND")
        fp.SetLocked(True)
    pcbnew.SaveBoard(a.output, b)
    print(json.dumps({"board": a.output, "vias": len(res["vias"]), "caps": len(res["caps"]), "clashes_with_back": clashes}))


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    r = sub.add_parser("reserve")
    r.add_argument("board")
    r.add_argument("-o", "--output", required=True)
    p = sub.add_parser("place")
    p.add_argument("board")
    p.add_argument("reserved")
    p.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    reserve(a) if a.cmd == "reserve" else place(a)


if __name__ == "__main__":
    main()
