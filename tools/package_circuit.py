#!/usr/bin/env python3
"""A package's circuit, for composing it into a board: every part by role (symbol, value,
footprint), every net inside the package, and the ports where the package meets the board.
Read from a reference board's netlist through the package's support table.

    package_circuit.py PACKAGE_DIR --board KEY --variant VARIANT --netlist NET --chip REF -o OUT

KEY is a board key of `schematic/support.json` (`qfn60`). The reference board's parts that
`support.json` names are the package's; the chip is role `U1`. Everything else on the board
(connectors, the board's own regulator, headers, holes) is the board's. A net with package
pins only is internal; one that reaches a board part, or a chip pin the board wires out, is a
port, named as the board names it. A port's kind is `ground`, `supply` (it holds a chip power
pin) or `signal`; ground and supply ports must be connected when the package is composed.
"""
import argparse
import json
import re
import sys

from package_nets import ANCHORS  # the package's own net names, by anchor pin


def parse(path):
    text = re.sub(r"\s+", " ", open(path).read())
    comps = {}
    for chunk in re.split(r"(?=\(comp \(ref )", text.split("(libparts")[0])[1:]:
        ref = re.match(r'\(comp \(ref "([^"]+)"\)', chunk).group(1)
        get = lambda k: (re.search(rf'\({k} "([^"]*)"\)', chunk) or [None, ""])[1]
        lib = re.search(r'\(libsource \(lib "([^"]*)"\) \(part "([^"]*)"\)', chunk)
        comps[ref] = {"value": get("value"), "footprint": get("footprint"), "symbol": f"{lib[1]}:{lib[2]}" if lib else ""}
    nets = {}
    for chunk in re.split(r"(?=\(net \(code )", text)[1:]:
        name = re.match(r'\(net \(code "?\d+"?\) \(name "([^"]*)"', chunk).group(1)
        nets[name] = re.findall(r'\(node \(ref "([^"]+)"\) \(pin "([^"]+)"\)', chunk)
    return comps, nets


def clean(name):
    """A board net name as a port name: no sheet path, KiCad's overbar markup kept as `~`."""
    return name.lstrip("/").replace("~{", "~").replace("}", "")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("package")
    ap.add_argument("--board", required=True)
    ap.add_argument("--variant", required=True)
    ap.add_argument("--netlist", required=True)
    ap.add_argument("--chip", required=True)
    ap.add_argument("--source", required=True, help="the package source id of the reference board")
    ap.add_argument("--footprint", action="append", default=[], metavar="OLD=NEW", help="a footprint the reference names that KiCad has since renamed")
    ap.add_argument("-o", "--output", required=True)
    a = ap.parse_args()
    renamed = dict(f.split("=", 1) for f in a.footprint)
    support = json.load(open(f"{a.package}/schematic/support.json"))
    ports_json = json.load(open(f"{a.package}/ports.json"))
    pins = ports_json["variants"][a.variant]["pins"]
    comps, nets = parse(a.netlist)

    role_of = {a.chip: "U1"}
    block_of = {"U1": "mcu"}
    for block, parts in support["blocks"].items():
        for role, by_key in parts.items():
            ref = by_key.get(a.board, {}).get("ref")
            if ref:
                role_of[ref] = role
                block_of[role] = block.split(" ")[0]
    parts = {}
    for ref, role in sorted(role_of.items(), key=lambda kv: kv[1]):
        c = comps[ref]
        parts[role] = {"block": block_of[role], "symbol": c["symbol"], "value": c["value"], "footprint": renamed.get(c["footprint"], c["footprint"]), "reference_ref": ref}
        if c["footprint"] in renamed:
            parts[role]["reference_footprint"] = c["footprint"]

    # Placement hints: the chip pin each decoupling capacitor serves (support.json, measured).
    by_ref = {v["reference_ref"]: r for r, v in parts.items()}
    for pin, d in sorted(support.get("decoupling_by_pin", {}).get(a.board, {}).items(), key=lambda kv: int(kv[0])):
        role = by_ref.get(d["cap"])
        if role and "near" not in parts[role] and role not in ("CIN", "COUT"):
            parts[role]["near"] = f"U1.{pin}"
    anchor_name = {pin: name for name, pin in ANCHORS.items()}
    internal, ports, problems = {}, {}, []
    for name, nodes in nets.items():
        mine = [f"{role_of[r]}.{p}" for r, p in nodes if r in role_of]
        theirs = [f"{r}.{p}" for r, p in nodes if r not in role_of]
        if not mine:
            continue
        mine.sort(key=lambda s: (not s.startswith("U1."), s))
        own = next((anchor_name[p] for p in mine if p in anchor_name), None)
        if theirs:
            classes = {ports_json["functions"][pins[p[3:]]["function"]]["class"] for p in mine if p.startswith("U1.") and p[3:] in pins}
            kind = "ground" if "GROUND" in classes else ("supply" if "POWER" in classes else "signal")
            # A signal keeps the package's name for it (USB_DP); a rail several package nets
            # share (+3V3 holds IOVDD and VREG_VIN) keeps the board's.
            key = own if own and kind == "signal" else clean(name)
            ports[key] = {"kind": kind, "pins": mine, "reference_net": name, "reference_board_pins": theirs}
        else:
            auto = name.startswith("Net-(") or name.startswith("unconnected-")
            internal[own or (f"N_{mine[0].replace('.', '_')}" if auto else clean(name))] = mine
    # Chip pins on no net at all are left unconnected by the reference; list them.
    used = {p for ps in list(internal.values()) + [v["pins"] for v in ports.values()] for p in ps}
    unconnected = sorted((p for p in pins if f"U1.{p}" not in used), key=lambda p: int(p) if p.isdigit() else 999)
    out = {
        "note": f"The package's circuit as the {a.variant} reference board fits it ({a.source}, netlist read by tools/package_circuit.py). Parts by role; `reference_ref` is the board's designator; `near` is the chip pin a decoupling capacitor serves, a placement hint. Nets inside the package by role pin; ports are where it meets the board, named as the board names them. Ground and supply ports must be connected.",
        "variant": a.variant,
        "board": a.board,
        "source": a.source,
        "parts": parts,
        "nets": dict(sorted(internal.items())),
        "ports": dict(sorted(ports.items(), key=lambda kv: (kv[1]["kind"] != "ground", kv[1]["kind"] != "supply", kv[0]))),
        "chip_pins_unconnected": [f"U1.{p}" for p in unconnected],
    }
    json.dump(out, open(a.output, "w"), indent=1, ensure_ascii=False)
    print(f"{len(parts)} parts, {len(internal)} internal nets, {len(ports)} ports ({sum(p['kind'] != 'signal' for p in ports.values())} power), {len(unconnected)} chip pins unconnected", file=sys.stderr)
    for p in problems:
        print(p, file=sys.stderr)


if __name__ == "__main__":
    main()
