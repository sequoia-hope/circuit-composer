#!/usr/bin/env python3
"""Floorplanner benchmark: our example boards and Cypress's ten small PCB benchmarks (ISPD 2025,
NVlabs/Cypress, Apache-2.0), each placed by every placer at several move budgets and seeds.

    tools/fp_bench.py [--seeds N] [--moves 50000,200000,1000000] [--jobs N] [--only NAME,...]

Cypress's boards are fetched at a pinned commit into bench/cypress/ (not committed) and turned
into floorplan problems with `ccomp floorplan --nets-from-board --outline-from-board --problem`:
every part and pad net from the board, the outline the box its own placement fills (they have
no Edge.Cuts). Their placement is scored as "board" for comparison; per Cypress's authors those
placements were cut from larger boards, so treat them as a reference, not a target.

A run counts as legal only when the floorplanner reports no overlap, edge or separation miss.
Results go to bench/results.json and docs/floorplan-benchmark.md. Wrap it in reserve:
    ~/.claude/bin/reserve -c 12 -l "floorplan benchmark" -- tools/fp_bench.py --jobs 12
"""
import argparse
import concurrent.futures as cf
import json
import os
import statistics
import subprocess
import sys
import urllib.request

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CCOMP = os.path.join(ROOT, "target/release/ccomp")
BENCH = os.path.join(ROOT, "bench")
CYPRESS_SHA = "743b07c4e33dc52a00584d55d2e67f38f70957f5"
CYPRESS = [f"small-{i}" for i in range(1, 11)]
OURS = ["rp2350-battery", "easy2350", "bolt-earring", "flame-earring"]
PLACERS = ["sa"]


def ccomp(*args):
    out = subprocess.run([CCOMP, *args], capture_output=True, text=True, cwd=ROOT)
    try:
        return json.loads(out.stdout)
    except json.JSONDecodeError:
        return {"ok": False, "error": out.stderr.strip() or out.stdout.strip()}


def fetch_cypress(name):
    os.makedirs(os.path.join(BENCH, "cypress"), exist_ok=True)
    path = os.path.join(BENCH, "cypress", f"{name}.kicad_pcb")
    if not os.path.exists(path):
        url = f"https://raw.githubusercontent.com/NVlabs/Cypress/{CYPRESS_SHA}/artifacts/benchmarks/{name}/kicad/{name}_kicad7.kicad_pcb"
        open(path, "wb").write(urllib.request.urlopen(url, timeout=60).read())
    return path


def problems():
    """name -> (problem path, the board's own placement's metrics or None)."""
    os.makedirs(os.path.join(BENCH, "problems"), exist_ok=True)
    out = {}
    for b in OURS:
        out[b] = (os.path.join(ROOT, "app/web/public/examples", b, "problem.json"), None)
    for b in CYPRESS:
        board = fetch_cypress(b)
        p = os.path.join(BENCH, "problems", f"cypress-{b}.json")
        r = ccomp("floorplan", board, "--nets-from-board", "--outline-from-board", "--problem", "-o", p)
        if r.get("error"):
            print(f"cypress-{b}: {r['error']}", file=sys.stderr)
            continue
        own = ccomp("floorplan", board, "--nets-from-board", "--outline-from-board", "--evaluate", board)
        out[f"cypress-{b}"] = (p, own.get("metrics"))
    return out


def run(job):
    name, path, placer, moves, seed = job
    r = ccomp("floorplan", "--input-problem", path, "--moves", str(moves), "--seed", str(seed))
    m = r.get("metrics", {})
    return {"board": name, "placer": placer, "moves": moves, "seed": seed, "ok": bool(r.get("ok")), "seconds": r.get("seconds"),
            "wire_mm": m.get("wire_mm"), "area_mm2": m.get("area_mm2"), "near_mm": m.get("near_mm"), "overlaps": m.get("overlaps"),
            "error": r.get("error")}


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--seeds", type=int, default=5)
    ap.add_argument("--moves", default="50000,200000,1000000")
    ap.add_argument("--jobs", type=int, default=int(os.environ.get("RESERVED_CORES", "4")))
    ap.add_argument("--only", default="")
    a = ap.parse_args()
    subprocess.run(["cargo", "build", "-q", "--release", "-p", "ccomp"], cwd=ROOT, check=True)
    probs = problems()
    if a.only:
        keep = set(a.only.split(","))
        probs = {k: v for k, v in probs.items() if k in keep}
    jobs = [(n, p, pl, int(m), s) for n, (p, _) in probs.items() for pl in PLACERS for m in a.moves.split(",") for s in range(1, a.seeds + 1)]
    results = []
    with cf.ThreadPoolExecutor(a.jobs) as ex:
        for r in ex.map(run, jobs):
            results.append(r)
            print(f"{r['board']:24} {r['placer']} {r['moves']:>8} s{r['seed']} ok={r['ok']} wire={r['wire_mm'] and round(r['wire_mm'])} {r['seconds'] and round(r['seconds'], 2)}s", flush=True)
    sizes = {}
    for n, (p, own) in probs.items():
        pr = json.load(open(p))
        sizes[n] = {"parts": len(pr["parts"]), "movable": sum(1 for q in pr["parts"] if not q.get("fixed")), "nets": len(pr["nets"]), "own": own}
    json.dump({"cypress_sha": CYPRESS_SHA, "sizes": sizes, "results": results}, open(os.path.join(BENCH, "results.json"), "w"), indent=1)
    write_doc(sizes, results, a)


def write_doc(sizes, results, a):
    lines = [
        "# Floorplanner benchmark",
        "",
        "Made by `tools/fp_bench.py` (see its header for how the boards are built). Each cell is over",
        f"{a.seeds} seeds: legal runs / runs, then the median wire (mm) and median time (s) of the legal ones.",
        "\"Board\" is the board's own placement scored the same way: our saved placements for our",
        "boards, and for Cypress's boards the placement they ship (cut from larger boards, per its",
        "authors; overlaps there are courtyard overlaps under our rules).",
        "",
    ]
    budgets = [int(m) for m in a.moves.split(",")]
    for placer in PLACERS:
        head = "| Board | Parts (movable) | Nets | Board's own: wire, overlaps | " + " | ".join(f"{placer} {m // 1000} k" for m in budgets) + " |"
        lines += [head, "|" + "---|" * (4 + len(budgets))]
        for n, sz in sizes.items():
            own = sz["own"]
            own_s = f"{own['wire_mm']:.0f}, {own['overlaps']}" if own else "—"
            cells = []
            for m in budgets:
                rs = [r for r in results if r["board"] == n and r["placer"] == placer and r["moves"] == m]
                legal = [r for r in rs if r["ok"]]
                if legal:
                    cells.append(f"{len(legal)}/{len(rs)}: {statistics.median(r['wire_mm'] for r in legal):.0f} mm, {statistics.median(r['seconds'] for r in legal):.2f} s")
                else:
                    cells.append(f"0/{len(rs)}")
            lines.append(f"| {n} | {sz['parts']} ({sz['movable']}) | {sz['nets']} | {own_s} | " + " | ".join(cells) + " |")
        lines.append("")
    open(os.path.join(ROOT, "docs/floorplan-benchmark.md"), "w").write("\n".join(lines) + "\n")
    print("wrote docs/floorplan-benchmark.md")


if __name__ == "__main__":
    main()
