#!/usr/bin/env python3
"""Write a composed board (`ccomp compose -o composed.json`) as a KiCad project: schematic,
board and library tables, through easy_pcb's KiCad writer.

    compose_kicad.py COMPOSED.json OUT_DIR [--packages packages] [--easy-pcb ~/Software/easy_pcb]

The schematic is drawn by rule, not by hand: one dashed box per instance and block (the
board's own parts in a box of their own), each pin a short wire and a net label, unused pins
marked no-connect, and a PWR_FLAG on every net that power inputs share with no power output
(a supply that arrives on a connector). Vendor symbols and footprints a package carries in
its `kicad/` folder are copied into OUT_DIR/lib and named in the project's library tables.
The board is the schematic pushed with easy_pcb's update_pcb: every footprint with its nets,
stacked at the origin for the floorplanner. KiCad's ERC runs on the result; its report is
written beside it. Prints one JSON result.
"""
import argparse
import json
import math
import os
import re
import shutil
import subprocess
import sys
from pathlib import Path

GRID = 1.27


def snap(v):
    return round(round(v / GRID) * GRID, 4)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("composed")
    ap.add_argument("out")
    ap.add_argument("--packages", default="packages")
    ap.add_argument("--easy-pcb", default=os.path.expanduser("~/Software/easy_pcb"))
    ap.add_argument("--lib", action="append", default=[], help="a folder of the board's own .kicad_sym files and .pretty libraries")
    a = ap.parse_args()
    sys.path.insert(0, str(Path(a.easy_pcb) / ".claude/skills/pcb-design/scripts"))
    import kicad_write as kw  # noqa: E402

    comp = json.load(open(a.composed))
    if not comp.get("ok"):
        sys.exit("the composition has errors; fix them first")
    out = Path(a.out)
    (out / "lib").mkdir(parents=True, exist_ok=True)
    board = comp["board"]

    # Vendor libraries from each package used.
    sym_files, fp_libs = [], {}
    dirs = [Path(a.packages) / b["package"].split("@")[0] / "kicad" for b in comp["bindings"].values()] + [Path(d) for d in a.lib]
    for kdir in dirs:
        if kdir.is_dir():
            for s in kdir.glob("*.kicad_sym"):
                shutil.copy(s, out / "lib" / s.name)
                sym_files.append(out / "lib" / s.name)
            for p in kdir.glob("*.pretty"):
                shutil.copytree(p, out / "lib" / p.name, dirs_exist_ok=True)
                fp_libs[p.stem] = p.name
    (out / "sym-lib-table").write_text("(sym_lib_table\n  (version 7)\n" + "".join(f'  (lib (name "{s.stem}")(type "KiCad")(uri "${{KIPRJMOD}}/lib/{s.name}")(options "")(descr ""))\n' for s in sym_files) + ")\n")
    (out / "fp-lib-table").write_text("(fp_lib_table\n  (version 7)\n" + "".join(f'  (lib (name "{n}")(type "KiCad")(uri "${{KIPRJMOD}}/lib/{d}")(options "")(descr ""))\n' for n, d in fp_libs.items()) + ")\n")
    pro = out / f"{board}.kicad_pro"
    if not pro.exists():
        pro.write_text(json.dumps({"meta": {"filename": pro.name, "version": 1}}, indent=2) + "\n")

    libs = kw.SymbolLib(sym_files)
    # A net with one pin is an open pin: drawn as a no-connect, not a label.
    net_of = {p: n for n, ps in comp["nets"].items() if len(ps) > 1 for p in ps}

    # Every pin a net names must be a pin of the symbol it will be drawn with (KiCad versions
    # rename pins: the USB-C shield is S1 in 9 and SH in 10).
    bad = []
    for n, ps in comp["nets"].items():
        for pin in ps:
            ref, num = pin.split(".", 1)
            if num not in kw.symbol_pins(libs.get(comp["parts"][ref]["symbol"])):
                bad.append(f"{n}: {pin}: {comp['parts'][ref]['symbol']} has no pin {num} in the libraries used here")
    if bad:
        print(json.dumps({"ok": False, "errors": bad}, indent=1))
        sys.exit(1)

    # Groups: each instance's blocks, then the board's own parts.
    groups = {}
    for ref, p in comp["parts"].items():
        key = (p["instance"], p.get("block") or "parts") if p.get("instance") else ("board", "parts")
        groups.setdefault(key, []).append(ref)
    natural = lambda r: (re.match(r"[A-Z]*", r).group(0), int(re.sub(r"\D", "", r) or 0))

    # Each part's pins (library coordinates) and the room its labels need.
    info = {}
    for ref, p in comp["parts"].items():
        sym = libs.get(p["symbol"])
        pins = kw.symbol_pins(sym)
        xs = [q["x"] for q in pins.values()] or [0]
        ys = [-q["y"] for q in pins.values()] or [0]
        longest = max((len(net_of.get(f"{ref}.{n}", "")) for n in pins), default=0)
        pad = 2.54 + longest * 0.8 + 2.54
        info[ref] = {"sym": sym, "pins": pins, "box": (min(xs) - pad, min(ys) - pad - 2.54, max(xs) + pad, max(ys) + pad + 2.54)}

    sch = kw.Schematic(board, title=board, libs=libs, paper="A0")
    placed_boxes = []
    # Groups flow left to right and wrap at `width`; the sheet is chosen to fit afterwards.
    x0, y0, row_h, width, max_x = 20.0, 20.0, 0.0, 780.0, 0.0
    gx, gy = x0, y0
    for (inst, block), refs in sorted(groups.items(), key=lambda kv: (kv[0][0] == "board", kv[0])):
        refs.sort(key=natural)
        # Lay the group's parts out in a row that wraps at about 300 mm.
        cx, cy, line_h, gw, gh = 0.0, 0.0, 0.0, 0.0, 0.0
        spots = []
        for r in refs:
            bx0, by0, bx1, by1 = info[r]["box"]
            w, h = bx1 - bx0, by1 - by0
            if cx and cx + w > 300:
                cx, cy, line_h = 0.0, cy + line_h + 5, 0.0
            spots.append((r, cx - bx0, cy - by0))
            cx += w + 5
            line_h = max(line_h, h)
            gw, gh = max(gw, cx), max(gh, cy + line_h)
        if gx > x0 and gx + gw > width:
            gx, gy, row_h = x0, gy + row_h + 25, 0.0
        for r, dx, dy in spots:
            p = comp["parts"][r]
            at = (snap(gx + 5 + dx), snap(gy + 10 + dy))
            fields = {k.upper() if k == "lcsc" else k.capitalize(): str(v) for k, v in p.items() if k in ("lcsc", "mpn")}
            part = sch.symbol(p["symbol"], r, p["value"], at, footprint=p["footprint"], fields=fields)
            for num, q in info[r]["pins"].items():
                pt = part.pin(num)
                net = net_of.get(f"{r}.{num}")
                if net is None:
                    if q["type"] != "no_connect":
                        sch.no_connect(pt)
                    continue
                # Outward from the body: the opposite of the pin's own direction.
                ang = (q["angle"] + 180) % 360
                dx_, dy_ = round(math.cos(math.radians(ang))), -round(math.sin(math.radians(ang)))
                end = (snap(pt[0] + dx_ * 2.54), snap(pt[1] + dy_ * 2.54))
                sch.wire(pt, end)
                rot = {(1, 0): 0, (-1, 0): 180, (0, -1): 90, (0, 1): 270}[(dx_, dy_)]
                sch.label(net.lstrip("/").replace("/", "_") if net.startswith("/") else net, end, rot=rot)
        title = "Board parts" if inst == "board" else f"{inst}: {block}"
        sch.box((snap(gx), snap(gy)), (snap(gx + gw + 10), snap(gy + gh + 15)), label=title)
        placed_boxes.append(title)
        max_x = max(max_x, gx + gw + 10)
        gx += gw + 25
        row_h = max(row_h, gh + 15)

    # A supply that only power inputs see needs a flag saying something drives it.
    types = {}
    for ref, p in comp["parts"].items():
        for num, q in info[ref]["pins"].items():
            n = net_of.get(f"{ref}.{num}")
            if n:
                types.setdefault(n, set()).add(q["type"])
    flagged = sorted(n for n, t in types.items() if "power_in" in t and "power_out" not in t)
    fx, fy = x0, gy + row_h + 30
    for i, n in enumerate(flagged):
        at = (snap(fx + (i % 12) * 30), snap(fy + (i // 12) * 15))
        flag = sch.power("power:PWR_FLAG", at)
        pt = flag.pin("1")
        end = (pt[0], snap(pt[1] + 2.54))
        sch.wire(pt, end)
        sch.label(n.lstrip("/").replace("/", "_") if n.startswith("/") else n, end, rot=270)

    # The smallest ISO sheet the drawing fits on, landscape.
    extent = (max(width, gx), fy + (len(flagged) // 12 + 1) * 15 + 20)
    sizes = [("A4", 297, 210), ("A3", 420, 297), ("A2", 594, 420), ("A1", 841, 594), ("A0", 1189, 841)]
    print(f"extent {max_x:.0f} x {extent[1]:.0f} mm", file=sys.stderr)
    sch.paper = next((n for n, w, h in sizes if max_x + 20 <= w and extent[1] + 20 <= h), "A0")
    sch_path = out / f"{board}.kicad_sch"
    sch.save(sch_path)
    erc_path = out / "erc.json"
    subprocess.run(["kicad-cli", "sch", "erc", "--format", "json", "--severity-all", "-o", str(erc_path), str(sch_path)], capture_output=True, text=True)
    erc = json.load(open(erc_path)) if erc_path.exists() else {}
    viol = [v for s in erc.get("sheets", []) for v in s.get("violations", [])]
    counts = {}
    for v in viol:
        k = f"{v.get('severity')}:{v.get('type')}"
        counts[k] = counts.get(k, 0) + 1

    pcb_path = out / f"{board}.kicad_pcb"
    log = []
    kw.update_pcb(sch_path, pcb_path, log=log.append)
    import pcbnew
    b = pcbnew.LoadBoard(str(pcb_path))
    pcb_nets = {}
    for fp in b.GetFootprints():
        for pad in fp.Pads():
            if pad.GetNetname():
                pcb_nets.setdefault(pad.GetNetname(), set()).add(f"{fp.GetReference()}.{pad.GetNumber()}")
    # The board's pads against the composed nets (labels rename /inst/net to inst_net).
    want = {frozenset(ps) for ps in comp["nets"].values() if len(ps) > 1}  # one-pin nets are open pins
    have = {frozenset(ps) for ps in pcb_nets.values() if len(ps) > 1}
    print(json.dumps({
        "schematic": str(sch_path), "board": str(pcb_path), "groups": placed_boxes, "power_flags": flagged,
        "erc": counts, "footprints": len(b.GetFootprints()),
        "nets_match": want == have, "nets_missing_on_board": len(want - have), "nets_extra_on_board": len(have - want),
    }, indent=1))


if __name__ == "__main__":
    main()
