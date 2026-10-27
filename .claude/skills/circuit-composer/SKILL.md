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

## Rules

- Net keys are KiCad's exact names, `/GPIO0` not `GPIO0` (the validator suggests the right one).
- A preset default is never evidence: a hard constraint needs a real source.
- If the schema changes, regenerate it with `ccomp schema -o schema/intent.schema.json`. The test
  suite fails if it is stale.
