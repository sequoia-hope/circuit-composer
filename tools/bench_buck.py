#!/usr/bin/env python3
"""Generate a buck block, put it on a test bench, verify it, and run KiCad's DRC on it.

    bench_buck.py BOARD ROLES_JSON OUT_DIR [--params PARAMS_JSON] [--layout GEN_JSON]

OUT_DIR gets: gen.json (the generator's output, unless --layout reuses one), fixture.kicad_pcb
(+ .kicad_pro copied from BOARD's project, so net-class rules hold), check.json, block.svg,
drc.json, and bench.json, the summary printed on stdout.

DRC findings are kept when they touch the block: errors between the block's parts or its
copper. Unconnected items on the anchor's other pins are expected on a bench and left out.
"""
import argparse
import json
import os
import shutil
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CCOMP = os.environ.get("CCOMP", os.path.join(ROOT, "target", "release", "ccomp"))
IGNORED = {"lib_footprint_issues", "lib_footprint_mismatch", "silk_overlap", "silk_over_copper", "silk_edge_clearance", "unconnected_items"}


def run(cmd, **kw):
    r = subprocess.run(cmd, capture_output=True, text=True, **kw)
    return r


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("board")
    ap.add_argument("roles")
    ap.add_argument("out")
    ap.add_argument("--params")
    ap.add_argument("--layout", help="reuse a generator output instead of generating")
    a = ap.parse_args()
    os.makedirs(a.out, exist_ok=True)
    roles = json.load(open(a.roles))
    role_arg = ",".join(f"{k}={v}" for k, v in roles.items())

    gen_path = os.path.join(a.out, "gen.json")
    if a.layout:
        shutil.copy(a.layout, gen_path)
    else:
        cmd = [CCOMP, "gen", "buck", a.board, "--roles", role_arg, "-o", gen_path]
        if a.params:
            cmd += ["--params", a.params]
        r = run(cmd)
        if r.returncode != 0:
            print(r.stdout)
            sys.exit(1)
    gen = json.load(open(gen_path))

    fixture = os.path.join(a.out, "fixture.kicad_pcb")
    pro = os.path.splitext(a.board)[0] + ".kicad_pro"
    if os.path.exists(pro):
        shutil.copy(pro, os.path.join(a.out, "fixture.kicad_pro"))
    r = run([sys.executable, "-I", os.path.join(ROOT, "tools", "apply_layout.py"), a.board, gen_path, a.roles, "-o", fixture, "--fixture"])
    applied = json.loads(r.stdout.strip().splitlines()[-1]) if r.stdout.strip() else {"ok": False, "error": r.stderr[-2000:]}
    if not applied.get("ok"):
        print(json.dumps(applied))
        sys.exit(1)

    r = run([CCOMP, "check", "buck", fixture, "--roles", role_arg, "--svg", os.path.join(a.out, "block.svg"), "-o", os.path.join(a.out, "check.json")])
    check = json.loads(r.stdout)

    drc_path = os.path.join(a.out, "drc.json")
    run(["kicad-cli", "pcb", "drc", "--format", "json", "--severity-all", "-o", drc_path, fixture])
    drc = json.load(open(drc_path))
    block_refs = {roles["vin"].split(".")[0]} | {roles[k] for k in ("cin", "cout", "inductor")}
    findings = []
    for v in drc["violations"]:
        if v["type"] in IGNORED:
            continue
        findings.append({"type": v["type"], "severity": v["severity"], "description": v["description"], "items": [i["description"] for i in v["items"]]})
    block_nets = {check["nets"][k] for k in ("vin", "sw", "gnd", "out")}
    open_in_block = [
        [i["description"] for i in u["items"]]
        for u in drc["unconnected_items"]
        if all(any(f" of {ref} " in i["description"] for ref in block_refs - {roles["vin"].split(".")[0]}) or "Track" in i["description"] or "Via" in i["description"] for i in u["items"])
    ]

    summary = {
        "ok": not findings and check.get("ok", False),
        "measurements": {m["check"]: round(m["measured"], 4) for m in check["measurements"]},
        "estimate": gen.get("estimate"),
        "placed": applied["placed"],
        "drc_errors": findings,
        "open_between_block_parts": open_in_block,
        "seconds": gen.get("seconds"),
        "params": gen.get("params"),
    }
    json.dump(summary, open(os.path.join(a.out, "bench.json"), "w"), indent=2)
    print(json.dumps(summary, indent=2))


if __name__ == "__main__":
    main()
