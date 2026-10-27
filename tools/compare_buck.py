#!/usr/bin/env python3
"""Measure one buck block on several boards the same way, for a side-by-side comparison.

    compare_buck.py SPEC_JSON OUT_DIR

SPEC_JSON: {"entries": [{"id", "label", "board", "roles": {...}, "note"}], "limits": {...},
"weights": {"hot_loop_area_mm2": 1.0, ...}}. An entry's "not_applicable" maps a
check to the reason its limit does not hold there (it is measured, not judged). Board paths are relative to SPEC_JSON. Writes
OUT_DIR/<id>.svg, OUT_DIR/<id>.check.json and OUT_DIR/compare.json, and prints compare.json.
"""
import json
import os
import subprocess
import sys

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
CCOMP = os.environ.get("CCOMP", os.path.join(ROOT, "target", "release", "ccomp"))


def main():
    spec_path, out = sys.argv[1], sys.argv[2]
    spec = json.load(open(spec_path))
    base = os.path.dirname(os.path.abspath(spec_path))
    os.makedirs(out, exist_ok=True)
    weights = spec.get("weights", {})
    rows = []
    for e in spec["entries"]:
        board = os.path.normpath(os.path.join(base, os.path.expanduser(e["board"])))
        roles = ",".join(f"{k}={v}" for k, v in e["roles"].items())
        cmd = [CCOMP, "check", "buck", board, "--roles", roles, "--block", e["id"], "--svg", os.path.join(out, e["id"] + ".svg"), "-o", os.path.join(out, e["id"] + ".check.json")]
        for check, lim in spec.get("limits", {}).items():
            if check not in e.get("not_applicable", {}):
                cmd += ["--limit", f"{check}{lim}"]
        r = subprocess.run(cmd, capture_output=True, text=True)
        res = json.loads(r.stdout)
        if "measurements" not in res:
            rows.append({"id": e["id"], "label": e["label"], "error": res.get("error")})
            continue
        m = {x["check"]: x for x in res["measurements"]}
        score = sum(w * m[k]["measured"] for k, w in weights.items() if k in m)
        rows.append({
            "id": e["id"],
            "label": e["label"],
            "note": e.get("note", ""),
            "board": os.path.relpath(board, ROOT) if board.startswith(ROOT) else e["board"],
            "measured": {k: round(v["measured"], 3) for k, v in m.items()},
            "passed": {k: v.get("passed") for k, v in m.items() if v.get("passed") is not None},
            "not_applicable": e.get("not_applicable", {}),
            "score": round(score, 3),
            "svg": e["id"] + ".svg",
        })
    result = {"weights": weights, "limits": spec.get("limits", {}), "rows": rows}
    json.dump(result, open(os.path.join(out, "compare.json"), "w"), indent=2)
    print(json.dumps(result, indent=2))


if __name__ == "__main__":
    main()
