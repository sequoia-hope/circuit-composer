#!/usr/bin/env bash
# Writes the example boards the app opens with no server behind it (GitHub Pages, or a plain
# static server): for each board in boards/, the floorplan problem (`ccomp floorplan --problem`),
# its saved placement, and its circuit (parts and nets), into web/public/examples/. A board with
# a loose composition (loose.json) also gets it and the packages its instances use, so the Circuit
# workspace can solve and compose it in the browser; the recipes, stock and interface types they
# all share go in examples/library.json. The output is committed, because the earrings' KiCad
# inputs (build/) are not.
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
  # A board with a composition: composed fresh, so its parts carry their instances and roles.
  comp=$(python3 -c 'import json, sys; f = json.load(open(sys.argv[1])); print(f["composition"] if f["exists"]["composition"] else "")' "$OUT/$b/files.json")
  if [ -n "$comp" ]; then $CCOMP compose "$comp" -o "$OUT/$b/composed.json" > /dev/null; fi
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
    fresh = os.path.join(d, "composed.json")
    composed = json.load(open(fresh if os.path.exists(fresh) else files["composed"]))
    if os.path.exists(fresh):
        os.remove(fresh)
    if composed.get("parts") and any(composed["parts"].values()):
        for ref, p in composed["parts"].items():
            parts[ref] = {k: p[k] for k in ("value", "footprint", "instance", "role", "block") if k in p}
        nets = composed["nets"]
        if os.path.exists(files["composition"]):
            comp = json.load(open(files["composition"]))
            instances = {k: ({"recipe": v["recipe"], "params": v.get("params", {})} if v.get("recipe") else {"package": v["package"], "variant": v["variant"]}) for k, v in comp["instances"].items()}
    else:
        circuit = json.load(open(os.path.join(files["dir"], "circuit.json")))
        for ref, p in circuit["components"].items():
            parts[ref] = {"value": p.get("value", ""), "footprint": p.get("footprint", ""), "block": p.get("part")}
        nets = circuit["nets"]
    json.dump({"parts": parts, "nets": nets, "instances": instances}, open(os.path.join(d, "circuit.json"), "w"), separators=(",", ":"))
    # The board's shape document, when it has one; the app makes one from the outline otherwise.
    if os.path.exists(files["shape"]):
        json.dump(json.load(open(files["shape"])), open(os.path.join(d, "shape.json"), "w"), indent=1)
    # The loose composition, and every package it or the composition names: spec -> its ports
    # and its circuit per variant.
    if files["exists"]["loose"]:
        loose = json.load(open(files["loose"]))
        json.dump(loose, open(os.path.join(d, "loose.json"), "w"), indent=1, ensure_ascii=False)
        pkgs = {}
        for src in [loose] + ([json.load(open(files["composition"]))] if files["exists"]["composition"] else []):
            for inst in src["instances"].values():
                if inst.get("recipe"):
                    continue
                name = inst["package"].split("@")[0]
                e = pkgs.setdefault(inst["package"], {"ports": json.load(open(os.path.join(files["packages"], name, "ports.json"))), "circuits": {}})
                e["circuits"][inst["variant"]] = json.load(open(os.path.join(files["packages"], name, "schematic", f"circuit.{inst['variant']}.json")))
        json.dump(pkgs, open(os.path.join(d, "packages.json"), "w"), separators=(",", ":"))
    pr = json.load(open(os.path.join(d, "problem.json")))
    json.dump(pr, open(os.path.join(d, "problem.json"), "w"), separators=(",", ":"))
    index.append({"name": b, "note": files.get("note", ""), "parts": len(pr["parts"]), "nets": len(pr["nets"]),
                  "fixed": sum(1 for p in pr["parts"] if p.get("fixed")), "shaped": pr.get("outline_polygon") is not None,
                  "shape": os.path.exists(files["shape"]), "loose": files["exists"]["loose"]})
json.dump({"boards": index}, open(os.path.join(out, "index.json"), "w"), indent=1)
# What every board's recipes and connections share.
recipes = {}
for f in sorted(os.listdir("library/recipes")):
    if f.endswith(".json"):
        r = json.load(open(os.path.join("library/recipes", f)))
        recipes[r["name"]] = r
json.dump({"recipes": recipes, "stock": json.load(open("library/stock.json")), "interfaces": json.load(open("library/interfaces.json"))},
          open(os.path.join(out, "library.json"), "w"), separators=(",", ":"), ensure_ascii=False)
print(json.dumps(index, indent=1))
PY
du -sh "$OUT"
