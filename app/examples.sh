#!/usr/bin/env bash
# Writes the example boards the app opens with no server behind it (GitHub Pages, or a plain
# static server): for each board in boards/, the floorplan problem (`ccomp floorplan --problem`),
# its saved placement, and its circuit (parts and nets), into web/public/examples/. The output is
# committed, because the earrings' KiCad inputs (build/) are not.
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
cargo build --release -q -p ccomp
CCOMP=target/release/ccomp
OUT=app/web/public/examples
rm -rf "$OUT"
mkdir -p "$OUT"
for b in $($CCOMP boards | python3 -c 'import json, sys; print(" ".join(json.load(sys.stdin)["boards"]))'); do
  mkdir -p "$OUT/$b"
  $CCOMP -b "$b" floorplan --problem -o "$OUT/$b/problem.json" > /dev/null
  $CCOMP -b "$b" boards > "$OUT/$b/files.json"
done
python3 - "$OUT" <<'PY'
import json, os, sys

out = sys.argv[1]
index = []
for b in sorted(os.listdir(out)):
    d = os.path.join(out, b)
    if not os.path.isdir(d):
        continue
    files = json.load(open(os.path.join(d, "files.json")))
    os.remove(os.path.join(d, "files.json"))
    # The saved placement, poses only.
    pl = json.load(open(files["placement"]))
    json.dump({"parts": pl["parts"]}, open(os.path.join(d, "placement.json"), "w"), separators=(",", ":"))
    # The circuit: parts with what the inspector shows, and nets by pin. A composed board has it
    # in composed.json (with instances from compose.json); the earrings in circuit.json.
    parts, nets, instances = {}, {}, {}
    composed = json.load(open(files["composed"]))
    if composed.get("parts") and any(composed["parts"].values()):
        for ref, p in composed["parts"].items():
            parts[ref] = {k: p[k] for k in ("value", "footprint", "instance", "role", "block") if k in p}
        nets = composed["nets"]
        if os.path.exists(files["composition"]):
            comp = json.load(open(files["composition"]))
            instances = {k: {"package": v["package"], "variant": v["variant"]} for k, v in comp["instances"].items()}
    else:
        circuit = json.load(open(os.path.join(files["dir"], "circuit.json")))
        for ref, p in circuit["components"].items():
            parts[ref] = {"value": p.get("value", ""), "footprint": p.get("footprint", ""), "block": p.get("part")}
        nets = circuit["nets"]
    json.dump({"parts": parts, "nets": nets, "instances": instances}, open(os.path.join(d, "circuit.json"), "w"), separators=(",", ":"))
    pr = json.load(open(os.path.join(d, "problem.json")))
    json.dump(pr, open(os.path.join(d, "problem.json"), "w"), separators=(",", ":"))
    index.append({"name": b, "note": files.get("note", ""), "parts": len(pr["parts"]), "nets": len(pr["nets"]),
                  "fixed": sum(1 for p in pr["parts"] if p.get("fixed")), "shaped": pr.get("outline_polygon") is not None})
json.dump({"boards": index}, open(os.path.join(out, "index.json"), "w"), indent=1)
print(json.dumps(index, indent=1))
PY
du -sh "$OUT"
