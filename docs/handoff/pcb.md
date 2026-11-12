# pcb → boardroom crates/board (as a module, `board::pcb` or similar, keeping this API)

`pcb` is the board as Composer's checks and generators see it: parts, pads with nets and copper
outlines, tracks, vias and zone fills per net, in **millimetres**, KiCad's frame (Y down), plus a
small plane-geometry library (`geom`) and the shortest copper path between two pads (`graph`).
It reads nothing itself: it wraps ultraroute's `kicad-pcb` crate (git dependency, rev
`0e581c467fd43013b60c1c2ddab490e671851bee`, 2026-10-30, which works in **metres**) and converts.
Size: 1,108 lines in `src/` (lib.rs 395, geom.rs 344, graph.rs 369), of which about 300 are unit
tests; no `tests/` directory. Depends on `kicad-pcb` and `serde`. Depended on by `verify`,
`blockgen`, `compile`, `recognise` and `ccomp` (Cargo.toml). `app/core` does not use `pcb`; it
uses `kicad-pcb` directly at the same rev (parse + `kicad_pcb::place`). Call sites
(`grep pcb::`): verify 23, compile 10, blockgen 5, recognise 1 (= 39), ccomp 19; plus about 35
`.pad("REF.PIN")` calls.

## Model in five minutes

- `Board` (`src/lib.rs:15`): `layers` (copper names, front to back), `parts: BTreeMap<ref, Part>`,
  `pads: Vec<Pad>`, `tracks`, `vias`, `fills`, `rules: Option<kicad_pcb::Rules>` (serde-skipped),
  `outline: Option<Vec<P>>` (outer Edge.Cuts loop, arcs as chords within 5 µm).
- `Part` (`lib.rs:34`): reference, value, footprint (library id), `at`, `rotation` (deg),
  `back`, `locked`, `courtyard` (convex hull, board frame), `pads` = **indices into
  `Board::pads`**.
- `Pad` (`lib.rs:49`): part, pin (pad number), net name (`""` for none), copper `layers`,
  `centre`, `shape: Vec<Vec<P>>` (a union of polygons; circles as 24-gons).
- `Track` (`lib.rs:60`), `Via` (`lib.rs:70`, `layers` = every copper layer the barrel spans),
  `Fill` (`lib.rs:80`, one filled polygon on one layer, as KiCad last computed it).
- Entry points: `Board::load(path)` (`lib.rs:93`) reads the board and `Rules::read` of the
  `.kicad_pro` beside it (which also applies a `.kicad_dru` beside it). `Board::parse(text)`
  (`lib.rs:106`), `Board::pad("U1.49")` (`lib.rs:200`), `Board::net_copper(net)` (`lib.rs:207`,
  every shape of a net per layer; vias as 16-gons, tracks as capsules),
  `Board::clearance(a, b)` (`lib.rs:102`, mm, `None` without a project file).
- `graph::copper_path(board, net, "A.1", "B.2") -> Option<Path>` (`src/graph.rs:42`): Dijkstra
  over a graph of pad nodes (one per layer), track ends, via nodes (one per layer), with joins:
  track end in/at a pad, track end within a via's radius, track ends/middles whose centrelines
  come within the half-widths' sum, a track passing through a pad, a via in a pad, stacked vias
  of different spans on a shared layer, and **every node inside a fill joined to every other in
  a straight line**. A pin with several pads is one terminal. `Path { points, layers, length }`.
- `geom` (`src/geom.rs`): `P = [f64; 2]`; `rotate` (KiCad's sign: positive = CCW on screen),
  `loop_area` (integral of |winding number| on a ~`cells`-per-side raster), `union_area` /
  `overlap_area` (raster), `poly_poly_dist`, `polyline_poly_dist`, `capsule`, `circle`, `hull`,
  `convex_overlap` (SAT), `inside` (even-odd), `winding`.

What of `kicad-pcb` it uses: `parse_board` → `(Pcb, Copper)`; `Pcb.nets` (number, name),
`Pcb.footprints` (reference, value, library_id, at, rotation_deg, side, pads: number, net,
position), `Pcb::outline_loops().outer.points(tol)`; `Copper.layers`, `locked_footprints`,
`courtyards`, `pads` (footprint index, pad index, layers, outline `Circle|Polygons`),
`Copper::expand` (wildcard layers to copper names), `tracks`, `arcs` (start, mid, end),
`vias` (`layers[0..2]` as the two end layers), `zones` (keepout, net, net_name, filled);
`Rules::read`, `Rules::clearance`, `Rules::min_clearance`; `Side`. `app/core` also uses
`kicad_pcb::place::{apply, Placement}` and `kicad_pcb::copper::Outline`.

Invariants a change must keep:
- Units mm, Y down, KiCad rotation sign (`geom::tests::kicad_rotation`).
- `Part.pads` index `Board::pads`; a pad is only kept if it has a copper layer.
- A via's `layers` is the contiguous span between its two end layers in copper order (a
  stacked/blind via is a short span; unknown layer names fall back to all layers).
- Fills exclude keepout zones; each `Fill` is one polygon (no holes field: KiCad's fills come
  as fractured outlines).
- `copper_path` must treat the four joins its tests name as joined (below); the published
  package numbers in `verify` depend on them to 1e-3.

## Solid

All pass (`cargo test -p pcb`, 14 tests, run 2026-11-12 on booked cores):
- Parsing: `tests::exposed_mask_segment_layers_parse`, `tests::paste_only_pads_are_ignored`,
  `tests::kicad10_zone_fills_preserve_net_name`.
- Geometry: `geom::tests::square_area`, `distances`, `figure_eight` (crossing loops are not
  cancelled), `sat`, `kicad_rotation`, `capsule_area`.
- Copper path: `graph::tests::track_through_a_pad_joins_it`, `overlapping_tracks_join` (0.099 mm
  joins, 0.101 mm does not, for two 0.1 mm tracks), `via_in_pad_joins_it`,
  `a_pin_with_several_pads_is_one_terminal`, `stacked_hdi_vias_join_across_layers`.
- Indirectly: `verify`'s `tests/package_layouts.rs` loads seven real fixtures through
  `Board::load` and must reproduce published numbers to 1e-3, including Raspberry Pi's own
  regulator copper with pours (`qfn60-reference`, `qfn80-reference`).

## Known wrong or weak

- **Four fixes live here, not in `kicad-pcb`** (2bbb620, flight-controller mining). They are
  workarounds over the pinned reader and are still missing from ultraroute's `kicad-pcb` at its
  current head (checked: `copper.rs` reads a segment's `text(child, "layer")?` and a zone's
  `net_name` only):
  1. `normalise_track_layers` (`lib.rs:234`) rewrites the board **text** before parsing:
     KiCad 8+ writes `(layers "B.Cu" "B.Mask")` on a `segment`/`arc` with an opened mask
     (exposed busbars, Altium imports), which `kicad-pcb` rejects. A hand-written s-expression
     scanner, keeps newlines so error line numbers hold.
  2. Paste-only pads (KiCad 10 0201 footprints' unnamed `F.Paste` apertures) are skipped
     because `expand` gives no copper layer (`lib.rs:134`).
  3. KiCad 10 zones write `(net "GND")` with no `net_name`; the name is looked up from `z.net`
     (`lib.rs:189`).
  4. Stacked HDI vias of different spans join on their shared layer — this one is in
     `graph.rs:148`, a connectivity fix, not a reader fix.
- `copper_path` across a fill is a **straight line** between any two nodes inside it: it can cut
  a corner or cross a void/antipad the fill does not have (module doc, `graph.rs:1`). Loop areas
  through pours are therefore lower bounds on the real path.
- `copper_path` is O(track ends × tracks) plus all-pairs inside each fill; on a big ground net
  with many vias in a pour it is slow (not measured). It rebuilds the net's graph on every call.
- Fills are as KiCad last computed them: a board edited without a refill gives stale paths.
- Arcs become two chords through the midpoint (`lib.rs:163`): sagitta error about 7.6 % of the
  radius for a 90° arc; fine for loop areas, loose for clearances.
- `Board::pad("REF.PIN")` returns the **first** pad of a multi-pad pin (exposed pads, shells);
  `copper_path` treats all of them as one terminal but callers that use `pad().centre` do not.
- Pads with no net get `net: ""`; several checks filter on `!net.is_empty()`, others do not.
- `Part.courtyard` is a convex hull, so L-shaped courtyards are over-approximated.
- `geom::inside` is even-odd; a fill polygon with a fractured slit is handled by KiCad's
  fracturing, not by holes.
- Doc disagreement: the crate comment says rules come from "`.kicad_pro` (and `.kicad_dru`)";
  that is true only because `kicad-pcb`'s `Rules::read` applies the `.kicad_dru` it finds beside
  the project, and only the custom rules it understands.

## Not covered by tests

- `Board::load` with a `.kicad_pro` (`rules`), `Board::clearance`, and `outline`.
- Arcs (the two-chord conversion), courtyards, `locked` flags, via span expansion for blind or
  unknown layers.
- `net_copper`.
- `copper_path` **through a zone fill** (no unit test at all; only exercised by the reference
  fixtures in `verify`), a pad joining a fill by overlap, and a track end on a via.
- `geom::overlap_area`, `hull`, `point_poly_dist`, `polyline_poly_dist` for a single point,
  `winding` on its own.

## Lessons to keep

- **Loop area is the integral of |winding number|, not the shoelace sum.** A crossing loop's
  lobes cancel in the shoelace sum; it under-reported Raspberry Pi's output loop by half
  (README "Milestone 2", b6ce22b). `geom::figure_eight` pins this.
- Copper touches when centrelines come within the half-widths' sum, as KiCad's connectivity
  counts it: ultraroute leaves segments that only overlap, and `check package` called nets open
  until this was added (975c0d1).
- A track through a pad and a via inside a pad join the pad (bb5636d); a pin with several pads is
  one terminal (73eb8c6, MOSFET source leads, split exposed pads). Published buck numbers did not
  move with either change.
- `pcb` and `app/core` read with the same `kicad-pcb` commit on purpose, so the browser's placement
  write and the checks agree (2a7abb1). `9114263` was the repin that reads footprints without
  uuids (9c9317a); `8c691ec` the one for KiCad 10 boards (bb5636d).
- Raster resolutions in use: `loop_area(_, 400)` in verify, `(_, 60)` in blockgen's estimates;
  `union_area` at 10 µm, `overlap_area` at 20 µm. Changing them moves the published numbers.

## For the move

- The spec merges this into Boardroom's `board` crate as a module that keeps its API (`Board`,
  `Part`, `Pad` with net, `pad("REF.PIN")`, `net_copper`, `graph::copper_path`, `geom`), built
  on `board`'s items instead of on `kicad-pcb` directly. Keep the field names so `verify`
  (→ `examine`), `blockgen`, `recognise` and the pieces of `compile` move unchanged first.
- Move the four fixes into the new `kicad` crate (1–3) and `board`'s connectivity (4), each with
  its test from here; then delete `normalise_track_layers` (a text rewrite before parsing should
  not survive into the one reader).
- Units: `kicad-pcb`/`board` are metres; this module is millimetres. Decide once; if `board`
  stays in metres, convert at this module's edge exactly as `mm()` does now (`lib.rs:88`).
- `copper_path` should be built on `board`'s connectivity (`connect`) rather than its own
  join rules, but only after checking the same seven `verify` fixtures reproduce to 1e-3.
  Consider caching the per-net graph: the router will call it in a loop.
- Test fixtures used through this crate live under `packages/rp2350/layouts/*/fixture.kicad_pcb`
  and `packages/tlv62569/layouts/sot23-5-4layer/` → Boardroom `packages/`.
- What I would do differently: give fills holes (or keep KiCad's fracture) and route through
  them by visibility rather than straight lines; make `pad()` return all pads of a pin.
