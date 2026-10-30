# Circuit Composer app — spec (draft)

Status: draft, 2026-10-29. Circuit Composer's half of easy_pcb's
[`docs/platform-spec.md`](../../easy_pcb/docs/platform-spec.md). That spec makes easy_pcb the
shell that holds projects, Circuit Composer the app that makes a board's circuit and floorplan,
and UltraRoute the app that routes it. This file says what the Circuit Composer app shows, what
you do in it, what it writes into a project, and how it is built. Questions for easy_pcb and
the owner are under **Open questions**.

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

## Partly defined: the AI-forward model

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
- **Parametric generators with sliders.** `boards/bolt-earring/shaper/` is the prototype and
  becomes the first built-in one.

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
  - hands AI jobs to easy_pcb's backend.

  It takes its port from `--port`, then `$PORT`, then `proj port`, and fails with none.
- **The CLI comes first.** Every action in the app is also a `ccomp` command that takes a
  project path. Claude drives the same commands a person's clicks do, and the app never has
  logic the CLI lacks.

**Windows.** The app runs standalone, at `/?project=<path>&board=<name>` in its own browser
window. It can also be embedded with `&embed=1`, which hides the project chrome and reports its
selection and status to the parent page by `postMessage`. Both modes come from one build.
Standalone is the default, because a canvas app wants the whole screen.

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

- **A. Skeleton.** Needs `board-draw` extracted (ultraroute's step); the server and core can
  start before it lands. The server, core and `board-draw` open a project path and show an
  existing floorplan read-only (`rp2350-battery`), with its metrics. Choose the polygon crate
  and port the bolt shaper's operations to it.
- **B. Board workspace.** Drag, lock and anneal live in a worker. Save `placement.json` and
  apply it to the KiCad board through the server.
- **C. Shape workspace.** Primitives, operations and the bolt generator built in. A drawn
  outline feeds the floorplanner.
- **D. Circuit workspace.** The block diagram of `compose.json`, editable. Compose and check run
  on save.
- **E. Open fields and AI fill.** One open block filled by easy_pcb's backend, reviewed and
  accepted in the app.
- **F. Copper and handoff.** Block layouts, regions, the LED-ring pattern generator, and
  **Send to UltraRoute**. Done when the bolt earring is rebuilt from the app, not from
  `build.sh`.

## Open questions

1. **Where this repo's boards live.** Decided 2026-10-29: they stay where they are for now
   (`circuit-composer/boards/`). The app therefore treats the *compose root* as a setting:
   `compose/` in an easy_pcb project, `boards/` in this repo, where each board directory keeps
   today's layout. Milestone A opens `rp2350-battery` in place, and milestone F rebuilds the
   bolt earring in place.
2. **The AI backend when Composer runs standalone.** Composer can either require the easy_pcb
   shell to be running, or run `claude -p` itself with the same request file. The proposal is
   to require the shell: one queue, one place to see jobs.
3. **Who owns net classes?** In an easy_pcb project, `design/nets.csv` is phase 3's output, and
   `ccomp import easy-pcb` turns it into `intent.json`. Once a project has `compose/`, the
   proposal is that `intent.json` is the source and `nets.csv` is generated from it for the
   skill's checks. Two hand-edited copies would drift.
4. **Naming.** easy_pcb's `design/intent.md` holds requirements, and Composer's `intent.json`
   holds net and block intent. Both are called "intent". Consider calling the easy_pcb file
   *requirements* in the UI, even if the filename stays.
5. **Rendering.** Decided 2026-10-29: WebGL2 through `board-draw`, shared with Studio, in its own
   repo (see Architecture). Agreed with ultraroute, which does the extraction. Creating the
   GitHub repo: the owner OK'd it on 2026-10-29.
6. **A symbol-level schematic editor** is out of scope. KiCad stays the place for that, and a
   hand-edited schematic is read back as the source for that board (see "Hand edits are kept").
