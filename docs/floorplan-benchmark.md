# Floorplanner benchmark

Made by `tools/fp_bench.py` (see its header for how the boards are built). Each cell is over
5 seeds: legal runs / runs, then the median wire (mm) and median time (s) of the legal ones.
"Board" is the board's own placement scored the same way: our saved placements for our
boards, and for Cypress's boards the placement they ship (cut from larger boards, per its
authors; overlaps there are courtyard overlaps under our rules).

| Board | Parts (movable) | Nets | Board's own: wire, overlaps | sa 50 k | sa 200 k | sa 1000 k |
|---|---|---|---|---|---|---|
| rp2350-battery | 60 (60) | 40 | — | 5/5: 367 mm, 0.07 s | 5/5: 311 mm, 0.27 s | 4/5: 251 mm, 1.47 s |
| easy2350 | 58 (56) | 62 | — | 5/5: 937 mm, 0.15 s | 5/5: 898 mm, 0.50 s | 5/5: 792 mm, 2.47 s |
| bolt-earring | 327 (75) | 186 | — | 5/5: 1666 mm, 1.03 s | 5/5: 1933 mm, 1.72 s | 5/5: 1547 mm, 5.52 s |
| flame-earring | 297 (75) | 169 | — | 5/5: 1488 mm, 0.66 s | 5/5: 1143 mm, 1.51 s | 5/5: 890 mm, 4.69 s |
| cypress-small-1 | 57 (57) | 34 | 177, 11 | 5/5: 52 mm, 0.06 s | 5/5: 50 mm, 0.23 s | 5/5: 48 mm, 1.14 s |
| cypress-small-2 | 240 (240) | 85 | 482, 37 | 5/5: 316 mm, 0.19 s | 5/5: 246 mm, 0.68 s | 5/5: 257 mm, 3.46 s |
| cypress-small-3 | 63 (63) | 24 | 137, 29 | 5/5: 78 mm, 0.08 s | 5/5: 70 mm, 0.28 s | 5/5: 65 mm, 1.38 s |
| cypress-small-4 | 43 (43) | 39 | 274, 10 | 5/5: 50 mm, 0.06 s | 5/5: 47 mm, 0.24 s | 5/5: 45 mm, 1.21 s |
| cypress-small-5 | 41 (41) | 29 | 172, 11 | 5/5: 57 mm, 0.06 s | 5/5: 51 mm, 0.21 s | 5/5: 49 mm, 1.08 s |
| cypress-small-6 | 46 (46) | 21 | 79, 12 | 5/5: 68 mm, 0.06 s | 5/5: 48 mm, 0.25 s | 5/5: 37 mm, 1.26 s |
| cypress-small-7 | 50 (50) | 44 | 206, 9 | 5/5: 105 mm, 0.10 s | 5/5: 68 mm, 0.24 s | 5/5: 82 mm, 1.01 s |
| cypress-small-8 | 115 (115) | 84 | 902, 6 | 5/5: 262 mm, 0.11 s | 5/5: 222 mm, 0.39 s | 5/5: 218 mm, 1.87 s |
| cypress-small-9 | 476 (476) | 252 | 15376, 35 | 5/5: 3859 mm, 0.58 s | 5/5: 2211 mm, 1.63 s | 5/5: 1762 mm, 7.41 s |
| cypress-small-10 | 136 (136) | 92 | 1325, 5 | 5/5: 115 mm, 0.14 s | 5/5: 78 mm, 0.44 s | 5/5: 73 mm, 2.11 s |

| Board | Parts (movable) | Nets | Board's own: wire, overlaps | analytic alone | analytic 50 k | analytic 200 k | analytic 1000 k |
|---|---|---|---|---|---|---|---|
| rp2350-battery | 60 (60) | 40 | — | 3/5: 271 mm, 0.03 s | 5/5: 267 mm, 0.10 s | 5/5: 252 mm, 0.32 s | 4/5: 256 mm, 1.42 s |
| easy2350 | 58 (56) | 62 | — | 5/5: 1280 mm, 0.33 s | 2/5: 956 mm, 0.49 s | 4/5: 1015 mm, 0.90 s | 3/5: 770 mm, 2.76 s |
| bolt-earring | 327 (75) | 186 | — | 5/5: 1029 mm, 2.45 s | 5/5: 1029 mm, 2.59 s | 5/5: 983 mm, 3.56 s | 5/5: 933 mm, 7.59 s |
| flame-earring | 297 (75) | 169 | — | 5/5: 972 mm, 2.40 s | 5/5: 972 mm, 2.83 s | 5/5: 936 mm, 3.39 s | 5/5: 871 mm, 7.16 s |
| cypress-small-1 | 57 (57) | 34 | 177, 11 | 5/5: 55 mm, 0.02 s | 5/5: 50 mm, 0.08 s | 5/5: 49 mm, 0.24 s | 5/5: 48 mm, 1.14 s |
| cypress-small-2 | 240 (240) | 85 | 482, 37 | 5/5: 231 mm, 0.11 s | 5/5: 210 mm, 0.27 s | 5/5: 203 mm, 0.82 s | 5/5: 197 mm, 4.02 s |
| cypress-small-3 | 63 (63) | 24 | 137, 29 | 5/5: 70 mm, 0.03 s | 5/5: 64 mm, 0.12 s | 5/5: 64 mm, 0.28 s | 5/5: 60 mm, 1.27 s |
| cypress-small-4 | 43 (43) | 39 | 274, 10 | 5/5: 51 mm, 0.02 s | 5/5: 44 mm, 0.07 s | 5/5: 44 mm, 0.30 s | 5/5: 43 mm, 1.09 s |
| cypress-small-5 | 41 (41) | 29 | 172, 11 | 5/5: 56 mm, 0.02 s | 5/5: 51 mm, 0.06 s | 5/5: 49 mm, 0.22 s | 5/5: 48 mm, 1.15 s |
| cypress-small-6 | 46 (46) | 21 | 79, 12 | 5/5: 43 mm, 0.02 s | 5/5: 35 mm, 0.10 s | 5/5: 34 mm, 0.26 s | 5/5: 31 mm, 1.17 s |
| cypress-small-7 | 50 (50) | 44 | 206, 9 | 5/5: 157 mm, 0.02 s | 5/5: 104 mm, 0.07 s | 5/5: 97 mm, 0.22 s | 5/5: 86 mm, 0.99 s |
| cypress-small-8 | 115 (115) | 84 | 902, 6 | 5/5: 245 mm, 0.08 s | 5/5: 217 mm, 0.17 s | 5/5: 202 mm, 0.45 s | 5/5: 179 mm, 1.94 s |
| cypress-small-9 | 476 (476) | 252 | 15376, 35 | 5/5: 1622 mm, 0.61 s | 5/5: 1622 mm, 1.02 s | 5/5: 1622 mm, 2.02 s | 5/5: 1609 mm, 7.63 s |
| cypress-small-10 | 136 (136) | 92 | 1325, 5 | 5/5: 64 mm, 0.10 s | 5/5: 58 mm, 0.20 s | 5/5: 56 mm, 0.50 s | 5/5: 54 mm, 2.12 s |

## Findings, 2026-10-31: the negotiated legaliser

The tables above are from after this change. The 2026-10-30 findings below quote the tables
before it.

`repair` is replaced by `negotiate` (`crates/floorplan/src/legal.rs`). A part still overlapping
after `legalise` takes the spot that costs least: its distance from where it was, plus a price for
each movable part it would evict. Fixed parts, the board edge and block separations are never
crossed. The evicted parts go next, and each eviction raises that part's price, as PathFinder's
history cost does. Candidate spots are where the part abuts its neighbours, tried nearest first.

- **The earrings are legal at every budget** (5/5 in every cell, both placers). Before, the
  flame earring was 0/5 below 1 M moves and the bolt was 1/5 with SA at 50 k and 200 k. On
  2026-10-30 the flame earring, seed 4, had no clear spot for D2, Q4, Q7 or Q8 anywhere on the
  board (a 0.1 mm scan, both orientations). It had room only once neighbours moved.
- **A short anneal is now legal on the earrings, but long.** SA at 50 k gives 1488–1666 mm
  against 871–890 mm at 1 M. The analytic start is the better short run: 972 mm with no anneal.
- **The analytic placer alone** takes about 2.4 s on the earrings (it was 4–5 s, mostly in
  `repair`'s random search). The analytic start legalises twice, once for the warm start and
  once at the end. The first pass does most of the work: about 230 placements.
- **easy2350, analytic, 20 seeds** (5 seeds are too few here; that row moves between runs):
  20/20 legal with no anneal (was 6/20), 16/20 at 50 k (was 12/20), 18/20 at 200 k (was
  14/20), and shorter wire at each (817, 773 and 769 mm against 980, 993 and 838). rp2350-battery
  is unchanged at 20 seeds. Its illegal runs with no anneal are a connector facing inward and a
  separation 0.01 mm short, not overlaps.
- **Separations are kept, not repaired.** A first version ignored them and parked a part of
  easy2350's core regulator against its crystal. A separation miss scores less than an overlap,
  so that result was kept. `negotiate` now refuses any spot that brings a block too close to its
  other block. One already too close without the part being moved is left as it is.
- Times in both tables are lower than on 2026-10-30 for boards this change doesn't touch, so the
  machine was less loaded. Compare times within one run.

## Findings, 2026-10-30

### The analytical placer (`--placer analytic`)

It places by gradient descent (`crates/floorplan/src/analytic.rs`), then legalises and anneals
cool for the budget. "Analytic alone" is 0 moves: gradient descent and legalising only.

- **On the Cypress boards it beats SA at every budget on nine of ten**, and usually by more
  than SA gains from five times the moves. small-9 (476 parts): 1624 mm alone in 1.5 s,
  against SA's 1762 mm after 1 M moves and 17 s. small-10: 58 mm at 50 k against 115 mm.
  small-8 at 1 M: 179 mm against 218 mm.
- **The exception is small-7**: 97 mm at 200 k against SA's 68 mm. It starts from a worse
  basin, and the cool anneal can't leave it.
- **On small-9 the anneal never improves the start**: every budget hands back the legalised
  start (1624 mm), because nothing the cool anneal finds scores better.
- **Our boards:** rp2350-battery is 5/5 legal at 50 k with 267 mm (SA: 367 mm), and easy2350 is
  4/5 with 858 mm (SA: 5/5, 937 mm). At 1 M, SA's easy2350 is a little shorter (792 mm against
  805 mm). The bolt earring is legal 4/5 at 50 k (SA: 1/5) and 908 mm at 1 M (SA: 1395 mm).
  The flame earring is no better, for the reason under "The earrings" below: the start doesn't
  change how full its back is.
- **Alone, it's often illegal on our boards** (3/5): the legaliser leaves a wedged part. Even
  50 k moves of cool anneal fixes most of these.
- **The earrings take 4–5 s with 0 moves**, against 0.02–1.5 s elsewhere. That hasn't been
  profiled yet.
- **Separation** at first had a constant pull, and blocks that must stay apart ended up
  interleaved. It now grows with the overlap weight, as a hard rule should.

The app now starts a placement analytically by default ("start: analytic"; "random" is SA).

### The earrings


- **The Cypress boards place legally every time**, at 50 k moves in 0.1–0.6 s. Wire keeps falling
  to 1 M moves on the larger ones (small-9, 476 parts: 3859 to 1762 mm). Their shipped
  placements carry 5–37 courtyard overlaps under our rules, so their wire is a reference, not a
  target.
- **The earrings are rarely legal below 1 M moves** (bolt 1/5, flame 0/5 at 200 k). It's not the
  edge connector: their backs carry about 180 small reserved keep-outs (rail vias, bypass-cap
  slots), and the anneal leaves overlaps of 0.04–0.6 mm² against them that `legalise` and
  `repair` don't clear. `build.sh` gets a legal one from eight seeds at 400 k.

  I tried a nearest-first repair (rings about the part, every quarter turn), and it found no
  clear spot anywhere for those parts (flame earring, seed 1: six parts). Those backs are full
  once their reservations count: the reservations were planned around one placement. That's
  not something a repair can fix. What would: a legalizer that pushes neighbours as well (a
  shove, or a small exact solve over the crowded patch), or reserving vias with the placement
  instead of before it. The ring search was reverted.
- **small-1** is in from the second run (a stale download kept it out of the first).

