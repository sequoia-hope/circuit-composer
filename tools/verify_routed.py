#!/usr/bin/env python3
"""Check a routed board against its intent: the seams between the locked blocks and the router.

    verify_routed.py BOARD INTENT ROUTING_JSON [--dru FILE] [-o OUT_JSON]

Runs KiCad's DRC (with the compiled .kicad_dru beside the board, if --dru is given; the board
is copied to a scratch dir first so its project is not touched), `ccomp check regions`, and
`ccomp check buck` on every buck block in the intent. Prints one JSON summary.
"""
import argparse
import collections
import json
import os
import shutil
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CCOMP = os.environ.get("CCOMP", os.path.join(ROOT, "target", "release", "ccomp"))
EXPECTED = {"lib_footprint_mismatch", "lib_footprint_issues"}


def ccomp(*args):
    r = subprocess.run([CCOMP, *args], capture_output=True, text=True)
    return json.loads(r.stdout)


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("intent")
    ap.add_argument("routing")
    ap.add_argument("--dru")
    ap.add_argument("--refill", action="store_true", help="refill zones with pcbnew first (a router leaves pours for KiCad)")
    ap.add_argument("-o", "--output")
    a = ap.parse_args()

    work = tempfile.mkdtemp(prefix="cc-verify-")
    stem = os.path.splitext(os.path.basename(a.board))[0]
    board = os.path.join(work, stem + ".kicad_pcb")
    shutil.copy(a.board, board)
    pro = os.path.splitext(a.board)[0] + ".kicad_pro"
    if os.path.exists(pro):
        shutil.copy(pro, os.path.join(work, stem + ".kicad_pro"))
    if a.dru:
        shutil.copy(a.dru, os.path.join(work, stem + ".kicad_dru"))
    if a.refill:
        code = "import pcbnew,sys; b=pcbnew.LoadBoard(sys.argv[1]); pcbnew.ZONE_FILLER(b).Fill(b.Zones()); b.Save(sys.argv[1])"
        subprocess.run([sys.executable, "-I", "-c", code, board], capture_output=True, check=True)

    drc_path = os.path.join(work, "drc.json")
    subprocess.run(["kicad-cli", "pcb", "drc", "--format", "json", "--severity-all", "-o", drc_path, board], capture_output=True)
    drc = json.load(open(drc_path))
    types = collections.Counter(v["type"] for v in drc["violations"] if v["type"] not in EXPECTED)
    lengths = [v["description"] + " " + " / ".join(i["description"] for i in v["items"][:1]) for v in drc["violations"] if v["type"] == "length_out_of_range"]
    unconnected = collections.Counter()
    for u in drc["unconnected_items"]:
        d = u["items"][0]["description"]
        unconnected[d.split("[")[1].split("]")[0] if "[" in d else "?"] += 1

    regions = ccomp("check", "regions", board, "--routing", a.routing)
    stats = collections.Counter()
    try:
        tracks = subprocess.run([sys.executable, "-I", "-c", "import pcbnew,sys,json; b=pcbnew.LoadBoard(sys.argv[1]); print(json.dumps({'tracks': sum(1 for t in b.Tracks() if t.GetClass()=='PCB_TRACK'), 'vias': sum(1 for t in b.Tracks() if t.GetClass()=='PCB_VIA'), 'locked': sum(1 for t in b.Tracks() if t.IsLocked())}))", board], capture_output=True, text=True)
        stats.update(json.loads(tracks.stdout.strip().splitlines()[-1]))
    except Exception:
        pass
    intent = json.load(open(a.intent))
    bucks = {}
    for bid, b in intent.get("blocks", {}).items():
        if b.get("type") == "buck":
            r = ccomp("check", "buck", board, "--intent", a.intent, "--block", bid)
            bucks[bid] = {m["check"]: round(m["measured"], 4) for m in r.get("measurements", [])} or r

    summary = {
        "ok": not unconnected and not types and regions.get("violations", 1) == 0,
        "unconnected": dict(unconnected),
        "drc_errors": dict(types),
        "length_violations": lengths,
        "region_violations": regions.get("violations"),
        "region_findings": [f for f in regions.get("findings", []) if f["violation"]][:20],
        "region_prices": collections.Counter(f"{f['region']}:{f['kind']}" for f in regions.get("findings", []) if not f["violation"]),
        "bucks": bucks,
        "copper": dict(stats),
    }
    shutil.rmtree(work, ignore_errors=True)
    if a.output:
        json.dump(summary, open(a.output, "w"), indent=2)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
