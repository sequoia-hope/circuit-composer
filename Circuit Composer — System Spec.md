# Circuit Composer — System Spec

Oct 27, 2026 · @Sequoia

## Overview

Circuit Composer turns board design into composition: chips are ingested once into verified, published packages, and boards are built by combining those packages. It covers the stage between schematic and routing: constraint definition and physical design planning. The autorouter stays a separate project and consumes this system's output.

**Two modes, one agent front end.**

- **Ingest** (slow, once per chip family): read datasheets, design guides and vendor eval boards; extract cited rules; generate and verify block layouts; publish a package.
- **Compose** (fast, per board): pick packages, connect their ports, floorplan, place, hand off to the router, verify the seams. A missing chip queues an ingest job, like a build system fetching a dependency.

**Toolset.** All tools read and write one shared intent file.

1. Intent spec and validator: the schema; refuses to pass incomplete intent.
2. Recognizer: matches netlist subgraphs to block types and net classes; the agent confirms against datasheets.
3. Block library: generators (buck, half-bridge, crystal, connector protection) plus the eval-board miner.
4. Floorplanner: places blocks as soft regions, separates aggressors from victims, honours the domain tree, outputs board size.
5. Constraint compiler: emits KiCad net classes, rule areas and custom rules, plus router cost grids.
6. Router: separate project; finishes everything not locked.
7. Verifier: fast physics proxies first, full EM simulation last; failures route back to the right stage.

&#91;embedded content: compose pipeline · 7 stages, fed by ingest\]

Ingest fills the package library once; each board then flows top to bottom, and verifier failures loop back to the stage named in the finding.

A command-line harness and a Claude Code skill expose every stage, singly or as a pipeline, with structured results.

**In scope:** two-to-six-layer boards in KiCad; switching power, power distribution, high-speed digital, clocks, precision analog, RF by template, interfaces and protection, general digital.

**Out of scope for now:** busbar and power-module mechanical design, flex and rigid-flex, Altium output, autonomous component selection beyond the ingested library.

## Intent schema

The intent file is a sidecar to the KiCad project, keyed by KiCad net names and reference designators. KiCad stays the source of truth for geometry; the intent file holds what KiCad cannot express. The schema is defined as Rust types with a generated JSON Schema, so the router, the tools and the agent's validator share one definition.

**Every attribute carries three things:** a value, a strength (`hard` or `soft` with a weight), and a provenance (source document, page or timestamp, and whether it is a datasheet claim, an expert claim, or simulation-verified).

**Net attributes.** Categories are presets over these, not the other way round.

| Attribute | Meaning | Example |
| --- | --- | --- |
| `aggressor` | How much noise the net emits: dv/dt, di/dt, edge rate | Switch node: strong |
| `victim` | How sensitive the net is to coupled noise | Feedback node: strong |
| `bandwidth` | Slow-precise vs fast-wideband | TIA output: wideband |
| `voltage` | Peak working voltage, for creepage and clearance | Photodiode bias: 120 V |
| `current` | Normal continuous and peak current | Motor phase: 30 A |
| `fault_current` | Current the copper must survive until protection opens | Battery positive: 200 A |
| `impedance` | Target and tolerance, single-ended or differential | USB: 90 ohm diff |
| `topology` | Point-to-point, daisy chain, star, fly-by | DDR address: fly-by |
| `match_group` | Length or delay matching group and tolerance | RGMII TX |
| `safety` | None, functional, patient-connected, high-voltage | TENS output: patient |

**Blocks.** A block has a type (from the block library), member parts, a placement goal, and ports. A placement goal is an objective plus constraints, for example a buck converter: minimise hot-loop area; switch-node copper at or below the area needed for its current; feedback trace at least a set distance from switch-node copper.

**Domain tree.** Borrowed from chip design's Unified Power Format. The board is a tree: board, then isolation islands, then ground zones, then blocks. Each domain declares its supply nets, its reference ground, its isolation barriers to siblings, and whether it can be powered off.

**Regions.** A region is a polygon (or a floorplanner-assigned soft region) with per-net-class rules: `must`, `prefer` (weight), `avoid` (weight), `never`. Isolation barriers and protected zones are region types with their own clearance rules.

**Ports.** A block's external interface. Each port carries intent: supply rail with voltage, current and noise budget; signal bus with protocol and class; differential pair with impedance.

**Protection paths.** An ordered pad sequence for a connector net: energy must reach the TVS or fuse pad before any branch, with no stub. Energy sources (battery, supply input, large capacitor banks) are flagged; a protected zone bounds all copper upstream of the first disconnect.

**Validation.** The validator refuses to pass a board until every net has its required fields for its class, every block has a resolved type, and every hard constraint has a provenance.

## Package format

A package is a versioned folder for one chip family, published openly on GitHub. Composition only ever reads packages; it never re-reads datasheets.

```
packages/rp2350/
  manifest.toml        family, variants, datasheet revisions, sourcing
  intent.json          domains, required support parts, ports (intent schema)
  rules.json           cited claims scoped to this chip
  layouts/
    qfn60-2layer/      KiCad design block + generator params + verify report
    qfn60-4layer/
  schematic/           reference schematic snippet
  sources/             links and hashes of PDFs, eval boards, design guides
```

**Manifest.** Family name, variant table (package, memory, pin count, differences), datasheet and design-guide revisions the package was built from, and LCSC part numbers with stock and price snapshot dates.

**Intent.** Power domains, required support parts with acceptable value ranges (for example core regulator inductor value and saturation current), interface blocks (USB, crystal, QSPI flash), and ports with full intent.

**Rules.** Claims extracted from the datasheet, hardware design guide and reference board, each cited. Example: core regulator inductor placed adjacent to the VREG pins, oriented as in the reference layout; crystal guard ring to ground.

**Layouts.** One or more solved layouts per package and layer count, each as a KiCad design block or generator parameters, with its verifier report attached (hot-loop area, clearances, return-path continuity).

**Families.** One package per family; variants override only what differs. The RP2350 package covers its QFN-60 and QFN-80 variants; ESP32 and nRF packages follow the same pattern.

**Versioning.** Semantic versions. A datasheet correction or a simulation result that overturns a rule triggers a republish. Boards record the exact package versions they used, like a lockfile.

**RP2350 worked example.** Inputs: the RP2350 datasheet, the hardware design guide PDF, and Raspberry Pi's minimal reference KiCad board. Output: the folder above, with the reference layout mined as the first template and the core regulator as the first verified sub-block.

## Tool interfaces

Every tool is a command-line program that reads files and writes files plus a JSON result. The agent never parses free-form logs.

| Tool | Reads | Writes |
| --- | --- | --- |
| Validator | Intent file, KiCad netlist | Pass/fail with missing or conflicting fields per net and block |
| Recognizer | KiCad netlist, package library | Proposed block types and net classes, each with confidence |
| Block generator | Block intent, package rules, parts | Locked sub-layout (KiCad design block) + score report |
| Eval-board miner | Vendor KiCad or Gerber files, datasheet | Extracted block layout + measured scores, as a template |
| Floorplanner | Intent file, block footprints, mechanical outline or none | Block placements, domain regions, board outline |
| Constraint compiler | Intent file, floorplan | KiCad net classes, rule areas, custom rules file; router cost grids |
| Router | Partly routed board, cost grids, constraints | Fully routed board |
| Verifier | Routed board, intent file | Per-check results with location, severity and the stage to fix |

**Router handoff.** The router receives a KiCad board with every part placed, critical blocks routed and locked, ground and power pours drawn, keepouts marked, plus one cost grid per net class (`must`, `prefer`, `avoid`, `never` as cost values per cell per layer). The router may not move parts or touch locked copper.

**Verifier result format.** Each finding names: the check, the nets or blocks involved, a location, a measured value against its limit, and a `fix_stage` (`intent`, `floorplan`, `block`, or `route`) so the agent knows where to loop back.

**Verifier checks, fast tier** (milliseconds, usable inside placement and routing loops): hot-loop area per switching block; return-path breaks under fast nets; parallel run length between aggressor and victim nets; creepage and clearance by voltage; protection-path order; fault-current copper width.

**Verifier checks, slow tier** (sign-off): field-solver impedance, PDN impedance, EM simulation of flagged regions, thermal estimate.

## Research library

The library is a database of individual claims, not documents. Every claim can be contradicted, tested, and promoted into a rule a tool enforces.

**Sources.** Datasheets and application notes; vendor eval boards; textbooks (for example Henry Ott on electromagnetic compatibility); long-form video talks and interviews, such as Rick Hartley's and Eric Bogatin's talks and Robert Feranec's interview series. Videos are transcribed with timestamps.

**Claim record.**

- Statement in one sentence, in the schema's vocabulary.
- Scope: circuit class, block type, or specific chip.
- Attributes affected (for example `hot_loop_area`, `feedback_clearance`).
- Citation: source, page or timestamp, speaker.
- Status: `asserted`, `contested`, `simulated`, `promoted`, `refuted`.
- Links: supports, contradicts, refines other claims.

**Contradictions are kept.** When a datasheet and an expert disagree, both claims stay, linked as contradicting. The agent sees the dispute instead of a false consensus.

**Settling claims by simulation.** A contested claim becomes an experiment: two layouts differing only in the disputed choice, run through the verifier's slow tier. The result is attached as evidence and the claim's status updates.

**Promotion.** A claim that survives testing becomes generator scoring weight, a verifier check, or a package rule, and keeps its links back to sources and evidence.

**Extraction.** Claude reads each source and proposes claims; a second pass deduplicates against existing claims and links contradictions. Start with switching power supply content, since it feeds the first generator.

## Milestones

Each milestone ends with something verified end to end, not a partial layer across the whole stack.

1. **Intent schema v0.** Rust types, JSON Schema, validator. Hand-annotate one real board from the benchmark corpus; add only fields some tool reads.
2. **One buck regulator, end to end.** Generator (enumerate arrangements, score hot-loop area, switch-node area, feedback clearance), fast-tier verifier checks, comparison against the vendor eval-board layout. Publish as the first package.
3. **Router integration.** Constraint compiler emits KiCad rules and cost grids; router consumes a partly routed board with locked blocks.
4. **RP2350 package.** Mined from the design guide and reference board; family variants; ports.
5. **Floorplanner and composer.** Compose a small board from packages, for example a battery-powered RP2350 board with charger, regulator, IMU and LEDs.
6. **Sweep.** Ingest one narrow group from LCSC (synchronous bucks in one common package), then ESP32 and nRF families.
7. **Stretch target.** The farming-robot brushless motor controller: switching supply, three half-bridges, current sensing, digital.

## Decisions log

| Decision | Why |
| --- | --- |
| Router is a separate project | Keeps it general; the handoff is a partly routed board plus cost grids |
| Intent lives in a sidecar keyed to KiCad names | KiCad stays the geometry source of truth; boards still open normally |
| Categories are presets over attributes | New circuit types (LiDAR, TENS, EV inverter) fit without schema changes |
| Critical blocks are generated, not routed | Switching loops are a small search, not an open routing problem |
| Claims keep contradictions | Manufacturer guidance is sometimes wrong; simulation settles disputes |
| Packages are per family, versioned, open source | Ingest once, compose cheaply, shareable on GitHub |

**Open questions**

- [ ] File format for the intent file: JSON, TOML, or KiCad-style S-expressions?
- [ ] Which EM solver for the slow tier (openEMS or other)?
- [ ] How the router represents locked copper and cost grids internally
