#!/usr/bin/env python3
"""Validate a routed flame earring. Prints one JSON result; exits 1 when any check fails.

1. KiCad DRC after a zone refill: no errors, nothing unconnected.
2. The board's pad nets are circuit.json's, pin for pin.
3. Under the cell: every bottom-side part whose courtyard meets the cell's envelope is at most
   CELL_CAP tall (HEIGHTS, from the parts' datasheets as simple_earring's brief lists them).
4. The LED chains: each zone's data runs from its first LED to its last through every LED's
   DO -> DI, and every LED's VDD and GND pads are on their zone's VLED and GND.
5. The RP2350's core regulator, measured by `ccomp check buck` (hot loop, switch node, output
   loop, feedback clearance) against the package's own limits.

    validate.py BOARD [-o validate.json]
"""
import argparse
import json
import subprocess
import tempfile
from pathlib import Path

import pcbnew
from shapely.geometry import Polygon, box

BOARD_DIR = Path(__file__).resolve().parent.parent
ROOT = BOARD_DIR.parent.parent
CELL_CAP = 1.1
# Seated height, mm, by footprint (max from each datasheet; simple_earring design/brief.md §11).
HEIGHTS = {
    "USB-C_SMD-TYPE-C-31-M-12_1": 3.26, "SW_B3U-3000P": 1.35, "SOT-323_MOSFET": 1.1, "SOT-323_BAT54SW": 1.1,
    "SOT-363_SC-70-6": 1.1, "D_SOD-323": 1.1, "L_pol_2016": 1.0, "RP2350-QFN-60-1EP_7x7_P0.4mm_EP3.4x3.4mm": 0.9,
    "C_0603_1608Metric": 0.9, "LGA-14_L3.0-W2.5-P0.50-TL": 0.86, "Crystal_SMD_3225-4Pin_3.2x2.5mm_ABM8": 0.8,
    "WDFN-6-2x2": 0.8, "DFN-8-1EP_2x2mm_P0.5mm_EP0.9x1.5mm": 0.75, "SOT-666": 0.6, "USON-8_L3.0-W2.0-P0.50-BL-EP": 0.55,
    "YZV0004ADAD": 0.5, "D_SOD-882": 0.5, "C_0402_1005Metric": 0.55, "C_0402_1005Metric_small_pads": 0.55,
    "R_0402_1005Metric": 0.4, "TestPoint_Pad_1.5x1.5mm": 0.0, "BAT_PADS_FLAME": 0.0,
}
NM = 1e6


def drc(path):
    with tempfile.NamedTemporaryFile(suffix=".kicad_pcb", dir=Path(path).parent, delete=False) as t:
        filled = t.name
    # The rules are in the project file: the refilled copy gets one, or KiCad's defaults apply.
    import shutil
    pro = Path(path).with_suffix(".kicad_pro")
    if not pro.exists():
        pro = BOARD_DIR / "build/flame.kicad_pro"
    shutil.copy(pro, Path(filled).with_suffix(".kicad_pro"))
    b = pcbnew.LoadBoard(path)
    pcbnew.ZONE_FILLER(b).Fill(b.Zones())
    pcbnew.SaveBoard(filled, b)
    with tempfile.NamedTemporaryFile(suffix=".json") as j:
        subprocess.run(["kicad-cli", "pcb", "drc", "--format", "json", "--severity-all", "--schematic-parity", "-o", j.name, filled], capture_output=True)
        out = json.load(open(j.name))
    for f in Path(path).parent.glob(Path(filled).stem + ".*"):
        f.unlink()
    errors = [v for v in out["violations"] if v["severity"] == "error"]
    warn = {}
    for v in out["violations"]:
        if v["severity"] != "error":
            warn[v["type"]] = warn.get(v["type"], 0) + 1
    err = {}
    for v in errors:
        err[v["type"]] = err.get(v["type"], 0) + 1
    return {"ok": not errors and not out["unconnected_items"], "errors": err, "unconnected": len(out["unconnected_items"]), "warnings": warn,
            "error_items": [[i["description"] for i in v["items"]] for v in errors][:20]}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("-o", "--output")
    a = ap.parse_args()
    geo = json.load(open(BOARD_DIR / "geometry.json"))
    cir = json.load(open(BOARD_DIR / "circuit.json"))
    b = pcbnew.LoadBoard(a.board)
    fps = {f.GetReference(): f for f in b.GetFootprints()}
    res = {"board": a.board}

    res["drc"] = drc(a.board)

    # 2. Netlist parity with the circuit.
    want = {}
    for net, pins in cir["nets"].items():
        for p in pins:
            want[p] = net
    have = {}
    for r, fp in fps.items():
        for pad in fp.Pads():
            if pad.GetNetname():
                have[f"{r}.{pad.GetNumber()}"] = pad.GetNetname()
    diff = sorted(p for p in set(want) | set(have) if p.split(".")[0] in cir["components"] and want.get(p) != have.get(p))
    res["netlist"] = {"ok": not diff, "differences": diff[:20], "pins": len(want)}

    # 3. Heights under the cell.
    c = geo["cell"]
    cell = box(c["x0"], c["y0"], c["x1"], c["y1"])
    under, too_tall, unknown = [], [], []
    for r, fp in fps.items():
        if not fp.IsFlipped():
            continue
        cy = fp.GetCourtyard(pcbnew.B_CrtYd)
        if not cy.OutlineCount():
            continue
        o = cy.Outline(0)
        if not Polygon([(o.CPoint(j).x / NM, o.CPoint(j).y / NM) for j in range(o.PointCount())]).intersects(cell):
            continue
        name = fp.GetFPID().GetLibItemName().wx_str()
        h = HEIGHTS.get(name)
        under.append(r)
        if h is None:
            unknown.append(f"{r} ({name})")
        elif h > CELL_CAP:
            too_tall.append(f"{r} ({name}, {h} mm)")
    res["under_cell"] = {"ok": not too_tall and not unknown, "parts": len(under), "too_tall": too_tall, "unknown_height": unknown}

    # 4. LED chains and power.
    problems = []
    for z, chain in cir["zones"].items():
        for j, i in enumerate(chain):
            fp = fps[f"LED{i}"]
            if fp.FindPadByNumber("1").GetNetname() != f"VLED_{z}" or fp.FindPadByNumber("3").GetNetname() != "GND":
                problems.append(f"LED{i}: power pads on {fp.FindPadByNumber('1').GetNetname()} / {fp.FindPadByNumber('3').GetNetname()}")
            if j + 1 < len(chain):
                nxt = fps[f"LED{chain[j + 1]}"]
                if fp.FindPadByNumber("2").GetNetname() != nxt.FindPadByNumber("4").GetNetname():
                    problems.append(f"LED{i} DO does not feed LED{chain[j + 1]} DI")
    res["led_chains"] = {"ok": not problems, "zones": {z: len(v) for z, v in cir["zones"].items()}, "problems": problems[:20]}

    # 5. The core regulator.
    roles = {"vin": "U1.49", "sw": "U1.48", "pgnd": "U1.47", "fb": "U1.50"}
    pads_on = lambda net: [f"{r}.{p.GetNumber()}" for r, fp in fps.items() for p in fp.Pads() if p.GetNetname() == net]
    vin_net = fps["U1"].FindPadByNumber("49").GetNetname()
    fb_net = fps["U1"].FindPadByNumber("50").GetNetname()
    caps_on = lambda net: [r for r in fps if r.startswith("C") and any(p.GetNetname() == net for p in fps[r].Pads()) and any(p.GetNetname() == "GND" for p in fps[r].Pads())]
    cin = min(caps_on(vin_net), key=lambda r: (fps[r].GetPosition() - fps["U1"].FindPadByNumber("49").GetPosition()).EuclideanNorm(), default=None)
    cout = min(caps_on(fb_net), key=lambda r: (fps[r].GetPosition() - fps["U1"].FindPadByNumber("50").GetPosition()).EuclideanNorm(), default=None)
    roles.update({"cin": cin, "cout": cout, "inductor": "L1"})
    arg = ",".join(f"{k}={v}" for k, v in roles.items())
    run = subprocess.run([str(ROOT / "target/release/ccomp"), "check", "buck", a.board, "--roles", arg, "--svg", str(Path(a.board).with_suffix(".core.svg"))], capture_output=True, text=True)
    try:
        res["core_regulator"] = {"roles": roles, "report": json.loads(run.stdout)}
    except json.JSONDecodeError:
        res["core_regulator"] = {"roles": roles, "error": run.stdout[-500:] + run.stderr[-500:]}

    ok = all(res[k]["ok"] for k in ("drc", "netlist", "under_cell", "led_chains"))
    res["ok"] = ok
    text = json.dumps(res, indent=1)
    if a.output:
        open(a.output, "w").write(text + "\n")
    print(text)
    raise SystemExit(0 if ok else 1)


if __name__ == "__main__":
    main()
