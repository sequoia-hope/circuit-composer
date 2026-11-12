# library → boardroom crates/library

The research library's code: loads every cited **claim** (`library/claims/*.json`, claims about
circuit classes, sources in `library/sources.json`; and every `packages/*/rules.json`, claims
about one chip, sources in the package's `sources/sources.json`) as one database, checks its
integrity, and answers queries. The data itself is described in `research-library.md`. Size:
383 lines of `src` (unit tests included), no `tests/` folder. Depends on `serde`, `serde_json`
only. Used by `ccomp` only (`library check`, `library query`). The one other reader of claims is
`ccomp check package` (`crates/ccomp/src/package_check.rs`), which reads a package's
`rules.json` as raw JSON for its part-value checks, not through this crate.

## Model in five minutes

`src/lib.rs`:
- `Source` (:21): `title`, `url` or `where`, `sha256`, flattened `extra` (publisher, doc,
  kind, revision, bytes, retrieved, member…).
- `Citation` (:35): `source`, `locator`, `quote`, flattened `extra` (`figure`, `measured`).
- `Claim` (:47): `id`, `statement`, `scope` (free map: `circuit`, `block`, `chip`…),
  `attributes`, `citation` or `citations`, `status` (one of `STATUSES`, :17: asserted,
  contested, simulated, promoted, refuted), `links` (list of `{kind: target}` with kind in
  `LINKS`: supports, refines, contradicts), `enforced_by`, flattened `extra` (`note`,
  `evidence`, `check`…).
- `ClaimSet` (:86): one file's claims and its own sources; name `library/buck` or
  `packages/rp2350`.
- `Library::load(root)` (:118); `source(set, id)` (:151): a citation `pkg/id` resolves in that
  package's sources, otherwise the set's own, then the library's; `incoming(id)` (:160);
  `check()` (:165) → `Report { ok, claims, sources, by_status, by_set, errors, warnings }`;
  `query(&Query)` (:251) by set, status, attribute, scope (case-insensitive, arrays match any),
  id or id prefix ending at a dot, text; `expand` (:267) adds the set, sources' titles/URLs and
  incoming links.

`check` errors: duplicate id across all sets, unknown status, empty statement, no citation,
unknown source, unknown link kind or target, a `contradicts` link not returned by its target, a
`promoted` claim with empty `enforced_by`, a source with neither `url` nor `where`, a malformed
`sha256`, a package source id that shadows a library source id. Warnings: citation with no
locator, `contested` with no contradicting link and no note/evidence, a source no claim cites.

`ccomp library check` (`crates/ccomp/src/main.rs:959`) is this `check` **plus**
`interfaces::check_all` **plus** `recipes_check` (main.rs:1179: each recipe expands with
defaults, its power contracts pass `interfaces::power::check`, its citations name library
sources), and drops the "no claim cites it" warning for sources a recipe cites. At 80ac000 it
passes: 351 claims (295 asserted, 48 contested, 8 promoted), 84 sources, 3 warnings (rp2350
sources `easy2350_core_audit`, `motor_controller_rev_a`, `rp2350b_minimal` cited by no claim).

Invariants: ids are globally unique across library and packages (links are by bare id); a
dispute is recorded on both sides; a document is listed once (package sources are reached as
`pkg/id`, and a package may not reuse a library source id).

## Solid

- `tests::contradictions_go_both_ways`, `tests::unknown_links_sources_and_statuses_fail`,
  `tests::package_sources_by_qualified_id`, `tests::query_by_scope_and_prefix`.
- `crates/ccomp/tests/library.rs`: `library_is_whole` (the real database passes) and
  `a_dispute_is_seen_from_both_sides` (`rp2350.vreg.lx_cutout` ↔ `buck.sw.keep_ground_under`).
  These run the `ccomp` binary, so they live with the CLI.

## Known wrong or weak

- **No quote check here.** `check` never looks at `quote`; quotes are checked only by
  `tools/library_quotes.py` (Python, needs `pdftotext` and the 155 MB untracked cache). 27
  claims (all in `packages/rp2350/rules.json`, the first package, written before the convention)
  cite with `locator` only: no quote, figure or measurement.
- The "source cited" bookkeeping keys on the last path part of the source id
  (`cit.source.rsplit('/')`, :186), so a package source `ds` cited as `chip/ds` also marks every
  other source called `ds` as cited. Only affects the unused-source warning.
- `claim(id)` and `incoming(id)` are linear scans, called per link inside `check`: O(n²) in
  claims. Fine at 351; not at the scale `research-library.md` describes.
- `scope` is untyped (`BTreeMap<String, Value>`); there is no vocabulary check on scope keys,
  attributes or `enforced_by` names (the spec wants statements "in the schema's vocabulary").
- `simulated` and `refuted` are statuses no claim has; the spec's "settling by simulation" and
  `evidence` records are not modelled beyond free fields.
- Statuses are per claim; nothing ties a package rule that a tool enforces (`enforced_by`) to
  the tool, so a renamed check leaves a stale name with no error.

## Not covered by tests

- `load` on a real tree from inside this crate (only via `ccomp` tests); a malformed claim file.
- Errors: empty statement, no citation, `promoted` without `enforced_by`, source with neither
  `url` nor `where`, bad `sha256`, a package source shadowing a library one.
- Warnings: missing locator, contested without link or note.
- `query` by `status`, by `text`, by `set` suffix; `expand`'s `linked_from` for a claim with
  several incoming links.

## Lessons to keep

- **Claims, not documents; contradictions are kept from both sides** (spec decision log:
  "manufacturer guidance is sometimes wrong; simulation settles disputes"). 17 disputes were on
  record at 2a6c562, e.g. TI AN-2155's EMI measurement against the RP2350 datasheet's switch-node
  cut-out.
- **Library claims and package rules are one database** (2a6c562): links cross between them
  (`rp2350.vreg.lx_cutout` contradicts `buck.sw.keep_ground_under`), so a chip's rule and the
  general guidance are seen together.
- **Each document listed once**: package sources are cited from anywhere as `pkg/id`; a package
  reusing a library id is an error, which stops two entries drifting apart.
- **A promoted claim names what enforces it**; `check` refuses a promotion with nothing behind it.
- Where a package departs from its datasheet because a real board does, the claim says so and is
  `contested` rather than the value being changed (f6b47a8).

## For the move

- Lands as `crates/library` (Understanding group). No dependencies to rename.
- `Library::load(root)` expects `root/library/sources.json`, `root/library/claims/` and
  `root/packages/*/rules.json`. The Boardroom layout lists `packages/` at the root but does not
  place the `library/` data folder; put it at the root beside `packages/` (also needed by
  `compose`, `pinsolve`, `interfaces`).
- Move `recipes_check` and the warning filter out of `ccomp/src/main.rs` (959-973, 1179-1203)
  into a library function (`library::check_all(root)` calling `interfaces::check_all` and a
  recipe check in `compose`), so `boardroom library check` and the server share it. Move the two
  `ccomp/tests/library.rs` tests to call that function, not a binary.
- `library_fetch.py`, `library_import.py`, `library_quotes.py` go to `packages/tools/` per the
  spec; `library/cache/` does not come (rebuilt by `library_fetch.py`; make its location a
  configured path, outside the repo).
- Differently: index claims by id once; add quote checking (or at least "has a quote, figure or
  measurement") as a `check` error with a migration for rp2350's 27; key "cited" sources by their
  resolved owner and id; type `scope` against intent's vocabulary.
