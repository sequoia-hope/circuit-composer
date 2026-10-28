#!/usr/bin/env bash
# The flame earring, end to end up to routing. Each step reads the last one's output.
set -euo pipefail
cd "$(dirname "$0")"
CCOMP=../../target/release/ccomp
python3 scripts/geometry.py -o geometry.json --svg preview.svg
python3 scripts/circuit.py
rm -f floorplan/reserved.json
python3 scripts/board.py -o build/flame.kicad_pcb
# Rail vias and bypass-cap slots are chosen before the back is placed, and kept clear.
python3 scripts/stitch.py reserve build/flame.kicad_pcb -o floorplan/reserved.json 2>/dev/null | tee build/reserve.json
python3 scripts/board.py -o build/flame.kicad_pcb > /dev/null
$CCOMP floorplan build/flame.kicad_pcb --composed floorplan/composed.json --spec floorplan/spec.json --seed ${SEED:-3} -o floorplan/placement.json > /dev/null || true
python3 -c "import json;r=json.load(open('floorplan/placement.json'));print('floorplan:',r['ok'],r['metrics']['overlap_pairs'][:5], round(r['metrics']['wire_mm']))"
python3 ../../tools/apply_placement.py build/flame.kicad_pcb floorplan/placement.json -o build/placed.kicad_pcb --keep-sides --keep-outline > build/apply1.json 2>/dev/null
python3 scripts/stitch.py place build/placed.kicad_pcb floorplan/reserved.json -o build/stitched.kicad_pcb 2>/dev/null
python3 scripts/planes.py build/stitched.kicad_pcb -o build/unrouted.kicad_pcb 2>/dev/null
