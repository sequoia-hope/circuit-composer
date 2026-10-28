#!/usr/bin/env python3
"""A package's nets in its own frame: which role pins sit on each net, and between which two
pins each net's length is measured. Read from the reference boards' netlists through the
package's support table, and cross-checked between the boards.

    package_nets.py PACKAGE_DIR --board KEY VARIANT NETLIST CHIP SPEC [--board ...] -o nets.json

KEY is a board key of `schematic/support.json` (`qfn60`), VARIANT its `ports.json` variant
(`RP2350A`); NETLIST is that board's KiCad
netlist; CHIP its chip's reference; SPEC its `ccomp measure` spec, whose paths give the length
pairs. The first board is the frame: its chip pin numbers are the package's. Other boards'
chip pins are translated by pin name through `ports.json`; a name with several pins (IOVDD) is
left out, since it cannot be translated one to one. Anchors below name each package net by one
pin in the frame. The frame board's pins are the package's; where another board differs, the
difference is listed.
"""
import argparse
import json
import os
import re
import sys

# Each package net, by one pin on it (RP2350A numbers, package part names).
ANCHORS = {
    "VREG_VIN": "U1.49",
    "VREG_LX": "U1.48",
    "DVDD": "U1.50",
    "GND": "U1.47",
    "XIN": "U1.21",
    "XOUT": "U1.22",
    "XOUT_XTAL": "XTAL.3",
    "USB_DP_PIN": "U1.52",
    "USB_DM_PIN": "U1.51",
    "USB_DP": "R_USB_DP.1",
    "USB_DM": "R_USB_DM.1",
    "QSPI_SCLK": "U1.56",
    "QSPI_SD0": "U1.57",
    "QSPI_SD1": "U1.59",
    "QSPI_SD2": "U1.58",
    "QSPI_SD3": "U1.55",
    "QSPI_SS": "U1.60",
    "FLASH_SS": "FLASH.1",
    "IOVDD": "FLASH.8",
    "VREG_AVDD": "U1.46",
}


def netlist(path):
    text = re.sub(r"\s+", " ", open(path).read())
    nets = {}
    for chunk in re.split(r"(?=\(net \(code )", text)[1:]:
        name = re.match(r'\(net \(code "?\d+"?\) \(name "([^"]*)"', chunk).group(1)
        nets[name] = re.findall(r'\(node \(ref "([^"]+)"\) \(pin "([^"]+)"\)', chunk)
    return nets


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("package")
    ap.add_argument("--board", nargs=5, action="append", required=True, metavar=("KEY", "VARIANT", "NETLIST", "CHIP", "SPEC"))
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    support = json.load(open(os.path.join(a.package, "schematic/support.json")))
    ports = json.load(open(os.path.join(a.package, "ports.json")))
    variants = ports["variants"]

    def names(variant):
        by_name = {}
        for num, p in variant["pins"].items():
            by_name.setdefault(p["name"], []).append(num)
        return by_name

    frame_names = names(variants[a.board[0][1]])

    boards = []
    for i, (key, variant_name, path, chip, spec) in enumerate(a.board):
        ref_to_part = {}
        for block in support["blocks"].values():
            for part, by_key in block.items():
                if by_key.get(key, {}).get("ref"):
                    ref_to_part[by_key[key]["ref"]] = part
        # This board's chip pins in the frame's numbers.
        if i == 0:
            to_frame = lambda pin: pin
        else:
            variant = variants[variant_name]
            mine = {num: p["name"] for num, p in variant["pins"].items()}
            vnames = names(variant)

            def to_frame(pin, mine=mine, vnames=vnames):
                n = mine.get(pin)
                if n and len(vnames.get(n, [])) == 1 and len(frame_names.get(n, [])) == 1:
                    return frame_names[n][0]
                return None
        pins_on = {}
        for net, nodes in netlist(path).items():
            out = set()
            for ref, pin in nodes:
                if ref == chip:
                    p = to_frame(pin)
                    if p:
                        out.add(f"U1.{p}")
                elif ref in ref_to_part:
                    out.add(f"{ref_to_part[ref]}.{pin}")
            pins_on[net] = out
        frame_paths = []
        for p in json.load(open(spec))["paths"]:
            ends = []
            for end in (p["from"], p["to"]):
                ref, pin = end.split(".", 1)
                if ref == chip:
                    pin = to_frame(pin)
                    ends.append(f"U1.{pin}" if pin else None)
                else:
                    ends.append(f"{ref_to_part[ref]}.{pin}" if ref in ref_to_part else None)
            frame_paths.append((p["name"], ends))
        boards.append({"key": key, "netlist": path, "pins_on": pins_on, "paths": frame_paths})

    out, problems = {}, []
    for net, anchor in ANCHORS.items():
        sets = []
        for b in boards:
            found = [s for s in b["pins_on"].values() if anchor in s]
            if not found:
                problems.append(f"{b['key']}: no net holds {anchor} ({net})")
                sets.append(set())
                continue
            sets.append(found[0])
        frame = sets[0]
        # The frame board's pins are the package's; each disagreement is reported.
        for b, s in zip(boards[1:], sets[1:]):
            for p in sorted(frame ^ s):
                where = boards[0]["key"] if p in frame else b["key"]
                problems.append(f"{net}: {p} only on {where}")
        entry = {"anchor": anchor, "pins": sorted(frame, key=lambda p: (p.split(".")[0] != "U1", p))}
        for name, ends in boards[0]["paths"]:
            if None not in ends and all(e in frame for e in ends):
                entry["length"] = {"from": ends[0], "to": ends[1], "measured_as": name}
                break
        out[net] = entry
    result = {
        "note": "Package nets in the package's frame (RP2350A pin numbers, part names from schematic/support.json): the pins the frame board puts on the net, and the pin pair a net's max_length is measured between. Generated by tools/package_nets.py; `differences` lists what the boards do not share.",
        "frame_variant": a.board[0][1],
        "boards": [b["key"] for b in boards],
        "nets": out,
        "differences": problems,
    }
    json.dump(result, open(a.output, "w"), indent=1)
    print(f"{len(out)} nets, {len(problems)} differences", file=sys.stderr)


if __name__ == "__main__":
    main()
