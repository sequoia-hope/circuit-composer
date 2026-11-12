# Circuit Composer: handoff notes for the move into Boardroom

Written 2026-11-12 by the last Circuit Composer session, for whoever moves this code into
Boardroom (`boardroom/docs/one-system-spec.md`, f936007 or later) and then builds on it. Each note
stands alone; you need nothing from that session's conversation. Git history is not imported:
this repository (github `sequoia-hope/circuit-composer`, 94 commits from 2026-10-27, head 80ac000
when written) stays as the record, and the notes cite its commits by SHA. Commit messages here
are long and carry most of the reasoning, so `git log -- <path>` in this repo is the first place
to look when a note is not enough.

## What Circuit Composer is

A board is built from **packages** (one per chip family: its ports, its reference circuit, its
intent, its rules, sometimes a verified layout), composed into a board, floorplanned, given
locked layouts for its critical blocks, and handed to the router with constraints. The owner's
picture is Lego: pre-computed, validated bricks combined by classical algorithms (see
`Circuit Composer — System Spec.md` at the repo root, which is the design document; its
"Decisions log" table is the short list of why things are as they are).

The intent file (`intent.json`, schema in `crates/intent`) is the shared language: per net a
class, voltage, current, noise, bandwidth, impedance, match group, maximum length, each with a
strength and where it came from; blocks with roles; domains; regions; protection paths. In
Boardroom it becomes the only seam between Composer and the router.

## The pipeline as it runs today

`tools/route_board.sh BOARD OUT_DIR [SEED]` (with `$ULTRAROUTE` = the old router binary):

1. `ccomp pins loose.json` (optional): pin solver picks pins, adds I2C pull-ups and regulators.
2. `ccomp compose compose.json -o composed.json --intent intent.json`: packages → one netlist
   plus the board's intent.
3. `tools/compose_kicad.py`: composed.json → KiCad schematic and board (uses Boardroom's
   `scripts/kicad_write.py`).
4. `ccomp floorplan`: simulated-annealing placement → `placement.json`.
5. `tools/apply_placement.py`, `tools/prepare_route.py` (pcbnew): place parts; stack-up, rules,
   net classes; each rigid block's layout applied, locked, in a KiCad group
   (`tools/apply_layout.py`); decoupling stubs locked.
6. `ccomp compile`: intent → `routing.json` (regions, max lengths, clear_under, widths) and
   `.kicad_dru` rules.
7. ultraroute (old router) with `--regions routing.json`.
8. `tools/verify_routed.py`: KiCad DRC, unconnected, length and region violations.

State when frozen (2026-11-03 onward): rp2350-battery runs end to end with 0 DRC errors and 0
region violations over 4 seeds, 11–13 soft package fails of 63; both its regulators are
generated, locked layouts. rp2350-matrix (RP2350A + 8×8 LED chain) routes clean in 27 s at 6 mm
pitch (b7d66fb).

In Boardroom steps 3, 5 and 6 change: board writing moves to the Rust `kicad` crate, and
`compile` is split up because the new router reads intent itself (see `compile.md`).

## The notes

| Note | Becomes in Boardroom |
|---|---|
| [intent.md](intent.md) | `crates/intent` (gains `Block.group`) |
| [recognise.md](recognise.md) | `crates/recognise` |
| [compile.md](compile.md) | split: `kicad`, the router's reading of intent, `route`/`examine`/`check` |
| [pcb.md](pcb.md) | a module of `crates/board`, API kept |
| [verify.md](verify.md) | the start of `crates/examine` |
| [blockgen.md](blockgen.md) | `crates/blockgen` |
| [compose.md](compose.md), [pinsolve.md](pinsolve.md), [interfaces.md](interfaces.md), [library.md](library.md) | same names |
| [floorplan.md](floorplan.md), [shape.md](shape.md) | same names |
| [wasm.md](wasm.md) | `crates/wasm` (from `app/core`) |
| [web-app.md](web-app.md) | the Compose mode of `web/` |
| [ccomp.md](ccomp.md) | `boardroom compose …` commands over library functions |
| [packages.md](packages.md), [research-library.md](research-library.md) | `packages/`, `library/` data |
| [tools.md](tools.md) | the 24 Python/shell tools: kept, to `bench/`, replaced, gone |
| [boards.md](boards.md) | the eight `boards/`: two designs, fixtures, mined data |

## Working rules written nowhere else

1. **Judge a placement or floorplan change over several seeds, never one.** Placement variance
   swamps single runs: rp2350-battery showed 10–16 package fails across 4 placements of the same
   setup.
2. **Every figure in a package or the library is cited** (source, locator, quote), and
   `tools/library_quotes.py` checks each quote is in the cited document.
3. **A hard limit is set only from a figure someone stands behind** (a requirement, datasheet,
   expert or simulation): a width's `min_mm`, a `never` region, a hard length. Recognised guesses
   set only soft ones (`opt_mm`, `avoid`). Why: the 50-board hand check found recognised currents
   within 2× on only 59 of 171 nets.
4. **Boards and files from others are read in place, never copied in;** their licences are
   recorded (the flight-controller boards are CERN-OHL-S and GPL; PCBench has mixed licences).
5. **Look at the board picture before trusting a table.**
6. **No hardcoded ports, `reserve` for heavy jobs** (the machine rules in `~/.claude/CLAUDE.md`).
7. **The Pages site is the app only;** standalone pages are debug output, linked from
   `debug.html`.

## Decisions that changed on 2026-11-12

- Power sections come from Composer as locked, routed blocks; the router never lays them out and
  only scores their hot loops (buck: input capacitor → switch → ground; boost: output capacitor →
  output). A `routing.json` `blocks` list was agreed with ultraroute that morning and then
  superseded: roles come from `Block.roles` in intent, and `Block.group` names the KiCad group
  holding the locked copper.
- **Check before relying on groups:** the committed routed board
  (`boards/rp2350-battery/routed/rp2350-battery.kicad_pcb`) keeps 65 locked items but has no
  `(group …)` at all, though `prepare_route.py` wraps each block in one. Either the old router
  drops groups when it writes, or the board predates groups; not established. The new `kicad`
  writer and router must round-trip groups, and a test should say so. Group names
  (`core_reg`, `reg`) also don't match intent block ids (`mcu/core_reg`, `reg/reg`); see
  `intent.md`.
- No Antigravity agents for any of this work (owner: "just not good at these things").
