#!/usr/bin/env python3
"""Draft a composition from an existing board's netlist, and check a composition against it.

    compose_from_netlist.py draft NETLIST --instance NAME PACKAGE@VER VARIANT BINDING [...] -o compose.json
    compose_from_netlist.py compare NETLIST COMPOSED.json --binding NAME BINDING [...]

draft: BINDING maps the package's roles to the board's designators (as `ccomp check package`
reads it). The board's other parts become the composition's own parts; each board net that
reaches one of them, or a package port, becomes a composition net naming `NAME.PORT` and the
board pins. Where the board departs from the package:
- a board net on an internal package net is a tap (`NAME.NET`);
- one board net holding several internal nets is a `join` (an omitted link part);
- a port the board splits, or joins to another, is a disagreement: the package parts on those
  nets are moved to the board's own parts, and the draft is tried again.

compare: the composed circuit (`ccomp compose -o`) against the netlist. Composed designators are
mapped back through the composed bindings and the reference bindings, then the two boards' nets
are compared as sets of pins; a two-pin resistor or capacitor counts once, either way round
(the comparison then cannot tell which of its ends a net is on). Prints one JSON result; exits 1 on any difference.
"""
import argparse
import json
import os
import re
import sys

sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
from package_circuit import parse  # noqa: E402


def circuit_for(packages, spec, variant):
    name, _ = spec.split("@")
    return json.load(open(os.path.join(packages, name, "schematic", f"circuit.{variant}.json")))


def draft(a):
    comps, nets = parse(a.netlist)
    instances, moved, notes = {}, {}, []
    specs = {name: (spec, variant, json.load(open(binding))) for name, spec, variant, binding in a.instance}
    circuits = {name: circuit_for(a.packages, spec, variant) for name, (spec, variant, _) in specs.items()}
    net_of = {f"{r}.{p}": net for net, nodes in nets.items() for r, p in nodes}
    reversed_ = {}
    while True:
        # Which package net (port or internal) each board pin of an instance's parts is on.
        owned, group_of = {}, {}
        for name, (_, _, b) in specs.items():
            role_ref = {r: d for r, d in dict(b["parts"], U1=b["chip"]).items() if r not in moved.get(name, ())}
            for role, ref in role_ref.items():
                owned[ref] = (name, role)
            circ = circuits[name]
            for kind, table in (("port", {k: v["pins"] for k, v in circ["ports"].items()}), ("net", circ["nets"])):
                for gname, pins in table.items():
                    for p in pins:
                        role, pin = p.split(".", 1)
                        if role in role_ref:
                            group_of[f"{role_ref[role]}.{pin}"] = (name, kind, gname)
            # A resistor or capacitor whose two ends are each on the other end's package net is
            # fitted the other way round.
            for role, ref in role_ref.items():
                if circ["parts"][role]["symbol"] not in ("Device:R", "Device:C") or role in reversed_.get(name, []):
                    continue
                g1, g2 = group_of.get(f"{ref}.1"), group_of.get(f"{ref}.2")
                others = lambda pin: {group_of[q] for q in (f"{r}.{p}" for r, p in nets.get(net_of.get(f"{ref}.{pin}"), [])) if q in group_of and not q.startswith(f"{ref}.")}
                if g1 and g2 and g1 != g2 and g2 in others("1") and g1 in others("2"):
                    reversed_.setdefault(name, []).append(role)
            for role in reversed_.get(name, []):
                ref = role_ref[role]
                group_of[f"{ref}.1"], group_of[f"{ref}.2"] = group_of[f"{ref}.2"], group_of[f"{ref}.1"]
        conflicts, out_nets, joins, taps = [], {}, {}, []
        port_net = {}
        for net, nodes in nets.items():
            pins = [f"{r}.{p}" for r, p in nodes]
            board = [p for p in pins if p.split(".")[0] not in owned]
            groups = sorted({group_of[p] for p in pins if p in group_of})
            ports = [g for g in groups if g[1] == "port"]
            internal = [g for g in groups if g[1] == "net"]
            for g in ports:
                if g in port_net:
                    conflicts.append((net, f"{g[0]}.{g[2]} is split over {port_net[g]} and {net}"))
                port_net[g] = net
            if len(ports) > 1 or (ports and internal):
                conflicts.append((net, f"{net} joins {', '.join(f'{g[0]}.{g[2]}' for g in groups)}"))
                continue
            if len(internal) > 1:
                # Named by the net on the chip's side.
                chip_first = sorted(internal, key=lambda g: not any(p.startswith("U1.") for p in circuits[g[0]]["nets"][g[2]]))
                joins.setdefault(internal[0][0], []).append([g[2] for g in chip_first])
            if not groups:
                # A one-pin net is an open pin, not a net.
                if len(board) > 1:
                    out_nets[net.lstrip("/")] = board
            elif ports:
                if board:
                    out_nets[net.lstrip("/")] = [f"{ports[0][0]}.{ports[0][2]}"] + board
            elif board:
                out_nets[net.lstrip("/")] = [f"{internal[0][0]}.{internal[0][2]}"] + board
                taps.append(f"{internal[0][0]}.{internal[0][2]} -> {net}")
        if not conflicts:
            break
        # Where board and package disagree, the package's parts on the smallest such net become
        # the board's own, and the draft is tried again.
        net = min((n for n, _ in conflicts), key=lambda n: len(nets[n]))
        bad = {owned[r] for r, _ in nets[net] if r in owned and owned[r][1] != "U1"}
        notes += [c for n, c in conflicts if n == net]
        if not bad:
            notes += [c for n, c in conflicts if n != net]
            break
        for n, r in sorted(bad):
            moved.setdefault(n, []).append(r)
    for name, (spec, variant, b) in specs.items():
        role_ref = dict(b["parts"], U1=b["chip"])
        omit = sorted({r for r in circuits[name]["parts"] if r not in role_ref} | set(moved.get(name, [])))
        inst = {"package": spec, "variant": variant}
        if a.keep_designators:
            inst["designators"] = {r: d for r, d in sorted(role_ref.items()) if r not in omit}
        if omit:
            inst["omit"] = omit
        if joins.get(name):
            inst["join"] = joins[name]
        if reversed_.get(name):
            inst["reversed"] = sorted(reversed_[name])
        instances[name] = inst
    parts = {ref: {"symbol": c["symbol"], "value": c["value"], "footprint": c["footprint"]} for ref, c in sorted(comps.items()) if ref not in owned}
    comp = {"board": a.board or os.path.splitext(os.path.basename(a.netlist))[0], "note": f"Drafted by tools/compose_from_netlist.py from {os.path.basename(a.netlist)}.", "instances": instances, "parts": parts, "nets": dict(sorted(out_nets.items()))}
    json.dump(comp, open(a.output, "w"), indent=1, ensure_ascii=False)
    report = {"instances": len(instances), "parts": len(parts), "nets": len(out_nets),
              "moved_to_board": {n: [f"{r} ({dict(specs[n][2]['parts'], U1=specs[n][2]['chip'])[r]})" for r in rs] for n, rs in moved.items()},
              "reversed": reversed_, "joins": joins, "taps": taps, "disagreements": notes}
    print(json.dumps(report, indent=1))
    return 0


def compare(a):
    _, nets = parse(a.netlist)
    composed = json.load(open(a.composed))
    back = {}
    for name, binding in a.binding:
        ref_b = json.load(open(binding))
        ref_roles = dict(ref_b["parts"], U1=ref_b["chip"])
        mine = composed["bindings"][name]
        for role, des in dict(mine["parts"], U1=mine["chip"]).items():
            back[des] = ref_roles.get(role, f"?{role}")
    comps, _ = parse(a.netlist)
    mapped = lambda pin: (lambda r, p: f"{back.get(r, r)}.{p}")(*pin.split(".", 1))
    # A resistor or capacitor fitted either way round is the same part: compare its pins as one.
    sym = {r: c["symbol"] for r, c in comps.items()}
    two_pin = lambda p: sym.get(p.split(".")[0]) in ("Device:R", "Device:C", "Device:R_Small", "Device:C_Small")
    canon = lambda p: f"{p.split('.')[0]}.~" if two_pin(p) else p
    theirs = {frozenset(canon(f"{r}.{p}") for r, p in nodes): name for name, nodes in nets.items() if len(nodes) > 1}
    ours = {frozenset(canon(mapped(p)) for p in pins): name for name, pins in composed["nets"].items()}
    # A one-pin net is an open pin on either side.
    theirs = {k: v for k, v in theirs.items() if len(k) > 1}
    ours = {k: v for k, v in ours.items() if len(k) > 1}
    only_ref = sorted(f"{v}: {' '.join(sorted(k))}" for k, v in theirs.items() if k not in ours)
    only_comp = sorted(f"{v}: {' '.join(sorted(k))}" for k, v in ours.items() if k not in theirs)
    renamed = sorted(f"{theirs[k]} -> {ours[k]}" for k in theirs if k in ours and theirs[k].lstrip("/") != ours[k].lstrip("/").split("/")[-1])
    r = {"ok": not only_ref and not only_comp, "nets_reference": len(theirs), "nets_composed": len(ours), "only_in_reference": only_ref, "only_in_composed": only_comp, "renamed": renamed}
    print(json.dumps(r, indent=1))
    return 0 if r["ok"] else 1


def main():
    ap = argparse.ArgumentParser()
    sub = ap.add_subparsers(dest="cmd", required=True)
    d = sub.add_parser("draft")
    d.add_argument("netlist")
    d.add_argument("--instance", nargs=4, action="append", required=True, metavar=("NAME", "PACKAGE@VER", "VARIANT", "BINDING"))
    d.add_argument("--packages", default="packages")
    d.add_argument("--board")
    d.add_argument("--keep-designators", action="store_true", help="the composed board keeps this board's designators")
    d.add_argument("-o", "--output", required=True)
    c = sub.add_parser("compare")
    c.add_argument("netlist")
    c.add_argument("composed")
    c.add_argument("--binding", nargs=2, action="append", required=True, metavar=("NAME", "BINDING"))
    a = ap.parse_args()
    sys.exit(draft(a) if a.cmd == "draft" else compare(a))


if __name__ == "__main__":
    main()
