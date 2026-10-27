---
name: circuit-composer
description: Write, import and validate a board's intent file (the sidecar of net, block, domain, region and protection intent beside a KiCad project) with the `ccomp` CLI. Use when annotating a board for Circuit Composer, importing easy_pcb's nets.csv, or fixing validator findings.
---

# Circuit Composer

`ccomp` (build: `cargo build --release` in this repo, binary `target/release/ccomp`) runs every
stage. Each command prints one JSON object; read it, never scrape text. Exit 0 = success,
1 = validation failed, 2 = could not run.

## Annotate a board

1. Start from what exists. For an easy_pcb project:
   `ccomp import easy-pcb <proj>/design/nets.csv --board <name> --netlist <proj>/hw/<board>/<variant>/<board>.kicad_sch -o boards/<name>/intent.json`.
   Snapshot the netlist into `boards/<name>/input/` with
   `kicad-cli sch export netlist --format kicadsexpr` and point `board.kicad_project` at it.
2. Run `ccomp validate boards/<name>/intent.json`. Fix every finding by its `code`:
   - `net_missing` / `net_missing_field`: add the net or the field its class requires
     (`ccomp classes` lists each class's required fields).
   - `hard_without_provenance` / `unknown_source`: cite a document in `sources` with a page or
     section. If you cannot cite it, make it `soft` with a weight. Never invent a citation.
   - `block_unresolved_type`: use a type from `ccomp classes`.
3. Add what nets.csv cannot say: aggressor and victim nets (from the datasheet's layout section),
   blocks with ports, the domain tree, regions (part-anchored where a datasheet says "under the
   inductor"), protection paths from connector pad to TVS pad, and energy sources.
4. Validate again and write the report: `ccomp validate ... -o boards/<name>/validate.json`.

## Generate and verify a buck block

1. Give the block its roles in the intent file: `vin`, `sw`, `pgnd` and `fb` are the
   regulator IC's pads (`U1.49`); `cin`, `cout` and `inductor` are parts. The validator
   requires all seven for a `buck`.
2. Measure what is there: `ccomp check buck BOARD --intent INTENT --block ID --svg block.svg`.
   Look at the SVG before trusting a number.
3. Generate on a test bench, booking cores (it takes about 30 s on 8):
   `~/.claude/bin/reserve -c 8 -l "buck bench" -- python3 tools/bench_buck.py BOARD ROLES.json OUT`.
   `bench.json` must show `"ok": true` and no `drc_errors`.
4. Compare against the vendor layout with `tools/compare_buck.py`. Every row goes through the
   same checker; never compare a number measured one way against one measured another.

## Compile for the router

`ccomp compile INTENT --board BOARD -o routing.json --dru OUT.kicad_dru --dru-base BOARD.kicad_dru`
writes ultraroute's regions file and KiCad length rules, keeping the board's own rules.
`ccomp check regions BOARD --routing routing.json` lists copper that breaks a region rule. Run it
on a router's output before accepting it.

## Rules

- Net keys are KiCad's exact names, `/GPIO0` not `GPIO0` (the validator suggests the right one).
- A preset default is never evidence: a hard constraint needs a real source.
- If the schema changes, regenerate it with `ccomp schema -o schema/intent.schema.json`. The test
  suite fails if it is stale.
