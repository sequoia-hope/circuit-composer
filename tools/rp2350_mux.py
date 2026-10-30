#!/usr/bin/env python3
"""The RP2350's GPIO function table (datasheet §1.2.3, Table 3) into packages/rp2350/ports.json.

    tools/rp2350_mux.py rp2350-datasheet.pdf [--check]

The PDF must be the revision the package cites (its sha256 is in packages/rp2350/sources/
sources.json). It is read with `pdftotext -layout`; each row's cells (wrapped cells joined) are
told apart by what they name, which is fixed by column: F0 HSTX, F1 SPI, F2 UART, F3 I2C, F4 PWM,
F5 SIO, F6-F8 PIO0-2, F9 trace, clocks and QMI, F10 USB, F11 UART again. Writes the "mux" key
only; everything else in ports.json is written by hand. With --check, says whether the file
already holds what the table gives, and writes nothing.
"""
import argparse
import hashlib
import json
import os
import re
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
PORTS = os.path.join(ROOT, "packages/rp2350/ports.json")
SOURCES = os.path.join(ROOT, "packages/rp2350/sources/sources.json")


def cells(line):
    """(start column, text) of each run separated by two or more spaces."""
    return [(m.start(), m.group()) for m in re.finditer(r"\S+(?: \S+)*", line)]


def rows(text):
    """GPIO number -> its cells, wrapped lines joined to the cell they sit under."""
    start = text.index("General Purpose Input/Output (GPIO) Bank 0 Functions")
    end = text.index("GPIO bank 0 function descriptions", start)
    out = {}
    cur = None
    for line in text[start:end].splitlines():
        c = cells(line)
        if not c:
            continue
        if re.fullmatch(r"\d+", c[0][1]) and c[0][0] < 4:
            cur = int(c[0][1])
            out[cur] = [[x, t] for x, t in c[1:]]
        elif cur is not None and c[0][0] > 10 and not line.strip().startswith(("Introduction", "RP2350", "GPIOs")):
            for x, t in c:
                # The cell this wrapped text continues: the last one starting at or before it.
                host = max((k for k in out[cur] if k[0] <= x + 2), key=lambda k: k[0])
                host[1] += " " + t
        else:
            cur = None
    return {g: [t for _, t in c] for g, c in out.items()}


def classify(gpio, texts):
    """Each cell's function select, from what it names."""
    out = []
    late = False  # past F9: a UART here is F11
    for t in texts:
        if t == "HSTX":
            f = "F0"
        elif t.startswith("SPI"):
            f = "F1"
        elif t.startswith("UART"):
            f = "F11" if late else "F2"
        elif t.startswith("I2C"):
            f = "F3"
        elif t.startswith("PWM"):
            f = "F4"
        elif t == "SIO":
            f = "F5"
        elif t in ("PIO0", "PIO1", "PIO2"):
            f = f"F{6 + int(t[3])}"
        elif t.startswith(("QMI", "TRACE", "CLOCK")):
            f = "F9"
        elif t.startswith("USB"):
            f = "F10"
            late = True
        else:
            sys.exit(f"GPIO{gpio}: no column names {t!r}")
        out.append((f, t))
    fs = [f for f, _ in out]
    if fs != sorted(fs, key=lambda f: int(f[1:])) or len(set(fs)) != len(fs):
        sys.exit(f"GPIO{gpio}: columns out of order: {fs}")
    return out


def entry(f, t):
    """[function select, instance, signal], in the datasheet's own words."""
    if t in ("SIO", "PIO0", "PIO1", "PIO2", "HSTX"):
        return [f, t, "IO"]
    if t.startswith("QMI"):
        return [f, "QMI", t.split()[1]]
    if t.startswith("TRACE"):
        return [f, "TRACE", t]
    if t.startswith("CLOCK"):
        return [f, "CLOCK", t.split()[1]]
    if t.startswith("USB"):
        return [f, "USB", "_".join(t.split()[1:])]
    inst, sig = t.split(" ", 1)
    return [f, inst, sig]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("pdf")
    ap.add_argument("--check", action="store_true")
    a = ap.parse_args()
    want = json.load(open(SOURCES))["rp2350_ds"]["sha256"]
    got = hashlib.sha256(open(a.pdf, "rb").read()).hexdigest()
    if got != want:
        sys.exit(f"{a.pdf}: sha256 {got}, but the package cites {want}")
    text = subprocess.run(["pdftotext", "-layout", a.pdf, "-"], capture_output=True, text=True, check=True).stdout
    table = rows(text)
    if sorted(table) != list(range(48)):
        sys.exit(f"expected GPIO0-47, found {sorted(table)}")
    mux = {
        "note": "Each GPIO's function selects (FUNCSEL), by GPIO number: [function, instance, signal] in the datasheet's words. A pin named GPIOn or GPIOn_ADCm is GPIOn. GPIO30-47 are QFN-80 (RP2350B) only. SIO and PIO0-2 reach every GPIO.",
        "citation": {"source": "rp2350_ds", "locator": "§1.2.3 Table 3, pp.17-19"},
        "pins": {f"GPIO{g}": [entry(f, t) for f, t in classify(g, table[g])] for g in range(48)},
    }
    ports = json.load(open(PORTS))
    if a.check:
        same = ports.get("mux") == mux
        print("ports.json's mux matches the datasheet" if same else "ports.json's mux differs from the datasheet")
        sys.exit(0 if same else 1)
    ports["mux"] = mux
    text = json.dumps(ports, indent=1)
    # One [function, instance, signal] to a line.
    text = re.sub(r'\[\s+("[^"]*"),\s+("[^"]*"),\s+("[^"]*")\s+\]', r"[\1, \2, \3]", text)
    with open(PORTS, "w") as f:
        f.write(text + "\n")
    print(f"wrote the mux for {len(mux['pins'])} GPIOs to {os.path.relpath(PORTS, ROOT)}")


if __name__ == "__main__":
    main()
