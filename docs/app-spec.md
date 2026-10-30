# Circuit Composer app — spec (draft)

Status: draft, 2026-10-29. Circuit Composer's half of easy_pcb's
[`docs/platform-spec.md`](../../easy_pcb/docs/platform-spec.md). That spec makes easy_pcb the
shell that holds projects, Circuit Composer the app that makes a board's circuit and floorplan,
and UltraRoute the app that routes it. This file says what the Circuit Composer app shows, what
you do in it, what it writes into a project, and how it is built. Questions for easy_pcb and
the owner are under **Open questions**.

## What Circuit Composer is

The owner's direction, 2026-10-30:

**Circuit Composer is a knowledge base that makes composition an algorithm.** An LLM's job is to
go out and gather part data (datasheets, reference designs, application notes) and bring it into
a local database. That database holds enough, in plain structured form, that **classical
algorithms do the composition, the floorplan and the layout**: the bolt earring (LEDs, an
RP2350, a USB-C port, a charger) can be put together in the web app with no language model
involved. Those algorithms could include learned models such as a diffusion placer, but not an
LLM.

The pieces follow from that:
- **The database is the point.** It holds circuit blocks, interfaces, pin functions, support
  parts, rules and their citations: buckets and categories for circuits, encoded so that
  composition is straightforward. The packages, intent schema and research library are its
  first parts.
- **The app works without an LLM.** Everything from composition to a placed board with routing
  constraints runs in the page: in WASM on GitHub Pages, or against a project on disk.
- **An LLM adds power and is not required.** Through the terminal or an MCP server it can:
  - ingest a part the database lacks;
  - fill a gap that needs judgment;
  - answer "why".

  Its output is new database entries or document edits, which the algorithms then use like any
  others.
- **Connections may be stated loosely.** "The sensor connects to the CPU" is enough. When the
  database knows the sensor talks I2C, which of the MCU's pins can be I2C, which address it
  takes and what pull-ups the bus needs, a solver picks the controller and the pins. No LLM is
  needed for that. Where the database doesn't know enough, the field stays *open* (see below),
  and an LLM, or a person, fills it.

## The database composition needs

What the packages hold today (pins by number, function kinds, supply domains, ports as pin
lists, support parts, cited rules) is enough to check a board. It is not yet enough to compose
one from loose connections. For that, each part or package also needs:

| Data | Example | Used by |
|---|---|---|
| **Pin-mux table** | RP2350 GPIO4 can be I2C0 SDA, SPI0 RX, UART1 TX, PWM2 A… (datasheet §9.4) | The pin solver |
| **Interfaces**, as typed bundles of ports | LSM6DSO: an `i2c` target (SDA, SCL; address 0x6A or 0x6B by SA0; 400 kHz), two `irq` outputs, `supply` 1.71–3.6 V | Matching interfaces to controllers |
| **Interface rules** | I2C: one pull-up pair per bus, value by bus capacitance and speed; addresses unique on a bus | Adding the support circuitry a connection implies |
| **Electrical domains** | IOVDD 3.3 V, so a 5 V STAT line needs a divider or shifter | Refusing or fixing incompatible connections |
| **Board-level recipes** | USB-C sink (CC pull-downs, ESD), 1S LiPo with load sharing, an LED chain | Blocks a person names by function |
| **Placement hints and layouts** | already there: `near`, rigid generated layouts, separations | The floorplanner |

The pin solver is a small constraint problem: interfaces to controller instances, signals to
pins, with conflicts, domains and preferences. Preferences include pins on the side facing the
peripheral, which ties it to the floorplan. Exact search is fine at these sizes.

## The parts browser

The database grows through a **parts browser** in the app (owner, 2026-10-30): search for parts,
see what the database already knows about each, and pull more in.

- **Supplier plug-ins.** Each supplier is a plug-in behind one interface: search, part details
  (parameters, package, stock and price, datasheet link), and the supplier's own part number.
  JLC/LCSC comes first, through their APIs, because that's where these boards are made.
  DigiKey, Farnell, Mouser and others follow as plug-ins, so a person can use the supplier they
  prefer. API keys stay on the person's machine (the local server, or Boardroom's settings),
  never in the page or the repository.
- **KiCad's libraries.** Symbols and footprints come from the KiCad libraries installed on the
  machine, and from a library's repository. A part needs both before it can be placed.
- **A board from GitHub.** Point it at a repository: it finds the KiCad boards and schematics,
  lists their parts, and ingests all of them or the ones the person picks. That's the vendor
  eval-board miner from the system spec, with a front end.
- **What ingesting means.** A supplier listing gives parameters and a datasheet. It doesn't give
  the interfaces, pin-mux tables, support parts and rules composition needs (see above).
  - **Structured data** (supplier parameters, footprints, pin tables in KiCad symbols) is taken
    directly.
  - **What has to be read from a datasheet** is the LLM's ingest job: its results go into a
    package with citations, and are checked the way existing packages are. Until they are,
    the browser shows the part as *listed, not composable*, and why.
- **Offline first.** Everything ingested is in the local database. On GitHub Pages the browser
  searches only what the site carries; supplier search needs the local server, because of keys
  and the browser's cross-origin rules.

## What the app produces

For one board in an easy_pcb project, the app produces everything up to routing:

- **the circuit:** a composition of package instances, the board's own parts and the nets
  joining them, and from it the KiCad schematic;
- **the board shape:** outline, holes, cut-outs and keep-outs, from drawing tools and
  parametric generators;
- **the floorplan:** every part placed, each one either locked by a person or left to the
  floorplanner;
- **locked copper:** what must exist before the router runs and that it may not touch.
  That is generated block layouts (a buck's hot loop), reserved vias and rails (the earrings'
  LED rails), and plane and pour assignments;
- **routing constraints:** net classes, `.kicad_dru` rules, and `routing.json` regions and
  length limits.

UltraRoute receives a placed KiCad board with the locked copper on it, plus `routing.json`.
That handoff is the one agreed in milestone 3 and stays as it is. The app does not route, do
silkscreen or make fab outputs.

## What it represents

Everything below exists today as Rust types or files. The app is a view and editor over them,
not a new model.

| Object | Today | Shown as |
|---|---|---|
| Package: manifest, circuit, ports, cited rules, generated layouts | `packages/<name>/`, `compose::Circuit` | Library panel; an instance expands to its package circuit (read only) |
| Composition: instances, board parts, nets as `INSTANCE.PORT` / `REF.PIN` endpoints, no-connects | `compose.json`, `compose::Composition` | Circuit canvas: blocks with ports, nets as wires |
| Intent: net attributes, each with a value, strength and provenance; blocks with placement goals; domains; regions; protection paths | `intent.json`, `intent::model` | Inspector on any selection; Rules table; region overlays |
| Floorplan problem: part geometry, edge connectors, fixed parts, rigid groups, separations, `near` hints, outline (rectangle or polygon), weights | `floorplan.json`, `floorplan::Problem` | Board canvas |
| Placement and its metrics: wire mm, area, overlaps, edge and separation misses | `placement.json`, `floorplan::Placement` | Board canvas and a metrics bar |
| Shape: outline polygon, holes, keep-outs per side | per-board scripts today (`geometry.py`) | Shape canvas, generator sliders |
| Locked copper: block layouts, reserved vias, rails, pours | `blockgen` output, `reserved.json`, `planes.py` | Copper layer on the board canvas |
| Compiled constraints | `routing.json`, `.kicad_dru` (`compile`) | Regions overlay, read-only preview of what UltraRoute will get |
| Findings: validator and verifier results, each with a location and `fix_stage` | `validate`, `verify`, `check package` JSON | Checks panel; clicking one jumps to the place and workspace that fixes it |
| Claims: cited statements from datasheets and experts, with disputes | `library/` | Shown wherever a rule or an AI fill cites one |

## Partly defined: what algorithms fill, and what an LLM fills

Algorithms fill everything the database lets them (rule 1 below). An LLM is asked only about
what stays open, and only when one is connected.

A person should be able to leave things undefined and have them filled in. Every editable
field in the composition, intent, shape and floorplan files is in one of three states:

| State | Meaning | Stored as |
|---|---|---|
| **set** | a person chose it | the plain value, as today |
| **filled** | a tool or the AI chose it | `{"value": …, "by": "ai" \| "preset" \| "floorplan", "why": "…", "cites": [claim ids], "job": "…"}` |
| **open** | nobody has chosen it yet | `{"open": true, "hint": "…"}` |

This extends the provenance the intent schema already carries on every attribute (`Attr`:
value, strength, provenance). It is not a parallel system.

Examples of what may be left open:

- a block given only by its function: "3.3 V buck, 500 mA from a LiPo", with no package
  chosen;
- an instance with no variant;
- a net with one end given loosely: "to the MCU, any free GPIO", with the pin open;
- a net with no class, or a part with no value;
- the board outline. The floorplanner already chooses one when none is set.

Rules:

1. **Deterministic tools fill first.** Presets fill net attributes, the floorplanner places
   unlocked parts, and package defaults fill part values. The AI is asked only for what needs
   judgment: part choice, pin assignment, net classes nobody's preset covers, block layout
   goals.
2. **A tool refuses an open field it needs.** `compose` refuses an instance with no package,
   and the finding names the field. "Fill with AI" sends the open set.
3. **AI fills are reviewable.** Each one shows its `why` and citations. Accepting one makes it
   **set**, and it keeps its provenance. Rejecting one makes it **open** again, and the hint
   records why.
4. **An AI job edits only Composer's files** in the project (`compose/`), marking every change
   `by: "ai"`. The app diffs the files when the job finishes and shows what changed.

An AI request is a file, `compose/<board>/ai/<job>.request.json`. It holds the open fields,
the files that matter, and the constraints in force. The app asks easy_pcb's AI backend to run
it: `claude -p` in the project, with the circuit-composer skill. The result is the edited files
plus `<job>.result.json`, which records what was filled and what was not, and why.

## Workspaces

One window: a library and project tree on the left, the canvas in the middle, the inspector on
the right, and checks and jobs along the bottom. The top bar has the workspace tabs and the
actions **Check**, **Fill with AI** and **Send to UltraRoute**.

### 1. Circuit

A block diagram, not a symbol-level schematic editor. Nodes are package instances, intent
blocks and the board's own parts. Each shows its ports, and edges are nets. You drag packages
in from the library, wire ports together, and leave things open. Open items are drawn dashed,
and AI-filled items are tinted until accepted. Opening an instance shows its package circuit
read-only. The KiCad schematic is an output (`tools/compose_kicad.py`, a box per block). It is
viewed in pcbview, not edited here.

### 2. Board (the main one)

The floorplan canvas. Layers you can toggle:

- outline and keep-outs;
- courtyards and pads, top and bottom;
- a live ratsnest;
- blocks as soft regions, and rigid groups;
- edge constraints, drawn as arrows for the facing direction;
- separations, drawn as distance rings on hover;
- locked copper;
- regions coloured by rule;
- a wirelength heat map.

What you do here:

- **Move parts:** drag, rotate, flip side.
- **Lock a part:** it becomes `fixed` in the floorplan problem and `(locked yes)` in KiCad.
- **Anneal live.** The `floorplan` crate runs in a Web Worker, holding locked parts still, and
  the board updates as it goes. Several seeds can run in parallel workers, so you can pick one
  or keep the best legal one, as `build.sh` does today.
- **Rewind** an anneal and keep an earlier state.
- **Watch the metrics bar:** the `Metrics` struct (wire mm, size, overlaps, misses), with
  deltas.
- **Evaluate** a placement read back from the KiCad board. This is how parts nudged and locked
  in UltraRoute Studio come back.

### 3. Shape

Board outline and mechanical features:

- **Primitives:** rectangle, rounded rectangle, circle, polygon, slot.
- **Operations:** union, difference, offset, round corners. These are the bolt shaper's
  operations.
- **Import** from SVG or DXF.
- **Holes,** with keep-outs per side and layer.
- **Symmetry as a constraint** (mirror or half turn), which pattern generators read.
- **Parametric generators with sliders.** The bolt earring's outline is the first built-in
  one (`crates/shape/src/generators.rs`).

### 4. Copper

Locked copper and routing regions:

- **Generated block layouts.** Place a `blockgen` layout, such as the RP2350 core buck, and see
  its score report against the package's.
- **Pattern generators.** For example, an LED ring field: rings offset from the outline, a
  pitch, partner pairs under a symmetry, rails, and seam rules. They create parts and copper
  together.
- **Reserved vias.**
- **Planes and pours** by layer and domain.
- **Regions** (`must`, `prefer`, `avoid`, `never`) drawn per net class.
- **A preview of the compiled `routing.json`.**

### 5. Rules

A table rather than a canvas:

- nets × attributes, with presets, strength and provenance;
- the domain tree;
- protection paths as ordered lists;
- each value's citations into the library, with disputes shown as disputes.

### Checks (always visible)

Validator, package-check and verifier findings. Each finding has a location and a `fix_stage`,
and clicking it opens the workspace that fixes it.

## Generators

A generator is a parameter schema plus a function. Its output is any of: outline polygons,
parts with poses, copper (tracks, vias, zones), keep-outs and reservations. There are two
kinds, with one output format:

- **Built-in:** Rust in the core, compiled to WASM, fast enough to re-run on every slider move.
  Examples: outline families, offset rings, grids and arrays, symmetric partner pairs.
- **Project scripts:** a script in `compose/<board>/generators/` plus a JSON parameter schema,
  run by the app's server. The earrings' `geometry.py` / `board.py` / `stitch.py` move here
  first, and are rewritten as built-ins when a second board wants them. This is also where the
  AI writes a one-off generator for an unusual board.

## Architecture

The same shape as UltraRoute Studio, so the two apps share habits and code.

- **`app/core`** is Rust compiled to WASM with wasm-bindgen. It depends on the existing
  `compose`, `intent`, `floorplan`, `verify` and `compile` crates. All of these compile for
  `wasm32-unknown-unknown` today: checked 2026-10-29, with no changes.
  - It holds the document model and runs annealing steps in a worker.
  - It turns Composer's objects into the shared board renderer's draw lists (below).
  - It does polygon operations. Offsets and booleans need a Rust crate, which is the first
    decision in milestone A.
- **`app/web`** is Svelte 5 and Vite, as in Studio. Boards are drawn by **one board renderer
  shared with UltraRoute Studio**, so the two apps draw a board identically:
  - a Rust crate that builds meshes and does hit testing;
  - a small JS module with Studio's WebGL2 shader, which cuts each quad to an anti-aliased
    capsule or a fill;
  - text on an overlay canvas;
  - 3D, where needed, through three.js and Threlte, as Waffle Iron does.

  The renderer is **`board-draw`** (working name), its own small MIT repo that both apps pin by
  revision: a git dependency in Cargo, and the JS module the same way. It holds:
  - a scene of layers, capsules, polygons, pads and colours, in board millimetres;
  - the mesh built from the scene;
  - hit testing: the part, net or item at a point;
  - `renderer.js`, with its WebGL2 shader and the text overlay.

  Studio hands it a board from its live-routing model; Composer hands it a floorplan.
  ultraroute extracts it from Studio and moves Studio onto it in the same change, drawing
  identically. Composer uses it from milestone A.
  WebGL2 already runs in headless Chrome and Playwright, Firefox on Linux and phones. Composer
  draws only 2D shapes and runs nothing on the GPU, so there is no WebGPU front end and no
  wgpu. WebGPU can come later behind the same interface, if it ever pays.
- **`app/server`** is Rust with tungstenite, as in Studio. It:
  - serves the page;
  - opens a project by path, and reads and writes `compose/` and the KiCad files it owns;
  - watches those files and pushes changes made by another app or an AI job;
  - runs the tools that need KiCad (`kicad-cli`, the pcbnew Python scripts) through `reserve`
    when they are heavy;
  - hands AI jobs to Boardroom (`POST /api/jobs`, progress on `/api/bus`);
  - answers `api/info`, so the page knows it is not on Pages.

  It takes its port from `--port`, then `$PORT`, then `proj port`, and fails with none.
- **The CLI comes first.** Every action in the app is also a `ccomp` command that takes a
  project path. Claude drives the same commands a person's clicks do, and the app never has
  logic the CLI lacks.

**Windows.** The app runs standalone, at `/?project=<path>&board=<name>` in its own browser
window. It can also be embedded with `&embed=1`, which hides the project chrome and reports its
selection and status to the parent page by `postMessage`. Both modes come from one build.
Standalone is the default, because a canvas app wants the whole screen. Boardroom (the suite's
main app, formerly easy_pcb) opens either: a window of its own in its windows mode, or a tab
inside its window in single-window mode. It reaches Composer through its proxy at
`/apps/composer/`, so every URL the app uses is relative. Pages are served with
`Cross-Origin-Opener-Policy: same-origin` and `Cross-Origin-Embedder-Policy: require-corp`,
because annealing workers will share memory, and the app embeds nothing from other sites.

**Three ways to run, one build:**

| Mode | Served by | Files | AI, terminal | Saving |
|---|---|---|---|---|
| **Suite** | Composer's server, through Boardroom | the project on disk, watched | yes | to the project |
| **Local** | Composer's server alone | the project on disk, watched | through Boardroom's daemon if it is running | to the project |
| **Pages** | GitHub Pages, static files | example boards built into the site | no: those parts of the UI explain why | download a file |

The Pages site is the public demo. The examples are this repository's boards, exported by a
build step into static JSON. Everything that runs in the WASM core works there too: live
annealing, locks, checks and the shape tools. What needs a server is shown, but disabled with
a note: AI fills, the terminal, KiCad output and **Send to UltraRoute**. The app finds out which
mode it is in by asking its server for `api/info`. If no server answers, it is in Pages mode.

## Terminal

Composer has a **composition terminal**: a real, interactive Claude Code session (`claude` in
a pty) in the project folder, loading the circuit-composer skill. It docks along the bottom
beside Checks and Jobs. Boardroom's spec covers how it runs:
- Boardroom's server owns every pty. Each app shows a terminal panel that connects to
  `/api/term/<id>`.
- A terminal outlives its window, as a tmux session does.
- Composer's and Boardroom's terminals are separate sessions, and each can hand the person to
  the other.
- A hook keeps a terminal out of files that an AI job is writing.

Rules for Composer's side:
- The terminal works through `ccomp` commands, so its edits land on the canvas as the
  files change.
- When the person says "this" or "here", Claude runs `ccomp context`. It prints the open board,
  the workspace, the selection, and the open fields under the selection.
- Edits made in the terminal count as the person's. Values the AI chose are still marked
  `by: "ai"`.

**The terminal itself.** In the web app it is a terminal component of our own, written in Rust
compiled to WASM plus a thin JS layer. Radio Room's terminal is the reference for it, not a
dependency, because it is mostly Python and JavaScript. It is shared through Boardroom's
`suite-ui` package. A native build, if one comes (for example Tauri), uses the platform's
native terminal instead. UltraRoute Studio has no terminal.

## Files in the project

Circuit Composer owns `compose/`. Each board has its own subdirectory, because a project can
hold more than one board.

```
compose/
  packages.lock               package name@version per instance: the lockfile
  <board>/
    compose.json              the composition, with open and filled fields
    intent.json               net, block, domain, region and protection intent
    shape.json                outline, holes, keep-outs, generator parameters
    floorplan.json            edges, rigid groups, blocks, separations, weights
    placement.json            current placement; locked parts marked
    copper.json               locked copper and reservations from generators
    generators/               project generator scripts and their parameter schemas
    ai/                       AI job requests and results
    out/                      composed.json, findings.json, routing.json
    written.json              hashes of the KiCad files Composer last wrote
```

Into `hw/<board>/<variant>/` it writes the generated schematic, and the placed, unrouted board
with locked copper, pours and rule areas. It also writes `.kicad_dru` (merged into an existing
one, never overwriting it) and `routing.json` beside the board, so UltraRoute finds it.

**Hand edits are kept.** Before writing a KiCad file, Composer compares it to the hash in
`written.json`. If someone else changed it (a hand edit in KiCad, or Studio moving parts), the
app reads back what it understands, such as poses and locks, and shows the difference. It
doesn't overwrite silently.

## How it maps onto the pcb-design phases

| Phase | In Circuit Composer |
|---|---|
| 1 Architecture and parts | Circuit workspace: blocks and packages |
| 2 Schematic | Generated from the composition |
| 3 Net classification | Rules workspace: `intent.json` (see open question 3) |
| 4 Mechanical and layout brief | Shape workspace: outline, holes, connector edges, sides |
| 5 Stackup and rules | Compiled: net classes, `.kicad_dru`, `routing.json` |
| 6 Placement | Board and Copper workspaces |
| 7 Routing | UltraRoute |

Gates stay easy_pcb's. Composer writes the evidence for gates 2, 3, 5 and 6, which is check
output saved as files, and records it in `design/status.json`.

## Milestones

Lettered, so they don't collide with the numbered system milestones. The prerequisite is that
every `ccomp` command accepts a project path (milestone 3 in easy_pcb's spec).

- **A. Skeleton.** Needs `board-draw` extracted (ultraroute's step). Until it lands, the page
  draws through a placeholder scene with the same shape as `board-draw`'s (layers, capsules,
  polygons, pads, parts with poses), not a mesh format of its own. Also the Pages build with the
  example boards. The server, core and `board-draw` open a project path and show an
  existing floorplan read-only (`rp2350-battery`), with its metrics. Choose the polygon crate
  and port the bolt shaper's operations to it.
- **B. Board workspace.** Drag, lock and anneal live in a worker. Save `placement.json` and
  apply it to the KiCad board through the server.
- **C. Shape workspace.** Done 2026-10-30. `crates/shape` evaluates `shape.json`: features
  (rectangle, circle, slot, polygon, generator) added or cut in order, corner rounding,
  symmetry, holes and keep-outs, and SVG/DXF import. The bolt generator matches
  `geometry.py` to 0.05 mm. The same code is behind `ccomp shape` and, compiled to WASM, the
  Shape tab: a feature list, handles for each kind, and sliders for generators. "Use as board
  outline" gives the floorplanner the outline, with keep-outs, holes and cut-outs as fixed
  keep-out parts, as `ccomp floorplan` does with a board's `shape.json`. The standalone shaper
  page is gone. Still to do: edge connectors on a shaped outline. The floorplanner slides
  connectors along a side only on a rectangle; on any other outline they miss their edge. That
  belongs to the floorplanner work.

- **D. Circuit workspace.** The block diagram of `compose.json`, editable. Compose and check run
  on save.
- **E. Open fields and AI fill.** One open block filled by easy_pcb's backend, reviewed and
  accepted in the app.
- **F. Copper and handoff.** Block layouts, regions, the LED-ring pattern generator, and
  **Send to UltraRoute**. Done when the bolt earring is rebuilt from the app, not from
  `build.sh`.

### Toward composition without an LLM

Separate from the app's milestones A–F, in this order:

1. **Floorplanner work** (in progress; `docs/floorplanning-approaches.md`):
   - the incremental cost (done, about 10×);
   - a benchmark of our boards plus Cypress's (done, `docs/floorplan-benchmark.md`);
   - an analytical placer (done: `--placer analytic`, the app's default start; it beats SA on
     nine of Cypress's ten boards and on three of our four);
   - a legaliser that shoves neighbours, for crowded backs (the flame earring);
   - edge connectors on shaped outlines;
   - diffusion as a starting-point generator.
2. **Interfaces and the pin-mux table** for the four packages (rp2350, lsm6dso, mcp73831,
   tlv62569), ingested from their datasheets and cited, as everything in a package is. Done:
   the types are in `library/interfaces.json`, each package's instances and the RP2350's mux
   table in its `ports.json`; `ccomp interfaces PACKAGE [--interface I2C0]` shows the pins each
   signal can take, and `ccomp library check` checks them.
3. **The pin solver.** The battery demo board written as loose connections ("imu.i2c to mcu",
   "imu irqs to mcu", "a LED to mcu"), solved into a pin-level composition that composes,
   floorplans and passes `ccomp check package`. It runs in `ccomp` and in the app's WASM core.
   Done, but not yet wired into the app's UI: `crates/pinsolve`, `ccomp pins` (`-b rp2350-battery` solves
   `boards/rp2350-battery/loose.json`), and `pins_solve` in the WASM core. It puts the IMU on
   I2C0's GPIO4/5, as the hand-drawn board has it; pinned like that board, it reproduces its
   `compose.json` net for net. Electrical domains and floorplan-side preferences are not in its
   cost yet.
4. **Interface rules and recipes**: I2C pull-ups, USB-C sink, LiPo power path, LED chain. With
   those, the bolt earring composes from its blocks in the browser.
5. **The parts browser**: LCSC/JLC first, then KiCad libraries and GitHub boards, then the other
   suppliers as plug-ins.

## Open questions

1. **Where this repo's boards live.** Decided 2026-10-29: they stay where they are for now
   (`circuit-composer/boards/`). The app therefore treats the *compose root* as a setting:
   `compose/` in an easy_pcb project, `boards/` in this repo, where each board directory keeps
   today's layout. Milestone A opens `rp2350-battery` in place, and milestone F rebuilds the
   bolt earring in place.
2. ~~The AI backend when Composer runs standalone~~ Answered by Boardroom
   (`easy_pcb/docs/shell-spec.md`): one queue. Composer asks Boardroom's daemon, starting it if
   needed. Jobs name their skill and a write scope (`compose/**`, or the board folder here).
   Composer reviews its own jobs field by field, and Boardroom's Accept opens Composer.
3. ~~Who owns net classes~~ Agreed: with `compose/`, `intent.json` is the source and
   `nets.csv` is generated. Boardroom shows it read-only.
4. ~~Naming~~ Agreed: Boardroom calls its file "Requirements".
5. **Rendering.** Decided 2026-10-29: WebGL2 through `board-draw`, shared with Studio, in its own
   repo (see Architecture). Agreed with ultraroute, which does the extraction. Creating the
   GitHub repo: the owner OK'd it on 2026-10-29.
6. **A symbol-level schematic editor** is out of scope. KiCad stays the place for that, and a
   hand-edited schematic is read back as the source for that board (see "Hand edits are kept").
