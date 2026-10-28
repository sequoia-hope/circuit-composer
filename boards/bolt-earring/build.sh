#!/usr/bin/env bash
# The lightning-bolt earring, end to end up to routing. Each step reads the last one's output.
set -euo pipefail
cd "$(dirname "$0")"
CCOMP=../../target/release/ccomp
python3 scripts/geometry.py -o geometry.json --svg preview.svg
python3 scripts/circuit.py
rm -f floorplan/reserved.json
python3 scripts/board.py -o build/bolt.kicad_pcb
# Rail vias and bypass-cap slots are chosen before the back is placed, and kept clear.
python3 scripts/stitch.py reserve build/bolt.kicad_pcb -o floorplan/reserved.json 2>/dev/null | tee build/reserve.json
python3 scripts/board.py -o build/bolt.kicad_pcb > /dev/null
# Eight seeds in parallel; the legal one with the least wire is kept.
~/.claude/bin/reserve -c 8 -l "bolt earring floorplan" -- bash -c 'for s in 1 2 3 4 5 6 7 8; do ../../target/release/ccomp floorplan build/bolt.kicad_pcb --composed floorplan/composed.json --spec floorplan/spec.json --seed $s -o floorplan/try$s.json > /dev/null & done; wait' 2>/dev/null || true
python3 - <<'PY'
import json, glob, shutil
runs = []
for f in glob.glob("floorplan/try*.json"):
    r = json.load(open(f))
    runs.append((not r["ok"], r["metrics"]["overlaps"], r["metrics"]["wire_mm"], f))
best = min(runs)
shutil.copy(best[3], "floorplan/placement.json")
for f in glob.glob("floorplan/try*.json"):
    import os; os.remove(f)
print("floorplan:", not best[0], best[1], round(best[2]), best[3])
PY
python3 ../../tools/apply_placement.py build/bolt.kicad_pcb floorplan/placement.json -o build/placed.kicad_pcb --keep-sides --keep-outline > build/apply1.json 2>/dev/null
python3 scripts/stitch.py place build/placed.kicad_pcb floorplan/reserved.json -o build/stitched.kicad_pcb 2>/dev/null
python3 scripts/planes.py build/stitched.kicad_pcb -o build/unrouted.kicad_pcb 2>/dev/null
# Routing: ultraroute, In1 as the only plane (the LED rails carry each zone's VLED; In2's pours
# are extra copper, filled after), seeds in parallel; the result is judged by scripts/validate.py.
