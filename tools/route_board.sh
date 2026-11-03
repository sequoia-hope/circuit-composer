#!/usr/bin/env bash
# A composed board from its composition to a routed, checked KiCad board:
#
#   compose (+ the board's intent) -> floorplan -> apply_placement -> prepare_route
#   -> ccomp compile -> ultraroute -> verify_routed
#
#   ULTRAROUTE=/path/to/ultraroute tools/route_board.sh BOARD OUT_DIR [SEED]
#
# BOARD is a name under boards/ with a composition, a floorplan spec and compose_kicad.py's board
# (`ccomp -b BOARD boards` lists them); its KiCad project's .kicad_pro is the rules' starting
# point. Every step's output stays in OUT_DIR; verify.json there is the verdict. The router is the
# ultraroute binary named by $ULTRAROUTE, with easy2350's recipe less --diff-pairs, plus --regions. Book cores for it:
#   ~/.claude/bin/reserve -c 2 -l "route BOARD" -- tools/route_board.sh ...
set -euo pipefail
cd "$(dirname "${BASH_SOURCE[0]}")/.."
[ $# -ge 2 ] || { echo "usage: ULTRAROUTE=... $0 BOARD OUT_DIR [SEED]" >&2; exit 2; }
BOARD=$1; OUT=$(realpath -m "$2"); SEED=${3:-1}
[ -x "${ULTRAROUTE:-}" ] || { echo "set ULTRAROUTE to the ultraroute binary" >&2; exit 2; }
CCOMP=target/release/ccomp
cargo build --release -q -p ccomp
mkdir -p "$OUT"
files=$($CCOMP -b "$BOARD" boards)
get() { python3 -c 'import json, sys; print(json.loads(sys.argv[1])[sys.argv[2]])' "$files" "$1"; }
DIR=$(get dir); PCB=$(get pcb); SPEC=$(get floorplan)
PRO=$(ls "$DIR"/kicad/*.kicad_pro | head -1)
NAME=$(basename "$PRO" .kicad_pro)

$CCOMP compose "$(get composition)" -o "$OUT/composed.json" --intent "$OUT/intent.json" > "$OUT/compose.out" \
  || { cat "$OUT/compose.out"; exit 1; }
$CCOMP -b "$BOARD" floorplan --composed "$OUT/composed.json" --intent "$OUT/intent.json" -o "$OUT/placement.json" > "$OUT/floorplan.out" \
  || { echo "floorplan not legal: $OUT/floorplan.out" >&2; exit 1; }
python3 tools/apply_placement.py "$PCB" "$OUT/placement.json" -o "$OUT/placed.kicad_pcb" > "$OUT/placed.out" 2> /dev/null
python3 tools/prepare_route.py "$OUT/placed.kicad_pcb" --intent "$OUT/intent.json" --composed "$OUT/composed.json" --spec "$SPEC" --pro "$PRO" \
  -o "$OUT/route-in" > "$OUT/prepare.out" 2> /dev/null || { cat "$OUT/prepare.out"; exit 1; }
IN="$OUT/route-in/$NAME"
$CCOMP compile "$OUT/intent.json" --pcb "$IN.kicad_pcb" -o "$OUT/route-in/routing.json" \
  --dru "$IN.kicad_dru" --dru-base "$IN.kicad_dru" > "$OUT/compile.out"
# No --diff-pairs: ultraroute's pair tuning (e3d5cce) meanders through `never` regions, and the
# USB pair here (full speed, 12 Mbps) needs no length match.
(cd "$OUT/route-in" && "$ULTRAROUTE" route "$NAME.kicad_pcb" -o "$OUT/routed.kicad_pcb" --strip --negotiate --rip-up \
  --tighten --prefanout --shove-plough region --shove-reach 0.5 --shove-repair 20 --shove-ties \
  --shove-passes 60 --shove-patience 8 --planes In1.Cu,In2.Cu --cell 0.035 --prefanout-order round \
  --regions routing.json --seed "$SEED" --threads 1 --timeout 240 > "$OUT/route.log" 2>&1)
python3 tools/verify_routed.py "$OUT/routed.kicad_pcb" "$OUT/intent.json" "$OUT/route-in/routing.json" \
  --pro "$IN.kicad_pro" --dru "$IN.kicad_dru" --refill -o "$OUT/verify.json" > /dev/null 2>&1 || true
python3 -c 'import json, sys; d = json.load(open(sys.argv[1])); print(json.dumps({k: d[k] for k in ("ok", "unconnected", "drc_errors", "length_violations", "region_violations", "copper")}))' "$OUT/verify.json"
