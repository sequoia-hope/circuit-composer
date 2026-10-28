#!/usr/bin/env python3
"""Build the flame earring's KiCad board from geometry.json and circuit.json, with pcbnew.

What is fixed here, and locked:
- the outline on Edge.Cuts, and the ear-wire hole by the point;
- every LED on the front at its ring position and 45-degree rotation, and the light sensor in
  the innermost ring's seam;
- the LED copper on F.Cu: each ring's GND and VDD rails (offset curves over the pad tips) and
  the data link from each LED's DO to the next one's DI;
- J1 (USB-C) on the back at the base, mouth out; SW2 (B3U side-push) on the back at the left
  edge above the cell, actuator out; BT1, the cell's two wire pads, either side of J1;
- simple_earring's MCU cluster (U1 and the parts round it, poses from its routed board),
  flipped onto the back under the cell.

Every other part is put on the back at the board's middle for `ccomp floorplan` to place.
Also written: floorplan/composed.json (parts with blocks, the nets) and floorplan/spec.json.

    board.py [-o flame.kicad_pcb]
"""
import argparse
import json
import math
from pathlib import Path

import pcbnew

HERE = Path(__file__).resolve().parent
BOARD_DIR = HERE.parent
LIB = BOARD_DIR / "vendor/simple_earring/library/parts.pretty"
OWN_LIB = BOARD_DIR / "lib/flame.pretty"

# JLCPCB 6-layer (JLC06161H-3313, 1.6 mm). Clearances as simple_earring's 4-layer, which JLC built.
RULES = {"min_track": 0.127, "min_clearance": 0.127, "track": 0.15, "via": 0.4, "drill": 0.2, "power_track": 0.3}
DATA_WIDTH = 0.15
# Where the MCU cluster sits on the back: centred under the cell, turned so its long side runs
# along the board (it is 12 x 18 mm; across, it would need the board's whole width).
MCU_AT = (-0.3, 33.5)
MCU_ROT = 0.0
# Parts placed here rather than by the floorplanner (pose on the back), because a pad of theirs
# needs a reserved escape (stitch.py ESCAPES): U5's VBUS pin sits between the USB pair's pins.
PLACED = {"U5": (-2.057, 50.711, 0.0)}
SW2_HEIGHT = 44.5      # height above the base of the button's centre (above the cell's top)
BT1_X = 6.75           # the cell wire pads, either side of J1
BT1_Y = 54.4


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
    h = geo["hanger"]
    (OWN_LIB / "HANGER_FLAME.kicad_mod").write_text(f'''(footprint "HANGER_FLAME"
  (version 20240108) (generator "flame-earring")
  (layer "F.Cu")
  (descr "Ear-wire hole: plated, no net, not assembled")
  (attr through_hole exclude_from_pos_files exclude_from_bom)
  (fp_text reference "H1" (at 0 -1.6) (layer "F.SilkS") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
  (fp_text value "HANGER" (at 0 1.6) (layer "F.Fab") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
  (fp_circle (center 0 0) (end {h["pad"] / 2 + 0.25} 0) (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))
  (fp_circle (center 0 0) (end {h["pad"] / 2 + 0.25} 0) (stroke (width 0.05) (type default)) (fill none) (layer "B.CrtYd"))
  (pad "1" thru_hole circle (at 0 0) (size {h["pad"]} {h["pad"]}) (drill {h["drill"]}) (layers "*.Cu" "*.Mask"))
)
''')
    (OWN_LIB / "BAT_PADS_FLAME.kicad_mod").write_text(f'''(footprint "BAT_PADS_FLAME"
  (version 20240108) (generator "flame-earring")
  (layer "F.Cu")
  (descr "Wire pads for a 501230-class LiPo: 1 = + (VBATT), 2 = - (GND); hand-soldered, not assembled")
  (attr smd exclude_from_pos_files exclude_from_bom)
  (fp_text reference "BT1" (at 0 -1.6) (layer "F.SilkS") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
  (fp_text value "CELL" (at 0 1.6) (layer "F.Fab") hide (effects (font (size 0.5 0.5) (thickness 0.08))))
  (fp_rect (start {-BT1_X - 0.85} -1.05) (end {-BT1_X + 0.85} 1.05) (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))
  (fp_rect (start {BT1_X - 0.85} -1.05) (end {BT1_X + 0.85} 1.05) (stroke (width 0.05) (type default)) (fill none) (layer "F.CrtYd"))
  (pad "1" smd rect (at {-BT1_X} 0) (size 1.2 1.6) (layers "F.Cu" "F.Paste" "F.Mask"))
  (pad "2" smd rect (at {BT1_X} 0) (size 1.2 1.6) (layers "F.Cu" "F.Paste" "F.Mask"))
)
''')


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
    ap.add_argument("-o", "--output", default=str(BOARD_DIR / "build/flame.kicad_pcb"))
    a = ap.parse_args()
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
        name = "BAT_PADS_FLAME" if c["part"] == "battery_pads" else VIALESS_QFN if c["footprint"] == VIALESS_QFN + "_ThermalVias" else c["footprint"]
        own = name in ("BAT_PADS_FLAME", VIALESS_QFN)
        fp = load(OWN_LIB if own else LIB, name)
        fp.SetFPID(pcbnew.LIB_ID("flame" if own else "parts", name))
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
    hanger = load(OWN_LIB, "HANGER_FLAME")
    hanger.SetFPID(pcbnew.LIB_ID("flame", "HANGER_FLAME"))
    hanger.SetReference("H1")
    board.Add(hanger)
    put(hanger, geo["hanger"]["x"], geo["hanger"]["y"], 0, False)
    hanger.SetLocked(True)

    fixed = {"H1"}
    # LEDs, in geometry order = LED1..N.
    for i, d in enumerate(geo["leds"], 1):
        fp = fps[f"LED{i}"]
        put(fp, d["x"], d["y"], d["rotation"], False)
        fp.SetLocked(True)
        fixed.add(f"LED{i}")

    # The light sensor, front, in the innermost ring's seam, along the ring.
    inner = geo["rings"][-1]
    sx, sy = inner["seam"]
    first = inner["leds"][0]
    ang = math.degrees(math.atan2(-first["tangent"][1], first["tangent"][0]))
    put(fps["Q1"], sx, sy, ang, False)
    fps["Q1"].SetLocked(True)
    fixed.add("Q1")

    # J1 on the back at the base: rotation 180 there puts the mouth toward +y. Its courtyard's
    # mouth edge goes USB_OVERHANG past the base.
    j1 = fps["J1"]
    put(j1, geo["usb"]["x"], 0, 180, True)
    cy = j1.GetCourtyard(pcbnew.B_CrtYd).BBox()
    put(j1, geo["usb"]["x"], (geo["usb"]["y_edge"] + geo["usb"]["overhang"]) - cy.GetBottom() / 1e6, 180, True)
    j1.SetLocked(True)
    fixed.add("J1")

    # BT1: the wire pads either side of J1, on the back.
    put(fps["BT1"], geo["usb"]["x"], BT1_Y, 0, True)
    fps["BT1"].SetLocked(True)
    fixed.add("BT1")

    # SW2 on the back at the left edge: its actuator (footprint -y) points out of the board.
    import sys
    sys.path.insert(0, str(HERE))
    import geometry as G
    z = SW2_HEIGHT
    lx = G.left_x(z)
    # The outward normal of the left edge at that height.
    dz = 0.2
    tx, ty = G.left_x(z + dz) - G.left_x(z - dz), -2 * dz
    nx, ny = ty, -tx
    if nx > 0:
        nx, ny = -nx, -ny
    ln = math.hypot(nx, ny)
    nx, ny = nx / ln, ny / ln
    sw = fps["SW2"]
    best = None
    for k in range(0, 360, 1):
        put(sw, 0, 0, k, True)
        # Where footprint-local -y points on the board, for this side and rotation.
        p0 = sw.GetPosition()
        probe = pcbnew.VECTOR2I(0, mm(-1))
        tr = sw.GetOrientation()
        q = pcbnew.VECTOR2I(probe)
        q = rotate_local(sw, (0, -1))
        dot = q[0] * nx + q[1] * ny
        if best is None or dot > best[0]:
            best = (dot, k)
    put(sw, 0, 0, best[1], True)
    bb = sw.GetCourtyard(pcbnew.B_CrtYd).BBox()
    # Body face 0.2 mm inside the edge along the normal: the actuator sticks out 0.7.
    cx = lx + nx * 0.0 - (bb.GetLeft() / 1e6) + 0.15
    put(sw, cx, G.LENGTH - z, best[1], True)
    sw.SetLocked(True)
    fixed.add("SW2")

    # The MCU cluster: simple_earring's poses relative to U1 (front there), placed front-side
    # about U1 here, then flipped left-right about U1 and turned, as one.
    cl = json.load(open(BOARD_DIR / "vendor/simple_earring/mcu_cluster.json"))
    skip = {"Q1", "C26"}
    members = [r for r in cl["parts"] if r in fps and r not in skip and not r.startswith("LED")
               and cir["components"][r]["footprint"].replace("_ThermalVias", "") == cl["parts"][r]["footprint"].replace("_ThermalVias", "")]
    for r in members:
        p = cl["parts"][r]
        put(fps[r], MCU_AT[0] + p["dx"], MCU_AT[1] + p["dy"], p["rotation"], False)
    centre = V(*MCU_AT)
    for r in members:
        fps[r].Flip(centre, pcbnew.FLIP_DIRECTION_LEFT_RIGHT)
        fps[r].Rotate(centre, pcbnew.EDA_ANGLE(MCU_ROT - cl["u1_rotation"], pcbnew.DEGREES_T))
        fps[r].SetLocked(True)
        fixed.add(r)

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
                continue   # a zone boundary or a link the geometry broke (the hanger)
            p, q = do.GetPosition(), di.GetPosition()
            # Along the ring line between the two pads, so a long link (an LED left out) follows
            # the curve between the rails instead of cutting across one.
            path = along_line(ring["line"], (p.x / 1e6, p.y / 1e6), (q.x / 1e6, q.y / 1e6))
            # A link that would cross a keep-out (J1's legs) is the router's: it goes under it.
            from shapely.geometry import LineString, Polygon
            if any(LineString(path).buffer(DATA_WIDTH / 2 + RULES["min_clearance"]).intersects(Polygon(k)) for k in geo["front_keepout"]):
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
    parts["H1"] = {}   # the ear-wire hole: its plated pad blocks both sides
    composed = {"board": "flame-earring", "parts": parts, "nets": cir["nets"]}
    json.dump(composed, open(fpdir / "composed.json", "w"), indent=1)
    bypass = [r for r in cir["components"] if r.startswith("C") and r[1:].isdigit() and int(r[1:]) >= 201]
    h = geo["hanger"]
    hr = h["pad"] / 2 + 0.25
    seams = [{"name": "hanger", "rect": [h["x"] - hr, h["y"] - hr, h["x"] + hr, h["y"] + hr], "side": "back"}] + [{"name": f"seam{r['ring']}", "rect": [r["seam"][0] - 1.0, r["seam"][1] - 1.0, r["seam"][0] + 1.0, r["seam"][1] + 1.0], "side": "back"} for r in geo["rings"]]
    spec = {
        "note": "Written by scripts/board.py. The back side is placed inside the flame outline; the front (LEDs, light sensor), J1, SW2, BT1, the ear-wire hole and the MCU cluster stay where board.py put them. The LED bypass capacitors are placed later, with the rail vias. Each ring's seam keeps the back clear for its data vias.",
        "outline_polygon": geo["outline"],
        "keep": sorted(fixed),
        "exclude": sorted(bypass),
        "keepouts": seams,
        "max_net_pins": 10,
        "options": {"moves": 1500000, "margin": 0.35, "w_area": 0.0, "clearance": 0.1},
    }
    res = fpdir / "reserved.json"
    if res.exists():
        # The caps with a rail slot are stitch.py's; any without one is floorplanned like the rest.
        r = json.load(open(res))
        spec["keepouts"] += r["keepouts"]
        spec["exclude"] = sorted(c["ref"] for c in r["caps"])
    json.dump(spec, open(fpdir / "spec.json", "w"), indent=1)
    print(json.dumps({"board": str(out), "footprints": len(fps) + 1, "fixed": len(fixed), "rail_segments": n_rail, "data_links": n_data, "mcu_cluster": len(members)}))


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
