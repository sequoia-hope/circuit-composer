# compile → boardroom: split (`kicad`, the router's reading of intent, `route`/`examine`/`check`)

The constraint compiler: an intent file plus its placed board, compiled to what the old router
(ultraroute, `--regions routing.json`) and KiCad read. It writes `routing.json` (regions with
per-layer rules, hard length limits, `clear_under`, skews, current-sized widths) and a marked block
of KiCad custom rules merged into the board's `.kicad_dru` (length limits, diff-pair skew, track
widths). It also holds `check_regions`, which lists copper that breaks a region rule. One file,
`src/lib.rs`, 733 lines (about 124 of them inline tests), plus `tests/regions.rs` (90 lines).
Depends on `intent`, `pcb` (board model and `geom`), `serde`, `serde_json`. Depended on only by
`ccomp` (`ccomp compile`, `ccomp check regions`; crates/ccomp/src/main.rs:665-680, 769-796), which
`tools/route_board.sh:36` and `tools/verify_routed.py` drive.

**It does not move as a crate.** The one-system spec ("What changes in kind") removes `compile`,
`routing.json` and `--regions`: the new router reads intent itself, regions come back through
intent, and only regions that need the board's geometry are derived by the router. Each function
below goes to its new home with its tests.

## Model in five minutes

`compile(&IntentFile, &pcb::Board) -> Result<(Routing, Vec<Note>), String>` (lib.rs:155), in order:

1. **Intent regions** (lib.rs:164-213). A region with no polygon (a floorplanner soft region) is
   skipped with a note. An `anchor` part moves the polygon into board coordinates:
   `add(part.at, rotate(q, part.rotation))`; a back-side anchor is an error ("mirrored anchors are
   not handled yet"). Class selectors are expanded to the intent's nets of that class (a class
   with no nets drops the rule with a note); `all_nets` becomes `classes: ["*"]`. Weights are
   multiplied by `WEIGHT_SCALE` = 10 (lib.rs:24): intent weights are relative in (0,1], the
   router's are extra-length multipliers. Region layers must exist on the board (error otherwise).
2. **Block constraints to regions** (`block_regions`, lib.rs:403): see "Geometry-derived regions".
3. **`max_lengths`** (lib.rs:218-227): hard `max_length` only; soft ones are noted and dropped.
4. **Skews** (lib.rs:229-257): a differential pair (`impedance.mode == differential` with `pair`)
   whose two nets share a `match_group` with `tolerance_mm`, both on the board, and named as KiCad
   9 names a pair (`kicad_pair_base`, lib.rs:96: last character P/N or +/-, the rest equal and
   non-empty). `tolerance_ps` is noted and skipped.
5. **Widths** (`widths`, lib.rs:279): from each non-GROUND net's `current`.

`merge_dru(existing, &Routing) -> String` (lib.rs:480) writes, between `DRU_BEGIN`/`DRU_END`
markers (lib.rs:475-476), one rule per net and kind, and keeps everything outside the markers:

```
(rule "cc max length /XIN"  (condition "A.NetName == '/XIN'")  (constraint length (max 10mm)))
(rule "cc skew /USB_D"      (condition "A.inDiffPair('/USB_D')") (constraint skew (max 0.15mm) (within_diff_pairs)))
(rule "cc width VBUS"       (condition "A.NetName == 'VBUS'")  (constraint track_width (min 0.3mm) (opt 0.5mm)))
```

`check_regions(&Board, &Routing) -> Vec<RegionFinding>` (lib.rs:673): per region and rule,
`never` reports tracks inside or within half-width of the polygon, vias within their radius, and
pours overlapping by more than 0.001 mm²; `must` reports track length outside and vias whose
centre is outside; `avoid`/`prefer` report the length that pays the price, `violation: false`.
Copper only touching an edge is outside (`EDGE_EPSILON_MM` = 0.5 µm, KiCad's tolerance).

### routing.json as the code writes it (`Routing`, lib.rs:27)

Checked against the contract agreed with ultraroute (2026-10-27 to 11-03):

```
{"version": 1, "generator": "circuit-composer 0.1.0",
 "weight_mapping": "router weight = 10 x intent weight ...",
 "regions": [{"name", "polygon": [[x,y],...] (mm, KiCad frame, y down), "layers"?: [..] (absent = all),
              "rules": [{"nets"?: [..] | "classes"?: ["*"], "kind": "never|must|avoid|prefer",
                         "weight"?: f, "vias"?: true}]}],
 "max_lengths": [{"nets": [..], "max_length_mm": f, "why": ".."}],
 "clear_under": [{"nets", "layers", "planes", "margin_mm"}],
 "skews":  [{"nets", "pair", "max_skew_mm", "why"}],        // for people; the router reads the .kicad_dru
 "widths": [{"nets", "min_mm"?, "opt_mm"?, "basis", "why"}]}  // a list per net, not one object
```

Matches the contract except that `widths` is a list of per-net entries carrying `nets` and `why`,
and the extra `generator`, `weight_mapping`, `skews` fields. The committed example
`boards/rp2350-battery/routed/routing.json` predates widths (775c162) and has 29 regions, all
4-vertex rectangles, and one `clear_under` (`/USB_D+`, `/USB_D-` on F.Cu and In2.Cu over In1.Cu).

## Solid

All 7 tests pass (2026-11-12, `cargo test -p compile`; `tests/regions.rs` took 13.8 s in a debug
build, cause not profiled).

- `tests::block_constraints_become_regions`: on a synthetic 4-layer 10 x 10 mm board with In1 GND
  fill on the left 6 mm: `unreferenced_layers` = In1.Cu, B.Cu; hard zero over-ground = `never` on
  those layers, only for nets on the board; the void x 6..9 inside the 1 mm rim on F.Cu and In2.Cu;
  one `clear_under`; a soft no-vias rule = `avoid` weight 5; a hard ceiling above zero = `avoid`
  weight 10 on the region and its voids.
- `tests::dru_merge_keeps_and_replaces`: existing rules kept, the block replaced not appended, skew
  and width rule text exact.
- `tests::currents_become_widths_hard_only_when_someone_stands_behind_them`: hard requirement ->
  `min_mm` from continuous, `opt_mm` from peak; recognised 0.35 -> `opt_mm` only, from continuous;
  recognised 0.2 -> none, with a note; GROUND and 0.05 A -> none; 1 A is about 0.30 mm.
- `tests::kicad_knows_pairs_by_their_last_character`.
- `tests/regions.rs` on `packages/rp2350/layouts/qfn60-4layer/fixture.kicad_pcb` (known copper: an
  LX run on F.Cu, ground vias, GND plane on In1.Cu): `anchored_region_lands_on_its_part`,
  `check_catches_never_must_and_vias` (and silence where nothing breaks a rule),
  `copper_touching_an_edge_is_outside`.
- In use: rp2350-battery routed by ultraroute 89f1161 and f179fcb over 4 seeds each with 0 DRC
  errors and 0 region violations against 29 compiled regions (boards/rp2350-battery/routed/README.md);
  easy2350's released route passes all 12 compiled region rules and 17 length limits (c6a85d3).
  KiCad parsed and enforced the skew rule (it failed a 0.15 mm rule with `skew_out_of_range`,
  README.md:85-89).

## Known wrong or weak

- **An unfilled board makes everything unreferenced.** `unreferenced_layers` (lib.rs:311) judges
  ground by the fills "as KiCad last computed them". With no fills, every layer is unreferenced,
  and a hard `unreferenced_length_mm <= 0` becomes a `never` over the whole outline on every
  layer: those nets cannot be routed. `prepare_route.py:283` fills zones first, which is what
  makes it work today. (By reading; not observed.) In Boardroom, `board`'s `fill` is ours, not
  KiCad's, so voids derived from it will differ from KiCad's.
- **Voids are bounding rectangles** on a 0.1 mm raster (`ground_voids`, lib.rs:341): an L-shaped
  void forbids the ground inside its box too. The 1 mm rim along the outline is left out
  entirely (`VOID_RIM`), so a void touching the edge is clipped. The raster is O(cells x fills x
  vertices) with an `edge_dist` over every outline edge per cell.
- **Voids are repeated per constraint.** Every over-ground constraint emits the full void set
  under its own name: rp2350-battery's 29 regions are 5 base regions plus 6 voids x 4
  constraints. One void set with a union of nets per rule kind would do.
- **Widths ignore the layer and stack-up.** `ipc2221_width_mm` (lib.rs:70) is outer layer, 1 oz,
  10 °C rise, applied to every layer; IPC-2221's inner-layer constant is half, so inner tracks
  are undersized by this figure.
- **`.kicad_dru` conditions do not escape net names**: a `'` in a net name breaks the rule.
- Regions: back-side anchors are refused; the rotation sign of anchored regions is untested (the
  test polygon is a square centred on the anchor); soft regions are skipped ("milestone 5");
  protected zones and isolation barriers go out as their rules only, their clearances are
  "milestone 7" (lib.rs:181), never done.
- Class selectors cover only nets listed in the intent with that class; a board net missing from
  the intent is never under a class rule.
- Soft `max_length` is dropped; `tolerance_ps` match groups are dropped; `Ge` bounds and every
  metric other than `via_count <= 0` and `unreferenced_length_mm <= limit` are noted "measured
  after routing, not routed to" (lib.rs:427-433), and generator metrics are skipped silently.
- `check_regions` (lib.rs:673): `must` judges a track by its centreline only (width ignored) and a
  via by its centre; vias are reported only for `never`/`must`, not priced for `avoid`/`prefer`;
  pours only for `never`; pads never; track length inside a polygon is sampled every 10 µm
  (`clipped_length`, lib.rs:728), slow for long tracks against many-vertex polygons. Arcs reach it
  as chords (pcb).
- The System Spec's overview says the compiler emits KiCad net classes and rule areas; it emits
  neither (net classes are written by `tools/prepare_route.py`). docs/app-spec.md says `app/core`
  depends on `compile`; it does not.
- Skew is whole-net in KiCad 9 and ultraroute 89f1161, so a pair whose nets include jumpers cannot
  meet a tight rule (see Lessons); the compiler cannot express "skew between these pads".

## Not covered by tests

- `compile()` end to end on a board with blocks (only `block_regions` directly, and `compile()` on
  regions without blocks); the notes it emits (missing outline, nets not on the board, soft
  constraint without weight, class with no nets).
- `max_lengths` and `skews` assembly inside `compile()` (only their DRU text from a hand-built
  `Routing`): match-group mismatch, `tolerance_ps`, pair not on the board, non-KiCad pair names.
- Region layer validation error; back-side anchor error; rotation of anchored regions.
- `ground_voids` with several fills, holes inside a fill polygon, several ground groups (e.g. 6
  layers), a void touching the rim.
- `merge_dru` with `existing = None`, and with markers in the wrong order.
- `check_regions`: `avoid`/`prefer` lengths, `must` vias, `all_nets` on tracks, a `never` pour that
  only touches the edge.

## Lessons to keep

- **Large region polygons stalled the old router.** A `must` region shaped as the In1 ground
  fill's outline (683 vertices) made prefanout take 156 s and escape 0 of 26 pads; ultraroute
  fixed its raster (f179fcb), but the compiler kept sending voids as small rectangles
  (boards/rp2350-battery/routed/README.md:167-169). The spec keeps this lesson.
- **A ceiling the router can't route to still gets a price.** QSPI's 9.22 mm off-ground ceiling
  compiled to no regions gave 8.25 mean package fails over 4 placements; compiled to `avoid`
  regions, 7.25; without the price QSPI ran up to 42 mm over +3V3 (98cbbe0; README.md:146-154).
  A hard ceiling above zero is an `avoid` at weight 10, never a `never`.
- **`clear_under` only for hard over-ground nets.** Other nets' vias opening plane antipads under
  a net break its return path (seed 2's USB_DP over a QSPI via's hole). Given for soft nets too it
  did not help: 14.5 package fails against 13.25 (9a0da31, 202869c; lib.rs:457-460). Margin
  0.05 mm because ultraroute measured 4 to 8 µm at 0 and the checker counts any gap
  (`CLEAR_UNDER_MARGIN`, lib.rs:113-114).
- **Widths: `min_mm` only from a figure someone stands behind, `opt_mm` from any** (775c162; spec
  decisions log). `min_mm` is a hard rule routed to like a class width, so only a hard
  requirement/datasheet/expert/simulated current sets it (from continuous); `opt_mm` is reached
  after routing by widening in place, costs no connection, so any figure may set it: a trusted one
  from its peak, a recognised one from its continuous (its "peak" is the source's capacity).
  Recognised currents under confidence 0.35 are skipped (the recogniser's no-load fallback sits at
  0.2); GROUND nets are planes; widths <= 0.1 mm never bind. Why: the 50-board hand check found
  recognised currents within 2x on only 59 of 171 nets.
- **KiCad 9 knows a pair only by a last character P/N or +/-.** `USB_DP`/`USB_DM` is no pair;
  rp2350-battery's nets were renamed `USB_D+`/`USB_D-` (98cbbe0). From ultraroute 89f1161 a pair
  is length-tuned only under a `.kicad_dru` skew rule, `(constraint skew (max ..)
  (within_diff_pairs))`, `within_diff_pairs` so KiCad does not compare pairs with each other.
- **No skew rule on USB-C.** rp2350-battery's pair has no match group: on USB-C, D+ joins A6 and
  B6 and D- joins A7 and B7, interleaved, so the jumpers alone make whole-net skew 5-8 mm. With a
  0.15 mm rule the router tuned 7.9 mm down to 2.7-5.7 and KiCad failed it on every seed;
  full-speed USB at 12 Mb/s needs no matching (README.md:77-89).
- **Hard constraints are `never` rules, soft ones `avoid` at 10 x weight** (72bc9d0): a via then
  costs `10 x weight` mm of track, a track `10 x weight` times its length.
- **Classes are expanded to net lists** so the output never depends on the KiCad project having a
  net class of the same name (lib.rs:13-14).
- **A rerun replaces only the generated `.kicad_dru` block**; the board's own rules are kept
  (c6a85d3).

## For the move

Function by function, each with its tests:

| Function / item (lib.rs) | New home | Tests that go with it |
|---|---|---|
| `merge_dru` (:480), `DRU_BEGIN`/`DRU_END` (:475), `fmt_mm` (:522) | `kicad` (writes `.kicad_dru` from intent; take lists of length/skew/width rules, not a `Routing`) | `dru_merge_keeps_and_replaces` |
| `kicad_pair_base` (:96), the skew assembly in `compile` (:229-257), `Skew` (:85) | `kicad` (diff-pair skew rules) | `kicad_knows_pairs_by_their_last_character`; the skew half of `dru_merge_keeps_and_replaces` |
| `max_lengths` assembly (:218-227), `MaxLength` (:140) | `kicad` (length rules) and the router's reading of intent (hard limits only) | length half of `dru_merge_keeps_and_replaces` (none for assembly) |
| `ipc2221_width_mm` (:70), `widths` (:279), `Width` (:54), `NEGLIGIBLE_WIDTH_MM` (:76), `RECOGNISED_MIN_CONFIDENCE` (:80) | the router's reading of intent ("`min_mm` only from a trusted figure"); `kicad` writes the same figures as `track_width (min ..) (opt ..)` | `currents_become_widths_hard_only_when_someone_stands_behind_them`; width half of `dru_merge_keeps_and_replaces` |
| Intent regions pass-through (:164-213): anchor transform, class expansion, layer check, `WEIGHT_SCALE` (:24) | the router's reading of intent (regions return through intent) | `anchored_region_lands_on_its_part` (tests/regions.rs) |
| `block_regions` (:403), `unreferenced_layers` (:311), `ground_groups` (:318), `ground_voids` (:341), `VOID_STEP`/`VOID_RIM` (:332-335), `ClearUnder` (:106), `CLEAR_UNDER_MARGIN` (:114) | `route`, or a function in `examine`/`check` that `route` calls (needs the board's geometry: `via_count <= 0`, `unreferenced_length_mm`, voids as small rectangles) | `block_constraints_become_regions` |
| `check_regions` (:673), `RegionFinding` (:656), `EDGE_EPSILON_MM` (:671), `clipped_length` (:728) | `check` or `examine` | `check_catches_never_must_and_vias`, `copper_touching_an_edge_is_outside` (tests/regions.rs) |
| `Routing`, `RRegion`, `RRule` (:27-137), `compile` itself, `Note` | gone with `routing.json`; the region/rule shape is worth keeping as the router's internal type | the tests above build a `Routing` through `compile()`; give them a constructor from intent + board instead |

Other changes:

- Fixture path: `../../packages/rp2350/layouts/qfn60-4layer/fixture.kicad_pcb` becomes Boardroom's
  `packages/rp2350/...`. The unit tests build their boards in memory against `pcb::{Board, Pad,
  Fill}`; with `pcb` a module of `board` keeping its API, only paths change. The fields used are
  `Board.{layers, parts[].at/rotation/back, pads[].net, tracks, vias, fills, outline}` and `geom::{add,
  rotate, bbox, inside, point_seg_dist, point_poly_dist, polyline_poly_dist, poly_poly_dist,
  overlap_area, centroid, dist, scale, sub}`.
- `ccomp compile` and `ccomp check regions` go away; `tools/route_board.sh` is gone (spec);
  `tools/verify_routed.py` stays until Rust covers it and must stop calling `ccomp check regions`.
- Differently: compute voids once per ground group, not per constraint; emit polygons that follow
  the void (or rectangles that tile it) rather than one bounding box; judge ground from fills the
  `board` crate computes the same way KiCad does, and fail loudly when a board has zones but no
  fills; size widths per layer from the stack-up's copper weight; escape net names in DRU
  conditions; let `check_regions` take copper width into account for `must`.
