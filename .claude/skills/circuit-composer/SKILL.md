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

## Measure interface blocks (crystal, USB, QSPI, decoupling)

Write a spec naming pad-to-pad paths (`{"name", "from": "U1.21", "to": "Y1.1"}`), nets to check
for ground beside them (`return_paths`), and the part whose power pins need decoupling, then run
`ccomp measure BOARD --spec SPEC.json -o measured.json`. Return-path results use the fills as
KiCad last saved them: refill a routed board first. Pin numbers differ between packages of one
family; look them up by name in the package's `ports.json`.

## Judge a board against a package

Write a binding: `{"package": "rp2350@0.2.0", "variant": "RP2350A", "chip": "U1", "parts":
{"CIN": "C14", ...}}`, mapping each package part name (the names in the package's
`intent.json` blocks) to the board's reference. Bind only parts the board really has; leave the
rest out and they are reported `unbound`. Then run `ccomp check package BOARD --package DIR
--binding BINDING.json -o check.json`.
- A `wiring` failure means the board joins a pin to a different net than the package.
- `unverifiable` means the board cannot show the value (a crystal valued "12MHz").
- `ok` is false only on a hard failure.

Refill zones before judging a routed board.

## Compose a board from packages

A composition (`compose.json`) has:
- `instances`: name → `{package: "rp2350@0.2.0", variant}`;
- `parts`: the board's own parts by designator, `{symbol, value, footprint}`;
- `nets`: name → endpoints, each `INSTANCE.PORT` or `REF.PIN`.

A package's ports and internal nets are in its `schematic/circuit.VARIANT.json`. Run
`ccomp compose compose.json -o composed.json --bindings DIR`. It refuses:
- a ground or supply port left unconnected;
- a pin on two nets;
- an unknown port.

An open signal port is listed; if that is intended, put it in `no_connect`.

Departures from the package must be stated per instance, never hidden:
- `omit`: roles not fitted;
- `parts`: a role's part changed (value, footprint, LCSC);
- `reversed`: a two-pin part fitted the other way round;
- `join`: internal nets an omitted link part separated;
- a tap: a board net naming an internal net, such as `mcu.DVDD` for a test point.

To recompose an existing board, use `tools/compose_from_netlist.py draft` with
`--keep-designators`, then check the result with `compare`. It must match.

## Write and floorplan a composed board

1. Check the package circuits against the KiCad the files are written with:
   `python3 tools/circuit_check.py packages/*/schematic/circuit.*.json --kicad 9`. Versions
   rename pins: the USB-C shield is S1 in 9 and SH in 10.
2. Write the project: `python3 tools/compose_kicad.py composed.json OUT_DIR [--lib BOARD_LIBS]`.
   It writes the schematic, board and library tables, and runs ERC. It refuses a net pin the
   symbol lacks. `nets_match` must be true.
3. Write a floorplan spec:
   - `outline`, if the mechanical design sets one;
   - `fixed` and `back` parts;
   - `edges`, with the side and the way the connector's mouth faces at rotation 0, read off
     the footprint;
   - `rigid` generated layouts (`layout.json` plus roles);
   - `blocks` for the board's own parts;
   - `separations`, taken from package rules.
4. Run `ccomp floorplan BOARD --composed composed.json --spec SPEC -o placement.json`. Try
   several `--seed`s; `ok` needs no overlaps, edge misses or separation misses.
5. Compare with a person's board using `--evaluate`. Apply with `tools/apply_placement.py`,
   which also runs KiCad's DRC.
6. Look at a render: the metrics have missed things before (a connector placed off the
   board).

## Consult and extend the research library

Before choosing a layout rule or a limit, look it up:

    ccomp library query --scope circuit=buck
    ccomp library query --attribute hot_loop_area
    ccomp library query --text "guard ring"

Each claim carries its citations (page and quote), status and links. A `contested` claim has a
contradicting claim; read both and say which you follow and why.

To add a topic:
1. Write an extraction in `library/README.md`'s shape: claims, not documents, each with a quote
   of at most 25 words that is really in the source.
2. Run `python3 tools/library_import.py X.json`.
3. Run `python3 tools/library_fetch.py`, then `python3 tools/library_quotes.py`, then
   `ccomp library check`.

A missing quote means the claim is wrong until shown otherwise. A contradiction must be
recorded on both claims, including a package's `rules.json`.

## Compile for the router

`ccomp compile INTENT --pcb BOARD -o routing.json --dru OUT.kicad_dru --dru-base BOARD.kicad_dru`
writes ultraroute's regions file and KiCad length rules, keeping the board's own rules.
`ccomp check regions BOARD --routing routing.json` lists copper that breaks a region rule. Run it
on a router's output before accepting it.

## Rules

- Net keys are KiCad's exact names, `/GPIO0` not `GPIO0` (the validator suggests the right one).
- A preset default is never evidence: a hard constraint needs a real source.
- If the schema changes, regenerate it with `ccomp schema -o schema/intent.schema.json`. The test
  suite fails if it is stale.
