# app/web → boardroom web/ (Compose mode)

The Circuit Composer page: a Svelte 5 + Vite single-page app that opens a board, shows it as a
block graph (Circuit), a floorplan you drag, lock and anneal live in a Web Worker (Board), and an
outline editor (Shape), with an inspector on the right and a dock of checks along the bottom.
Everything it computes runs in the WASM core built from `app/core` (crate `composer-core`, see
`wasm.md`); there is no server. Size: 3,293 lines of source (`src/App.svelte` 834,
`src/lib/*` 2,418 of which `store.js` 434 is a copy of Boardroom's, `main.js` + `app.css` 41),
not counting the generated `src/core/` (wasm-pack output, gitignored); 941 lines of Playwright
tests (`tests/*.spec.js` + `helpers.js`) and an 80-line `playwright.config.js`. Depends on
(`package.json`): `board-draw` (npm, git pin `b45cc3f`, the WebGL2 renderer; the Cargo side of
`composer-core` pins the same commit), dev: `svelte` 5.57.2, `vite` 8.3.4,
`@sveltejs/vite-plugin-svelte` 7.3.1, `@playwright/test` 1.64.1. Depended on by: Boardroom's web
app, which embeds it in an iframe or opens it in a window (`boardroom/app/web/src/App.svelte:172`,
`COMPOSER` in `boardroom/app/web/src/lib/api.js:29`: `/apps/composer/` through the proxy, or the
Pages site). Built by `app/build.sh` (wasm-pack, then `vite build` into `app/dist/`); published
by `.github/workflows/pages.yml` (the app alone, at the Pages root).

## Model in five minutes

**Entry and state.** `src/main.js` mounts `App.svelte`. All state lives in `App.svelte` as Svelte
runes: `doc` (`App.svelte:276` `makeDoc`: `{ name, board: core Board, problem, outline, box,
parts, rats }`), `circuit` (parts and nets for inspector and graph), `composition` (`{loose,
packages, library}` for a board with `loose.json`), `live` (the latest solve), `shows`
(`'drawn' | 'solved'`) with `other` holding the board put aside, `selection` (indices into the
problem's parts), `metrics`, `project` (set when the board came from a project store). `version`
is bumped on every `refresh()` (`App.svelte:391`) so derived values and `BoardView` redraw; the
core `Board` object is mutable and not reactive by itself.

**Where boards come from** (`src/lib/source.js`):
- *Opened by Boardroom*: `?store=…&pid=…&ws=…&board=…&variant=…[&pcb=…][&embed=1]`.
  `fromLocation` (store.js) makes the store; `loadFromStore` (`source.js:88`) reads
  `compose/<board>/problem.json` (made by Boardroom's snapshot, never by this app) and
  `hw/<board>/<variant>/<board>.kicad_pcb` (or `?pcb=`), and gets parts/nets from the board via
  the core's `kicad_circuit`; poses and locks come from the `.kicad_pcb`
  (`Board.set_placement_from_kicad_pcb`). No selector, no demo board; a missing problem is an
  error (`source.js:91`).
- *Standalone*: `detectMode` (`source.js:27`) fetches `api/info`; a JSON answer means `local`,
  else `pages` on `*.github.io`, else `static`. It lists `examples/index.json` (Composer's
  bundled boards) and, on github.io or with `?boardroom=<snapshot>`, Boardroom's example projects
  from `/boardroom/snapshot/projects.json` (`boardroomBoards`, `source.js:114`), opened through a
  browser store; a board Boardroom has is dropped from Composer's own list (`App.svelte:104`).
- `loadBoard` (`source.js:47`) fetches `examples/<board>/{problem,placement,circuit}.json` and,
  where `index.json` says they exist, `shape.json`, `loose.json`, `packages.json`; plus the shared
  `examples/library.json` once. Fetches retry on 5xx and dropped connections (`retrying`,
  three tries; Pages answers 503 now and then, 4eff0ea).

**The bundled examples** (`public/examples/`, 656 KB, committed) are written by
`app/examples.sh`: for each board `ccomp boards` lists under `boards/`, it runs
`ccomp -b <b> floorplan --problem` (the floorplan problem), copies the saved placement's poses,
composes `compose.json` fresh (so parts carry `instance`/`role`) or takes the board's
`circuit.json` (earrings), writes `circuit.json` = `{parts, nets, instances, floorplan:
{max_net_pins, skip_nets, exclude}}`, copies `shape.json` and `loose.json` where present, bundles
every package that `loose.json`/`compose.json` names as `packages.json` (`spec → {ports,
circuits: {variant → circuit}}` from `packages/<name>/ports.json` and
`schematic/circuit.<variant>.json`), and writes `library.json` from `library/recipes/*.json`,
`library/stock.json`, `library/interfaces.json`. Today: bolt-earring, easy2350, flame-earring,
rp2350-battery.

**Workspaces** (`?view=`; `?ws=` is the store's workspace id, `App.svelte:24`; keys 1–5):
- *Circuit* (`CircuitView.svelte`): parts grouped into blocks (package instance, else floorplan
  block, else circuit block, else designator letters `C*`), edges are nets between groups, rails
  faint. With `loose.json`, `Connections.svelte` solves and composes in the browser on every edit
  (`compose.js:167` `solve` → core `pins_solve` then `compose_board`), keeps the drawn board's
  designators (`designators`), lets you pin a connection's pin, "Drawn pins"/"Free all", and edit
  recipe parameters (`recipe_expand`). Result goes up through `onsolved` → `solvedAgain`
  (`App.svelte:296`).
- *Board* (`BoardView.svelte`): board-draw `Renderer` with three uploaded meshes per frame —
  slot 0 grid, 1 board (`doc.board.mesh`), 2 overlay (blocks, ratsnest, locks, selection, dashed
  box of an unset outline) — and labels on a 2D canvas over it. Drag (only an already selected,
  unlocked part, after 5 px; 0.05 mm snap, Alt for none), R rotate, L lock, F fit, Esc cancels a
  drag or clears, Ctrl+Z undo (100 steps of `poses()`), pinch/pan on touch. "Place unlocked
  parts" / "Refine" run `anneal.worker.js` (`App.svelte:465`): a fresh worker, a fresh core
  `Board` from `problem()` + `poses()`, `anneal(seed, moves, warm, every, cb)` posting snapshots;
  Stop terminates the worker (the anneal is one synchronous WASM call and cannot hear a message),
  keeping the last snapshot unlegalised. Options: start `analytic|sa`, seed, moves (50k–1M;
  boards over 150 parts default to 50k), fill (0–1). Drawn/Solved toggle (`show`,
  `App.svelte:307`) builds the solved board's problem in JS (`solvedProblem`, `compose.js:209`).
- *Shape* (`ShapeView.svelte`, 680 lines, its own 2D canvas, not board-draw): edits a
  `shape.json` document (features add/cut in order, rounding, symmetry, holes, keep-outs,
  generators with sliders, SVG/DXF import), evaluated by core `shape_evaluate` on every edit;
  a synced document (`sync.driver` not `composer`, or `synced`) is locked until "Take over"
  (`shape_take_over`). "Use as board outline" (`applyShape`, `App.svelte:355`) sets the outline
  on both drawn and solved boards and turns keep-outs, cut-outs and holes into keep-out
  rectangles (their bounding boxes, as `ccomp floorplan` does).
- *Copper*, *Rules*: placeholders (`App.svelte:695–718`): Copper prints a paragraph and counts
  rigid groups and locked parts; Rules lists nets by pin count.
- Inspector (`Inspector.svelte`): find by reference/value/block, a part's nets, pose inputs,
  lock, rotate, select locked/free. Dock (`Dock.svelte`): Checks from the floorplanner's metrics
  only (overlap, edge, separation, region misses); "AI jobs" and "Terminal" are static text.

**Saving** (project mode only, `save`, `App.svelte:239`): `doc.board.write_kicad_pcb(text read)`
(core, via ultraroute's `kicad-pcb`) then `store.write(pcb, …, {base: rev read})`; a `Conflict`
offers Reload or Save as a copy (`<board>-2.kicad_pcb`, next free n). Then
`compose/<board>/placement.json` is written with no base (last save wins). `store.watch` reloads
the board when another app changed it and there are no unsaved moves (`heard`, `App.svelte:213`).
Standalone, "Download placement" and the Shape tab's download are the only outputs.

**Store** (`src/lib/store.js`): Boardroom's `app/store/store.js` at 32b8bdd, byte for byte below a
3-line header (checked with diff against `boardroom/app/store/store.js` today: identical below
the header; Boardroom has not changed it since 32b8bdd). API: `read/text/write/history/watch`;
server backend (`/api/projects/<id>/files/`, ETag/If-Match, 412 → `Conflict`) and browser backend
(IndexedDB workspace over a published snapshot, BroadcastChannel as the bus). UltraRoute Studio
has a third copy (`boardroom/vendor/ultraroute/studio/web/src/lib/store.js`), also identical
below its header.

**Embedding.** `?embed=1` hides the header for a slim bar (workspaces, save state, Save). Selection
is posted to `window.parent` as `{source: 'circuit-composer', type: 'select', board, refs}` and
taken from `{type: 'select', refs, board?}` messages (`App.svelte:426–437`).

**Pages and local serving.** Root `index.html` redirects to `app/dist/index.html` and links
`debug.html` (the old status page, 319 lines, local only; the app links it as `../../debug.html`
when not on Pages). Pages deploys only `app/dist/` at the root. Locally `serve.sh` serves the repo
root on `$PORT` or `proj port` (no fallback), so the app is `/app/dist/`. `vite.config.js` uses
`base: './'` so the same build works at `/app/dist/`, on Pages and under `/apps/composer/`.
Playwright's base URL is `$PORT` or `proj url circuit-composer` + `app/dist/`, failing loudly
otherwise (`playwright.config.js:4`); it uses system Chrome at `/usr/bin/google-chrome` and
workers from `$RESERVED_CORES`.

**Invariants a change must keep.** Part indices (`selection`, `refIndex`, `near`, group members,
region members) index the *current* problem's parts and must be remapped whenever the problem is
rebuilt (`indexParts`, `show`). References containing `:` (`keepout:…`, `shape:…`) are keep-outs:
never locked, never counted, never labelled. The anneal must run off the main thread and be
stoppable by terminating the worker. A save must name the revision it read. Every URL is relative.

## Solid

Playwright suite (`npm run test:e2e`, desktop 1440×900 and Pixel 7 projects; last recorded run
passed, `test-results/.last-run.json` dated 2026-11-05, i.e. before 22e9298; 39 tests at
3c6525d). Not run for this note.
- `shell-and-boards.spec.js`: shell visible; every example board loads with no page errors and no
  failed requests except the `api/info` probe; each draws non-blank WebGL2 pixels; Drawn/Solved
  toggle on boards with `loose.json`.
- `workspaces.spec.js`: tabs and keys 1–5 switch and set `?view=`; reload restores; Circuit block
  selection carries into Board; Shape outline applies; dock tabs collapse; `?embed=1` bar; a
  sub-0.001 mm² rounding overlap is not a problem; closing the dock gives the board the room.
- `layers-and-inspector.spec.js`: six layer toggles; find `U4`, inspect, rotate + Ctrl+Z,
  lock/unlock; Select locked / Select free on flame-earring.
- `placement.spec.js`: a 50k-move place updates "% packed"; Stop leaves valid metrics.
- `shape-lock.spec.js`: a synced (Waffle Iron) shape is locked, Unlock warns, Take over keeps
  1850.0 mm² and the cut-out (shape.json injected with `page.route`).
- `mobile.spec.js`: no horizontal overflow, header wraps, tap targets ≥ 32 px, graph labels ≥ 8 px.
- `theme.spec.js`: light/dark tokens and live redraw on scheme change.
The core functions the page calls are tested in Rust (`wasm.md`); the page's own logic is not
unit-tested.

## Known wrong or weak

- **`solvedProblem` (`compose.js:209`) does not remap `problem.regions[].members`** (indices into
  `parts`, added in 22e9298). It rebuilds `parts` in a new order and spreads `...problem`, so a
  board with regions would bind the wrong parts on the Solved board. No bundled example has
  regions yet, so nothing shows it. It also drops every net's `max_mm` and `planar` (sets
  `weight: 1.0` only), which `ccomp floorplan` derives from intent (`floorplan_cmd.rs:324–326`).
- **`solvedProblem` is a JS re-implementation of `ccomp floorplan`'s problem building** (net
  filtering by `max_net_pins`/`skip_nets`/`exclude`, near hints, groups). Two copies of one rule
  that have already drifted (above).
- **The bundled examples are stale.** `public/examples/` was last regenerated at 5038c8b
  (2026-10-31): no `max_mm` or `planar` on any net (added 72bc9d0, 202869c), no regions, and
  `rp2350-matrix`, which `ccomp boards` now lists, is missing. The browser therefore floorplans a
  different problem than the CLI does for the same board. The local `app/dist/` and
  `src/core/*.wasm` are from 2026-11-05, before 22e9298's app changes (unverified whether the
  local Playwright target was rebuilt since).
- **"Fill with AI" and "Send to UltraRoute" have no click handler** (`App.svelte:579–580`). They
  are disabled unless `mode === 'local'`, which nothing can produce since there is no server, so
  today they are always disabled; with a server they would be enabled and do nothing.
- **There is no `app/server`.** docs/app-spec.md's Architecture describes one (tungstenite,
  `api/info`, file watching, jobs, COOP/COEP headers); `source.js:6` says so. The same comment
  says "every mode is pages for now", but off github.io the mode is `static`.
- **app-spec claims the code doesn't meet:** several seeds in parallel workers (one worker);
  Rewind (only a 100-step undo); flip side (no flip; side is read-only in the inspector); Checks
  with validator, package-check and verifier findings and `fix_stage` (floorplan metrics only);
  embed reports "selection and status" (selection only); Copper, Rules, AI jobs, Terminal,
  Supplies and Library workspaces (placeholders or absent); `app/core` depending on `intent`,
  `verify`, `compile` (it depends on `floorplan`, `shape`, `pinsolve`, `compose`, `interfaces`);
  COOP/COEP headers (Pages and `python3 -m http.server` send none). Milestone list and "Files in
  the project" (`compose/<board>/{compose,intent,shape,floorplan,copper}.json`, `written.json`)
  are not implemented by the app: it reads only `problem.json` and the `.kicad_pcb` and writes
  the `.kicad_pcb` and `placement.json`.
- **Project mode ignores the project's shape.** `openProject` uses `shapeFromProblem` and never
  reads a `shape.json` or Boardroom's `interface/` files (`App.svelte:195`); Shape edits are never
  saved to a store, only downloaded, and are lost on board switch (`{#key current}`).
- **Project mode has no Circuit solve**: `composition = null` (`App.svelte:186`); `loose.json` in a
  project is not read.
- **Save is two writes, not one.** The `.kicad_pcb` write is guarded by its revision; the
  `placement.json` write after it has no base and can fail after the board was written.
- **Multi-selection drags only the part under the pointer** (`BoardView.svelte:220` keeps one `i`).
- **`postMessage` uses target `'*'` and `fromParent` checks no origin** (`App.svelte:429,432`):
  any page that frames Composer can set its selection. Boardroom's side does check origin.
- **`?debug=wheel`** (c0a197d) is a debugging aid left in `BoardView` for a Firefox wheel-reversal
  oddity that was never resolved (unverified that it was).
- `examples.sh`'s header says it writes `web/public/examples/`; it writes `app/web/public/examples`.
- The Pages workflow uses `dtolnay/rust-toolchain@stable`, not a pinned toolchain.
- `Dock`'s "legal" sentence omits regions, though region misses are counted.
- `run()` keeps an unused `started` (`App.svelte:468, 491`).
- The Playwright suite was drafted by a Gemini agent and reviewed (3c6525d).

## Not covered by tests

- Everything in project mode: `?store=` opening, `loadFromStore`, Save, `Conflict` → Reload /
  Save as a copy, `heard` reload on another app's change, Ctrl+S, `?pcb=`, Boardroom's example
  projects in the standalone selector (`boardroomBoards`, `?boardroom=`).
- The Connections panel: pinning a pin, Drawn pins / Free all, recipe parameter edits, refusals,
  `drawnPort`.
- `solvedProblem`'s added/dropped/missing parts, groups kept only when whole, start positions of
  new parts; `solvedAgain` swapping boards when the solve changes.
- Dragging a part on the board (mouse or touch), Esc to cancel a drag, pinch zoom, locked-part
  refusal message.
- `postMessage` selection in and out.
- Shape: SVG/DXF import, generators' sliders, holes and keep-outs as keep-out parts after "Use as
  board outline", download; anneal after a shape is applied.
- Download placement; Refine (warm) as distinct from Place; the `sa` start; `fill`.
- 5xx retry in `retrying`.
- `app/examples.sh` itself (no check that its output matches the crates).

## Lessons to keep

- **A press selects; only a selected, unlocked part moves, and only after 5 px** (6341498): the
  owner clicked a part to lock it and moved it. A locked part refuses with a message, and the
  cursor says no-entry.
- **On touch, a finger pans and a tap selects; a part moves only once selected** (5a279b1), so
  panning a dense board does not drag what it starts on.
- **Resize rules** (4eff0ea, 3c6525d): a width change refits a view that still follows the board;
  a height-only change (a panel opening below) keeps the last pressed point where it was on
  screen at the same zoom, else the part tapped jumps away; a height change with nothing pressed
  while auto-fit is on refits, else the board's bottom is cut off on load.
- **A courtyard overlap under 0.001 mm² is not a problem** (3c6525d: flame-earring showed
  "OVERLAP J1 Q2 0.000").
- **Keep-outs are references with `:`**: neither locked nor free (b91708e: Copper had counted 222
  locked parts on flame-earring; 143 are real).
- **Locks live in the board file**: read from the `.kicad_pcb` (a group follows its anchor) and
  written back as `(locked yes)` only where changed, so a board saved with no lock changed is the
  same bytes (5038c8b). A lock made in KiCad reaches Composer.
- **The anneal is one synchronous WASM call**: progress comes by callback every `moves/120`
  moves, and Stop means terminate the worker. A stopped run is not legalised, and the UI says so.
- **Big boards start with 50k moves**, because the analytic start does most of the work
  (`App.svelte:150`); the bolt's 200k-move anneal went from 30.5 s to 2.8 s with the incremental
  cost (f52cb41).
- **Solving in the browser is fast enough to run on every edit**: the bolt's 15 connections solve
  and compose in about 40 ms (22c209f). Solve on open, so Board can show the Solved board before
  Circuit is visited.
- **On rp2350-battery the solved pins cost wire**: drawn placement with solved pins 294 mm vs the
  drawn board's 241 mm; refined, 226 mm (docs/app-spec.md, Toward composition step 4). Judge such
  numbers over several seeds.
- **Ask only for files the index says exist**: Pages 404s showed as console errors and failed the
  clean-load test (3c6525d). Pages also answers 503 at times: retry three times and offer Try
  again (4eff0ea).
- **`?view=` is the workspace; `?ws=` belongs to the store** (697163c). Keep store parameters and
  app parameters in separate names.
- **A board's file need not be named after the board** (be44b06: servodrive's
  `hw/ethernet/e/servodrive_E.kicad_pcb`); take the path from the project, not a pattern.
- **One build, relative URLs** (`base: './'`) served at three paths without change.
- **Phones: controls at least 36 px; the embedded bar's Save button carries the save state as a
  dot** because the status text clipped the last workspace tab at 390 px (ec4928f).
- **Never hardcode a port**: Playwright and `serve.sh` take `$PORT` or `proj`, and fail loudly.

## For the move

- **Destination**: the Compose mode of Boardroom's one Svelte app (`web/`), route
  `/p/<project>/compose`, popped out with `?window=1`. Drop the standalone shell: the header,
  board selector, mode badge, `debug` link, `?embed=1` and its slim bar, `detectMode`/`api/info`,
  and the github.io snapshot discovery (`boardroomBoards`). The project and board come from the
  route; the store is Boardroom's one `store.js` (delete this copy). Selection between windows
  goes on the `BroadcastChannel` the spec names, replacing `postMessage` and its `'*'` origin.
- **WASM**: `src/core/composer_core.js` becomes the one `wasm` crate's package (from `app/core`).
  `board-draw` (npm `b45cc3f` here, `d37b1f8` in Boardroom today) becomes `render`'s JS package;
  pick one revision; Composer's renderer contract is y-down (`new Renderer(canvas, {yDown:
  true})`) and `upload(slot, mesh)`/`draw(slot, view, rect)`.
- **Where the problem comes from**: today `compose/<board>/problem.json` is written by Boardroom's
  snapshot calling `ccomp floorplan --problem`, and the solved board's problem is rebuilt in JS.
  Move problem building (from `crates/ccomp/src/floorplan_cmd.rs`) into the `floorplan` crate (or
  `compose`), expose it through `wasm`, and delete `solvedProblem` from `compose.js`. That fixes
  the region-member and `max_mm`/`planar` drift in one place.
- **Saving**: `write_kicad_pcb` moves to the `kicad` crate; keep the revision check and the
  Reload / Save-as-copy choice; make the board and `placement.json` one call (a server function)
  rather than two writes. Write `compose/<board>/placement.json` under the project's `compose/`
  folder only, as the spec's "each mode writes only its own folder" requires.
- **Examples**: `public/examples/` and `app/examples.sh` go. Sample boards become projects in
  `examples/` (rp2350-battery and rp2350-matrix; easy2350 is already there) and the static build
  (`build:pages`) reads their snapshot. The earrings become their own repos. `library.json`
  (recipes, stock, interfaces) and per-board `packages.json` should be served from `packages/`
  and `library/` by the server, or built into the static snapshot by one Rust step, not Python.
- **Shape**: read and write the project's shape document (and `interface/` for board-interface
  sync) through the store, instead of `shapeFromProblem` and download.
- **Placeholders**: Copper, Rules, AI jobs, Terminal, Fill with AI, Send to UltraRoute. Boardroom
  already has terminals and jobs; wire the dock to those or remove the tabs. "Send to UltraRoute"
  becomes switching to the Route mode on the same project.
- **Tests**: port the seven specs to Boardroom's harness (it uses `playwright-core` scripts, not
  `@playwright/test`); replace `EXAMPLE_BOARDS` (hardcoded four) with the projects in
  `examples/`; base URL from Boardroom's one port. Add project-mode tests first (save, conflict,
  reload) since none exist.
- **What I would do differently**: keep no rule in the page that a crate also has (problem
  building, keep-out rectangles from a shape — `applyShape` repeats `ccomp floorplan`'s); put
  every board edit behind a core call so the CLI and the page share it; and make `App.svelte`
  smaller by moving project load/save into a module, since it holds board switching, solving,
  saving and the toolbar in one 834-line file.
