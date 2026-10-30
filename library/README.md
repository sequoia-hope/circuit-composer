# Research library

The research library holds individual claims, not documents. Each claim is one design rule in a
sentence, with:
- its scope (circuit class, block, role),
- the attributes it affects,
- citations with page and quoted text,
- a status: `asserted`, `contested`, `simulated`, `promoted` or `refuted`,
- links that support, refine or contradict other claims.

Disagreements are kept. Both claims stay, linked as contradicting from both sides.

`library/` holds claims about circuit classes. Each package's `rules.json` holds claims about one
chip. Together they are one database: links run between them, and `ccomp library` reads both.

`interfaces.json` is not claims: it defines the interface types (`i2c`, `spi`, `uart`, `pwm`,
`logic`, `analog`, `pio`, `usb`, `swd`, `qspi`) that each package's `ports.json` declares
instances of. `ccomp library check` checks those too.

| Topic | Claims | Contested | Sources |
|---|---|---|---|
| `buck` | 30 | 11 | TI SNVA021C (AN-1149), SLYT614, SNVA803, SNVA638A (AN-2155), SLVA477C, SLTA055; ROHM 60AN066E; Murata SimSurfing MLCC measuring conditions |
| `half_bridge` | 28 | 4 | TI SLVA959B, SLUA618A, SLYT512, SLUP419; Allegro AN296278; Infineon transients in gate-driven power stages; EPC WP010, How2AppNote 007 |
| `crystal` | 25 | 6 | ST AN2867 (Rev 10); Microchip AVR186, AN826, AN943, AN949; TI SWRA495L, SWCA076; Abracon ABM8-272-T3; the RP2350 design guide and minimal board (package sources) |
| `signal` | 30 | 10 | TI SPRAAR7J, SLLA414A, SLOA069, SWPA222A; Microchip AN26.2; Bogatin, Smith and Sandler, "The Myth of Three Capacitor Values"; CompuPhase antiresonance paper |
| `interface` | 30 | 6 | TI SLLA270, SLOA101B, SLLA279A, SLVA680A, SLVAE37, TCAN1042-Q1 and TPS25947 datasheets; Microchip AN228, MCP2542FD datasheet; Olson, IPC-2152 tutorial |

All five topics were extracted on 2026-10-27.

## Disputes on record

| Question | One side | Other side |
|---|---|---|
| Ground under a switch node | `buck.sw.keep_ground_under`: TI measured 6 dBuV/m less EMI margin with it cut | `buck.sw.cut_ground_under_inductor` (ROHM: eddy currents) and `rp2350.vreg.lx_cutout` (RP2350 datasheet) |
| Power ground | `buck.ground.solid_plane`, `half_bridge.ground.single_plane` | `buck.ground.single_point`, `half_bridge.ground.split_power_quiet` |
| Hot loop through vias | `buck.hot_loop.vertical_loop` (TI SNVA803: 2 mm² instead of 20) | `buck.hot_loop.no_vias` (TI SLYT614) |
| Vias per amp; trace width per amp | `buck.vias.per_amp`, `buck.trace.width_per_amp` | `buck.vias.per_200ma`, `buck.trace.width_per_amp_rohm` |
| CIN and COUT grounds | `buck.ground.solid_plane` (together) | `buck.ground.cin_cout_separation` (10–20 mm apart) |
| Motor bulk capacitor | `half_bridge.power.bulk_at_entry` | `half_bridge.power.dc_link_at_bridge` |
| Ground under a crystal | `crystal.ground.solid_plane` (TI; the RP2350A minimal board) | `crystal.ground.local_plane` (ST, Microchip) |
| Crystal case pads | `crystal.case.grounded` | `crystal.case.not_osc_ground` |
| 32 kHz series resistor | `crystal.drive.rext_32k_none` | `crystal.drive.rext_32k_spurious` |
| USB pair spacing | `signal.usb2.spacing_fixed` (0.76 mm) | `signal.usb2.spacing_scaled` (3 × max(width, height)) |
| USB maximum length | `signal.usb2.max_length_ti` (305 mm) | `signal.usb2.max_length_mchp` (150 mm) |
| Plane a fast signal references | `signal.return.ground_reference_only` | `signal.return.any_plane_reference` |
| Decoupling values | `signal.decoupling.mixed_values` | `signal.decoupling.same_values` |
| One capacitor's value | `signal.decoupling.single_cap_max_value` | `signal.decoupling.lower_value_for_hf` |
| CAN terminator on a node | `interface.can.termination_not_on_node` | `interface.can.termination_on_node_allowed` |
| Where the TVS goes | `interface.esd.tvs_near_connector` | `interface.protection.tvs_near_protected_device` |
| Inner-layer trace current | `interface.trace.conservative_chart` | `interface.trace.internal_double_area` |

The switch-node, crystal-ground and decoupling-value disputes can be settled by simulation, as
the spec describes, once the slow tier exists.

## Gaps

These sources could not be read:

| Source | Why |
|---|---|
| ADI AN136, AN139, MT-101, AN-1123 | analog.com timed out |
| TI SLUP230 | needs a login |
| Silicon Labs AN0016, AN0046; Microchip AN2711 | refused (403) |
| NXP AN10853, AN10678, AH1014 | not found (404) |
| Nexperia ESD handbook | refused (403) |
| ST AN2867 Rev 24 | st.com timed out; Rev 10 was read from a mirror |
| IPC-2221 and IPC-2152 | paywalled |

The IPC-2221 gap means **no claim gives clearance by voltage**. Motor-voltage creepage comes from
TI's excerpt of IEC 60664-1 (`half_bridge.clearance.motor_bus`).

No source gives a millimetre value for buck feedback clearance or a crystal keep-out. The CAN
sources predate CAN FD.

## Files

| File | What |
|---|---|
| `sources.json` | Every document: publisher, document number, revision as printed, URL, SHA-256, size, date retrieved |
| `claims/TOPIC.json` | One topic's claims |
| `cache/` | Downloaded documents, git-ignored (the vendors own them) |

A citation names a source in `library/sources.json`, in its own package's sources, or in another
package's as `rp2350/rp2350_hdg`, so each document is listed once. A citation has `locator`
(section and printed page) and one of:
- `quote`: verbatim text of at most 25 words,
- `figure`: a description of a figure or table,
- `measured`: a measurement of a reference board.

## Commands

    ccomp library check                    # ids, statuses, sources, links, both sides of each dispute
    ccomp library query --scope circuit=buck --status contested
    ccomp library query --attribute hot_loop_area
    ccomp library query --id rp2350.vreg   # one claim or an id prefix; adds the links pointing at it
    python3 tools/library_fetch.py         # download into library/cache, compare hashes
    python3 tools/library_quotes.py        # find each quote in its document

`library_fetch.py` exits 1 when a PDF's hash has changed, meaning the vendor has published a new
revision: re-read it and update the claims that cite it. A web article's hash drifts with its
ads, so a change there is reported but does not fail.

`library_quotes.py` searches each page in both pdftotext modes, plain and `-layout`. A quote
found only as its words in order with gaps, because another column is interleaved, is reported
`scattered` for a reader to confirm. At 2026-10-27:
- 392 quotes: 369 found whole, 23 scattered and confirmed by eye (21 of them from ROHM's
  two-column note), none missing.
- Every PDF matched its recorded hash. 68 MB of documents are cached.

## Adding a topic

An agent reads the sources and writes an extraction: `{"topic", "sources", "claims"}` in the shape
above. Then:

    python3 tools/library_import.py EXTRACTION.json --note "..."
    python3 tools/library_fetch.py && python3 tools/library_quotes.py
    ccomp library check

The import refuses a claim id that is already taken, or a source id that names a different
document. The check refuses a contradiction recorded on one side only. When a claim contradicts
a package rule, add the reverse link to the package's `rules.json`, as was done for
`rp2350.vreg.lx_cutout`.
