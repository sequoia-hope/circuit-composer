#!/usr/bin/env python3
"""Build the lightning-bolt earring's KiCad board from geometry.json and circuit.json, with pcbnew.

What is fixed here, and locked:
- the outline on Edge.Cuts;
- every LED on the front at its ring position and 45-degree rotation, and the light sensor in
  the innermost ring's seam;
- the LED copper on F.Cu: each ring's GND and VDD rails (offset curves over the pad tips) and
  the data link from each LED's DO to the next one's DI;
- J1 (USB-C) on the back in the top tab, mouth up: the earring hangs from a plug in it;
- BT1, the cell's two wire pads, beside the cell's upper end (found by search: inside the
  outline, off the cell's envelope); SW2 (B3U side-push) on the lower stroke's left edge,
  actuator out;
- simple_earring's MCU cluster (U1 and the parts round it, poses from its routed board),
  flipped onto the back under the cell, at the rotation that keeps it furthest inside;
- U5 (USB ESD) below J1, for its reserved VBUS escape.

Every other part is put on the back for `ccomp floorplan` to place.
Also written: floorplan/composed.json (parts with blocks, the nets) and floorplan/spec.json.

    board.py [-o bolt.kicad_pcb]
"""
import argparse
import json
import math
from pathlib import Path

import pcbnew

HERE = Path(__file__).resolve().parent
BOARD_DIR = HERE.parent
LIB = BOARD_DIR / "vendor/simple_earring/library/parts.pretty"
OWN_LIB = BOARD_DIR / "lib/bolt.pretty"

# JLCPCB 6-layer (JLC06161H-3313, 1.6 mm). Clearances as simple_earring's 4-layer, which JLC built.
RULES = {"min_track": 0.127, "min_clearance": 0.127, "track": 0.15, "via": 0.4, "drill": 0.2, "power_track": 0.3}
DATA_WIDTH = 0.15
# Where the MCU cluster sits on the back: centred under the cell, turned so its long side runs
# along the board (it is 12 x 18 mm; across, it would need the board's whole width).
MCU_ROTATIONS = tuple(float(a) for a in range(-24, 25, 4)) + tuple(float(a) for a in range(156, 205, 4))
# Parts placed here rather than by the floorplanner (pose on the back), because a pad of theirs
# needs a reserved escape (stitch.py ESCAPES): U5's VBUS pin sits between the USB pair's pins.
SW2_Y = 50.0           # where on the left edge the button would rather be
TAB_CLEAR = 12.0       # no button above this: the tab and its flare


def mm(v):
    return int(round(v * 1e6))


def V(x, y):
    return pcbnew.VECTOR2I(mm(x), mm(y))


def rules(board):
    bds = board.GetDesignSettings()
    bds.m_TrackMinWidth = mm(RULES["min_track"])
    bds.m_ViasMinSize = mm(RULES["via"])
    bds.m_MinThroughDrill = mm(RULES["drill"])
    bds.m_MinClearance = mm(RULES["min_clearance"])
    bds.m_HoleClearance = mm(0.2)
    bds.m_HoleToHoleMin = mm(0.2)
    bds.m_ViasMinAnnularWidth = mm(0.05)
    bds.m_CopperEdgeClearance = mm(0.2)
    bds.m_SolderMaskMinWidth = mm(0.1)
    ns = bds.m_NetSettings
    d = ns.GetDefaultNetclass()
    d.SetTrackWidth(mm(RULES["track"]))
    d.SetClearance(mm(RULES["min_clearance"]))
    d.SetViaDiameter(mm(RULES["via"]))
    d.SetViaDrill(mm(RULES["drill"]))
    for name, pattern, width in (("power", "VBUS", 0.4), ("vled", "VLED_*", RULES["power_track"]), ("supply", "VSYS", RULES["power_track"])):
        nc = pcbnew.NETCLASS(name)
        nc.SetTrackWidth(mm(width))
        nc.SetClearance(mm(RULES["min_clearance"]))
        nc.SetViaDiameter(mm(RULES["via"]))
        nc.SetViaDrill(mm(RULES["drill"]))
        ns.SetNetclass(name, nc)
        ns.SetNetclassPatternAssignment(pattern, name)


VIALESS_QFN = "RP2350-QFN-60-1EP_7x7_P0.4mm_EP3.4x3.4mm"


def own_footprints(geo):
    """The board's own footprints: the ear-wire hole, the cell's wire pads, and the RP2350's
    QFN without the thermal vias in its exposed pad (they would come up through the LED field;
    the pad reaches GND through B.Cu to a rail via instead)."""
    OWN_LIB.mkdir(parents=True, exist_ok=True)
    src = (LIB / (VIALESS_QFN + "_ThermalVias.kicad_mod")).read_text()
    (OWN_LIB / (VIALESS_QFN + ".kicad_mod")).write_text(drop_blocks(src, "(pad ", "thru_hole").replace(VIALESS_QFN + "_ThermalVias", VIALESS_QFN))
    # The cell's wire pads, at board positions found by bt1_spots(); drawn mirrored in y so the
    # footprint, flipped to the back about the origin (KiCad mirrors the footprint's y), puts
    # them there.
    (p1, p2) = bt1_spots(geo)
    rects = "".join(f'  (fp_rect (start {x - 0.85} {-y - 1.05}) (end {x + 0.85} {-y + 1.05}) (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))\n' for x, y in (p1, p2))
    (OWN_LIB / "BAT_PADS_BOLT.kicad_mod").write_text(f'''(footprint "BAT_PADS_BOLT"
  (version 20240108) (generator "bolt-earring")
  (layer "F.Cu")
  (descr "Wire pads for a 10 mm-wide LiPo: 1 = + (VBATT), 2 = - (GND); hand-soldered, not assembled")
  (attr smd exclude_from_pos_files exclude_from_bom)
  (fp_text reference "BT1" (at 0 -1.6) (layer "F.SilkS") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
  (fp_text value "CELL" (at 0 1.6) (layer "F.Fab") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
{rects}  (pad "1" smd rect (at {p1[0]} {-p1[1]}) (size 1.2 1.6) (layers "F.Cu" "F.Paste" "F.Mask"))
  (pad "2" smd rect (at {p2[0]} {-p2[1]}) (size 1.2 1.6) (layers "F.Cu" "F.Paste" "F.Mask"))
)
''')


def bt1_spots(geo):
    """Two pad spots (1.2 x 1.6, upright) beside the cell's upper end, one each side: inside the
    outline by 0.5, 0.4 off the cell's envelope and J1's span, nearest the envelope's top corners."""
    from shapely.geometry import Polygon, box
    outline = Polygon(geo["outline"]).buffer(-0.5)
    cell = Polygon(geo["cell"]["polygon"])
    j1 = box(geo["usb"]["x"] - 4.6, -2, geo["usb"]["x"] + 4.6, 6.4)
    corners = sorted(geo["cell"]["polygon"], key=lambda p: p[1])[:2]
    out = []
    for cx, cy in sorted(corners, key=lambda p: p[0]):
        best = None
        for i in range(-60, 61):
            for j in range(-20, 61):
                x, y = cx + i * 0.1, cy + j * 0.1
                pad = box(x - 0.6, y - 0.8, x + 0.6, y + 0.8)
                if not outline.contains(pad) or pad.distance(cell) < 0.4 or pad.intersects(j1):
                    continue
                d = (x - cx) ** 2 + (y - cy) ** 2
                if best is None or d < best[0]:
                    best = (d, (round(x, 3), round(y, 3)))
        out.append(best[1])
    return out


def drop_blocks(text, head, marker):
    """The S-expression text without each `head`... block whose own text contains `marker`,
    found by matching parentheses (a block may span lines)."""
    out, i = [], 0
    while True:
        j = text.find(head, i)
        if j < 0:
            out.append(text[i:])
            return "".join(out)
        depth, k = 0, j
        while True:
            if text[k] == "(":
                depth += 1
            elif text[k] == ")":
                depth -= 1
                if depth == 0:
                    break
            k += 1
        block = text[j:k + 1]
        out.append(text[i:j])
        if marker not in block:
            out.append(block)
        else:
            # Drop the line's leading whitespace and the newline after the block too.
            out[-1] = out[-1].rstrip(" \t")
            k = k + 1 if text[k + 1:k + 2] == "\n" else k
        i = k + 1


def load(lib, name):
    fp = pcbnew.FootprintLoad(str(lib), name)
    if fp is None:
        raise SystemExit(f"{name} not in {lib}")
    return fp


def put(fp, x, y, rot, back):
    fp.SetPosition(V(x, y))
    if back and not fp.IsFlipped():
        fp.Flip(fp.GetPosition(), pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
    fp.SetOrientationDegrees(rot)


def track(board, a, b, width, layer, net, locked=True):
    t = pcbnew.PCB_TRACK(board)
    t.SetStart(V(*a))
    t.SetEnd(V(*b))
    t.SetWidth(mm(width))
    t.SetLayer(layer)
    t.SetNet(net)
    t.SetLocked(locked)
    board.Add(t)
    return t


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--output", default=str(BOARD_DIR / "build/bolt.kicad_pcb"))
    a = ap.parse_args()
    import sys
    sys.path.insert(0, str(HERE))
    import geometry as G
    geo = json.load(open(BOARD_DIR / "geometry.json"))
    cir = json.load(open(BOARD_DIR / "circuit.json"))
    own_footprints(geo)
    out = Path(a.output)
    out.parent.mkdir(parents=True, exist_ok=True)
    for f in out.parent.glob(out.stem + ".kicad_p*"):
        f.unlink()
    board = pcbnew.NewBoard(str(out))
    board.SetCopperLayerCount(6)
    rules(board)

    # Outline.
    poly = geo["outline"]
    for i in range(len(poly)):
        s = pcbnew.PCB_SHAPE(board)
        s.SetShape(pcbnew.SHAPE_T_SEGMENT)
        s.SetStart(V(*poly[i]))
        s.SetEnd(V(*poly[(i + 1) % len(poly)]))
        s.SetLayer(pcbnew.Edge_Cuts)
        s.SetWidth(mm(0.05))
        board.Add(s)

    nets = {}

    def net(name):
        if name not in nets:
            ni = pcbnew.NETINFO_ITEM(board, name)
            board.Add(ni)
            nets[name] = ni
        return nets[name]

    for n in cir["nets"]:
        net(n)

    fps = {}
    for ref, c in cir["components"].items():
        name = "BAT_PADS_BOLT" if c["part"] == "battery_pads" else VIALESS_QFN if c["footprint"] == VIALESS_QFN + "_ThermalVias" else c["footprint"]
        own = name in ("BAT_PADS_BOLT", VIALESS_QFN)
        fp = load(OWN_LIB if own else LIB, name)
        fp.SetFPID(pcbnew.LIB_ID("bolt" if own else "parts", name))
        fp.SetReference(ref)
        fp.SetValue(c["value"])
        if c["lcsc"]:
            fp.SetField("LCSC", c["lcsc"])
        board.Add(fp)
        for pad in fp.Pads():
            n = c["pins"].get(pad.GetNumber())
            if n:
                pad.SetNet(net(n))
        fps[ref] = fp
    fixed = set()
    # LEDs, in geometry order = LED1..N.
    for i, d in enumerate(geo["leds"], 1):
        fp = fps[f"LED{i}"]
        put(fp, d["x"], d["y"], d["rotation"], False)
        fp.SetLocked(True)
        fixed.add(f"LED{i}")

    # The light sensor, front, in the innermost ring's seam, along the ring.
    als = next(r for r in geo["rings"] if r["ring"] == G.ALS_RING)["als"]
    sx, sy = als["x"], als["y"]
    ang = math.degrees(math.atan2(-als["tangent"][1], als["tangent"][0]))
    put(fps["Q1"], sx, sy, ang, False)
    fps["Q1"].SetLocked(True)
    fixed.add("Q1")

    from shapely.geometry import Polygon as SPoly, Point as SPoint
    outline = SPoly(geo["outline"])

    # J1 on the back in the top tab: rotation 0 there puts the mouth toward -y. Its courtyard's
    # mouth edge goes USB_OVERHANG above the top edge.
    j1 = fps["J1"]
    put(j1, geo["usb"]["x"], 0, 0, True)
    cy = j1.GetCourtyard(pcbnew.B_CrtYd).BBox()
    put(j1, geo["usb"]["x"], -geo["usb"]["overhang"] - cy.GetTop() / 1e6, 0, True)
    j1.SetLocked(True)
    fixed.add("J1")

    # BT1: its pads are where bt1_spots() put them; the footprint sits at the origin.
    put(fps["BT1"], 0, 0, 0, True)
    fps["BT1"].SetLocked(True)
    fixed.add("BT1")

    from shapely.geometry import box as sbox
    # SW2 on the back at the board edge, facing out: its body face (courtyard edge at footprint
    # -y) on the edge line and its actuator 0.7 past it, on a straight stretch of the left side
    # (within 3 degrees over the button's length), clear of the cell, nearest SW2_Y.
    ext = outline.exterior
    cell_env = SPoly(geo["cell"]["polygon"]).buffer(0.3)
    sw = fps["SW2"]
    put(sw, 0, 0, 0, True)
    face = -sw.GetCourtyard(pcbnew.B_CrtYd).BBox().GetTop() / 1e6   # face distance from the origin
    best = None
    n_s = int(ext.length / 0.5)
    for i in range(n_s):
        d = i * ext.length / n_s
        q = ext.interpolate(d)
        if q.y < TAB_CLEAR or q.x > SPoly(geo["cell"]["polygon"]).centroid.x:
            continue
        a_, b_, c_, d_ = ext.interpolate(d - 2.4), ext.interpolate(d - 0.1), ext.interpolate(d + 0.1), ext.interpolate(d + 2.4)
        t1 = math.atan2(b_.y - a_.y, b_.x - a_.x)
        t2 = math.atan2(d_.y - c_.y, d_.x - c_.x)
        if abs(math.degrees((t1 - t2 + math.pi) % (2 * math.pi) - math.pi)) > 3:
            continue
        tx, ty = d_.x - a_.x, d_.y - a_.y
        ln = math.hypot(tx, ty)
        nx, ny = ty / ln, -tx / ln
        if outline.contains(SPoint(q.x + 0.2 * nx, q.y + 0.2 * ny)):
            nx, ny = -nx, -ny
        # The rotation that points footprint -y along the outward normal (on the back KiCad's
        # rotation runs the other way, so it is searched, not computed).
        k = max((a * 0.5 for a in range(720)), key=lambda a: (put(sw, 0, 0, a, True), (lambda v: v[0] * nx + v[1] * ny)(rotate_local(sw, (0, -1))))[1])
        put(sw, q.x - nx * face, q.y - ny * face, k, True)
        o = sw.GetCourtyard(pcbnew.B_CrtYd).Outline(0)
        court = SPoly([(o.CPoint(j).x / 1e6, o.CPoint(j).y / 1e6) for j in range(o.PointCount())])
        if not outline.buffer(0.02).contains(court) or court.intersects(cell_env):
            continue
        if not all(outline.buffer(-0.25).contains(sbox(p.GetBoundingBox().GetLeft() / 1e6, p.GetBoundingBox().GetTop() / 1e6, p.GetBoundingBox().GetRight() / 1e6, p.GetBoundingBox().GetBottom() / 1e6)) for p in sw.Pads()):
            continue
        score = abs(q.y - SW2_Y)
        if best is None or score < best[0]:
            best = (score, q.x - nx * face, q.y - ny * face, k)
    if best is None:
        raise SystemExit("no straight edge for SW2")
    put(sw, best[1], best[2], best[3], True)
    sw.SetLocked(True)
    fixed.add("SW2")

    # The MCU cluster: simple_earring's poses relative to U1 (front there), placed front-side
    # about U1 at the cell's centre, flipped left-right about U1 and turned as one, at the
    # rotation that keeps its courtyards furthest inside the outline.
    cl = json.load(open(BOARD_DIR / "vendor/simple_earring/mcu_cluster.json"))
    skip = {"Q1", "C26"}
    members = [r for r in cl["parts"] if r in fps and r not in skip and not r.startswith("LED")
               and cir["components"][r]["footprint"].replace("_ThermalVias", "") == cl["parts"][r]["footprint"].replace("_ThermalVias", "")]
    cc = SPoly(geo["cell"]["polygon"]).centroid
    mcu_at = (cc.x, cc.y)
    centre = V(*mcu_at)
    scored = []
    for rot_ in MCU_ROTATIONS:
        for r in members:
            p = cl["parts"][r]
            fps[r].SetPosition(V(mcu_at[0] + p["dx"], mcu_at[1] + p["dy"]))
            if fps[r].IsFlipped():
                fps[r].Flip(fps[r].GetPosition(), pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
            fps[r].SetOrientationDegrees(p["rotation"])
        for r in members:
            fps[r].Flip(centre, pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
            fps[r].Rotate(centre, pcbnew.EDA_ANGLE(rot_ - cl["u1_rotation"], pcbnew.DEGREES_T))
        polys = []
        for r in members:
            o = fps[r].GetCourtyard(pcbnew.B_CrtYd).Outline(0)
            polys.append(SPoly([(o.CPoint(j).x / 1e6, o.CPoint(j).y / 1e6) for j in range(o.PointCount())]))
        inside = all(outline.contains(q) for q in polys)
        margin = min(outline.exterior.distance(q) for q in polys)
        scored.append(((inside, margin), rot_))
    mcu_rot = max(scored)[1]
    for r in members:
        p = cl["parts"][r]
        fps[r].SetPosition(V(mcu_at[0] + p["dx"], mcu_at[1] + p["dy"]))
        if fps[r].IsFlipped():
            fps[r].Flip(fps[r].GetPosition(), pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
        fps[r].SetOrientationDegrees(p["rotation"])
    for r in members:
        fps[r].Flip(centre, pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
        fps[r].Rotate(centre, pcbnew.EDA_ANGLE(mcu_rot - cl["u1_rotation"], pcbnew.DEGREES_T))
        fps[r].SetLocked(True)
        fixed.add(r)

    # U5 (USBLC6) goes where its USB pins are nearest J1's, clear of every part fixed so far
    # (courtyards 0.15 apart), inside the outline, with room on the far side of pin 5 for its
    # VBUS escape via: pin 5 sits between the USB pair's pins, so the router cannot reach it.
    def court_of(fp):
        o = fp.GetCourtyard(pcbnew.B_CrtYd).Outline(0)
        return SPoly([(o.CPoint(j).x / 1e6, o.CPoint(j).y / 1e6) for j in range(o.PointCount())])
    taken = [court_of(fps[r]) for r in fixed if fps[r].IsFlipped() and fps[r].GetCourtyard(pcbnew.B_CrtYd).OutlineCount()]
    # Pads past their courtyard (J1's signal row) count too, grown by the clearance.
    from shapely.geometry import box as sbox
    for r in fixed:
        if fps[r].IsFlipped():
            for pad in fps[r].Pads():
                bb = pad.GetBoundingBox()
                taken.append(sbox(bb.GetLeft() / 1e6, bb.GetTop() / 1e6, bb.GetRight() / 1e6, bb.GetBottom() / 1e6).buffer(0.2))
    j1_usb = [fps["J1"].FindPadByNumber(n).GetPosition() for n in ("A6", "A7", "B6", "B7") if fps["J1"].FindPadByNumber(n)]
    jx = sum(p.x for p in j1_usb) / len(j1_usb) / 1e6 if j1_usb else geo["usb"]["x"]
    jy = sum(p.y for p in j1_usb) / len(j1_usb) / 1e6 if j1_usb else 6.0
    u5 = fps["U5"]
    best = None
    for rot_ in (0, 90, 180, 270):
        for i in range(-50, 51):
            for j in range(0, 70):
                x, y = jx + i * 0.2, jy + 1.0 + j * 0.2
                put(u5, x, y, rot_, True)
                c = court_of(u5)
                if not outline.buffer(-0.3).contains(c) or any(c.distance(t) < 0.15 for t in taken):
                    continue
                p5 = u5.FindPadByNumber("5").GetPosition()
                via = SPoint(p5.x / 1e6 + (p5.x / 1e6 - x) * 2.2, p5.y / 1e6 + (p5.y / 1e6 - y) * 2.2)
                if any(via.buffer(0.35).intersects(t) for t in taken) or not outline.buffer(-0.5).contains(via):
                    continue
                d = math.hypot(x - jx, y - jy)
                if best is None or d < best[0]:
                    best = (d, x, y, rot_)
    if best is None:
        raise SystemExit("no spot for U5 below J1")
    PLACED = {"U5": (best[1], best[2], best[3])}
    for ref, (x, y, r) in PLACED.items():
        put(fps[ref], x, y, r, True)
        fps[ref].SetLocked(True)
        fixed.add(ref)

    # Everything else: on the back, at the board's middle, for the floorplanner.
    for ref, fp in fps.items():
        if ref not in fixed:
            put(fp, 0, 30, 0, True)

    # LED copper: rails and data links, F.Cu, locked.
    zone_of = {i: z for z, chain in cir["zones"].items() for i in chain}
    n_rail = n_data = 0
    for ring in geo["rings"]:
        k = ring["ring"]
        led_ids = [geo["leds"].index(d) + 1 for d in ring["leds"]]
        z = zone_of[led_ids[0]]
        gnd_side = ring["gnd_rail"]
        for side, pieces in ring["rails"].items():
          name = "GND" if side == gnd_side else f"VLED_{z}"
          for pts in pieces:
            for i in range(len(pts) - 1):
                a_, b_ = pts[i], pts[i + 1]
                if math.dist(a_, b_) < 1e-4:
                    continue
                track(board, a_, b_, G.RAIL_WIDTH, pcbnew.F_Cu, net(name))
                n_rail += 1
        for j in range(len(led_ids) - 1):
            a_fp, b_fp = fps[f"LED{led_ids[j]}"], fps[f"LED{led_ids[j + 1]}"]
            do = a_fp.FindPadByNumber("2")
            di = b_fp.FindPadByNumber("4")
            if do.GetNetname() != di.GetNetname():
                continue   # a zone boundary or a link the geometry broke (a keep-out)
            p, q = do.GetPosition(), di.GetPosition()
            # Along the ring line between the two pads, so a long link (an LED left out) follows
            # the curve between the rails instead of cutting across one.
            path = along_line(ring["line"], (p.x / 1e6, p.y / 1e6), (q.x / 1e6, q.y / 1e6))
            # A link that would cross a keep-out (J1's legs) is the router's: it goes under it.
            from shapely.geometry import LineString, Polygon
            if any(LineString(path).buffer(DATA_WIDTH / 2 + RULES["min_clearance"]).intersects(Polygon(k)) for k in geo["front_keepout"]):
                continue
            # Nor may it run under the light sensor, in the gap opposite ring 2's seam.
            band = LineString(path).buffer(DATA_WIDTH / 2 + RULES["min_clearance"])
            if any(band.intersects(Polygon([(c.x / 1e6, c.y / 1e6) for c in [pd.GetEffectivePolygon(pcbnew.F_Cu).Outline(0).CPoint(j) for j in range(pd.GetEffectivePolygon(pcbnew.F_Cu).Outline(0).PointCount())]])) for pd in fps["Q1"].Pads()):
                continue
            for u, w in zip(path, path[1:]):
                track(board, u, w, DATA_WIDTH, pcbnew.F_Cu, do.GetNet())
            n_data += 1

    pcbnew.SaveBoard(str(out), board)

    # Floorplan inputs: parts with blocks, nets, and the spec.
    fpdir = BOARD_DIR / "floorplan"
    fpdir.mkdir(exist_ok=True)
    parts = {}
    for ref, c in cir["components"].items():
        blk = None
        if ref.startswith("C") and ref[1:].isdigit() and int(ref[1:]) >= 201:
            blk = "led_bypass"
        parts[ref] = {"block": blk} if blk else {}
    composed = {"board": "bolt-earring", "parts": parts, "nets": cir["nets"]}
    json.dump(composed, open(fpdir / "composed.json", "w"), indent=1)
    bypass = [r for r in cir["components"] if r.startswith("C") and r[1:].isdigit() and int(r[1:]) >= 201]
    seams = [{"name": f"seam{r['ring']}", "rect": [r["seam"][0] - 1.0, r["seam"][1] - 1.0, r["seam"][0] + 1.0, r["seam"][1] + 1.0], "side": "back"} for r in geo["rings"]]
    spec = {
        "note": "Written by scripts/board.py. The back side is placed inside the bolt outline; the front (LEDs, light sensor), J1, SW2, BT1, U5 and the MCU cluster stay where board.py put them. The LED bypass capacitors are placed later, with the rail vias. Each ring's seam keeps the back clear for its data vias.",
        "outline_polygon": geo["outline"],
        "keep": sorted(fixed - {"BT1"}),
        "exclude": sorted(bypass),
        "keepouts": seams,
        "max_net_pins": 10,
        "options": {"moves": 1500000, "margin": 0.35, "w_area": 0.0, "clearance": 0.1},
    }
    # BT1's two pads are far apart: as one part it would be one rectangle across the board, so
    # it stays out of the floorplan and each pad is a keep-out of its own.
    spec["exclude"] = sorted(set(spec["exclude"]) | {"BT1"})
    for pad in fps["BT1"].Pads():
        bb = pad.GetBoundingBox()
        spec["keepouts"].append({"name": f"bt1_{pad.GetNumber()}", "rect": [bb.GetLeft() / 1e6 - 0.3, bb.GetTop() / 1e6 - 0.3, bb.GetRight() / 1e6 + 0.3, bb.GetBottom() / 1e6 + 0.3], "side": "back"})
    res = fpdir / "reserved.json"
    if res.exists():
        # The caps with a rail slot are stitch.py's; any without one is floorplanned like the rest.
        r = json.load(open(res))
        spec["keepouts"] += r["keepouts"]
        spec["exclude"] = sorted(set(spec["exclude"]) | {c["ref"] for c in r["caps"]})
    json.dump(spec, open(fpdir / "spec.json", "w"), indent=1)
    print(json.dumps({"mcu_rotation": mcu_rot, "board": str(out), "footprints": len(fps), "fixed": len(fixed), "rail_segments": n_rail, "data_links": n_data, "mcu_cluster": len(members)}))


def along_line(line, a, b, step=0.3):
    """From a to b: to the ring line, along it the short way round, then to b."""
    from shapely.geometry import LineString, Point
    ls = LineString(line)
    sa, sb = ls.project(Point(a)), ls.project(Point(b))
    L = ls.length
    fwd = (sb - sa) % L
    if fwd > L / 2:
        return [a] + [tuple(ls.interpolate((sa - i * step) % L).coords[0]) for i in range(1, int((L - fwd) / step))] + [b]
    return [a] + [tuple(ls.interpolate((sa + i * step) % L).coords[0]) for i in range(1, int(fwd / step))] + [b]


def rotate_local(fp, v):
    """Where a footprint-local direction points on the board (side and rotation applied)."""
    o = fp.GetPosition()
    pad = pcbnew.PAD(fp)
    pad.SetFPRelativePosition(V(*v))
    p = pad.GetPosition()
    return ((p.x - o.x) / 1e6, (p.y - o.y) / 1e6)


def seg_dist(p, a, b):
    dx, dy = b[0] - a[0], b[1] - a[1]
    t = max(0.0, min(1.0, ((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy or 1)))
    return math.hypot(p[0] - a[0] - t * dx, p[1] - a[1] - t * dy)


if __name__ == "__main__":
    main()
