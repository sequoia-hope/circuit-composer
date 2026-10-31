# Floorplanning approaches compared

Status: survey, 2026-10-29. What else could place our boards, judged against what the floorplanner
does today, and what to try first. Citations were checked to exist; where a claim comes only from
a vendor or from an abstract, it says so.

## Where we start

`crates/floorplan` places every part by simulated annealing. Its cost is weighted half-perimeter
wirelength (power and ground left out), courtyard overlap with a weight that rises through the
run, board area, `near` distance (a decoupling capacitor to its pin), block cohesion, edge
connectors flush and facing out, block separations, and parts outside a rectangle or a polygon
outline (a signed-distance grid). Rigid groups move and turn as one, locked parts stay, and
through-hole pads collide with the other side. A part's side is an input, not a decision. Moves
are shift, quarter turn and swap. After the anneal, `legalise` pushes overlaps apart and
`negotiate` (`legal.rs`) re-seats anything still in trouble, evicting movable neighbours to make
room where it must. A warm start anneals cooler, with shorter moves, from a given placement.

The weakness is that every move calls `State::cost`, which rebuilds every pose and box and walks
every pair of parts: O(n²) per move. The bolt earring (327 parts) takes 20 s native for 200k
moves. Yet only 75 of its parts move (252 are fixed LEDs and rails), and the same holds for the
other examples:

| Example | Parts | Movable | Nets | Outline |
|---|---|---|---|---|
| bolt-earring | 327 | 75 | 186 | polygon |
| flame-earring | 297 | 75 | 169 | polygon |
| easy2350 | 58 | 56 | 62 | rectangle |
| rp2350-battery | 60 | 60 | 40 | none (output) |

So our real problems are 50–80 movable parts among up to ~330, and the stated range goes to 500.
That is small by chip standards, which matters for which published results transfer.

Two unrelated things are called "diffusion" below: *density diffusion* (Ren et al. 2005), which
moves parts down a heat-equation density gradient, and learned *diffusion models*.

## The approaches

### Simulated annealing, and SA with incremental cost

Random moves, accepted when better or, with probability falling with temperature, when worse
([Kirkpatrick, Gelatt and Vecchi, Science 1983](https://doi.org/10.1126/science.220.4598.671);
TimberWolf brought it to placement). Any cost can be added, which is why ours carries every
constraint natively. The fix for our speed is standard: a move changes one part (or one group),
so only the nets on its pins and the parts near it need re-scoring. With per-net bounding boxes
cached and a spatial hash for overlap, a move costs O(pins on the part + neighbours) instead of
O(n²). The block-centre and area terms need running sums and a cached bounding box; the
separation term needs per-block boxes. Our estimate is one to two orders of magnitude on the
earrings; that is a guess to measure, not a result.

SA is deterministic per seed, needs no data, warm-starts and already runs in a worker, but
quality grows slowly with moves and it ignores gradients. On PCBs there is published evidence both ways:
[SA-PCB](https://github.com/The-OpenROAD-Project/SA-PCB) (OpenROAD, archived) is the open
baseline in Cypress's comparison below, where it routed 82–100% of pin pairs but had 4–50× the
wirelength of the best tool. That SA-PCB balanced its terms badly (it "pushes components to the
borders", per the Cypress authors), so it is evidence about one SA, not about SA. On chips, the
updated UCSD assessment ([Cheng et al., ISPD 2023, rev. 2026](https://arxiv.org/abs/2302.11014))
found a well-tuned, multithreaded SA competitive with RL for macro placement.

### Analytical (electrostatic) global placement, plus legalization

Wirelength is smoothed (log-sum-exp or weighted-average), overlap becomes a density penalty, and
the whole cost is minimised by gradient descent. ePlace models each part as a charge and solves
the field by FFT, then uses Nesterov's method
([Lu et al., TODAES 2015](https://doi.org/10.1145/2699873)); RePlAce adds routability-driven
inflation ([Cheng et al., TCAD 2019](https://doi.org/10.1109/TCAD.2018.2859220)); DREAMPlace
puts it on GPU through a deep-learning framework
([Lin et al., DAC 2019](https://doi.org/10.1145/3316781.3317803),
[code](https://github.com/limbo018/DREAMPlace)). A legalizer then removes the remaining overlap
(Tetris-style, or [Abacus](https://doi.org/10.1145/1353629.1353640) for rows).

This is the one approach with a strong PCB result. **Cypress** (Zhang et al., Cornell and
NVIDIA, ISPD 2025, [paper](https://www.csl.cornell.edu/~zhiruz/pdfs/cypress-ispd2025.pdf),
[code](https://github.com/NVlabs/Cypress)) builds on DREAMPlace for two-sided PCBs. It adds an
orientation-aware wirelength in which each part's legal orientations are a Gumbel-softmax choice,
a differentiable net-crossing term between two-pin connections, halos for spacing, fixed parts
excluded from updates, and legalization of each side separately on one CPU thread. It ships an
open benchmark of ten small designs of 41–476 components, our range exactly. Routing all of them
with Freerouting on two layers, Cypress routed 100% of pin pairs on all ten, against SA-PCB
81.5–100%, NS-Place 17–93%, Quilter 100% (it failed on the largest) and the human placement
100% on eight (98.5% and 90.4% on the others). Its HPWL averaged 0.07 of SA-PCB's. Caveats: the human baselines were cut from larger
boards, the authors ran their own comparison, its results are on their own new suite, the
parameter search took 3–29 minutes on four V100s, and nobody has replicated it yet as far as we
found.

What it handles natively: wirelength, density, fixed parts, rotation (relaxed, then snapped),
two sides (separate density maps), spacing. Bolted on: edge connectors, separations, `near`,
block cohesion and the shaped outline are all smooth penalties that drop straight in; our SDF
is already bilinear and so differentiable. A rigid group is one body whose gradient is the sum of
its members' through the chain rule. Our sizes need no GPU: 80 movable parts and a 64×64 density
grid are a small FFT per iteration, so a Rust implementation should run hundreds of iterations a
second in WASM (an estimate). It is deterministic on the CPU, needs no data, and warm-starts
naturally (start the descent from the current placement with the density weight already high).
It leaves overlap to the legalizer, and our parts vary 100× in size, so our `legalise`/`negotiate`
and a short warm SA would be its detailed stage.

### Quadratic and force-directed placement

Connections are springs and the placement is the solution of a sparse linear system, with extra
forces or anchors to spread parts. The first PCB version is old
([Quinn and Breuer, IEEE Trans. Circuits and Systems 1979](https://doi.org/10.1109/TCS.1979.1084652));
modern chip versions alternate the solve with a rough legalization that pulls parts toward
spread positions ([SimPL, Kim, Lee and Markov, ICCAD 2010](https://web.eecs.umich.edu/~imarkov/pubs/conf/iccad10-simpl.pdf)).
It is the fastest thing here: one solve on 80 parts is microseconds, so it runs on every drag.
It handles wirelength and locked parts (anchors) natively and nothing else; rotation, sides,
edges, outline and separation are all heuristics around it, and quadratic wirelength over-weights
long nets. Deterministic, no data. Evidence is decades of chip use; on PCBs, mostly historical.
Its role for us: a better start than our random spread, and a live preview while dragging.

### Density-diffusion migration (Ren et al. 2005)

Density is diffused as heat, and each part moves with velocity −∇d/d, so crowded regions spread
while relative order is kept ([Ren, Pan, Alpert and Villarrubia, DAC 2005](https://doi.org/10.1145/1065579.1065712);
extended with legalization in TCAD 2007). It is a migration and legalization method: it takes a
good placement and spreads it, and has no wirelength objective of its own. ultraroute built it
(`copper-morph::diffuse`) and measured no gain: spreading by routing demand made every board worse
to route (bluebuzz-1024 10 open connections unspread, 29 spread). Its ROADMAP gives the reasons:
pin demand near fine-pitch parts put peak density at 2.4–7 against a target of 0.8, so nearly
every part moved its full limit, and a moved board started its route afresh in the same time
budget. Fast, deterministic, no data. Evidence: chip-only in the literature, negative so far on our
PCBs. It belongs after routing, as a spreader toward open airlines (ultraroute's next step).

### Learned diffusion models for placement

A denoiser learns to turn random positions into placements, conditioned on the netlist. At
sampling time, gradients of hand-written costs (overlap, wirelength) steer each step, so legality
is guidance, not a guarantee.

- **Lee et al., Chip Placement with Diffusion Models** ([ICML 2025](https://proceedings.mlr.press/v267/lee25y.html),
  [arXiv](https://arxiv.org/abs/2407.12282)). Conditions on the netlist graph: node features are
  object width and height, edge features are pin offsets; coordinates are normalised to the
  canvas. A GNN with attention, 0.2–6.3M parameters. Trained only on synthetic circuits (random
  non-overlapping objects, Rent's-rule pin counts, distance-biased connections; about 100k circuits
  of ~200 objects a day on 32 CPUs). Guidance is an overlap potential plus HPWL, with a learned
  Lagrangian weight; 1000 DDPM steps; minutes per circuit on a GPU. It places macros (and
  clusters); standard cells go to DREAMPlace. Reported >50% lower HPWL than learned baselines on
  macro-only IBM and ISPD2005, ~4% better than DREAMPlace on mixed-size. No rotation, fixed
  objects or irregular canvas.
- **MacroDiff+**, Physics-Guided Geometric Diffusion for Macro Placement Generation
  ([Yoon, Jeon and Kang, IJCAI 2026](https://arxiv.org/abs/2605.16451),
  [code](https://github.com/jhy00n/MacroDiff-plus)): heterogeneous GNN plus Transformer denoiser,
  physics gradients during sampling; reports 6.1–6.2% lower wirelength on ISPD2005 macro
  benchmarks. I read only the abstract: its training data and legality mechanism are not
  confirmed here.
- **FlowPlace** ([Xie et al., DAC 2026](https://arxiv.org/abs/2604.23658)): flow matching instead
  of diffusion, mask-guided synthetic data, and hard-constraint sampling; reports zero overlaps
  and 10–50× faster sampling. Abstract only.

For us: native handling covers non-overlap (soft) and the canvas bound. Rotations, two sides,
edges, rigid groups, separations, polygon outlines and locked parts would all have to be either
in the conditioning (and so in the training data) or in the guidance. Guidance is the practical
route, and the guidance terms are exactly the differentiable cost an analytical placer needs.
Data is the hard part: there is no PCB placement corpus of any size. [PCBench](https://github.com/PCBench/PCBench)
has 164 KiCad boards, but it is a routing dataset; Cypress's suite has ten. Synthetic boards,
which our composer can generate from packages, are the realistic source, and Lee et al. show
synthetic-only training can transfer, on chips. A model of a few million parameters could run in
a browser on WebGPU, but nobody has shown it, and 1000 steps is far from interactive (FlowPlace's
20–50 would help). Deterministic given the seed on one device. Evidence: chip-only, macro-only,
unreplicated by independent groups, and no PCB result at all. The genuine appeal for PCBs is
diversity: sampling gives several distinct starting placements cheaply, which is ultraroute's
Phase 2b ("several complete layouts").

### Reinforcement learning

An agent places parts one at a time (or adjusts them) and is rewarded on the final cost. AlphaChip
([Mirhoseini et al., Nature 2021](https://doi.org/10.1038/s41586-021-03544-w)) is the reference,
and its claims are disputed: the UCSD assessment ([Cheng, Kahng, Kundu, Wang and Wang, ISPD 2023](https://doi.org/10.1145/3569052.3578926))
and Markov's meta-analysis ([The False Dawn, arXiv](https://arxiv.org/abs/2306.09633)) found it
behind SA and commercial placers; the authors replied that the critics skipped pre-training and
under-trained ([That Chip Has Sailed, arXiv 2024](https://arxiv.org/abs/2411.10053)). A
benchmark that judged AI chip placers by final PPA rather than proxies found them unsatisfying and
weakly correlated with their proxy metrics ([ChiPBench, NeurIPS 2025](https://arxiv.org/abs/2407.15026)).

On PCBs: **RL_PCB** ([Vassallo and Bajada, DATE 2024](https://ieeexplore.ieee.org/document/10546526),
[code](https://github.com/LukeVassallo/RL_PCB)) learns policies by which each part adjusts its
own position, and claims lower post-route wirelength than stochastic methods, compared with SA on
its own small test boards. [Lim, arXiv 2026](https://arxiv.org/abs/2602.23540) places passives
around a fixed main part with DQN and actor-critic and compares with SA on nine boards; unreviewed.
Quilter uses RL with a router in the loop (vendor description); Cypress found it fully routable
but slow (2.6 hours for 176 components, per the Cypress paper). Constraints go in the reward or the action
mask; training needs a GPU and many episodes. Evidence on PCBs: small, single-group, unreplicated.

### Evolutionary and black-box search

Populations of placements, recombined and mutated. The best recent chip result is WireMask-EA, an
evolution strategy over macro positions decoded by a greedy wire-mask placer
([Shi, Xue, Song and Qian, NeurIPS 2023](https://proceedings.neurips.cc/paper_files/paper/2023/hash/15d6717f8bb33b3a74df26ce1eee0b9a-Abstract.html));
it can also refine another placer's output. Any constraint fits the fitness function, as with SA. Many "genetic PCB placement" papers are
about assembly sequencing, not layout. For us it is SA with more bookkeeping, worth trying only
as a multi-worker variant once the cost is incremental.

### Exact methods (CP, ILP, SMT)

Positions on a grid as integers, non-overlap as disjunctions, solved to optimality or a bound.
Floorplanning as a mixed-integer program goes back to
Sutanthavibul, Shragowitz and Rosen (TCAD 1991), and SMT has been tried for VLSI floorplans
([arXiv 1709.07241](https://arxiv.org/abs/1709.07241)). OR-Tools' [CP-SAT](https://developers.google.com/optimization/cp/cp_solver)
has a native two-dimensional no-overlap constraint, and quarter turns are optional intervals.
Edges, separations, locked parts, sides and rigid groups are all linear constraints; wirelength
is linear in bounding-box variables; a polygon outline needs a grid of allowed cells. The catch
is scale: the non-overlap disjunctions grow as n². On real PCBs, NS-Place's MILP legalizer
([Cheng, Ho and Holtz, ASP-DAC 2022](https://github.com/choltz95/pcb-placement)) timed out at
60 minutes on eight of Cypress's ten small benchmarks. So exact methods are for sub-blocks of
perhaps 10–25 parts (a buck converter's generated layout, a connector's ESD cluster), offline,
and as a quality ceiling for the heuristics on those blocks. Deterministic, no data. Running
CP-SAT in a browser is not something I could confirm.

### Commercial tools

Cadence's Allegro X AI claims placement and routing "from days to minutes" for small and medium
boards, with one unnamed customer's placement going from three days to 75 minutes with 14%
shorter wirelength ([All About Circuits](https://www.allaboutcircuits.com/news/cadence-launches-cloud-ai-tool-to-shrink-pcb-placement-and-routing-times/)).
Quilter ([product](https://quilter.ai/product)) and InstaDeep's DeepPCB ([site](https://deeppcb.ai))
offer cloud placement and routing by RL. All figures are the vendors' own; the only independent
measurement found is Cypress's run of Quilter. None publishes enough to compare methods.

## Summary tables

Constraint handling. **N** native, **P** a penalty or guidance term that drops in, **H** needs a
heuristic or extra stage, **D** has to be in training data or reward.

| | 90° turns | Sides | Edge conn. | Rigid groups | Separations | Shaped outline | Locked | Routability |
|---|---|---|---|---|---|---|---|---|
| SA (ours) | N | N (input) | N | N | N | N | N | P (proxy) |
| Analytical + legalize | P (softmax) | N | P | N (chain rule) | P | P (SDF) | N | P (RUDY, crossings) |
| Quadratic | H | H | H | N | H | H | N | H |
| Density diffusion | — | N | — | N | — | N (walls) | N | N (demand map) |
| Diffusion model | D/P | D/P | P | D/P | P | P | P | P |
| RL | N | N | D | D | D | D | N | D |
| Evolutionary | N | N | N | N | N | N | N | P |
| CP / ILP | N | N | N | N | N | H (grid) | N | H |

Practicalities at our sizes (≤ 500 parts, ~80 movable).

| | Speed (ours, est.) | Browser, warm start | Deterministic | Data | Evidence on PCBs |
|---|---|---|---|---|---|
| SA, incremental | < 1 s for 200k moves | yes, yes | per seed | none | moderate (SA-PCB; ours) |
| Analytical + legalize | ~1 s | yes, yes | yes (CPU) | none | strong but single-group (Cypress) |
| Quadratic | ms | yes, yes | yes | none | historical |
| Density diffusion | ms–s | yes, yes | yes | none | negative so far (ultraroute) |
| Diffusion model | s–min on GPU | unshown, partial (noise and resample) | per seed | large, synthetic | none (chip-only) |
| RL | fast inference | possible, policy-dependent | varies | episodes | weak (RL_PCB, Quilter claims) |
| Evolutionary | like SA × population | yes, yes | per seed | none | little for layout |
| CP / ILP | s–hours, explodes by ~50 | unconfirmed, yes | yes | none | MILP failed at 41–476 parts |

## Recommendation

1. **Build the benchmark first.** Inputs: our four `app/web/public/examples/*/problem.json`,
   plus Cypress's ten small designs converted to `floorplan::Problem` (they cover 41–476 parts,
   where our examples stop at 80 movable, and they come with published SA-PCB, Quilter, human and
   Cypress numbers to sanity-check against). Each placer gets the same wall-clock budgets (1 s,
   10 s, 60 s), native and WASM, ten seeds, on cores booked through `reserve`. Score with
   `floorplan::evaluate`: a placement counts only if `overlaps` is 0 and `edge_misses` and
   `separation_misses` are empty; then report `wire_mm`, `area_mm2` and `near_mm` as median and
   spread, plus a cost-against-time curve. `evaluate` does not enforce rigid groups, so the
   harness checks group shape and locked poses itself. Compare against each example's shipped
   `placement.json`. Routability: route each legal placement with ultraroute under a fixed time
   budget and record open connections and new DRC errors, and with Freerouting on two layers for
   comparison with Cypress's table.
2. **Make the SA cost incremental.** Same algorithm, same results per seed if done carefully, far
   more moves per second. Every other method should be compared against this baseline, not the
   O(n²) one. Seed it from a quadratic solve instead of a random spread, and measure whether that
   helps.
3. **Write an analytical global placer in Rust** (weighted-average wirelength, FFT density per
   side, our terms as smooth penalties, rotation as softmax then snapped), followed by the
   existing `legalise`, `repair` and a short warm SA. Cypress is the strongest PCB evidence there
   is, and at our sizes it needs no GPU. Its differentiable cost is reused by step 5.
4. **Add a routability term, and check it before trusting it.** Net crossings (Cypress) and a
   RUDY-style demand map with capacity scaled by layer count, both cheap. First measure whether
   they correlate with ultraroute's open connections on the benchmark; ChiPBench is the warning
   about proxies that don't.
5. **Then diffusion, as a starting-point generator.** A small model conditioned on the part graph
   and outline, trained on synthetic boards from the composer (PCBench placements for
   validation), guided by step 3's cost, and always followed by legalize and warm SA. It is worth
   keeping if, at equal total time, it beats quadratic and random starts, or if it gives several
   distinct legal placements that route differently. Without steps 1 and 3, a diffusion result
   could not be judged.
6. **CP-SAT for small rigid sub-layouts**, offline, as a ceiling for the heuristics on blocks of
   10–25 parts.
7. **Not now:** RL (weak, disputed evidence and the highest cost), and Ren-style density
   diffusion as a placer (keep it in ultraroute as a post-route spreader).

## The floorplanning/routing boundary

A placer needs a *model* of routing, not a router. What it should know:

- **Airlines and their crossings.** Two-pin connections from the minimum spanning tree of each
  net; crossings between them are the cheapest routability signal and Cypress's results back it.
- **Congestion against capacity.** Demand per cell (RUDY or pin density) against a capacity that
  depends on layer count, trace and space rules, and what pads and through-holes block. ultraroute's
  lesson applies: normalise demand per part, or fine-pitch parts saturate the map and drive
  everything.
- **Escape room.** Room beside fine-pitch packages for their fan-out, which a courtyard does not
  give. A per-package halo (from the package rules, like separations) is the simple form.
- **Which nets are planes.** Power and ground are already left out of wirelength; the layer
  count decides whether that is true.

What belongs in ultraroute's studio: anything that needs real copper. Routed congestion,
open connections after a timed route, rip-up, and the "nudge it for me" placement of one part
with the rest locked are all better done where the router is. The useful loop is: the composer
places; ultraroute routes and returns a coarse overflow map and the open airlines (alongside the
agreed `routing.json` handoff, in the other direction); the composer raises the weight of those
nets or the density target there and refines from a warm start. If the two apps share a routability estimate, it should
live in the MIT core, not in copperroute, so both can link it into WASM.
