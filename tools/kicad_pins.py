#!/usr/bin/env python3
"""One part's pin table from a KiCad netlist: number, pin name, pin type, net.

    kicad_pins.py NETLIST REF [--rename OLD=NEW ...]

NETLIST is a KiCad `.net` (`kicad-cli sch export netlist`), one node per line (KiCad 9) or one
field per line (KiCad 10). Prints {"pins": {number: {"name",
"type", "net"}}}. --rename fixes a symbol's pin names where they differ from the datasheet's.
"""
import argparse
import json
import re


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("netlist")
    ap.add_argument("ref")
    ap.add_argument("--rename", action="append", default=[])
    a = ap.parse_args()
    rename = dict(r.split("=", 1) for r in a.rename)
    pins, net = {}, None
    text = re.sub(r"\s+", " ", open(a.netlist).read())
    for line in re.split(r"(?=\(net \(code |\(node )", text):
        m = re.match(r'\(net \(code "?\d+"?\) \(name "([^"]*)"', line)
        if m:
            net = m.group(1)
        m = re.match(r'\(node \(ref "([^"]+)"\) \(pin "([^"]+)"\)(?: \(pinfunction "([^"]*)"\))?(?: \(pintype "([^"]*)"\))?', line)
        if m and m.group(1) == a.ref:
            name = m.group(3) or m.group(2)
            pins[m.group(2)] = {"name": rename.get(name, name), "type": m.group(4), "net": net}
    order = sorted(pins, key=lambda p: (not p.isdigit(), int(p) if p.isdigit() else 0, p))
    print(json.dumps({"pins": {p: pins[p] for p in order}}, indent=1))


if __name__ == "__main__":
    main()
