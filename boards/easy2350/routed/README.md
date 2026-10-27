# easy2350 routed by ultraroute, checked at the seams

The milestone-3 handoff, end to end: a board whose critical blocks are placed, routed and
locked goes to the router, and what comes back is checked against the intent.

**Input:** `../input/easy2350.kicad_pcb` (easy_pcb 2d56198) with its `.kicad_pro`, and the
compiled `../compiled/easy2350.kicad_dru`. 232 locked tracks and vias, among them RP2350's
regulator block, transplanted from Raspberry Pi's layout.

**Router:** ultraroute ad79cd7, built from a clone. It ran with easy_pcb's recipe and the flags
from ultraroute's own easy2350 job, without `--regions`, which ultraroute does not have yet:

    ultraroute route easy2350.kicad_pcb -o routed.kicad_pcb --strip --negotiate --rip-up --tighten \
      --prefanout --diff-pairs --shove-plough region --shove-reach 0.5 --shove-repair 20 --shove-ties \
      --shove-passes 60 --shove-patience 8 --planes In1.Cu,In2.Cu --cell 0.035 \
      --prefanout-order round --seed 1 --threads 1 --timeout 240

It took 69 s and left 1 connection open, which the pours join (`route.log`).

**Check:** `tools/verify_routed.py easy2350-ultraroute-ad79cd7.kicad_pcb ../intent.json
../compiled/routing.json --pro ../input/easy2350.kicad_pro --dru ../compiled/easy2350.kicad_dru
--refill` → `verify.json`:

| | |
|---|---|
| KiCad DRC, after a zone refill | 0 errors, 0 unconnected; 3 dangling-track warnings |
| Length limits (17, compiled into the `.kicad_dru`) | all met |
| Region rules (12, `routing.json`) | 0 violations |
| Locked copper | 232 items, all kept |
| Regulator block | unchanged: every buck metric equals the released board's |

One false alarm was found and fixed on the way. A via the router placed with its ring exactly on
the In1 cut-out's edge counted as inside; contact now counts as outside, to KiCad's 0.5 µm.

## Rerun with `--regions` (ultraroute 971e723)

`regions-971e723/`: the same recipe, plus `--regions ../compiled/routing.json`, on seeds 0, 1 and
2. ultraroute now blocks `never` and `must` regions in its grid and prices `avoid`/`prefer`. Each
run took 17–19 s and left 1 connection open, which the pours join. The seed-1 board is kept;
the logs and checks are kept for all three seeds.

| seed | DRC errors | unconnected | dangling warnings | lengths | region violations | locked kept | regulator block |
|---|---|---|---|---|---|---|---|
| 0 | 0 | 0 | 3 | 17/17 | 0 | 232 | unchanged |
| 1 | 0 | 0 | 2 | 17/17 | 0 | 232 | unchanged |
| 2 | 0 | 0 | 3 | 17/17 | 0 | 232 | unchanged |

On every seed, ultraroute's own report says `region lx_inner_cutout: 1 pieces break its Never
(vias) rule`. It is the GND via at (113.25, 122.2). Its 0.3 mm ring touches the cut-out's
edge at x = 112.95 with a gap of 0.000 mm. Our check and KiCad count contact as outside, to
0.5 µm. The router's grid allowed the via there, but its report counts contact as inside.
This was reported to ultraroute; no board needs changing.

**Fixed in ultraroute 724f835:** its report now uses the same rule as its grid, with 0.5 µm
tolerance. The same three seeds rerun with 724f835 give no broken-region lines. Our check still
finds 0 DRC errors, 0 unconnected, 17/17 lengths, 0 region violations and 232 locked items
kept, with the block unchanged. 724f835 also routes length-limited nets first.
