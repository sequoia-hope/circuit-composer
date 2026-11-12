# library/ → boardroom library/ (data)

`library/` is the research library's data plus the data the composer and pin solver share:
cited **claims** about circuit classes, the **sources** they cite, the interface **types**, the
**recipes** (parametric board-level blocks), and the **stock** recipes choose parts from. 21
tracked files, about 300 KB; plus `library/cache/`, **155 MB, untracked** (git-ignored), the
downloaded documents. Code that reads it: `crates/library` (claims; see `library.md`),
`crates/interfaces` (`interfaces.json`), `crates/compose` (`recipes/`, `stock.json`),
`crates/pinsolve` (all three), `ccomp library check|query`, `ccomp recipe`, and three Python
tools. The design is the system spec's "Research library" section. `library/README.md` is the
manual and is accurate except where noted below. The Boardroom spec's layout does not place
this folder; put it at the root beside `packages/`, because `compose`, `pinsolve` and
`interfaces` find it as `packages/../library`.

## What is in it

| Path | Format | Size today |
|---|---|---|
| `sources.json` | source id → `title`, `publisher`, `doc`, `kind` (datasheet, app_note, article, design_guide, standard, talk), `revision` as printed, `url` (or `where`), `sha256`, `bytes`, `retrieved`, optional `member` (PDF inside a zip), `vendor_url` | 62 sources (31 app notes, 22 datasheets, 6 articles, 1 design guide, 1 standard, 1 talk), all hashed |
| `claims/TOPIC.json` | `{ "claims": [...] }`; a claim: `id` (dotted, `buck.sw.keep_ground_under`), `statement` (one sentence), `scope` (circuit, block, role, chip…), `attributes` (intent attribute names), `citation` or `citations` (each `source`, `locator` = section and printed page, and one of `quote` ≤ 25 words verbatim, `figure` (description of a figure/table), `measured`), `status` (asserted, contested, simulated, promoted, refuted), `links` (`{supports|refines|contradicts: id}`), `enforced_by`, `note` | 5 topics, 143 claims: buck 30, crystal 25, half_bridge 28, interface 30, signal 30 |
| `interfaces.json` | interface types (see `interfaces.md`), with `i2c.rules.pullup` (SLVA689A's three modes: tr, Cb max, VOL at 3 mA; `pin_pf` 10 and `wiring_pf` 20 are stated assumptions) | 10 types |
| `recipes/NAME.json` | a parametric circuit (see `compose.md`): params, parts with `choose`/`from`/`for`/`if`, nets, ports with `volts`/`levels`/`power`, interfaces, `requires`, `citations` (with quotes) | 12: button_esd, divider, enable_or, i2c_pullups, led, led_chain, light_sensor, lipo_load_share, long_press_reset, nfet_level_shift, power_latch, usb_c_sink |
| `stock.json` | tables (one footprint and series each): `symbol`, `footprint`, `checked`, `parts` (`value`, `si`, `lcsc`, `mpn`, JLC `type`) | 1 table: `resistor_0402`, 29 JLC basic/preferred values, checked 2026-10-30 |
| `cache/` | `SOURCEID.pdf` (or `.html` + `.txt` for articles), `.txt` beside PDFs with `--text` | 79 PDFs, 80 text files, 1 HTML; 155 MB. Never committed: the vendors own them. |

Package claims (`packages/*/rules.json`, 208 claims) and package sources
(`packages/*/sources/sources.json`, 22) are part of the same database: `ccomp library check` at
80ac000 reports 351 claims (295 asserted, 48 contested, 8 promoted; none simulated or refuted)
and 84 sources. A citation names a library source by id, its own package's by id, or another
package's as `pkg/id`; a document is listed once.

## The rule: every figure is cited, with a quote a tool checks

Every number or design rule in the library or a package carries `source` + `locator`, and a
claim's citation carries a verbatim `quote` (≤ 25 words), or a `figure` description when the
fact is in a drawing or a text-less scan, or a `measured` record for a reference board. Then
`tools/library_quotes.py` finds every quote in the cached document. This is how an agent's
extraction is kept honest: a paraphrase or invented figure fails.

Where it holds and where it does not (checked 2026-11-12):
- Claims: 665 citations, 594 with `quote`, 43 `figure`, 2 `measured`; **27 with neither**, all
  in `packages/rp2350/rules.json` (the first package, before the convention).
- `packages/*/ports.json` (logic levels, power contracts, interface facts): 0 quotes; source +
  locator only. `library_quotes.py` does not read these files.
- `library/recipes/*.json`: quotes present (72), but `library_quotes.py` does not read recipes
  either.
- `ccomp library check` checks that every cited source id exists, not that a quote does.
At 2a6c562: 392 quotes, 369 found whole, 23 `scattered` (two-column pages, 21 from ROHM's note)
confirmed by eye, 0 missing. At b96844e: 570 found.

## The Python pipeline

All three print one JSON result and exit non-zero on failure.

- `tools/library_fetch.py [--root .] [--cache library/cache] [--only ID…] [--text]` (112 lines):
  downloads every source with an http URL from `library/sources.json` and every
  `packages/*/sources/sources.json` into the cache, unzips `member`, compares SHA-256. A PDF whose
  hash changed fails (the vendor published a new revision: re-read it and re-check the claims
  citing it); a web article's change is reported only (ads drift). Articles are kept as HTML and
  stripped text. `--text` writes `pdftotext -layout` beside each PDF. Sends a generic browser-like
  User-Agent. Sources with `where` and no URL (rp2350's local audit, the motor controller board)
  are skipped.
- `tools/library_quotes.py [--root .] [--cache …] [--claims FILE…]` (114 lines): for each claim
  quote in `library/claims/*.json` and `packages/*/rules.json`, searches each page of the cached
  document in both `pdftotext` modes (`-layout` and plain), normalised (case, spacing, line-end
  hyphens, typographic punctuation, µ/Ω, CJK kept); `...` elisions require every piece on the
  page; a quote across a page break is tried; failing that, words in order within 60 words of
  each other on one page = `scattered` (for a person to confirm). Reports PDF pages so locators
  can be compared. Needs `pdftotext` (poppler) and the cache.
- `tools/library_import.py EXTRACTION.json [--note] [--replace]` (92 lines): adds an agent's
  extraction `{topic, sources, claims}` as `library/claims/TOPIC.json`, merging sources (a reused
  id must have the same sha256), refusing claim ids used elsewhere, moving a parenthesised quote
  to `figure`, writing the compact style. Library topics only; package `rules.json` files are
  written by hand (by an agent) and checked the same way afterwards.

Workflow for a topic: agent reads sources → extraction JSON → `library_import.py` →
`library_fetch.py && library_quotes.py` → `ccomp library check`. For a package: the agent writes
`rules.json`, `ports.json`, `sources/sources.json`, circuit, README; then `library_fetch.py`,
`library_quotes.py`, `circuit_check.py`, `ccomp library check`, and compose it.

## Known wrong or weak

- The cache (155 MB for 84 sources, about 2 MB a document) lives inside the repo tree, ignored;
  every clone must rebuild it by downloading, and some vendors refuse automated fetches (the
  README's Gaps table: ADI timed out, TI SLUP230 needs a login, Silicon Labs and Nexperia 403,
  NXP 404, IPC paywalled; ST AN2867 read at Rev 10 from a mirror). A refused source cannot be
  re-checked by anyone without the original file.
- `library_quotes.py` re-runs `pdftotext` twice on every cited document on every run; no text
  cache keyed by hash.
- No quote checking for `ports.json` and recipes (above), and no check that a claim has at least
  one of quote/figure/measured.
- README's topic table is from 2026-10-27 and its recipe list names 6 of the 12 recipes; the
  root README's "143 cited claims from 41 vendor documents" counts library topics only.
- `stock.json` has one table; JLC stock and prices are not recorded as data anywhere (package
  manifests keep them as comments), so the supply rule cannot rank by stock.
- No claim is `simulated` or `refuted`: the spec's settling of disputes by simulation has no
  slow-tier verifier to run it yet.
- Gaps the README records: no claim gives clearance by voltage (IPC-2221 paywalled); no mm value
  for buck feedback clearance or crystal keep-out; CAN sources predate CAN FD.

## What it would need to scale (LCSC, STM32, ESP32 mining)

Data mining at scale is a first-class track for the owner. Today's shape is hand-ingestion: an
agent reads one datasheet and writes 9-27 claims, a ports.json and a circuit per chip family,
about a dozen families so far. To go to thousands of parts:

1. **Separate parametric part data from claims.** LCSC-scale data (MPN, LCSC number, package,
   value, ratings, JLC type, stock and price at a date) is a table, not claims. Give it its own
   store (per-category files or SQLite), keyed by LCSC/MPN, each row citing its source (the LCSC
   API or `jlc_parts.py` snapshot, with a retrieval date). `stock.json` becomes a view over it
   (resistors, capacitors, LEDs…), and the supply rule's missing "stock" ranking reads it.
2. **Machine-extracted figures marked as such.** A figure parsed by a script from a datasheet
   table should carry a provenance kind like intent's `recognised` (with confidence and the
   page) and be **soft until a person or a quote check confirms it**, by the same rule that keeps
   recognised guesses from setting hard limits.
3. **Pin and mux tables from machine-readable sources where they exist.** `rp2350_mux.py` parses
   one PDF table. For STM32, ST publishes CubeMX's per-part pin and alternate-function XML
   (e.g. the STM32_open_pin_data repository); for ESP32, ESP-IDF's SoC headers and the TRM's
   IO_MUX tables (both unverified here; check licences). One importer per vendor format that
   writes `ports.json` (`variants`, `mux`, `interfaces`) and a `--check` mode, as
   `rp2350_mux.py` has. Families with hundreds of variants need `ports.json` generated, and
   per-variant circuits generated from one template rather than one file each.
4. **Sources at scale.** Shard `sources.json` (per family or vendor); deduplicate by sha256;
   move the cache to a configured path outside the repo (it is already 155 MB), content-addressed
   by hash, with a text cache so quote checks are incremental; record why a fetch failed so
   unreachable vendors are retried or replaced by a mirror with the same hash.
5. **Checks that scale.** `library::check` resolves links and ids by linear scan (O(n²)); index
   once. `library_quotes.py` should take a changed-files list. Extend quote checking to
   `ports.json` (add `quote` to `interfaces::Citation`) and recipes, and make "no quote, figure
   or measurement" an error.
6. **Licensing kept apart.** Datasheets stay out of the repo (as now). Mined boards from others
   are read in place with their licence recorded (flight controllers are CERN-OHL-S and GPL;
   PCBench has mixed licences and must never enter the repo).
7. **A narrow first sweep**, as milestone 7 of the system spec says: one LCSC group
   (synchronous bucks in one package) end to end, power contracts included, so the supply rule
   has real choices; then ESP32 and nRF families.

## For the move

- Copy `library/` without `cache/` (spec: `library/cache/` does not come). Make the cache path
  configurable for `library_fetch.py` and `library_quotes.py` (`--cache` exists; default it from
  config, not the repo tree).
- Tools go to `packages/tools/` (spec): `library_fetch.py`, `library_import.py`,
  `library_quotes.py`. They assume `--root` holds `library/` and `packages/`.
- Rust readers find it as `ROOT/library` (`library::Library::load`, `interfaces::check_all`) or
  `packages/../library` (`compose::library_dir`, `pinsolve::load`, `ccomp pins`). Tests reading
  it: `compose/tests/library_recipes.rs`, `interfaces` unit tests, all `pinsolve` tests,
  `ccomp/tests/library.rs`.
- Update `library/README.md`'s counts and recipe list after the move; drop the root README's
  stale figures.
