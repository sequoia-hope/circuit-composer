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
5. Constraint compiler: emits KiCad net classes, rule areas and custom rules, plus router regions.
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

## Supply selection

**Status: proposed 2026-11-07, not yet built.** The goal: a person names the chips and where power comes in, and classical algorithms fill in every regulator between them, with the alternatives shown. Today a loose composition names its regulator instance by hand, and the pin solver's only rule that adds circuitry is the I2C pull-up rule. This adds a second one for power. It needs three things the packages don't yet hold in a matchable form.

**1. Power contracts on ports.** A power port's function in `ports.json` (and a recipe port) gains a `power` object, every figure cited like the rest of the package. `nominal_v` stays as the logic-level solver reads it.

- **`power_in`**: `v`, the recommended operating range (the RP2350's IOVDD, 1.62–3.63 V); `draw_a`, `{typ, max, when}`, at a stated condition (the RP2350 at 150 MHz with both cores running). A port with no cited draw takes the recogniser's part-based figure (`recognise::parts::load`), marked recognised, never hard.
- **`power_out`**: `from`, the input port it is fed from; `topology` (`ldo`, `buck`, `boost`, `buck_boost`, `charger`, `switch`, `diode`); `v_out`, the regulated range (the NCP730's 3.247–3.373 V); `i_max_a`; `vin`, the input range over which it runs (the `from` port's `v`); and how it behaves below regulation: `dropout_v` at a current for an LDO, `r_on_ohm` for a buck at 100 % duty or a switch, `off_below_v` for one that turns off. Optional: `efficiency` at a stated load and input, `iq_a`.
- **`enable`**: the port that turns it on and what to do with it when nothing drives it (`tie: VIN` when the datasheet allows it; the TLV62569's EN must not float).

**2. Sources.** A rail fed from outside the board is a source: a recipe port with `volts` and `i_max_a` (`usb_c_sink`'s VBUS, 4.75–5.5 V, 0.5 A at default USB power), or a connector under the loose composition's `sources` (`"VBAT": {"v": [3.0, 4.2], "kind": "lipo", "via": "J2.1"}`). The ranges are the person's to narrow: a cell cut off at 3.5 V is `[3.5, 4.2]`.

**3. The supply rule.** It runs in the pin solver after the connections are solved, so it sees every consumer. For each rail the loose composition names (`rails`, today `{"+3V3": 3.3}`, which may become `{"v": 3.3, "from": "VSYS", "supply": "tlv62569"}`) that has consumers and no `power_out` on it:

- **Budget.** It sums the rail's draws, typical and maximum. A downstream regulator counts as its output power over its efficiency at its lowest input (or its output current for an LDO). Rails are worked from the loads up, so a chain (USB to a charger to a cell, then a buck) is costed end to end.
- **Candidates.** Every package variant and recipe with a `power_out` whose `v_out` lies inside every consumer's `v` on the rail.
- **Checks, each with its reason when it fails:**
  - its `vin` covers the input rail's whole range;
  - `i_max_a` ≥ the rail's maximum draw;
  - across the input range, the output stays inside every consumer's `v`, counting where it leaves regulation: an LDO follows `VIN − dropout`, a buck at 100 % duty `VIN − I·r_on`. This is the check that decides a LiPo board: whether a 3.3 V regulator is acceptable depends on what its consumers accept at the bottom of the cell, not on whether it stays in regulation. On an RP2350 rail the floor is 3.135 V (VREG_AVDD and USB_OTP_VDD), so an NCP730 (up to 480 mV dropout) fails on a cell taken to 3.0 V, and the result says so;
  - its dissipation against the package's cited θJA and junction limit, when both are cited (otherwise a warning that it wasn't checked).
- **Ranking**, in order: passes every check; has a verified layout for the board's layer count; lower power lost at typical load at the input's nominal voltage (within 10 % counts as a tie); fewer parts; smaller area; every part in stock at the board's assembler. The order is fixed and stated so that a person can see why one option ranks above another.
- **Result.** The best option is added to the solved composition as an instance (`as`, else `reg_<rail>`), its EN handled as its `enable` says, like the I2C rule's `added`. The solver result gains `supplies`: per rail, its budget and every candidate with pass or fail, its reasons, its loss, parts and area. The app reads this. A rail's `supply` in the loose composition pins the choice. A pinned option that fails a check is still placed, with the check as an error.
- **Checking a drawn board.** `ccomp check supplies` runs the same budget and checks against a hand-drawn composition's own regulators, so `compose.json` boards are checked too.

**Out of scope for now:** the rule fills one regulator between named rails; it doesn't invent intermediate rails or power paths (load sharing stays a recipe the person names). Also out of scope: settable outputs (a buck's divider computed for any voltage, as recipes compute values), power sequencing, and modes (a consumer whose need holds only when USB is present).

## Tool interfaces

Every tool is a command-line program that reads files and writes files plus a JSON result. The agent never parses free-form logs.

| Tool | Reads | Writes |
| --- | --- | --- |
| Validator | Intent file, KiCad netlist | Pass/fail with missing or conflicting fields per net and block |
| Recognizer | KiCad netlist, package library | Proposed block types and net classes, each with confidence |
| Block generator | Block intent, package rules, parts | Locked sub-layout (KiCad design block) + score report |
| Eval-board miner | Vendor KiCad or Gerber files, datasheet | Extracted block layout + measured scores, as a template |
| Floorplanner | Intent file, block footprints, mechanical outline or none | Block placements, domain regions, board outline |
| Constraint compiler | Intent file, floorplan | KiCad net classes, rule areas, custom rules file; `routing.json` (regions, length limits, nets kept clear under, widths) |
| Router | Partly routed board, `routing.json`, KiCad rules | Fully routed board |
| Verifier | Routed board, intent file | Per-check results with location, severity and the stage to fix |

**Router handoff.** The router receives a KiCad board with every part placed, critical blocks routed and locked, ground and power pours drawn, keepouts marked, plus `routing.json`, passed to ultraroute as `--regions`. It holds polygon regions per layer, each naming its nets and a mode (`must`, `prefer`, `avoid` with a weight, `never`); `max_lengths` per net; `clear_under`, nets under which no other net's via or hole may open a plane antipad; and `widths`, each net's track width from its current. A width's `min_mm` is a hard rule, routed to like a class width, and is set only from a figure a person or a document stands behind (a hard requirement, datasheet, expert or simulation). Its `opt_mm` is reached after routing by widening tracks in place where there is room, so it never costs a connection, and any figure may set it: a trusted one from its peak, any other (a recognised one included) from its continuous current, since the recogniser's peak is what the source could deliver. Each width names its `basis`, the kind of figure it came from. The same widths go into the `.kicad_dru` as `track_width (min ..) (opt ..)`. Differential-pair skew limits go in the `.kicad_dru` rules, which the router reads. A rigid block arrives as a KiCad group of its parts and its locked copper, pours included: the router holds the parts still and keeps other nets off the group's pours. The router may not move parts or touch locked copper.

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
3. **Router integration.** Constraint compiler emits KiCad rules and `routing.json` regions; router consumes a partly routed board with locked blocks.
4. **RP2350 package.** Mined from the design guide and reference board; family variants; ports.
5. **Floorplanner and composer.** Compose a small board from packages, for example a battery-powered RP2350 board with charger, regulator, IMU and LEDs.
6. **Recognizer v0 and the corpus run.** From a KiCad board or netlist (refdes, values, footprints, pad nets, net names, and part numbers where present; the corpus has none), write an intent file whose every attribute is marked recognised, with a confidence and its evidence. Start with ground and power rails, switch nodes, crystals and clocks, USB and other named pairs, feedback and sense nodes, analog inputs and high-voltage nets. Run it over the PCBench corpus (1182 boards) and hand-check a 50-board sample for precision. Corpus-derived data lives outside this repository until licensing is settled.
7. **Sweep.** Ingest one narrow group from LCSC (synchronous bucks in one common package), then ESP32 and nRF families.
8. **Stretch target.** The farming-robot brushless motor controller: switching supply, three half-bridges, current sensing, digital.

## Decisions log

| Decision | Why |
| --- | --- |
| Router is a separate project | Keeps it general; the handoff is a partly routed board plus KiCad rules and `routing.json` |
| Intent lives in a sidecar keyed to KiCad names | KiCad stays the geometry source of truth; boards still open normally |
| Categories are presets over attributes | New circuit types (LiDAR, TENS, EV inverter) fit without schema changes |
| Critical blocks are generated, not routed | Switching loops are a small search, not an open routing problem |
| Claims keep contradictions | Manufacturer guidance is sometimes wrong; simulation settles disputes |
| Packages are per family, versioned, open source | Ingest once, compose cheaply, shareable on GitHub |
| Router constraints are polygon regions, not cost grids | Smaller, exact at the edges, and what ultraroute built (f179fcb) |
| Recognizer before more hand-annotated boards | Four related boards can't show the schema generalises; the corpus run can |
| One verifier, shared with ultraroute, grown from `crates/verify` | Two checkers drift apart; kept independent of the block generators so a wrong model can't pass its own output |
| Current-sized widths: `min_mm` only from a figure someone stands behind, `opt_mm` from any | The 50-board hand check found recognised currents within 2× on only 59 of 171 nets, mostly low; an `opt_mm` that is wrong costs no connection (ultraroute e54f7e9) |
| A rigid block reaches the router as a KiCad group | No new format: ultraroute already keeps a group's parts still and its pours clear; vendor reference copper (`extract_layout.py`) can travel with it |
| Supplies are chosen by a fixed-order rule over cited power contracts (proposed) | Classical and explainable: every option shows the check it failed, and a person's choice overrides the rank without hiding a failing check |

**Open questions**

- [x] File format for the intent file: JSON, with a JSON Schema generated from the Rust types.
- [ ] Which EM solver for the slow tier (openEMS or other)?
- [x] How the router represents locked copper and constraints: polygon regions, not per-cell grids (`--regions routing.json`, ultraroute f179fcb).
