# Floorplanner benchmark

Made by `tools/fp_bench.py` (see its header for how the boards are built). Each cell is over
5 seeds, run on sequoia-server: legal runs / runs, then the median wire (mm) and median time (s) of the legal ones.
"Board" is the board's own placement scored the same way: our saved placements for our
boards, and for Cypress's boards the placement they ship (cut from larger boards, per its
authors; overlaps there are courtyard overlaps under our rules).

| Board | Parts (movable) | Nets | Board's own: wire, overlaps | sa 50 k | sa 200 k | sa 1000 k |
|---|---|---|---|---|---|---|
| rp2350-battery | 60 (60) | 40 | — | 5/5: 367 mm, 0.16 s | 5/5: 311 mm, 0.51 s | 4/5: 251 mm, 2.47 s |
| easy2350 | 58 (56) | 62 | — | 5/5: 937 mm, 0.30 s | 5/5: 898 mm, 1.13 s | 5/5: 792 mm, 5.51 s |
| bolt-earring | 327 (75) | 186 | — | 1/5: 1666 mm, 2.28 s | 1/5: 1179 mm, 3.50 s | 4/5: 1395 mm, 11.41 s |
| flame-earring | 297 (75) | 169 | — | 0/5 | 0/5 | 4/5: 896 mm, 9.34 s |
| cypress-small-1 | 57 (57) | 34 | 177, 11 | 5/5: 52 mm, 0.14 s | 5/5: 50 mm, 0.55 s | 5/5: 48 mm, 2.79 s |
| cypress-small-2 | 240 (240) | 85 | 482, 37 | 5/5: 316 mm, 0.50 s | 5/5: 246 mm, 1.68 s | 5/5: 257 mm, 7.39 s |
| cypress-small-3 | 63 (63) | 24 | 137, 29 | 5/5: 78 mm, 0.12 s | 5/5: 70 mm, 0.58 s | 5/5: 65 mm, 2.85 s |
| cypress-small-4 | 43 (43) | 39 | 274, 10 | 5/5: 50 mm, 0.14 s | 5/5: 47 mm, 0.51 s | 5/5: 45 mm, 2.20 s |
| cypress-small-5 | 41 (41) | 29 | 172, 11 | 5/5: 57 mm, 0.11 s | 5/5: 51 mm, 0.48 s | 5/5: 49 mm, 2.15 s |
| cypress-small-6 | 46 (46) | 21 | 79, 12 | 5/5: 68 mm, 0.14 s | 5/5: 48 mm, 0.43 s | 5/5: 37 mm, 2.39 s |
| cypress-small-7 | 50 (50) | 44 | 206, 9 | 5/5: 105 mm, 0.15 s | 5/5: 68 mm, 0.52 s | 5/5: 82 mm, 2.10 s |
| cypress-small-8 | 115 (115) | 84 | 902, 6 | 5/5: 262 mm, 0.29 s | 5/5: 222 mm, 0.91 s | 5/5: 218 mm, 4.74 s |
| cypress-small-9 | 476 (476) | 252 | 15376, 35 | 5/5: 3859 mm, 1.32 s | 5/5: 2211 mm, 3.84 s | 5/5: 1762 mm, 11.51 s |
| cypress-small-10 | 136 (136) | 92 | 1325, 5 | 5/5: 115 mm, 0.34 s | 5/5: 78 mm, 1.11 s | 5/5: 73 mm, 4.08 s |

## Findings, 2026-10-30

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

