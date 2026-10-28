#!/usr/bin/env python3
"""Check a package circuit (`schematic/circuit.VARIANT.json`) against the KiCad libraries it
names, so a composed board's schematic and PCB can be written from it:

- every part's symbol is in a symbol library (`(extends ...)` followed), and its footprint in a
  footprint library;
- every role pin on a net or port is a pin number of the part's symbol and a pad of its
  footprint (an exposed pad the symbol lacks is reported, not failed, when a same-net pad exists);
- every symbol pin is on a net or port, or named in `chip_pins_unconnected`, or is a no-connect
  pin by type;
- the roles nets name are parts, and each pin is on one net only.

    circuit_check.py CIRCUIT.json [...] [--lib DIR ...]

Libraries are looked up in each package's `kicad/` folder first, then `--lib` folders, then
KiCad's own: with --kicad 10 (the default) KICAD10_SYMBOL_DIR / KICAD10_FOOTPRINT_DIR or the
Flatpak's; with --kicad 9 the system's (/usr/share/kicad), which compose_kicad.py writes with.
Versions rename things (the USB-C shield pin is S1 in 9 and SH in 10): check against both.
Prints one JSON result; exits 1 on any error.
"""
import argparse
import glob
import json
import os
import re
import sys


def sexp(text):
    """A minimal S-expression reader: lists, strings and atoms."""
    tokens = re.findall(r'"(?:[^"\\]|\\.)*"|[()]|[^\s()"]+', text)
    stack, cur = [], []
    for t in tokens:
        if t == "(":
            stack.append(cur)
            cur = []
        elif t == ")":
            done, cur = cur, stack.pop()
            cur.append(done)
        else:
            cur.append(t[1:-1].replace('\\"', '"') if t.startswith('"') else t)
    return cur[0] if cur else []


def std_dirs(kind, version="10"):
    if version == "9":
        # The system KiCad, which easy_pcb's writer and pcbnew use.
        return [d for d in [os.environ.get(f"KICAD9_{kind.upper()}_DIR"), f"/usr/share/kicad/{kind}s"] if d and os.path.isdir(d)][:1]
    env = os.environ.get(f"KICAD10_{kind.upper()}_DIR")
    cands = [env] if env else []
    cands += glob.glob(os.path.expanduser(f"~/.local/share/flatpak/runtime/org.kicad.KiCad.Library.{kind.capitalize()}s/x86_64/stable/active/files/{kind}s"))
    cands += [f"/var/lib/flatpak/runtime/org.kicad.KiCad.Library.{kind.capitalize()}s/x86_64/stable/active/files/{kind}s", f"/usr/share/kicad/{kind}s"]
    return [c for c in cands if c and os.path.isdir(c)][:1]


class Libs:
    def __init__(self, dirs_sym, dirs_fp):
        self.dirs_sym, self.dirs_fp, self.cache = dirs_sym, dirs_fp, {}

    def symbol_lib(self, lib):
        if lib not in self.cache:
            path = next((os.path.join(d, f"{lib}.kicad_sym") for d in self.dirs_sym if os.path.exists(os.path.join(d, f"{lib}.kicad_sym"))), None)
            tree = sexp(open(path).read()) if path else None
            self.cache[lib] = ({s[1]: s for s in tree if isinstance(s, list) and s and s[0] == "symbol"} if tree else None, path)
        return self.cache[lib]

    def pins(self, lib_id):
        """Pin number -> (name, type) of a symbol, or None if not found."""
        lib, name = lib_id.split(":", 1)
        syms, _ = self.symbol_lib(lib)
        if not syms or name not in syms:
            return None
        sym = syms[name]
        ext = next((x[1] for x in sym if isinstance(x, list) and x[0] == "extends"), None)
        if ext:
            return self.pins(f"{lib}:{ext}")
        out = {}

        def walk(node):
            for x in node:
                if isinstance(x, list) and x:
                    if x[0] == "pin":
                        num = next(y[1] for y in x if isinstance(y, list) and y[0] == "number")
                        pname = next(y[1] for y in x if isinstance(y, list) and y[0] == "name")
                        out[num] = (pname, x[1])
                    else:
                        walk(x)

        walk(sym)
        return out

    def pads(self, fp_id):
        lib, name = fp_id.split(":", 1)
        for d in self.dirs_fp:
            p = os.path.join(d, f"{lib}.pretty", f"{name}.kicad_mod")
            if os.path.exists(p):
                tree = sexp(open(p).read())
                return {x[1] for x in tree if isinstance(x, list) and x and x[0] == "pad" and x[1] != ""}
        return None


def check(path, extra, version="10"):
    circ = json.load(open(path))
    pkg = os.path.dirname(os.path.dirname(os.path.abspath(path)))
    local = [os.path.join(pkg, "kicad")] if os.path.isdir(os.path.join(pkg, "kicad")) else []
    libs = Libs(local + extra + std_dirs("symbol", version), local + extra + std_dirs("footprint", version))
    errors, warnings = [], []
    pins, pads = {}, {}
    for role, part in circ["parts"].items():
        p = libs.pins(part["symbol"])
        if p is None:
            errors.append(f"{role}: symbol {part['symbol']} not found")
        pins[role] = p or {}
        f = libs.pads(part["footprint"])
        if f is None:
            errors.append(f"{role}: footprint {part['footprint']} not found")
        pads[role] = f or set()
    on = {}
    groups = [(f"net {n}", ps) for n, ps in circ["nets"].items()] + [(f"port {n}", p["pins"]) for n, p in circ["ports"].items()]
    for where, ps in groups:
        for rp in ps:
            role, num = rp.split(".", 1)
            if role not in circ["parts"]:
                errors.append(f"{where}: {rp}: no part {role}")
                continue
            if rp in on:
                errors.append(f"{rp} is on {on[rp]} and {where}")
            on[rp] = where
            if pins[role] and num not in pins[role]:
                errors.append(f"{where}: {rp}: {circ['parts'][role]['symbol']} has no pin {num}")
            if pads[role] and num not in pads[role]:
                errors.append(f"{where}: {rp}: footprint {circ['parts'][role]['footprint']} has no pad {num}")
    unconnected = set(circ.get("chip_pins_unconnected", []))
    for role, p in pins.items():
        for num, (pname, ptype) in sorted(p.items()):
            rp = f"{role}.{num}"
            if rp not in on and rp not in unconnected and ptype != "no_connect":
                errors.append(f"{rp} ({pname}) is on no net and not listed as unconnected")
        for pad in sorted(pads[role] - set(p)):
            if p:
                warnings.append(f"{role}: footprint pad {pad} has no symbol pin (an exposed or mounting pad: check it is joined to the right net in the footprint)")
    for n, port in circ["ports"].items():
        if port.get("kind") not in ("ground", "supply", "signal"):
            errors.append(f"port {n}: kind {port.get('kind')!r} is not ground, supply or signal")
    return {"circuit": path, "ok": not errors, "parts": len(circ["parts"]), "errors": errors, "warnings": warnings}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("circuits", nargs="+")
    ap.add_argument("--lib", action="append", default=[])
    ap.add_argument("--kicad", choices=["9", "10"], default="10", help="whose standard libraries: KiCad 10 (Flatpak) or 9 (the system's, which tools/compose_kicad.py writes with)")
    a = ap.parse_args()
    results = [check(c, a.lib, a.kicad) for c in a.circuits]
    print(json.dumps({"ok": all(r["ok"] for r in results), "circuits": results}, indent=1))
    sys.exit(0 if all(r["ok"] for r in results) else 1)


if __name__ == "__main__":
    main()
