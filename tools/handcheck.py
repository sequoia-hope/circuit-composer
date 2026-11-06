#!/usr/bin/env python3
"""The recogniser's hand check: blind review sheets, truth files, and the score.

    handcheck.py sheet ID...        write HANDCHECK/ID/sheet.md
    handcheck.py strata             count the corpus's boards per sample stratum (for weighting)
    handcheck.py score [ID...]      score every board with a truth.json (or the ones named)

PCBENCH (default ~/Software/ultraroute-lab/pcbench) holds boards/ID/board.kicad_pcb and
intent/sample50.json; HANDCHECK is PCBENCH/handcheck. Nothing here writes into this repository:
the corpus's licences keep its data, and labels made from it, outside it.

A sheet is blind: parts (reference, value, footprint) and nets (every pad on them), never the
recogniser's guesses, so labels made from it are not anchored on what is being scored. Nothing on
it comes from copper (tracks, pours, widths): currents are judged from the parts alone, as the
recogniser must.

truth.json, written by hand beside the sheet:

    {"coverage": "all" | "named",   # "named": unnamed nets only looked at where a part's
                                    #   pins made their job plain (a switch node, a crystal)
     "nets": {"GND": "GROUND",
              "+5V": {"class": "POWER", "continuous_a": 0.12, "peak_a": 1.0, "why": "..."},
              "/XIN": "CLOCK"},     # nets not listed are SIGNAL
     "blocks": [{"type": "ldo", "parts": ["U2", "C3", "C4"]}],
     "notes": "..."}

`score` turns each into an intent file, runs `ccomp recognise board --compare` on it, and sums
the comparisons, per board and weighted back to the corpus by the sample's strata.
"""
import argparse
import json
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path

REPO = Path(__file__).resolve().parent.parent
CCOMP = REPO / "target" / "release" / "ccomp"


def parse_board(path):
    """Parts {ref: (value, footprint)} and nets {name: [ref.pad]} from a .kicad_pcb."""
    text = Path(path).read_text(errors="replace")
    parts, nets = {}, defaultdict(list)
    for chunk in re.split(r"\n\t\(footprint ", text)[1:]:
        lib = re.match(r'"([^"]*)"', chunk)
        ref = re.search(r'\(property "Reference" "([^"]*)"', chunk) or re.search(r'\(fp_text reference "([^"]*)"', chunk)
        val = re.search(r'\(property "Value" "([^"]*)"', chunk) or re.search(r'\(fp_text value "([^"]*)"', chunk)
        ref = ref.group(1) if ref else "?"
        parts[ref] = (val.group(1) if val else "", lib.group(1) if lib else "")
        for pad in re.finditer(r'\(pad "([^"]*)"(.*?)\n\t\t\)', chunk, re.S):
            net = re.search(r'\(net (?:\d+ )?"([^"]*)"\)', pad.group(2))
            if net and net.group(1):
                nets[net.group(1)].append(f"{ref}.{pad.group(1)}")
    return parts, nets


def ref_key(r):
    m = re.match(r"([A-Za-z_#]*)(\d*)(.*)", r)
    return (m.group(1).upper(), int(m.group(2) or 0), m.group(3))


def sheet(root, bid):
    parts, nets = parse_board(root / "boards" / bid / "board.kicad_pcb")
    out = [f"# {bid}", "", f"{len(parts)} parts, {len(nets)} nets.", "", "## Parts", "", "| ref | value | footprint |", "|---|---|---|"]
    for r in sorted(parts, key=ref_key):
        v, f = parts[r]
        out.append(f"| {r} | {v} | {f} |")
    out += ["", "## Nets", "", "Pads as `ref.pad`. Unnamed nets are KiCad's `Net-(...)` or `unconnected-...`.", ""]
    for n in sorted(nets, key=lambda n: (n.startswith("Net-") or n.startswith("unconnected"), n)):
        pads = sorted(set(nets[n]), key=lambda p: ref_key(p))
        more = f" … and {len(pads) - 40} more" if len(pads) > 40 else ""
        out.append(f"- `{n}` ({len(pads)}): {' '.join(pads[:40])}{more}")
    d = root / "handcheck" / bid
    d.mkdir(parents=True, exist_ok=True)
    (d / "sheet.md").write_text("\n".join(out) + "\n")
    return d / "sheet.md"


def stratum_of(sample, layers, pads):
    lo, hi = sample["pad_tertiles"]
    return ("2L" if layers <= 2 else "4L" if layers == 4 else "6+L") + "/" + ("small" if pads <= lo else "medium" if pads <= hi else "large")


def strata(root):
    """The corpus's boards per stratum, counted as ultraroute drew the sample: copper layers and
    footprint pads, split at the sample's pad tertiles."""
    sample = json.loads((root / "intent" / "sample50.json").read_text())
    count = defaultdict(int)
    for b in sorted((root / "boards").glob("*/board.kicad_pcb")):
        t = b.read_text(errors="replace")
        layers = len(re.findall(r'"[^"]*\.Cu"', re.search(r"\n\t\(layers\n(.*?)\n\t\)", t, re.S).group(1)))
        count[stratum_of(sample, layers, len(re.findall(r'\n\t\t\(pad "', t)))] += 1
    (root / "handcheck" / "strata.json").write_text(json.dumps(dict(sorted(count.items())), indent=1))
    print(dict(sorted(count.items())), sum(count.values()))


def truth_intent(bid, truth, board_nets):
    prov = {"source": "handcheck", "kind": "expert"}
    # Nets the truth leaves out are SIGNAL, spelled out so a recognised SIGNAL scores as agreeing.
    nets = {n: {"class": "SIGNAL"} for n in board_nets}
    for name, t in truth["nets"].items():
        t = {"class": t} if isinstance(t, str) else t
        n = {"class": t["class"]}
        if "continuous_a" in t:
            cur = {"continuous_a": t["continuous_a"]}
            if t.get("peak_a") is not None:
                cur["peak_a"] = t["peak_a"]
            n["current"] = {"value": cur, "strength": "soft", "provenance": dict(prov, note=t.get("why", ""))}
        nets[name] = n
    blocks = {f"{b['type']}_{i}": {"type": b["type"], "parts": b["parts"]} for i, b in enumerate(truth.get("blocks", []))}
    return {"schema_version": "0.1", "board": {"name": bid}, "sources": {"handcheck": {"title": "PCBench hand check"}}, "nets": nets, "blocks": blocks}


def score(root, ids):
    hc = root / "handcheck"
    sample = json.loads((root / "intent" / "sample50.json").read_text())
    stratum = {b["id"]: stratum_of(sample, b["copper_layers"], b["pads"]) for b in sample["boards"]}
    ids = ids or sorted(p.parent.name for p in hc.glob("*/truth.json"))
    per = {}
    for bid in ids:
        truth = json.loads((hc / bid / "truth.json").read_text())
        tp = hc / bid / "truth.intent.json"
        tp.write_text(json.dumps(truth_intent(bid, truth, parse_board(root / "boards" / bid / "board.kicad_pcb")[1]), indent=1))
        r = subprocess.run([str(CCOMP), "recognise", "board", str(root / "boards" / bid / "board.kicad_pcb"), "--compare", str(tp), "-o", os.devnull],
                           capture_output=True, text=True)
        if r.returncode:
            print(f"{bid}: {r.stderr.strip()}", file=sys.stderr)
            continue
        c = json.loads(r.stdout)["comparison"]
        per[bid] = c
        (hc / bid / "comparison.json").write_text(json.dumps(c, indent=1))
    # Each board stands for corpus[stratum] / sample[stratum] boards of the corpus.
    sp = hc / "strata.json"
    corpus = json.loads(sp.read_text()) if sp.exists() else None
    weight = {bid: corpus[stratum[bid]] / sample["strata"][stratum[bid]] if corpus else 1.0 for bid in per}
    print(f"{len(per)} boards scored" + ("" if corpus else " (unweighted: handcheck/strata.json, the corpus's boards per stratum, is missing)"))
    tot, wtot = defaultdict(lambda: [0, 0, 0]), defaultdict(lambda: [0.0, 0.0, 0.0])
    for bid, c in per.items():
        for k, s in c["classes"].items():
            for i, f in enumerate(("agree", "wrong", "missed")):
                tot[k][i] += s.get(f, 0)
                wtot[k][i] += s.get(f, 0) * weight[bid]
    pr = lambda a, w, m: (a / (a + w) if a + w else float("nan"), a / (a + m) if a + m else float("nan"))
    print(f"{'class':12} {'agree':>6} {'wrong':>6} {'missed':>6} {'precision':>9} {'recall':>7} {'weighted p/r':>13}")
    for k in sorted(tot):
        p, r = pr(*tot[k])
        wp, wr = pr(*wtot[k])
        print(f"{k:12} {tot[k][0]:6} {tot[k][1]:6} {tot[k][2]:6} {p:9.2f} {r:7.2f} {wp:6.2f}/{wr:.2f}")
    # Currents: does ours land within 2x of the hand figure, and does our peak cover the hand peak?
    within = covered = peaks = total = 0
    worst = []
    for bid, c in per.items():
        truth = json.loads((hc / bid / "truth.json").read_text())["nets"]
        tpk = {n.lstrip("/"): t.get("peak_a") for n, t in truth.items() if isinstance(t, dict)}
        total += sum(1 for t in truth.values() if isinstance(t, dict) and "continuous_a" in t)
        for n, cont, peak, t in c["currents"]:
            ok = t / 2 <= cont <= t * 2 or abs(cont - t) <= 0.02
            within += ok
            if tpk.get(n) is not None:
                peaks += 1
                covered += max(cont, peak or 0) >= tpk[n] * 0.8
            if not ok:
                worst.append((max(cont / t, t / cont) if t and cont else float("inf"), bid, n, cont, t))
    if total:
        print(f"currents: hand figures on {total} nets, ours on {sum(len(c['currents']) for c in per.values())} of them; "
              f"continuous within 2x (or 20 mA): {within}; our peak covers the hand peak: {covered} of {peaks}")
        for r, bid, n, cont, t in sorted(worst, reverse=True)[:12]:
            print(f"  {bid} {n}: ours {cont:g} A, hand {t:g} A")
    blocks = defaultdict(lambda: [0, 0, 0])
    for c in per.values():
        for k, v in c["blocks"].items():
            for i in range(3):
                blocks[k][i] += v[i]
    print("blocks (ours, hand, ours sharing a part with a hand block): " + ", ".join(f"{k} {tuple(v)}" for k, v in sorted(blocks.items())))


def main():
    ap = argparse.ArgumentParser(description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--pcbench", type=Path, default=Path.home() / "Software" / "ultraroute-lab" / "pcbench")
    sub = ap.add_subparsers(dest="cmd", required=True)
    s = sub.add_parser("sheet")
    s.add_argument("ids", nargs="*", help="board ids, or none to read them one per line from stdin")
    sub.add_parser("strata")
    s = sub.add_parser("score")
    s.add_argument("ids", nargs="*")
    a = ap.parse_args()
    if a.cmd == "sheet":
        for bid in a.ids or [line.strip() for line in sys.stdin if line.strip()]:
            print(sheet(a.pcbench, bid))
    elif a.cmd == "strata":
        strata(a.pcbench)
    else:
        score(a.pcbench, a.ids)


if __name__ == "__main__":
    main()
