#!/usr/bin/env python3
"""Make a placed, composed board ready for the router: stack-up, design rules, net classes from
the intent, planes, and each rigid block's generated copper, locked.

    prepare_route.py PLACED.kicad_pcb --intent INTENT.json --spec floorplan.json -o OUT_DIR
        [--root DIR] [--pro BASE.kicad_pro] [--layers 4] [--plane In1.Cu=/GND ...]

PLACED is `apply_placement.py`'s board. In OUT_DIR it writes BOARD.kicad_pcb, BOARD.kicad_pro and
BOARD.kicad_dru, the router's input (`ccomp compile --dru-base` then adds the length limits to
that .kicad_dru).

- **Stack-up:** the copper layer count (`--layers`, else the intent's `board.layers`, else 4).
- **Rules:** a fab preset, the one easy2350 was routed and checked with (JLCPCB, 4 layers).
- **Net classes:** each net in the class its intent names, the class's track and clearance from
  `CLASSES`. A net whose intent carries more current than its class's track can take
  (IPC-2221, outer layer, 1 oz, 10 C rise) gets a `.kicad_dru` rule for the wider track.
- **Rigid blocks:** every `rigid` entry of the floorplan spec has its generated layout's
  tracks, vias and keep-outs applied at its anchor by `apply_layout.py`, all locked. The
  layout's chip pins (`U1.49`) are the anchor's.
- **Decoupling stubs:** with `--composed`, each capacitor a `decoupling_path_mm` constraint names
  gets a locked track to the pin it decouples (its `near`), when one fits within the limit:
  placement puts it close, but a router wires it into the net anywhere.
- **Planes:** a zone over the whole board per `--plane LAYER=NET`, by default the ground net on
  In1.Cu and the POWER net with the most pads on In2.Cu.

Prints one JSON result.
"""
import argparse
import json
import math
import shutil
import subprocess
import sys
import tempfile
from pathlib import Path

import pcbnew

NM = 1e6
HERE = Path(__file__).resolve().parent

# Fab rules (mm): easy2350's .kicad_pro, from easy_pcb's JLCPCB 4-layer preset.
RULES = {
    "min_clearance": 0.1, "min_copper_edge_clearance": 0.3, "min_hole_clearance": 0.2, "min_hole_to_hole": 0.25,
    "min_through_hole_diameter": 0.2, "min_track_width": 0.09, "min_via_annular_width": 0.1, "min_via_diameter": 0.45,
    "min_microvia_diameter": 0.2, "min_microvia_drill": 0.1, "min_silk_clearance": 0.15, "solder_mask_to_copper_clearance": 0.0,
}

# Intent class → (track, clearance, via diameter, via drill), mm. easy2350's classes; GROUND as
# POWER, ANALOG between SIGNAL and CLOCK.
CLASSES = {
    "Default": (0.1, 0.1, 0.5, 0.2),
    "SIGNAL": (0.1, 0.1, 0.5, 0.2),
    "SE_HS": (0.1, 0.12, 0.5, 0.2),
    "CLOCK": (0.15, 0.15, 0.5, 0.2),
    "ANALOG": (0.15, 0.12, 0.5, 0.2),
    "DIFF_HS": (0.2, 0.15, 0.5, 0.2),
    "POWER": (0.25, 0.12, 0.6, 0.3),
    "GROUND": (0.25, 0.12, 0.6, 0.3),
    "SWITCH": (0.4, 0.12, 0.6, 0.3),
}


def ipc2221_width_mm(amps, rise_c=10.0, oz=1.0, k=0.048):
    """Track width for a current on an outer layer: A = (I / (k dT^0.44))^(1/0.725) mil^2."""
    area_mil2 = (amps / (k * rise_c ** 0.44)) ** (1 / 0.725)
    return area_mil2 / (1.378 * oz) * 0.0254


def run(cmd):
    r = subprocess.run(cmd, capture_output=True, text=True)
    if r.returncode != 0:
        sys.exit(json.dumps({"ok": False, "error": f"{' '.join(map(str, cmd))}: {r.stderr.strip() or r.stdout.strip()}"}))
    return json.loads(r.stdout.strip().splitlines()[-1]) if r.stdout.strip().startswith("{") else r.stdout


def netclass(name, track, clearance, via, drill, priority):
    return {
        "name": name, "priority": priority, "track_width": track, "clearance": clearance, "via_diameter": via, "via_drill": drill,
        "microvia_diameter": 0.3, "microvia_drill": 0.1, "diff_pair_width": track, "diff_pair_gap": max(clearance, 0.15),
        "diff_pair_via_gap": 0.25, "line_style": 0, "pcb_color": "rgba(0, 0, 0, 0.000)", "schematic_color": "rgba(0, 0, 0, 0.000)",
        "wire_width": 6, "bus_width": 12,
    }


# A stub keeps this much from other nets' copper, mm: the widest class clearance here.
STUB_CLEARANCE = 0.15


def stub_paths(p, q):
    """Paths from p to q, shortest first: straight; the two right-angle bends; the two
    45-degree-then-straight bends."""
    (px, py), (qx, qy) = p, q
    dx, dy = qx - px, qy - py
    d = min(abs(dx), abs(dy))
    sx, sy = math.copysign(d, dx), math.copysign(d, dy)
    out = [[p, q], [p, (qx, py), q], [p, (px, qy), q], [p, (px + sx, py + sy), q], [p, (qx - sx, qy - sy), q]]
    length = lambda path: sum(math.dist(a, b) for a, b in zip(path, path[1:]))
    return sorted(([pt for i, pt in enumerate(path) if i == 0 or math.dist(pt, path[i - 1]) > 1e-6] for path in out), key=length)


def decoupling_stubs(b, intent, composed):
    """For each power pin a `decoupling_path_mm` constraint covers: a track on the pin's layer
    from the pin to its capacitor's pad on the same net (the capacitor whose `near` is that pin),
    the first shape of `stub_paths` that clears every other net's pads, tracks and vias and every
    rule area, and is within the limit. Locked, so the router builds the net round it."""
    if composed is None:
        return {"skipped": "no --composed"}
    near = {ref: p["near"] for ref, p in composed["parts"].items() if p.get("near")}
    wanted = {}
    for bid, blk in intent.get("blocks", {}).items():
        for c in (blk.get("placement_goal") or {}).get("constraints", []):
            v = c["value"]
            if v["metric"] == "decoupling_path_mm" and v["op"] == "<=":
                for ref in v.get("parts", []):
                    wanted[ref] = v["limit"]
    obstacles = []  # (net, shape on F.Cu or None for any layer, layers)
    for fp in b.GetFootprints():
        for pad in fp.Pads():
            obstacles.append((pad.GetNetCode(), pad))
    for t in b.GetTracks():
        obstacles.append((t.GetNetCode(), t))
    areas = [z for z in b.Zones() if z.GetIsRuleArea() and z.GetDoNotAllowTracks()]
    made, missed = {}, {}
    for ref, limit in sorted(wanted.items()):
        if ref not in near:
            missed[ref] = "no near pin"
            continue
        chip, pin = near[ref].split(".")
        pp = b.FindFootprintByReference(chip).FindPadByNumber(pin)
        cap = b.FindFootprintByReference(ref)
        cp = next((q for q in cap.Pads() if q.GetNetCode() == pp.GetNetCode()), None)
        layer = pcbnew.B_Cu if cap.IsFlipped() else pcbnew.F_Cu
        if cp is None or not pp.IsOnLayer(layer):
            missed[ref] = "not on one layer with its pin"
            continue
        width = min(pp.GetSize().x, pp.GetSize().y, int(CLASSES["POWER"][0] * NM))
        P = (pp.GetPosition().x / NM, pp.GetPosition().y / NM)
        Q = (cp.GetPosition().x / NM, cp.GetPosition().y / NM)
        # Or first out of the pin along its long axis, away from the chip, clear of its
        # neighbours' ends, then on to the capacitor.
        long_x = pp.GetSize().x > pp.GetSize().y
        a_rad = math.radians(pp.GetOrientationDegrees())
        axis = (math.cos(a_rad), -math.sin(a_rad)) if long_x else (math.sin(a_rad), math.cos(a_rad))
        centre = (pp.GetParentFootprint().GetPosition().x / NM, pp.GetParentFootprint().GetPosition().y / NM)
        if (P[0] - centre[0]) * axis[0] + (P[1] - centre[1]) * axis[1] < 0:
            axis = (-axis[0], -axis[1])
        half = max(pp.GetSize().x, pp.GetSize().y) / NM / 2
        paths = stub_paths(P, Q)
        for extra in (0.05, 0.3, 0.6):
            e = half + STUB_CLEARANCE + width / NM / 2 + extra
            E = (P[0] + axis[0] * e, P[1] + axis[1] * e)
            paths += [[P] + rest for rest in stub_paths(E, Q)]
        paths.sort(key=lambda path: sum(math.dist(u, v) for u, v in zip(path, path[1:])))
        for path in paths:
            if sum(math.dist(u, v) for u, v in zip(path, path[1:])) > limit:
                missed[ref] = f"no stub within {limit} mm"
                break
            segs = []
            for u, v in zip(path, path[1:]):
                t = pcbnew.PCB_TRACK(b)
                t.SetStart(pcbnew.VECTOR2I(int(round(u[0] * NM)), int(round(u[1] * NM))))
                t.SetEnd(pcbnew.VECTOR2I(int(round(v[0] * NM)), int(round(v[1] * NM))))
                t.SetWidth(int(width))
                t.SetLayer(layer)
                t.SetNetCode(pp.GetNetCode())
                segs.append(t)
            clear = int(STUB_CLEARANCE * NM)
            def hits(t):
                shape = t.GetEffectiveShape()
                for net, item in obstacles:
                    if net == pp.GetNetCode() or not item.IsOnLayer(layer):
                        continue
                    other = item.GetEffectiveShape(layer) if isinstance(item, pcbnew.PAD) else item.GetEffectiveShape()
                    if shape.Collide(other, clear):
                        return True
                return any(z.IsOnLayer(layer) and z.Outline().Collide(t.GetStart()) or z.IsOnLayer(layer) and z.Outline().Collide(t.GetEnd()) for z in areas)
            if any(hits(t) for t in segs):
                continue
            for t in segs:
                t.SetLocked(True)
                b.Add(t)
                obstacles.append((t.GetNetCode(), t))
            made[ref] = {"pin": near[ref], "mm": round(sum(math.dist(u, v) for u, v in zip(path, path[1:])), 3), "bends": len(path) - 2}
            break
        else:
            missed[ref] = "every shape blocked"
    return {"made": made, "missed": missed}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("--intent", required=True)
    ap.add_argument("--spec", required=True, help="the floorplan spec: its rigid blocks")
    ap.add_argument("--root", default=".", help="where the spec's layout paths start")
    ap.add_argument("--pro", help="a .kicad_pro to start from (default: the one beside BOARD)")
    ap.add_argument("--layers", type=int)
    ap.add_argument("--plane", action="append", default=[], help="LAYER=NET, a zone over the whole board")
    ap.add_argument("--composed", help="the composed board (`ccomp compose -o`): its parts' `near` pins, for the decoupling stubs")
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()

    intent = json.load(open(a.intent))
    spec = json.load(open(a.spec))
    out = Path(a.output)
    out.mkdir(parents=True, exist_ok=True)
    name = intent["board"]["name"]
    pcb, pro, dru = out / f"{name}.kicad_pcb", out / f"{name}.kicad_pro", out / f"{name}.kicad_dru"
    layers = a.layers or intent["board"].get("layers") or 4
    nets = intent["nets"]

    # Project: rules and net classes, written before the board is loaded so pcbnew uses them.
    base = Path(a.pro) if a.pro else Path(a.board).with_suffix(".kicad_pro")
    project = json.load(open(base)) if base.exists() else {"board": {"design_settings": {"rules": {}}}, "net_settings": {}, "meta": {"filename": pro.name, "version": 1}}
    project.setdefault("meta", {})["filename"] = pro.name
    project["board"]["design_settings"]["rules"].update(RULES)
    used = sorted({n.get("class") for n in nets.values() if n.get("class")} - {None})
    unknown = [c for c in used if c not in CLASSES]
    if unknown:
        sys.exit(json.dumps({"ok": False, "error": f"no track sizes for classes {unknown}; add them to CLASSES"}))
    ns = project.setdefault("net_settings", {})
    ns["classes"] = [netclass("Default", *CLASSES["Default"], 2147483647)] + [netclass(c, *CLASSES[c], i) for i, c in enumerate(used)]
    ns["netclass_patterns"] = [{"netclass": n["class"], "pattern": net} for net, n in sorted(nets.items()) if n.get("class")]
    ns["netclass_assignments"] = None
    pro.write_text(json.dumps(project, indent=2) + "\n")

    # Current: a wider track than the class gives, as a custom rule.
    widened, lines = {}, ["(version 1)", ""]
    for net, n in sorted(nets.items()):
        amps = (n.get("current") or {}).get("value", {}).get("continuous_a")
        if not amps or n.get("class") in ("GROUND",):
            continue
        need = ipc2221_width_mm(amps)
        have = CLASSES[n.get("class", "Default")][0]
        if need > have:
            w = math.ceil(need * 100) / 100
            widened[net] = w
            lines += [f"# {net} carries {amps} A (intent). IPC-2221, outer 1 oz, 10 C rise: {need:.3f} mm.",
                      f'(rule "current width {net}"', f"  (condition \"A.NetName == '{net}'\")", f"  (constraint track_width (min {w}mm)))", ""]
    dru.write_text("\n".join(lines))

    # Stack-up, then each rigid block's copper.
    b = pcbnew.LoadBoard(a.board)
    b.SetCopperLayerCount(layers)
    b.Save(str(pcb))
    rigid = []
    with tempfile.TemporaryDirectory() as t:
        for r in spec.get("rigid", []):
            layout = Path(a.root) / r["layout"]
            pkg_roles = json.load(open(layout.parent / "roles.json"))
            roles = {k: f"{r['anchor']}.{v.split('.', 1)[1]}" if "." in v else v for k, v in pkg_roles.items()}
            roles.update(r.get("roles", {}))
            rf = Path(t) / f"{r['name']}.roles.json"
            rf.write_text(json.dumps(roles))
            res = run([sys.executable, str(HERE / "apply_layout.py"), str(pcb), str(layout), str(rf), "-o", str(pcb)])
            rigid.append({"name": r["name"], "anchor": r["anchor"], "tracks": res["tracks"], "vias": res["vias"], "keepouts": res["keepouts"]})

    # Each decoupling capacitor's stub to its pin, locked.
    b = pcbnew.LoadBoard(str(pcb))
    stubs = decoupling_stubs(b, intent, json.load(open(a.composed)) if a.composed else None)
    b.Save(str(pcb))

    # Planes over the board.
    b = pcbnew.LoadBoard(str(pcb))
    planes = dict(p.split("=", 1) for p in a.plane)
    if not planes:
        ground = [n for n, v in nets.items() if v.get("class") == "GROUND"]
        pads = {}
        for fp in b.GetFootprints():
            for pad in fp.Pads():
                pads[pad.GetNetname()] = pads.get(pad.GetNetname(), 0) + 1
        power = sorted((n for n, v in nets.items() if v.get("class") == "POWER"), key=lambda n: -pads.get(n, 0))
        planes = {"In1.Cu": ground[0]} if ground else {}
        if layers >= 4 and power:
            planes["In2.Cu"] = power[0]
    layer_id = {b.GetLayerName(i): i for i in range(pcbnew.PCB_LAYER_ID_COUNT)}
    box = b.GetBoardEdgesBoundingBox()
    for layer, net in planes.items():
        ni = b.FindNet(net)
        if ni is None:
            sys.exit(json.dumps({"ok": False, "error": f"plane {layer}: no net {net}"}))
        z = pcbnew.ZONE(b)
        ls = pcbnew.LSET()
        ls.AddLayer(layer_id[layer])
        z.SetLayerSet(ls)
        z.SetNet(ni)
        z.SetPadConnection(pcbnew.ZONE_CONNECTION_THERMAL)
        z.SetMinThickness(int(0.2 * NM))
        z.SetLocalClearance(int(0.2 * NM))
        z.SetZoneName(f"plane {net}")
        o = z.Outline()
        o.NewOutline()
        for x, y in [(box.GetLeft(), box.GetTop()), (box.GetRight(), box.GetTop()), (box.GetRight(), box.GetBottom()), (box.GetLeft(), box.GetBottom())]:
            o.Append(x, y)
        b.Add(z)
    pcbnew.ZONE_FILLER(b).Fill(b.Zones())
    b.Save(str(pcb))
    pro.write_text(json.dumps(project, indent=2) + "\n")  # pcbnew's save rewrites it without the classes

    print(json.dumps({"ok": True, "board": str(pcb), "layers": layers, "classes": used, "nets_classed": len(ns["netclass_patterns"]),
                      "widened": widened, "planes": planes, "rigid": rigid, "stubs": stubs}, indent=1))


if __name__ == "__main__":
    main()
