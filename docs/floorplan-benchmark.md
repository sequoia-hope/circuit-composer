# Floorplanner benchmark

Made by `tools/fp_bench.py` (see its header for how the boards are built). Each cell is over
5 seeds: legal runs / runs, then the median wire (mm) and median time (s) of the legal ones.
"Board" is the board's own placement scored the same way: our saved placements for our
boards, and for Cypress's boards the placement they ship (cut from larger boards, per its
authors; overlaps there are courtyard overlaps under our rules).

| Board | Parts (movable) | Nets | Board's own: wire, overlaps | sa 50 k | sa 200 k | sa 1000 k |
|---|---|---|---|---|---|---|
| rp2350-battery | 60 (60) | 40 | — | 5/5: 367 mm, 0.16 s | 5/5: 311 mm, 0.58 s | 4/5: 251 mm, 2.92 s |
| easy2350 | 58 (56) | 62 | — | 5/5: 937 mm, 0.31 s | 5/5: 898 mm, 1.14 s | 5/5: 792 mm, 5.52 s |
| bolt-earring | 327 (75) | 186 | — | 1/5: 1666 mm, 2.56 s | 1/5: 1179 mm, 3.50 s | 4/5: 1395 mm, 12.71 s |
| flame-earring | 297 (75) | 169 | — | 0/5 | 0/5 | 4/5: 896 mm, 11.14 s |
| cypress-small-1 | 57 (57) | 34 | 177, 11 | 5/5: 52 mm, 0.13 s | 5/5: 50 mm, 0.48 s | 5/5: 48 mm, 2.75 s |
| cypress-small-2 | 240 (240) | 85 | 482, 37 | 5/5: 316 mm, 0.51 s | 5/5: 246 mm, 1.78 s | 5/5: 257 mm, 8.42 s |
| cypress-small-3 | 63 (63) | 24 | 137, 29 | 5/5: 78 mm, 0.16 s | 5/5: 70 mm, 0.58 s | 5/5: 65 mm, 2.85 s |
| cypress-small-4 | 43 (43) | 39 | 274, 10 | 5/5: 50 mm, 0.13 s | 5/5: 47 mm, 0.50 s | 5/5: 45 mm, 2.35 s |
| cypress-small-5 | 41 (41) | 29 | 172, 11 | 5/5: 57 mm, 0.13 s | 5/5: 51 mm, 0.50 s | 5/5: 49 mm, 2.42 s |
| cypress-small-6 | 46 (46) | 21 | 79, 12 | 5/5: 68 mm, 0.14 s | 5/5: 48 mm, 0.51 s | 5/5: 37 mm, 2.47 s |
| cypress-small-7 | 50 (50) | 44 | 206, 9 | 5/5: 105 mm, 0.14 s | 5/5: 68 mm, 0.51 s | 5/5: 82 mm, 2.45 s |
| cypress-small-8 | 115 (115) | 84 | 902, 6 | 5/5: 262 mm, 0.30 s | 5/5: 222 mm, 1.00 s | 5/5: 218 mm, 4.45 s |
| cypress-small-9 | 476 (476) | 252 | 15376, 35 | 5/5: 3859 mm, 1.43 s | 5/5: 2211 mm, 3.93 s | 5/5: 1762 mm, 17.34 s |
| cypress-small-10 | 136 (136) | 92 | 1325, 5 | 5/5: 115 mm, 0.33 s | 5/5: 78 mm, 1.12 s | 5/5: 73 mm, 5.31 s |

| Board | Parts (movable) | Nets | Board's own: wire, overlaps | analytic alone | analytic 50 k | analytic 200 k | analytic 1000 k |
|---|---|---|---|---|---|---|---|
| rp2350-battery | 60 (60) | 40 | — | 3/5: 271 mm, 0.05 s | 5/5: 267 mm, 0.21 s | 5/5: 252 mm, 0.68 s | 4/5: 256 mm, 3.17 s |
| easy2350 | 58 (56) | 62 | — | 3/5: 988 mm, 0.22 s | 4/5: 858 mm, 0.49 s | 5/5: 818 mm, 1.31 s | 5/5: 805 mm, 5.66 s |
| bolt-earring | 327 (75) | 186 | — | 2/5: 1405 mm, 4.45 s | 4/5: 1145 mm, 5.22 s | 4/5: 995 mm, 6.96 s | 5/5: 908 mm, 16.52 s |
| flame-earring | 297 (75) | 169 | — | 0/5 | 0/5 | 0/5 | 3/5: 929 mm, 13.18 s |
| cypress-small-1 | 57 (57) | 34 | 177, 11 | 5/5: 55 mm, 0.03 s | 5/5: 50 mm, 0.11 s | 5/5: 49 mm, 0.59 s | 5/5: 48 mm, 2.79 s |
| cypress-small-2 | 240 (240) | 85 | 482, 37 | 5/5: 231 mm, 0.27 s | 5/5: 210 mm, 0.69 s | 5/5: 203 mm, 1.95 s | 5/5: 197 mm, 8.26 s |
| cypress-small-3 | 63 (63) | 24 | 137, 29 | 5/5: 70 mm, 0.03 s | 5/5: 64 mm, 0.11 s | 5/5: 64 mm, 0.54 s | 5/5: 60 mm, 2.83 s |
| cypress-small-4 | 43 (43) | 39 | 274, 10 | 5/5: 51 mm, 0.02 s | 5/5: 44 mm, 0.16 s | 5/5: 44 mm, 0.51 s | 5/5: 43 mm, 2.39 s |
| cypress-small-5 | 41 (41) | 29 | 172, 11 | 5/5: 56 mm, 0.04 s | 5/5: 51 mm, 0.16 s | 5/5: 49 mm, 0.52 s | 5/5: 48 mm, 2.38 s |
| cypress-small-6 | 46 (46) | 21 | 79, 12 | 5/5: 45 mm, 0.03 s | 5/5: 36 mm, 0.16 s | 5/5: 35 mm, 0.54 s | 5/5: 31 mm, 2.44 s |
| cypress-small-7 | 50 (50) | 44 | 206, 9 | 5/5: 157 mm, 0.05 s | 5/5: 104 mm, 0.15 s | 5/5: 97 mm, 0.54 s | 5/5: 86 mm, 2.42 s |
| cypress-small-8 | 115 (115) | 84 | 902, 6 | 5/5: 245 mm, 0.19 s | 5/5: 217 mm, 0.43 s | 5/5: 202 mm, 1.16 s | 5/5: 179 mm, 4.98 s |
| cypress-small-9 | 476 (476) | 252 | 15376, 35 | 5/5: 1624 mm, 1.47 s | 5/5: 1624 mm, 2.25 s | 5/5: 1624 mm, 4.97 s | 5/5: 1616 mm, 16.16 s |
| cypress-small-10 | 136 (136) | 92 | 1325, 5 | 5/5: 64 mm, 0.21 s | 5/5: 58 mm, 0.48 s | 5/5: 56 mm, 1.26 s | 5/5: 54 mm, 3.58 s |

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

