# intent → boardroom crates/intent

The schema of `intent.json`, the sidecar beside a KiCad project that holds what KiCad cannot say:
per-net class, voltage, current, noise, bandwidth, impedance, match group, maximum length, each
with a strength and a provenance; blocks with roles; a domain tree; regions; protection paths.
The Rust types are the one definition; the JSON Schema (`schema/intent.schema.json`) is generated
from them. The crate also holds the validator, the class presets and block-type list, a KiCad
S-expression netlist reader, and an importer for easy_pcb's `design/nets.csv`. Size: 1,834 lines
of `src` (about 24 of them inline tests), 122 lines in `tests/`. Depends on `serde`,
`serde_json`, `schemars` 1, `thiserror` 2; no workspace crates. Depended on by `ccomp`,
`compile`, `compose` (`board_intent.rs`, which assembles a board's intent from packages') and
`recognise`. Not by `app/core` (docs/app-spec.md says it is; Cargo.toml says no).

## Model in five minutes

Types, all in `crates/intent/src/model.rs`, all `#[serde(deny_unknown_fields)]` (a typo fails at
load, test `unknown_field_is_refused_at_load`):

- `IntentFile` (model.rs:16): `schema_version` (`"0.1"`, `SCHEMA_VERSION` at :12), `board`
  (`BoardInfo` :43: name, kicad_project, layers, `packages` lock map `name -> version`),
  `sources` (id -> `Source` :59), `nets` (KiCad net name exactly as the netlist spells it ->
  `NetIntent`), `blocks` (id -> `Block`), `domains` (root `Domain`), `regions`,
  `protection_paths`, `energy_sources`.
- `Attr<T>` (:78): `value`, `strength` (`hard`|`soft`), `weight` (soft only, in (0,1]),
  `provenance`. `Provenance` (:98): `source` (a key of `sources`, or `preset:<CLASS>`),
  `locator`, `kind` (`ClaimKind` :122: datasheet, expert, simulated, requirement, preset,
  recognised), `date`, `note`, and for recognised claims `confidence` and `evidence` (one fact
  per string).
- `NetIntent` (:144): `class` (+ `class_provenance` when recognised) and optional `aggressor`,
  `victim` (`Noise`), `bandwidth`, `voltage` (peak_v), `current` (continuous_a, peak_a),
  `fault_current`, `impedance` (ohms, tolerance_pct, mode, `pair` = the other net),
  `topology`, `match_group` (group, exactly one of tolerance_mm / tolerance_ps), `safety`,
  `max_length` (mm), `note`.
- `Block` (:301): `type` (serde rename of `kind`), `package` (`name@version`), `parts` (refdes),
  `roles` (role -> `REF.PIN` for a pad or `REF` for a part; a buck names `vin`, `sw`, `pgnd`,
  `fb`, `cin`, `cout`, `inductor`), `placement_goal` (objectives and `Attr<MetricBound>`
  constraints in the `Metric` vocabulary :386), `ports`, `provenance`, `note`.
- `Domain` (:464) tree; `Region` (:509): kind (polygon, soft, isolation_barrier,
  protected_zone), `polygon` in board mm (KiCad frame, y down) or in an `anchor` part's frame,
  `block` for a soft region, `layers` (empty = all), `rules` (`RegionRule` :541: `applies_to`
  = `{"class":..}` | `{"net":..}` | `"all_nets"`, `vias` flag, `mode` must|prefer|avoid|never,
  `weight`). Rule meanings are the router's: `must` = may not leave.
- `ProtectionPath` (:583): ordered `REF.PIN` sequence, connector first; `EnergySource` (:599).

Entry points:

- `intent::load(path)` (lib.rs:16), `intent::json_schema()` (lib.rs:22).
- `validate::validate(&IntentFile, Option<&Netlist>) -> Report` (validate.rs:161). Reports every
  finding at once, each with a stable `code`, `subject`, `id`, `field`. With no netlist, net and
  part references are checked against the intent file only (warning `no_netlist`).
- `presets::NET_CLASSES` (presets.rs:74: 14 classes with required fields and default
  aggressor/victim), `presets::resolve` (:96, fills soft preset defaults, weight 0.5),
  `presets::BLOCK_TYPES` (:134: 17 types; only `buck` has required roles and `generator: true`).
- `netlist::Netlist::load` (netlist.rs:67): reads a `.net`, or for `.kicad_sch`/`.kicad_pro`
  runs `kicad-cli sch export netlist --format kicadsexpr` into a temp file. `find_net` (:116)
  matches with or without the root sheet's leading `/`. Own minimal S-expression reader
  (:145).
- `import::from_easy_pcb_nets_csv` (import.rs:23): every value a hard `requirement` citing its
  row; POWER with v_max 0 becomes GROUND; impedance tolerance 10 % assumed (and said so in the
  note); differential partners found by name swap (DP/DM, D+/D-, _P/_N, +/-, P/N, last
  occurrence).

Invariants a change must keep:

- A hard `Attr` needs a provenance whose kind is not `preset` and not `recognised`
  (validate.rs:117-150). Soft needs a weight in (0,1]. A region with a must/never rule needs a
  provenance. This is the "a hard limit only from a figure someone stands behind" rule.
- A part is in at most one block; a role naming a part names one of the block's parts; a
  differential `impedance.pair` names a net that names this one back with the same target.
- The committed schema equals `json_schema()` (test `committed_schema_is_current`); regenerate
  with `ccomp schema -o schema/intent.schema.json` after any type change.
- GROUND refines KiCad/easy_pcb's POWER: not reported as a class disagreement (validate.rs:197).

## Solid

All 17 tests pass (2026-11-12, `cargo test -p intent`).

- `tests/easy2350.rs`, against the hand-annotated `boards/easy2350/intent.json` and its netlist
  snapshot `boards/easy2350/input/easy2350.net`: `easy2350_passes` (every netlist net has passing
  intent, no part unassigned), and one test per refusal: `missing_net_is_refused`,
  `required_field_is_refused`, `hard_without_provenance_is_refused`,
  `uncited_source_is_refused`, `unresolved_block_type_is_refused`,
  `part_in_two_blocks_is_refused`, `broken_pair_is_refused`,
  `protection_pad_off_net_is_refused`, `peak_below_continuous_is_a_conflict`,
  `soft_needs_weight`, `unknown_field_is_refused_at_load`, `round_trips`,
  `committed_schema_is_current`.
- `import.rs` unit tests: `csv_quotes_and_commas`, `pairs_by_name`, `impedance_forms`.
- Exercised from outside: `recognise`'s `recognised_intent_has_no_hard_or_provenance_errors`
  (the validator accepts recogniser output's provenance), `compose`'s and `ccomp`'s tests
  (composed rp2350-battery intent validates: 42 of 42 nets, 11 blocks, per 16726a8).

## Known wrong or weak

- **No `group` on `Block` yet.** The one-system spec ("What changes in kind") adds `Block.group`,
  naming the KiCad group that holds the block's locked layout, so the router keeps that copper
  and scores it by roles without matching groups by part sets. Today the link does not exist and
  the names do not line up:
  - `tools/apply_layout.py` names the group `--group NAME` or `<block type>_<anchor>`
    (apply_layout.py:184; anchor = `roles["anchor"]` or the part of `roles["vin"]`, i.e. the IC).
  - In the composed flow `tools/prepare_route.py:242` passes `--group <rigid entry name>` from
    the floorplan spec (`core_reg`, `reg` on rp2350-battery), while the intent's block ids are
    `mcu/core_reg` and `reg/reg` (`INSTANCE/NAME`, from `compose`).
  - The recogniser names its blocks `<type>_<anchor>` too, but anchors a buck on its inductor
    (`buck_L1`), not the IC, so a recognised id never equals apply_layout's default group name.
  - The committed routed board `boards/rp2350-battery/routed/rp2350-battery.kicad_pcb` contains
    no `(group` at all (grep count 0), so the groups did not survive into the router's output
    there; why is unverified.
- **Hot-loop roles are already in `Block.roles`** (model.rs:310-313), as the spec says, but only
  `buck` requires any (presets.rs:135). `verify::buck::BuckRoles::from_map`
  (crates/verify/src/buck.rs:56) requires all seven buck roles and reads optional `diode`,
  `rfb_top`, `rfb_bot`, `cff`; the presets do not name those optional roles, and `boost` lists no
  roles although the recogniser writes `cin`, `cout`, `vout`, `pgnd`, `sw`, `inductor`, `fb` for
  one. The role vocabulary per block type lives in three places (presets, verify, recognise).
- The validator does not check: region `layers` against anything (compile checks them against
  the board), `Barrier` values' positivity, `edge_ns`, `tolerance_pct` upper bounds, a domain's
  `supplies` being POWER-class nets, port `direction`, a soft region's polygon, or `Source.sha256`
  against the file. Voltage positivity is checked on `abs(peak_v)`, so negative rails pass.
- `MatchGroup.tolerance_ps` is accepted but nothing downstream uses it (`compile` notes and skips
  it).
- `Current.peak_a` means two things: a designer's worst-case draw, or (from `recognise`) the most
  the rail's source could deliver. Only `compile::widths` knows to treat them differently, by
  provenance kind. The schema does not say.
- `presets::resolve` fills only aggressor/victim defaults; the spec's "class supplies soft
  defaults for the rest" is otherwise not implemented.
- README.md's intent section lists provenance kinds without `recognised` (added in af5894b).
- `Netlist::is_unconnected` treats any net with fewer than two nodes as unconnected, so a
  one-pin named net (a test point) needs no intent.
- `schema_version` has stayed `"0.1"` through additive changes (roles in b6ce22b, recognised
  provenance in af5894b); with `deny_unknown_fields`, an older reader refuses a newer file
  rather than reporting a version mismatch.

## Not covered by tests

- Validator paths with no test: `hard_from_preset`, `hard_from_recognition` (only asserted
  absent, in recognise), `weight_on_hard`, `bad_weight`, `bad_confidence`, `bad_provenance`,
  `net_unknown` and the KiCad-name hint, `class_differs_from_kicad`, `unknown_class`,
  `block_missing_role`, `unknown_pad`, `role_outside_block`, package lock checks
  (`bad_package`, `package_not_locked`, `package_version`), port shape checks, all of
  `check_domains` (duplicate names, `block_in_two_domains`, `barrier_target`,
  `block_without_domain`), all of `check_regions` (`region_shape`, soft region block,
  `soft_without_weight` on prefer/avoid), `path_too_short`, `bad_pad`, all `energy_source`
  checks, `fault_current` conflicts, `match_group` exactly-one rule.
- `presets::resolve`; `Netlist::load` through `kicad-cli` (only a `.net` file is read in tests);
  `Netlist::find_net`; `from_easy_pcb_nets_csv` end to end (only its helpers).

## Lessons to keep

- **Categories are presets over attributes**, never the reverse: a class names the fields a net
  must state and supplies soft defaults; a default never satisfies a requirement and can never be
  hard (spec decisions log; presets.rs header). New circuit kinds fit with no schema change.
- **Hard needs a cited, trusted figure.** `preset` and `recognised` provenance can never be
  hard (validate.rs:123-132). Downstream this became: a width's `min_mm`, a `never` region, a
  hard length come only from requirement/datasheet/expert/simulated (see compile.md). Why: the
  50-board hand check found recognised currents within 2x on only 59 of 171 nets (spec decisions
  log; 66 of 177 after 415e568).
- `requirement` exists because a board budget (500 mA USB) is the designer's claim, not a
  datasheet's; `preset` exists so defaults are visibly weaker than evidence (README decisions).
- Region rules take the router's meanings (`must` = may not leave) and `vias` is a per-rule flag,
  not a selector (c6a85d3), so the compiler passed them through unchanged.
- Regions may be anchored to a part, because package rules are stated relative to parts before
  placement exists (README decisions).
- Protected zones are required only for batteries and capacitor banks; a supply input with no
  fuse has no "upstream of the first disconnect" (b6ce22b; validate.rs:567-571).
- Unknown fields are refused at load, so a typo never passes silently (lib.rs:15).
- Boards keep a netlist snapshot in `input/` so validation is reproducible without the source
  checkout (README decisions).

## For the move

- **Add `group: Option<String>` to `Block`** (spec, "What changes in kind"), with
  `#[serde(default, skip_serializing_if = "Option::is_none")]`. Two struct literals must gain it:
  `crates/compose/src/board_intent.rs:386` and `crates/recognise/src/lib.rs:182`. Regenerate the
  schema. Decide who writes it: the step that applies a locked layout (today
  `apply_layout.py`/`prepare_route.py`, in Boardroom `kicad`) should write the group name it
  creates back into the block, so nobody has to reconcile `core_reg` vs `mcu/core_reg` vs
  `<type>_<anchor>` by convention. Consider bumping `schema_version`.
- Keep hot-loop roles in `Block.roles`; write down the role vocabulary per block type once (in
  `presets::BLOCK_TYPES`), including the optional `diode`, `rfb_top`, `rfb_bot`, `cff` that
  `verify` reads, and `boost`'s roles, so `recognise`, `blockgen`, `examine` and the router agree.
- The spec also extends intent with the layer plan, frames by region and repeats; none exist yet.
- `netlist.rs` is "Composer's netlist reader", which the spec moves to `kicad`. Move it there
  (with its S-expression reader replaced by `kicad`'s) and have `validate` take the netlist type
  from `kicad`; `intent` may depend on Foundation crates.
- Paths tests read: `../../boards/easy2350/intent.json` and
  `../../boards/easy2350/input/easy2350.net` become `examples/easy2350/...` (spec: check
  `examples/easy2350` holds `input/`, `floorplan/` and the intent first);
  `../../schema/intent.schema.json` goes wherever Boardroom keeps generated schemas.
- Data in the intent format that must keep validating: `packages/*/intent.json` (4 files),
  `boards/{easy2350,rp2350-battery,rp2350-matrix}/intent.json`, and the
  `.claude/skills/circuit-composer/SKILL.md`, which names the schema path.
- `ccomp schema|classes|validate|resolve|netlist|import easy-pcb` become `boardroom compose …`
  (or `boardroom intent …`) commands over these same functions.
- Differently: tests for the untested validator branches above are cheap (each is a one-line
  mutation of easy2350, like the existing ones) and should come with the move.
