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
