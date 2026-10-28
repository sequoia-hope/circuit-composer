#!/usr/bin/env python3
"""The flame earring's geometry, the single source for outline, LEDs and keep-outs.

Coordinates are KiCad's (mm, y down), seen from the front. The point is at the top (y = 0)
and the blunt base at the bottom (y = LENGTH), where the USB-C mouth sits on a flat.

The shape is two edges, each a cubic spline through control points (`LEFT`, `RIGHT`: height
above the base -> x). Below about 40 mm the body stays wide enough for the cell, a straight
12.5 mm-wide prism, to hide behind it. Above, the left edge sweeps over and the right edge
pulls in under the curl and swells again, so the tip hooks to the right: a point-tipped S.

LEDs sit on nested flames: inward offsets of the outline, one ring pitch apart, so each ring is
a smaller flame inside the last. Rings are numbered from the outside in.

Each WS2816C is turned 45 degrees to its ring, so its DO and DI pads lie on the ring line (DO
toward the next LED, DI toward the one before) and its GND and VDD pads point across it. Data
runs along the ring on F.Cu, corner to corner; a GND rail and a VDD rail, exact offset curves at
RAIL_OFFSET either side of the ring line, run over the tips of those pads. Nothing on the LED
side needs a via except where rails stitch to the planes and where data enters and leaves a
ring, at its seam: one LED is left out to make room (two on the innermost ring, whose seam also
holds the ambient light sensor). Seams are on the left flank, level with the cell, at the ring
point nearest SEAM_TOWARD, so their vias come out on the back beside the MCU and not on J1.

    geometry.py [-o geometry.json] [--svg preview.svg]
"""
import argparse
import json
import math

LENGTH = 59.0
# Height above the base (mm) -> edge x (mm). The base is a 10 mm flat for USB-C.
LEFT = [(0.0, -5.0), (1.0, -7.3), (3.5, -9.3), (9.0, -10.4), (16.0, -10.4), (24.0, -9.8), (32.0, -8.7),
        (38.0, -7.8), (42.0, -7.3), (45.5, -6.9), (49.0, -6.0), (52.0, -4.3), (54.5, -1.9), (56.6, 0.9), (58.0, 3.0), (59.0, 4.6)]
RIGHT = [(0.0, 5.0), (1.0, 7.3), (3.5, 9.1), (9.0, 10.0), (16.0, 9.9), (23.0, 9.2), (30.0, 8.3), (36.0, 7.5),
         (40.0, 6.6), (43.5, 5.2), (46.5, 3.9), (49.5, 3.3), (52.0, 3.6), (54.3, 4.4), (56.3, 4.9), (57.8, 5.0), (59.0, 4.6)]

EDGE_MARGIN = 1.25        # ring 0's line to the board edge: pad tips 0.92, + 0.3 copper to edge
LED_PITCH = 2.35          # along a ring: the 45-degree courtyard reaches 1.13 each way
LED_MIN_GAP = 2.28        # any two LED centres
RING_SPACING = 2.55       # between ring lines: rails and pad tips leave a 0.71 mm street for 0.4 mm vias
RAIL_OFFSET = 0.75        # rail centre from the ring line, over the GND / VDD pad tips
RAIL_WIDTH = 0.25
SEAM_TOWARD = (-14.0, 33.0)   # each ring's seam is its point nearest this
ALS_RING_SKIP = 2             # LEDs left out at the innermost ring's seam
# WS2816C-1313 pads at rotation 0 (vendor footprint): 1 VDD, 2 DO, 3 GND, 4 DI.
LED_PADS = {"VDD": (-0.4, -0.4), "DO": (-0.4, 0.4), "GND": (0.4, 0.4), "DI": (0.4, -0.4)}
CELL = {"w": 12.5, "l": 32.0, "slop": 0.3, "from_base": 7.6}   # 501230-class, PCM end at the base
USB_DEPTH = 7.35          # TYPE-C-31-M-12 body depth from its mating face
USB_OVERHANG = 1.3
HANGER = {"from_tip": 6.2, "drill": 1.0, "pad": 1.8}
# Front-side keep-outs, board coordinates (x0, y0, x1, y1): J1's through-hole shell legs and
# locating pegs come up through the board under the LED field. Read off J1 as board.py places it
# (TYPE-C-31-M-12, back, rotation 180, mouth USB_OVERHANG past the base); grown by 0.25 below.
# The whole span between the legs is kept out: a rail piece there would be cut off at both ends
# with J1's body under it on the back, so no via could ever feed it.
FRONT_KEEPOUTS = [(-4.93, 52.455, 4.93, 58.535)]


def spline(points):
    """A natural cubic spline through (t, v) points, as a function of t."""
    n = len(points)
    t = [p[0] for p in points]
    v = [p[1] for p in points]
    h = [t[i + 1] - t[i] for i in range(n - 1)]
    a = [0.0] * n
    b = [0.0] * n
    c = [0.0] * n
    d = [0.0] * n
    for i in range(1, n - 1):
        a[i], b[i], c[i] = h[i - 1], 2 * (h[i - 1] + h[i]), h[i]
        d[i] = 6 * ((v[i + 1] - v[i]) / h[i] - (v[i] - v[i - 1]) / h[i - 1])
    b[0] = b[-1] = 1.0
    m = [0.0] * n
    for i in range(1, n):
        w = a[i] / b[i - 1]
        b[i] -= w * c[i - 1]
        d[i] -= w * d[i - 1]
    for i in range(n - 1, -1, -1):
        m[i] = (d[i] - (c[i] * m[i + 1] if i < n - 1 else 0)) / b[i]

    def f(x):
        x = min(max(x, t[0]), t[-1])
        i = max(j for j in range(n - 1) if t[j] <= x) if x < t[-1] else n - 2
        dx = x - t[i]
        hi = h[i]
        return (m[i] * (t[i + 1] - x) ** 3 / (6 * hi) + m[i + 1] * dx ** 3 / (6 * hi)
                + (v[i] / hi - m[i] * hi / 6) * (t[i + 1] - x) + (v[i + 1] / hi - m[i + 1] * hi / 6) * dx)
    return f


left_x = spline(LEFT)
right_x = spline(RIGHT)


def half(z):
    return (right_x(z) - left_x(z)) / 2


def centre(z):
    return (right_x(z) + left_x(z)) / 2


def to_board(height, x):
    """Height above the base and lateral x -> board (x, y)."""
    return (x, LENGTH - height)


def outline(step=0.25):
    """The outline as a closed polygon, counter-clockwise on screen from the base's left corner."""
    hs = [i * step for i in range(int(LENGTH / step) + 1)]
    left = [to_board(z, centre(z) - max(half(z), 0.0)) for z in hs]
    right = [to_board(z, centre(z) + max(half(z), 0.0)) for z in hs]
    return left + right[::-1][1:-1]


def inside_margin(p, poly, margin):
    """p is inside poly and at least `margin` from every edge."""
    x, y = p
    inside = False
    dmin = 1e9
    for i in range(len(poly)):
        (x1, y1), (x2, y2) = poly[i], poly[(i + 1) % len(poly)]
        if (y1 > y) != (y2 > y) and x < x1 + (y - y1) * (x2 - x1) / (y2 - y1):
            inside = not inside
        dx, dy = x2 - x1, y2 - y1
        t = max(0.0, min(1.0, ((x - x1) * dx + (y - y1) * dy) / (dx * dx + dy * dy or 1)))
        dmin = min(dmin, math.hypot(x - x1 - t * dx, y - y1 - t * dy))
    return inside and dmin >= margin


def offset_rings(poly, d):
    """The outline offset inward by d: its closed exteriors, largest first."""
    from shapely.geometry import Polygon
    g = Polygon(poly).buffer(-d, join_style="round", quad_segs=16)
    if g.is_empty:
        return []
    parts = [g] if g.geom_type == "Polygon" else list(g.geoms)
    return [list(q.exterior.coords) for q in sorted(parts, key=lambda q: -q.area)]


def rot(p, deg):
    """KiCad's rotation of a footprint-local point (counter-clockwise on screen, y down)."""
    c, s_ = math.cos(math.radians(deg)), math.sin(math.radians(deg))
    return (p[0] * c + p[1] * s_, -p[0] * s_ + p[1] * c)


def led_rotation(t, n_in, gnd_inside):
    """The rotation that points DO along t (downstream) and GND toward n_in (or away)."""
    best = None
    for k in range(0, 720):
        r = k * 0.5
        do = rot(LED_PADS["DO"], r)
        gnd = rot(LED_PADS["GND"], r)
        dl = math.hypot(*do)
        err = 1 - (do[0] * t[0] + do[1] * t[1]) / dl
        side = (gnd[0] * n_in[0] + gnd[1] * n_in[1]) * (1 if gnd_inside else -1)
        if side > 0 and (best is None or err < best[0]):
            best = (err, r)
    return best[1]


def resample(coords, pitch, skip=1):
    """Points every `pitch` along a closed ring, starting at its seam (the point nearest
    SEAM_TOWARD), going counter-clockwise on screen, with each point's unit tangent. The first
    `skip` points are the seam and are left out."""
    n = len(coords) - 1
    start = min(range(n), key=lambda i: math.dist(coords[i], SEAM_TOWARD))
    pts = coords[start:n] + coords[:start + 1]
    if sum((pts[i + 1][0] - pts[i][0]) * (pts[i + 1][1] + pts[i][1]) for i in range(len(pts) - 1)) < 0:
        pts = pts[::-1]   # make it counter-clockwise on screen (shoelace sign with y down)
    total = sum(math.dist(pts[i], pts[i + 1]) for i in range(len(pts) - 1))
    count = max(1, int(total // pitch))
    step = total / count
    out = []
    acc, i, target = 0.0, 0, 0.0
    while len(out) < count and i < len(pts) - 1:
        seg = math.dist(pts[i], pts[i + 1])
        if seg and acc + seg >= target:
            t = (target - acc) / seg
            x = pts[i][0] + t * (pts[i + 1][0] - pts[i][0])
            y = pts[i][1] + t * (pts[i + 1][1] - pts[i][1])
            out.append((x, y, ((pts[i + 1][0] - pts[i][0]) / seg, (pts[i + 1][1] - pts[i][1]) / seg)))
            target += step
        else:
            acc += seg
            i += 1
    return out[skip:], total, pts[0]


CLEARANCE = 0.127
PAD = 0.5
COURTYARD = 1.6


def led_shapes(x, y, r):
    """The LED's four pads (by function) and its courtyard, as shapely polygons on the board."""
    from shapely.geometry import Polygon

    def square(cx, cy, size):
        h = size / 2
        return Polygon([(x + q[0], y + q[1]) for q in (rot((cx + u, cy + v), r) for u, v in ((-h, -h), (h, -h), (h, h), (-h, h)))])
    pads = {name: square(px, py, PAD) for name, (px, py) in LED_PADS.items()}
    return pads, square(0, 0, COURTYARD)


def leds(poly, hanger_at):
    """The rings: their LEDs (chain order), rails and seams. With DO pointing along the way the
    chain runs, the pin order puts GND on the left of travel: counter-clockwise on screen, that
    is every ring's inside. So each ring has GND inside and VDD outside.

    Rails keep every piece of their offset curve that lies along the ring (an inside rail splits
    where the ring pinches). An LED is placed only if, checked exactly: its GND pad touches an
    inside rail and its VDD pad an outside rail; every pad clears every rail of another net, and
    its data pads every rail, by CLEARANCE; its courtyard overlaps no other LED's."""
    from shapely.geometry import LineString, Point, Polygon
    from shapely.ops import unary_union
    out_rings, placed, courtyards = [], [], []
    rails = []   # (ring, side, buffered geometry)
    k = 0
    while True:
        d = EDGE_MARGIN + k * RING_SPACING
        lines = offset_rings(poly, d)
        if not lines:
            break
        coords = lines[0]
        ring_poly = Polygon(coords)
        inner = [c for c in offset_rings(poly, d + RAIL_OFFSET) if ring_poly.buffer(0.01).contains(Polygon(c))]
        outer = [c for c in offset_rings(poly, d - RAIL_OFFSET) if Polygon(c).buffer(0.01).contains(ring_poly)]
        if not inner or not outer:
            break
        innermost = not offset_rings(poly, d + RING_SPACING + RAIL_OFFSET)
        pts, length, seam = resample(coords, LED_PITCH, skip=ALS_RING_SKIP if innermost else 1)
        if len(pts) < 3:
            break
        inside_geom = unary_union([LineString(c).buffer(RAIL_WIDTH / 2) for c in inner])
        outside_geom = unary_union([LineString(c).buffer(RAIL_WIDTH / 2) for c in outer])
        rails.append((k, "inside", inside_geom))
        rails.append((k, "outside", outside_geom))
        out_rings.append({"ring": k, "offset": round(d, 4), "length_mm": round(length, 2), "seam": [round(seam[0], 4), round(seam[1], 4)],
                          "gnd_rail": "inside", "line": [[round(a, 4), round(b, 4)] for a, b in coords], "leds": [], "cand": pts,
                          "rails": {"inside": [[[round(a, 4), round(b, 4)] for a, b in c] for c in inner],
                                    "outside": [[[round(a, 4), round(b, 4)] for a, b in c] for c in outer]},
                          "poly": ring_poly})
        k += 1
    # Rails are known for every ring before any LED is judged against them. Keep-outs (the
    # ear-wire hole, J1's through-holes) cut the rails and exclude LEDs.
    from shapely.geometry import box
    hanger = unary_union([Point(*hanger_at).buffer(HANGER["pad"] / 2 + CLEARANCE)] + [box(*r).buffer(0.25) for r in FRONT_KEEPOUTS])
    cut = hanger.buffer(RAIL_WIDTH / 2 + 0.02)
    for idx, (kk, sd, g) in enumerate(rails):
        rails[idx] = (kk, sd, g.difference(cut))
    dropped = {"rail": 0, "crowd": 0, "hanger": 0}
    for ring in out_rings:
        k = ring["ring"]
        for x, y, t in ring.pop("cand"):
            nx, ny = -t[1], t[0]
            if not ring["poly"].contains(Point(x + 0.1 * nx, y + 0.1 * ny)):
                nx, ny = -nx, -ny
            r = led_rotation(t, (nx, ny), True)
            do = rot(LED_PADS["DO"], r)
            if (do[0] * t[0] + do[1] * t[1]) / math.hypot(*do) < 0.995:
                raise SystemExit(f"ring {k} LED at ({x:.2f}, {y:.2f}): no rotation points DO along the ring with GND inside")
            pads, court = led_shapes(x, y, r)
            if court.intersects(hanger):
                dropped["hanger"] += 1
                continue
            if any(court.intersects(c) for c in courtyards):
                dropped["crowd"] += 1
                continue
            own_in = next(g for kk, sd, g in rails if kk == k and sd == "inside")
            own_out = next(g for kk, sd, g in rails if kk == k and sd == "outside")
            ok = pads["GND"].intersects(own_in) and pads["VDD"].intersects(own_out)
            for name, pad in pads.items():
                for kk, sd, g in rails:
                    same = (name == "GND" and sd == "inside") or (name == "VDD" and sd == "outside" and zone_of_ring(kk) == zone_of_ring(k))
                    if not same and pad.distance(g) < CLEARANCE:
                        ok = False
            if not ok:
                dropped["rail"] += 1
                continue
            led = {"x": round(x, 4), "y": round(y, 4), "ring": k, "rotation": r, "tangent": [round(t[0], 5), round(t[1], 5)]}
            ring["leds"].append(led)
            led["_pads"] = pads
            placed.append(led)
            courtyards.append(court)
        ring.pop("poly")
        # The rails as pieces: each side's curves less the keep-outs, kept only where a piece
        # touches a pad of its own net (a piece no LED reaches would float).
        for side, net_pad in (("inside", "GND"), ("outside", "VDD")):
            pieces = []
            for c in ring["rails"][side]:
                left = LineString(c).difference(cut)
                for seg in ([left] if left.geom_type == "LineString" else list(getattr(left, "geoms", []))):
                    if seg.is_empty or seg.length < 0.05:
                        continue
                    band = seg.buffer(RAIL_WIDTH / 2)
                    touching = [l["_pads"][net_pad] for l in ring["leds"] if l["_pads"][net_pad].intersects(band)]
                    if not touching:
                        continue
                    if seg.is_ring:
                        pieces.append([[round(a, 4), round(b, 4)] for a, b in seg.coords])
                        continue
                    # An open piece runs only from its first pad to its last: no dangling ends.
                    from shapely.ops import substring
                    at = [seg.project(p_.centroid) for p_ in touching]
                    trimmed = substring(seg, max(min(at) - 0.05, 0), min(max(at) + 0.05, seg.length))
                    if trimmed.length > 0.05:
                        pieces.append([[round(a, 4), round(b, 4)] for a, b in trimmed.coords])
            ring["rails"][side] = pieces
    for l in placed:
        l.pop("_pads", None)
    print(f"LEDs dropped: {dropped}", file=__import__("sys").stderr)
    for ring in out_rings:
        ring["keepout"] = None
    leds.keepout = hanger
    return [r for r in out_rings if r["leds"]]


def zone_of_ring(k):
    """L is ring 0, R ring 1, C the rest: each zone has its own VLED net."""
    return "L" if k == 0 else "R" if k == 1 else "C"


def cell_rect():
    """The cell's worst-case envelope on the back: an axis-aligned rectangle centred on the
    straight lower body, from just above the USB-C body upward."""
    z0 = CELL["from_base"]
    z1 = z0 + CELL["l"]
    cx = sum(centre(z0 + (z1 - z0) * i / 20) for i in range(21)) / 21
    w = CELL["w"] / 2 + CELL["slop"]
    return {"x0": round(cx - w, 3), "x1": round(cx + w, 3), "y0": round(LENGTH - z1 - CELL["slop"], 3), "y1": round(LENGTH - z0 + CELL["slop"], 3)}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--output", default="geometry.json")
    ap.add_argument("--svg")
    a = ap.parse_args()
    poly = outline()
    hz = LENGTH - HANGER["from_tip"]
    hanger_at = to_board(hz, centre(hz))
    rings_ = leds(poly, hanger_at)
    leds_ = [d for r in rings_ for d in r["leds"]]
    cell = cell_rect()
    # How far inside the outline the cell envelope stays (its corners and side midpoints).
    def clearance(p):
        best = 1e9
        for i in range(len(poly)):
            (x1, y1), (x2, y2) = poly[i], poly[(i + 1) % len(poly)]
            dx, dy = x2 - x1, y2 - y1
            t = max(0.0, min(1.0, ((p[0] - x1) * dx + (p[1] - y1) * dy) / (dx * dx + dy * dy or 1)))
            best = min(best, math.hypot(p[0] - x1 - t * dx, p[1] - y1 - t * dy))
        return best
    samples = [(cell[xk], cell["y0"] + (cell["y1"] - cell["y0"]) * i / 20) for xk in ("x0", "x1") for i in range(21)]
    cell_margin = min(clearance(p) if inside_margin(p, poly, 0) else -clearance(p) for p in samples)
    usb_x = centre(0.0)
    out = {
        "note": "Generated by scripts/geometry.py. Board coordinates, mm, y down, front view.",
        "length_mm": LENGTH,
        "width_mm": round(max(p[0] for p in poly) - min(p[0] for p in poly), 2),
        "outline": [[round(x, 4), round(y, 4)] for x, y in poly],
        "hanger": {"x": round(hanger_at[0], 4), "y": round(hanger_at[1], 4), **HANGER},
        "usb": {"x": round(usb_x, 4), "y_edge": LENGTH, "overhang": USB_OVERHANG, "depth": USB_DEPTH},
        "cell": {**cell, "spec": CELL, "margin_to_edge_mm": round(cell_margin, 3)},
        "rings": rings_,
        "front_keepout": [[[round(a, 4), round(b, 4)] for a, b in g.exterior.coords] for g in (getattr(leds.keepout, "geoms", None) or [leds.keepout])],
        "leds": leds_,
    }
    json.dump(out, open(a.output, "w"), indent=1)
    if a.svg:
        xs = [p[0] for p in poly]
        x0, x1 = min(xs) - 2, max(xs) + 2
        s = 30
        def square(d):
            a = math.radians(-d["rotation"])
            c, sn = math.cos(a), math.sin(a)
            return " ".join(f"{d['x'] + u * c - v * sn:.3f},{d['y'] + u * sn + v * c:.3f}" for u, v in ((-0.65, -0.65), (0.65, -0.65), (0.65, 0.65), (-0.65, 0.65)))
        svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{(x1 - x0) * s:.0f}" height="{(LENGTH + 4) * s:.0f}" viewBox="{x0} -2 {x1 - x0} {LENGTH + 4}">',
               '<rect x="-100" y="-100" width="300" height="300" fill="#111"/>',
               f'<polygon points="{" ".join(f"{x:.3f},{y:.3f}" for x, y in poly)}" fill="#1d3b2a" stroke="#c9a227" stroke-width="0.08"/>',
               f'<rect x="{cell["x0"]}" y="{cell["y0"]}" width="{cell["x1"] - cell["x0"]}" height="{cell["y1"] - cell["y0"]}" fill="none" stroke="#5aa0ff" stroke-width="0.08" stroke-dasharray="0.4 0.3"/>',
               f'<circle cx="{hanger_at[0]}" cy="{hanger_at[1]}" r="{HANGER["pad"] / 2}" fill="#c9a227"/>']
        for d in leds_:
            hue = 5 + 9 * d["ring"]
            svg.append(f'<polygon points="{square(d)}" fill="hsl({hue:.0f},95%,60%)"/>')
        for r in rings_:
            for side, colour in (("inside", "#4a8"), ("outside", "#a84")):
                for piece in r["rails"][side]:
                    svg.append(f'<polyline points="{" ".join(f"{x:.3f},{y:.3f}" for x, y in piece)}" fill="none" stroke="{colour}" stroke-width="{RAIL_WIDTH}" opacity="0.7"/>')
        svg.append("</svg>")
        open(a.svg, "w").write("\n".join(svg))
    print(json.dumps({k: out[k] for k in ("length_mm", "width_mm")} | {"rings": [len(r["leds"]) for r in rings_], "leds": len(leds_), "cell_margin_mm": out["cell"]["margin_to_edge_mm"], "cell": cell}))


if __name__ == "__main__":
    main()
