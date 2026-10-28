#!/usr/bin/env python3
"""The lightning-bolt earring's geometry, the single source for outline, LEDs and keep-outs.

Coordinates are KiCad's (mm, y down), seen from the front. The top edge (y = 0) is a narrow tab
that carries USB-C on the back, mouth up: the earring hangs from a plug in it. Below the tab
the bolt is fat with an obtuse zigzag: a broad upper stroke, a gentle notch, and a lower stroke
tapering to the point at the bottom left. The outline is a polygon (`BOLT`, drawn upright,
then leaned LEAN degrees below the tab), its corners rounded by CORNER_R.

The cell, a 10 mm-wide prism, lies along the bolt under the upper stroke, turned to fit; its
worst-case envelope is found by search (`cell_place`).

LEDs sit on nested bolts: inward offsets of the outline, one ring pitch apart. Each WS2816C is
turned 45 degrees to its ring, so its DO and DI pads lie on the ring line and its GND and VDD
pads point across it. Data runs along the ring on F.Cu, corner to corner; a GND rail and a VDD
rail, exact offset curves at RAIL_OFFSET either side of the ring line, run over the tips of
those pads. Nothing on the LED side needs a via except where rails stitch to the planes and
where data enters and leaves a ring, at its seam: one LED is left out to make room (two on the
innermost ring, whose seam also holds the light sensor). Seams are on the left flank, at the
ring point nearest SEAM_TOWARD, so their vias come out on the back beside the MCU.

    geometry.py [-o geometry.json] [--svg preview.svg]
"""
import argparse
import json
import math

TAB = 5.3         # half-width of the USB-C tab: J1's shell legs reach 4.93, + rounding and 0.2 to the edge
# The body: a fat band along a leaning axis whose centreline steps sideways at the middle (the
# zigzag), tapering to a point at each end. The centreline's sideways offset is an odd function
# of position along the axis and the half-width an even one, so the body is unchanged by a half
# turn about its centre: the LED field built on it is too.
BODY_CENTRE = (0.0, 40.0)     # board coordinates of the body's centre of symmetry
BODY_LEN = 68.0               # end point to end point, along the axis
AXIS_DEG = 14.0               # the axis leans this far from vertical, top to the right
HALF_W = 10.2                 # the band's half-width in its middle: room for a fourth ring
TAPER = 21.0                  # the length over which each end narrows to its point
STEP = 3.8                    # the centreline's sideways step either side of the centre: the zigzag
STEP_SHARP = 1.1              # how short (mm) the step's rise is: smaller, a crisper notch (still obtuse)
CORNER_R = 0.6
FLARE = 9.0                   # how much of the body's top end the tab's flare takes in
TAB_LEN = 7.0                 # the tab's straight part, from the top edge down; then it flares into the body

EDGE_MARGIN = 1.25        # ring 0's line to the board edge: pad tips 0.92, + 0.3 copper to edge
LED_PITCH = 2.35          # along a ring: the 45-degree courtyard reaches 1.13 each way
LED_MIN_GAP = 2.28        # any two LED centres
RING_SPACING = 2.55       # between ring lines: rails and pad tips leave a 0.71 mm street for 0.4 mm vias
RAIL_OFFSET = 0.75        # rail centre from the ring line, over the GND / VDD pad tips
RAIL_WIDTH = 0.25
SEAM_TOWARD = (-16.0, 24.0)   # each ring's seam is its point nearest this
ALS_RING = 2                  # the ring whose seam also holds the ambient light sensor
ALS_RING_SKIP = 2             # LEDs left out there (and, by symmetry, opposite)
# WS2816C-1313 pads at rotation 0 (vendor footprint): 1 VDD, 2 DO, 3 GND, 4 DI.
LED_PADS = {"VDD": (-0.4, -0.4), "DO": (-0.4, 0.4), "GND": (0.4, 0.4), "DI": (0.4, -0.4)}
CELL = {"w": 10.0, "l": 30.0, "slop": 0.3}   # 401030 / 501030-class, PCM end toward the tab
USB_DEPTH = 7.35          # TYPE-C-31-M-12 body depth from its mating face
USB_OVERHANG = 1.3
HANGER = None     # it hangs from the USB-C plug
# Front-side keep-out, board coordinates (x0, y0, x1, y1): J1's through-hole shell legs and
# locating pegs come up through the tab. J1 is TYPE-C-31-M-12 on the back at rotation 0, its
# mouth USB_OVERHANG above the top edge; its legs and pegs span x +-4.93 and y 0.47 to 6.55 from
# there (read off the footprint). The whole span is kept out, grown by 0.25 below.
def front_keepouts():
    x = tab_centre()
    return [(x - 4.93, 0.465, x + 4.93, 6.545)]


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


def body_polygon():
    """The half-turn-symmetric body, without the tab."""
    from shapely.geometry import Polygon
    a = math.radians(AXIS_DEG)
    u = (math.sin(a), -math.cos(a))   # along the axis, toward the top end
    n = (math.cos(a), math.sin(a))    # across it, to the right
    cx, cy = BODY_CENTRE
    left, right = [], []
    steps = 400
    for i in range(steps + 1):
        t = -BODY_LEN / 2 + BODY_LEN * i / steps
        off = STEP * math.tanh(t / STEP_SHARP)          # odd: the zigzag
        end = BODY_LEN / 2 - abs(t)
        w = HALF_W * min(1.0, end / TAPER) ** 0.85         # even: pointed ends
        px, py = cx + t * u[0] + off * n[0], cy + t * u[1] + off * n[1]
        left.append((px - w * n[0], py - w * n[1]))
        right.append((px + w * n[0], py + w * n[1]))
    poly = Polygon(right + left[::-1]).buffer(0)
    return poly.buffer(-CORNER_R, join_style="round").buffer(CORNER_R, join_style="round")


def bolt_polygon():
    """The outline: the body and the USB-C tab grafted onto its top end. The tab sits straight
    above the body's top point, and flares into the body: the hull of the tab's foot and the body
    within FLARE of that point."""
    from shapely.geometry import Point, box
    body = body_polygon()
    top = min(body.exterior.coords, key=lambda p: p[1])
    tab = box(top[0] - TAB, 0.0, top[0] + TAB, TAB_LEN)
    flare = tab.union(body.intersection(Point(top).buffer(FLARE))).convex_hull
    g = body.union(tab).union(flare)
    return g.buffer(CORNER_R, join_style="round").buffer(-2 * CORNER_R, join_style="round").buffer(CORNER_R, join_style="round")


def tab_centre():
    """The tab's centre x at the top edge."""
    xs = [x for x, y in bolt_polygon().exterior.coords if y < 0.05]
    return (min(xs) + max(xs)) / 2


def outline(step=None):
    """The outline as a closed polygon, its points every ~0.25 mm."""
    p = bolt_polygon().exterior
    n = int(p.length / 0.25)
    return [tuple(p.interpolate(i * p.length / n).coords[0]) for i in range(n)]


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
    # The seam is on a straight stretch (within 5 degrees over +-2.5 mm): what sits in it, the
    # light sensor and the data vias, is straight and the rails stay parallel either side.
    from shapely.geometry import LineString as _LS
    ring = _LS(coords)

    def straight(i):
        d = ring.project(_LS([coords[i], coords[i]]).centroid) if False else ring.project(__import__("shapely.geometry", fromlist=["Point"]).Point(coords[i]))
        L = ring.length
        a, b, c, e = (ring.interpolate((d + o) % L) for o in (-2.5, -0.1, 0.1, 2.5))
        t1 = math.atan2(b.y - a.y, b.x - a.x)
        t2 = math.atan2(e.y - c.y, e.x - c.x)
        return abs(math.degrees((t1 - t2 + math.pi) % (2 * math.pi) - math.pi)) < 5
    ok = [i for i in range(n) if straight(i)] or list(range(n))
    start = min(ok, key=lambda i: math.dist(coords[i], SEAM_TOWARD))
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
    from shapely.geometry import LineString, Point, Polygon, box
    from shapely.ops import unary_union
    out_rings, placed, courtyards = [], [], []
    rails = []   # (entry, ring, side, buffered geometry)
    pairs = []   # (entry a, index a, entry b, index b): partners under the half turn
    cx, cy = BODY_CENTRE

    def turn(p_):
        return (2 * cx - p_[0], 2 * cy - p_[1], (-p_[2][0], -p_[2][1]))

    def entry(k, d, coords, cand, length, seam):
        ring_poly = Polygon(coords)
        inner = [c for c in offset_rings(poly, d + RAIL_OFFSET) if ring_poly.buffer(0.01).contains(Polygon(c))]
        outer = [c for c in offset_rings(poly, d - RAIL_OFFSET) if Polygon(c).buffer(0.01).contains(ring_poly)]
        if not inner or not outer:
            return None
        e = len(out_rings)
        rails.append((e, k, "inside", unary_union([LineString(c).buffer(RAIL_WIDTH / 2) for c in inner])))
        rails.append((e, k, "outside", unary_union([LineString(c).buffer(RAIL_WIDTH / 2) for c in outer])))
        out_rings.append({"ring": k, "offset": round(d, 4), "length_mm": round(length, 2), "seam": [round(seam[0], 4), round(seam[1], 4)],
                          "gnd_rail": "inside", "line": [[round(a, 4), round(b, 4)] for a, b in coords], "leds": [], "cand": cand,
                          "rails": {"inside": [[[round(a, 4), round(b, 4)] for a, b in c] for c in inner],
                                    "outside": [[[round(a, 4), round(b, 4)] for a, b in c] for c in outer]},
                          "poly": ring_poly})
        return e

    k = 0
    while True:
        d = EDGE_MARGIN + k * RING_SPACING
        parts = offset_rings(poly, d)
        if not parts:
            break
        skip = ALS_RING_SKIP if k == ALS_RING else 1
        made = False
        if len(parts) == 1:
            # One loop, symmetric in itself: its first half placed, the rest the half turn of it.
            pts, length, seam = resample(parts[0], LED_PITCH, skip=0)
            first = pts[:len(pts) // 2][skip:]
            cand = first + [turn(p_) for p_ in first]
            e = entry(k, d, parts[0], cand, length, seam) if len(cand) >= 4 else None
            if e is not None:
                pairs += [(e, i, e, i + len(first)) for i in range(len(first))]
                made = True
                if k == ALS_RING:
                    # The light sensor: in the gap opposite the seam (the seam keeps the data via),
                    # at the half turn of the seam gap's middle, along the ring.
                    a_, b_ = pts[0], pts[1]
                    mid = ((a_[0] + b_[0]) / 2, (a_[1] + b_[1]) / 2, a_[2])
                    x_, y_, t_ = turn(mid)
                    out_rings[e]["als"] = {"x": round(x_, 4), "y": round(y_, 4), "tangent": [round(t_[0], 5), round(t_[1], 5)]}
        else:
            # Two islands, each the half turn of the other: one placed, the other its image.
            a_coords = min(parts[:2], key=lambda c: Polygon(c).centroid.distance(Point(SEAM_TOWARD)))
            b_coords = parts[1] if a_coords is parts[0] else parts[0]
            pts, length, seam = resample(a_coords, LED_PITCH, skip=skip)
            if len(pts) >= 2:
                ea = entry(k, d, a_coords, pts, length, seam)
                eb = entry(k, d, b_coords, [turn(p_) for p_ in pts], length, turn((*seam, (0, 0)))[:2]) if ea is not None else None
                if ea is not None and eb is not None:
                    pairs += [(ea, i, eb, i) for i in range(len(pts))]
                    made = True
                elif ea is not None:
                    out_rings.pop()
                    rails[:] = [r for r in rails if r[0] != ea]
        if not made:
            break
        k += 1
    # Rails are known for every ring before any LED is judged against them. Keep-outs (J1's
    # through-holes) cut the rails and exclude LEDs.
    hanger = unary_union([box(*r).buffer(0.25) for r in front_keepouts()])
    cut = hanger.buffer(RAIL_WIDTH / 2 + 0.02)
    for idx, (e, kk, sd, g) in enumerate(rails):
        rails[idx] = (e, kk, sd, g.difference(cut))
    dropped = {"rail": 0, "crowd": 0, "keepout": 0}

    def judge(e, x, y, t):
        """The LED's rotation, pads and courtyard if it may go here, else why not."""
        ring = out_rings[e]
        k = ring["ring"]
        nx, ny = -t[1], t[0]
        if not ring["poly"].contains(Point(x + 0.1 * nx, y + 0.1 * ny)):
            nx, ny = -nx, -ny
        r = led_rotation(t, (nx, ny), True)
        do = rot(LED_PADS["DO"], r)
        if (do[0] * t[0] + do[1] * t[1]) / math.hypot(*do) < 0.995:
            raise SystemExit(f"ring {k} LED at ({x:.2f}, {y:.2f}): no rotation points DO along the ring with GND inside")
        pads, court = led_shapes(x, y, r)
        if court.intersects(hanger):
            return "keepout"
        if any(court.intersects(c) for c in courtyards):
            return "crowd"
        own_in = next(g for ee, kk, sd, g in rails if ee == e and sd == "inside")
        own_out = next(g for ee, kk, sd, g in rails if ee == e and sd == "outside")
        if not (pads["GND"].intersects(own_in) and pads["VDD"].intersects(own_out)):
            return "rail"
        for name, pad in pads.items():
            for ee, kk, sd, g in rails:
                same = (name == "GND" and sd == "inside") or (name == "VDD" and sd == "outside" and zone_of_ring(kk) == zone_of_ring(k))
                if not same and pad.distance(g) < CLEARANCE:
                    return "rail"
        return (r, pads, court)

    chosen = {}
    for ea, ia, eb, ib in pairs:
        a = judge(ea, *out_rings[ea]["cand"][ia])
        b = judge(eb, *out_rings[eb]["cand"][ib]) if not isinstance(a, str) else a
        if isinstance(a, str) or isinstance(b, str) or a[2].intersects(b[2]):
            dropped[a if isinstance(a, str) else b if isinstance(b, str) else "crowd"] += 2
            continue
        chosen[(ea, ia)], chosen[(eb, ib)] = a, b
        courtyards += [a[2], b[2]]
        partners_of = chosen.setdefault("_pairs", [])
        partners_of.append(((ea, ia), (eb, ib)))
    for e, ring in enumerate(out_rings):
        for j, (x, y, t) in enumerate(ring["cand"]):
            res = chosen.get((e, j))
            if res is None:
                continue
            r, pads, court = res
            led = {"x": round(x, 4), "y": round(y, 4), "ring": ring["ring"], "rotation": r % 360, "tangent": [round(t[0], 5), round(t[1], 5)], "partner": None, "_pads": pads, "_key": (e, j)}
            ring["leds"].append(led)
            placed.append(led)
    for ring in out_rings:
        ring.pop("cand")
    for ring in out_rings:
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
    # Each LED's partner, by number in the final order (1-based, geometry order).
    number = {l["_key"]: n + 1 for n, l in enumerate(l for r in out_rings for l in r["leds"])}
    for a, b in chosen.get("_pairs", []):
        for l in placed:
            if l["_key"] == a:
                l["partner"] = number[b]
            elif l["_key"] == b:
                l["partner"] = number[a]
    for l in placed:
        l.pop("_pads", None)
        l.pop("_key", None)
    print(f"LEDs dropped: {dropped}", file=__import__("sys").stderr)
    for ring in out_rings:
        ring["keepout"] = None
    leds.keepout = hanger
    return [r for r in out_rings if r["leds"]]


def zone_of_ring(k):
    """L is ring 0, R ring 1, C the rest: each zone has its own VLED net."""
    return "L" if k == 0 else "R" if k == 1 else "C"


def cell_place(poly):
    """The cell's worst-case envelope (its size plus slop all round), placed by search about the
    body's centre and along its axis: inside the outline, clear of J1, as far from the edge as it
    can be. Returns the polygon's corners."""
    from shapely.geometry import box as sbox
    from shapely import affinity
    w, l = CELL["w"] + 2 * CELL["slop"], CELL["l"] + 2 * CELL["slop"]
    j1 = sbox(*front_keepouts()[0]).buffer(0.4)
    cx, cy = BODY_CENTRE
    best = None
    for ang in [AXIS_DEG + a * 0.5 for a in range(-16, 17)]:
        for dx in [i * 0.25 for i in range(-16, 17)]:
            for dy in [i * 0.25 for i in range(-24, 25)]:
                r = affinity.rotate(sbox(-w / 2, -l / 2, w / 2, l / 2), ang, origin=(0, 0))
                r = affinity.translate(r, cx + dx, cy + dy)
                if not poly.contains(r) or r.intersects(j1):
                    continue
                m = poly.exterior.distance(r)
                if best is None or m > best[0]:
                    best = (m, r, ang)
    if best is None:
        raise SystemExit("the cell does not fit")
    m, r, ang = best
    return {"polygon": [[round(x, 4), round(y, 4)] for x, y in r.exterior.coords][:-1], "angle": ang, "margin_to_edge_mm": round(m, 3), "spec": CELL}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("-o", "--output", default="geometry.json")
    ap.add_argument("--svg")
    a = ap.parse_args()
    poly = outline()
    body = body_polygon()
    rings_ = leds([tuple(c) for c in body.exterior.coords][:-1], None)
    leds_ = [d for r in rings_ for d in r["leds"]]
    from shapely.geometry import Polygon as SPoly
    cell = cell_place(SPoly(poly))
    out = {
        "note": "Generated by scripts/geometry.py. Board coordinates, mm, y down, front view.",
        "length_mm": round(SPoly(poly).bounds[3] - SPoly(poly).bounds[1], 2),
        "symmetry_centre": list(BODY_CENTRE),
        "width_mm": round(max(p[0] for p in poly) - min(p[0] for p in poly), 2),
        "area_mm2": round(SPoly(poly).area, 1),
        "outline": [[round(x, 4), round(y, 4)] for x, y in poly],
        "usb": {"x": round(tab_centre(), 4), "y_edge": 0.0, "overhang": USB_OVERHANG, "depth": USB_DEPTH},
        "cell": cell,
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
        svg = [f'<svg xmlns="http://www.w3.org/2000/svg" width="{(x1 - x0) * s:.0f}" height="{(max(p[1] for p in poly) + 4) * s:.0f}" viewBox="{x0} -2 {x1 - x0} {max(p[1] for p in poly) + 4}">',
               '<rect x="-100" y="-100" width="300" height="300" fill="#111"/>',
               f'<polygon points="{" ".join(f"{x:.3f},{y:.3f}" for x, y in poly)}" fill="#1d3b2a" stroke="#c9a227" stroke-width="0.08"/>',
               f'<polygon points="{" ".join(f"{x},{y}" for x, y in cell["polygon"])}" fill="none" stroke="#5aa0ff" stroke-width="0.08" stroke-dasharray="0.4 0.3"/>']
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
