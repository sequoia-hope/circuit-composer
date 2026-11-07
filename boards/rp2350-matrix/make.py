#!/usr/bin/env python3
"""Writes the matrix demo board's composition and floorplan spec.

    make.py [--pitch MM] [--dir DIR]

ultraroute's quick demo: rp2350-battery's power path and RP2350 (USB-C sink, MCP73831 charger,
load sharing, TLV62569 buck) driving an 8 x 8 serpentine chain of WS2816C-1313 LEDs through a
level shifter, every part on the top side. The LEDs sit on a fixed grid at --pitch; the board
grows with it, the electronics in a strip below the grid, so routing time can be measured
against board size.
"""
import argparse
import json
import subprocess
import tempfile
from pathlib import Path

N = 8
STRIP = 30.0  # mm below the grid for the electronics
MARGIN = 3.0  # mm from the outline to the outer LEDs' centres


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--pitch", type=float, default=6.0)
    ap.add_argument("--dir", default=str(Path(__file__).parent))
    a = ap.parse_args()
    out = Path(a.dir)
    batt = json.loads((Path(__file__).parent.parent / "rp2350-battery" / "compose.json").read_text())

    parts = {k: v for k, v in batt["parts"].items() if k not in ("R4", "D1")}
    leds = {f"D{i}": f"LED{i}" for i in range(1, N * N + 1)}
    compose = {
        "board": "rp2350-matrix",
        "note": "ultraroute's quick demo board: rp2350-battery's USB-C sink, MCP73831 charger, load sharing (Q1, D3) and TLV62569 buck feeding the RP2350A, which drives an 8 x 8 serpentine chain of WS2816C-1313 LEDs on GPIO16 through an inverting BSS138 level shifter to VSYS (the chain's DI wants 0.7 VDD, above 3.3 V at a LiPo's 4.2 V). Every part is on the top side. SWD on J3. Written by make.py.",
        "instances": {
            **{k: v for k, v in batt["instances"].items() if k != "imu"},
            "shift": {"recipe": "nfet_level_shift", "reversed": ["R_PU"], "designators": {"Q1": "Q2", "R_PU": "R5"}},
            "leds": {"recipe": "led_chain", "params": {"n": N * N, "cap_every": 4, "cap_first": 2},
                     "designators": {**leds, **{f"C{i}": f"C{100 + i}" for i in range(1, 17)}}},
        },
        "parts": parts,
        "nets": {
            "GND": ["mcu.GND", "chg.GND", "reg.GND", "shift.GND", "leds.GND", "J1.A1", "J1.A12", "J1.B1", "J1.B12", "J1.S1", "R1.2", "R2.2", "J2.2", "R3.2", "J3.2"],
            "VBUS": batt["nets"]["VBUS"],
            "VBAT": batt["nets"]["VBAT"],
            "VSYS": batt["nets"]["VSYS"] + ["shift.VOUT", "leds.VDD"],
            "+3V3": ["reg.VOUT", "mcu.+3V3"],
            "CC1": batt["nets"]["CC1"],
            "CC2": batt["nets"]["CC2"],
            "USB_D+": batt["nets"]["USB_D+"],
            "USB_D-": batt["nets"]["USB_D-"],
            "SWCLK": batt["nets"]["SWCLK"],
            "SWDIO": batt["nets"]["SWDIO"],
            "LED_DATA": ["mcu.GPIO16", "shift.IN"],
            "LED_DIN": ["shift.OUT", "leds.DIN"],
        },
        "no_connect": batt["no_connect"] + ["leds.DOUT"],
        "intent": json.loads(json.dumps(batt["intent"])),
    }
    intent = compose["intent"]
    intent["sources"] = {"rp2350_matrix_brief": {"title": "rp2350-matrix: ultraroute's quick demo board (compose.json's note)", "revision": "2026-11-07"}}
    budget = lambda a, note: {"value": {"continuous_a": a}, "strength": "hard", "provenance": {"source": "rp2350_matrix_brief", "locator": "power budget", "kind": "requirement", "note": note}}
    for net in intent["nets"].values():
        for k in ("current", "voltage"):
            if k in net:
                net[k]["provenance"]["source"] = "rp2350_matrix_brief"
    intent["default_net"]["voltage"]["provenance"]["source"] = "rp2350_matrix_brief"
    intent["nets"]["VBAT"]["current"] = budget(1.0, "On battery the cell feeds the buck (0.6 A) and the LEDs, which firmware holds to 0.4 A, through Q1.")
    intent["nets"]["VBUS"]["current"] = budget(1.213, "The charger's 213 mA and, through D3, the buck's 0.6 A and the LEDs' 0.4 A.")
    intent["nets"]["VSYS"] = {"current": budget(1.0, "The buck's 0.6 A input at full load and the 64 LEDs, held to 0.4 A by firmware (6 mA each on average: the buck's locked reference layout feeds VIN at 0.5 mm, sized for less)."),
                              "voltage": {"value": {"peak_v": 5.5}, "strength": "hard", "provenance": {"source": "rp2350_matrix_brief", "locator": "power path", "kind": "requirement", "note": "VBUS through D3 or the cell through Q1."}}}
    intent["nets"]["LED_DIN"] = {"class": "SIGNAL", "voltage": {"value": {"peak_v": 5.5}, "strength": "hard", "provenance": {"source": "rp2350_matrix_brief", "locator": "level shifter", "kind": "requirement", "note": "Pulled up to VSYS."}}}

    p = a.pitch
    w = max(7 * p + 2 * MARGIN, 40.0)
    x0 = (w - 7 * p) / 2
    fixed = {}
    for i in range(N * N):
        row, col = divmod(i, N)
        c = col if row % 2 == 0 else N - 1 - col
        fixed[f"LED{i + 1}"] = {"x": round(x0 + c * p, 4), "y": round(MARGIN + row * p, 4), "rotation": 0.0 if row % 2 == 0 else 180.0}
    # Each capacitor just below the LED it serves (LED 2, 6, 10, ...: cap_first 2, cap_every 4).
    for k in range(16):
        led = fixed[f"LED{2 + 4 * k}"]
        fixed[f"C{101 + k}"] = {"x": led["x"], "y": round(led["y"] + 1.7, 4), "rotation": 0.0}
    batt_fp = json.loads((Path(__file__).parent.parent / "rp2350-battery" / "floorplan.json").read_text())
    # The rigid layouts name their parts by role; the composition numbers them.
    root = Path(__file__).resolve().parents[2]
    with tempfile.TemporaryDirectory() as tmp:
        (Path(tmp) / "compose.json").write_text(json.dumps(compose))
        subprocess.run([str(root / "target/release/ccomp"), "compose", str(Path(tmp) / "compose.json"), "-o", str(Path(tmp) / "composed.json")], cwd=root, check=True, capture_output=True)
        bind = json.loads((Path(tmp) / "composed.json").read_text())["bindings"]
    mcu, reg = bind["mcu"], bind["reg"]
    rigid = [
        {**batt_fp["rigid"][0], "anchor": mcu["chip"], "roles": {"cin": mcu["parts"]["CIN"], "cout": mcu["parts"]["COUT"], "inductor": mcu["parts"]["L1"]}},
        {**batt_fp["rigid"][1], "anchor": reg["chip"], "roles": {"cin": reg["parts"]["CIN"], "cout": reg["parts"]["COUT"], "inductor": reg["parts"]["L1"],
                                                                 "rfb_top": reg["parts"]["RFB_TOP"], "rfb_bot": reg["parts"]["RFB_BOT"], "cff": reg["parts"]["CFF"]}},
    ]
    spec = {
        "note": f"The matrix demo at {p} mm LED pitch: the LEDs fixed on the grid, serpentine (odd rows turned 180 so each DO faces the next DI), the rest floorplanned in the {STRIP} mm strip below. USB-C on the bottom edge, the battery connector beside it. Nothing on the back. Written by make.py.",
        "outline": [0.0, 0.0, round(w, 4), round(7 * p + 2 * MARGIN + STRIP, 4)],
        "fixed": fixed,
        "edges": {"J1": {"side": "bottom", "facing": [0, 1]}, "J2": {"side": "bottom", "facing": [0, 1]}},
        "rigid": rigid,
        "blocks": {k: v for k, v in batt_fp["blocks"].items() if k not in ("R4", "D1")},
        "separations": [s for s in batt_fp["separations"] if "imu" not in s["a"] + s["b"]],
        "keepouts": [{"name": "LED field", "rect": [0.0, 0.0, round(w, 4), round(7 * p + MARGIN + 2.5, 4)], "side": "both"}],
    }
    out.mkdir(parents=True, exist_ok=True)
    (out / "compose.json").write_text(json.dumps(compose, indent=1) + "\n")
    (out / "floorplan.json").write_text(json.dumps(spec, indent=1) + "\n")


if __name__ == "__main__":
    main()
