// A loose composition solved and composed in the browser, by the core's pin solver and composer:
// the same `ccomp pins` and `ccomp compose` do on the command line.
import { pins_solve, compose_board } from '../core/composer_core.js';

/** Each package instance's circuit and ports, as the core takes them, from packages.json
 *  (package spec → its ports and its circuit per variant). Recipe instances are expanded by the
 *  core itself. */
export function instancePackages(comp, packages) {
  const out = {};
  for (const [name, inst] of Object.entries(comp.instances)) {
    if (inst.recipe) continue;
    const circuit = packages?.[inst.package]?.circuits?.[inst.variant];
    if (!circuit) throw new Error(`${name}: ${inst.package} (${inst.variant}) is not in this board's packages.json`);
    out[name] = { circuit, ports: packages[inst.package].ports };
  }
  return out;
}

/** The drawn board's designators by instance and role, so a solved board keeps them. */
export function designators(circuit) {
  const out = {};
  for (const [ref, p] of Object.entries(circuit.parts)) {
    if (p.instance && p.role) (out[p.instance] ??= {})[p.role] = ref;
  }
  return out;
}

/** Solve `loose` and compose the result. `drawn` is the board's own circuit: its designators
 *  are kept. Returns `{ solved, composed }`; `composed` is null when the solver refused. */
export function solve(loose, packages, library, drawn) {
  const comp = structuredClone(loose);
  const keep = designators(drawn);
  for (const [name, inst] of Object.entries(comp.instances)) {
    if (keep[name] && !inst.designators) inst.designators = keep[name];
  }
  const lib = JSON.stringify({ recipes: library.recipes, stock: library.stock });
  const solved = JSON.parse(pins_solve(JSON.stringify(comp), JSON.stringify(library.interfaces), JSON.stringify(instancePackages(comp, packages)), lib));
  let composed = null;
  if (solved.ok) {
    composed = JSON.parse(compose_board(JSON.stringify(solved.composition), JSON.stringify(instancePackages(solved.composition, packages)), lib));
  }
  return { solved, composed };
}

/** The pin a solved row's partner has on the drawn board (a port name, `GPIO6`), or null: the
 *  drawn net holding the device's end of the solved net, and the partner's chip pin on it. */
export function drawnPort(row, composed, drawn, partnerCircuit) {
  const partner = row.partner.split('.')[0];
  const chip = composed.bindings?.[partner]?.chip;
  if (!chip) return null;
  const mine = `${chip}.${row.pin}`;
  const net = Object.values(composed.nets).find((ps) => ps.includes(mine));
  const others = (net || []).filter((p) => !p.startsWith(`${chip}.`));
  const was = Object.values(drawn.nets).find((ps) => others.some((o) => ps.includes(o)));
  const pin = was?.find((p) => p.startsWith(`${chip}.`))?.split('.')[1];
  if (!pin) return null;
  return Object.entries(partnerCircuit.ports).find(([, p]) => p.pins.includes(`U1.${pin}`))?.[0] ?? null;
}

/** `GPIO10` after `GPIO9`. */
export function natural(a, b) {
  return a.localeCompare(b, undefined, { numeric: true });
}

/** The floorplan problem of a solved board, made from the drawn board's: each part keeps its
 *  footprint geometry, block, pose and lock, and the nets are the solved board's, filtered as the
 *  floorplan spec filters them (`drawn.floorplan`: max_net_pins, skip_nets, exclude). A part the
 *  drawn board lacks (a longer LED chain, a pull-up the rules added) borrows the geometry of a
 *  drawn part with the same footprint and starts beside the pin it sits near; one whose footprint
 *  no drawn part has is left out and reported. `placement` is the poses to start from, by
 *  reference. Returns `{ problem, placement, added, dropped, missing }`. */
export function solvedProblem(problem, placement, composed, drawn) {
  const fp = drawn.floorplan || {};
  const max = fp.max_net_pins ?? 12, skip = new Set(fp.skip_nets || []), exclude = new Set(fp.exclude || []);
  const old = new Map(problem.parts.map((p) => [p.reference, p]));
  const byFootprint = new Map();
  for (const p of problem.parts) {
    const f = drawn.parts[p.reference]?.footprint;
    if (f && !byFootprint.has(f)) byFootprint.set(f, p);
  }
  const blocks = problem.parts.some((p) => p.block);
  const parts = [], added = [], missing = [];
  for (const [ref, c] of Object.entries(composed.parts)) {
    if (exclude.has(ref)) continue;
    const was = old.get(ref);
    if (was) { parts.push({ ...was }); continue; }
    const like = byFootprint.get(c.footprint);
    if (!like) { missing.push(`${ref} (${c.footprint || 'no footprint'})`); continue; }
    const block = blocks && c.instance ? (c.block ? `${c.instance}/${c.block}` : c.instance) : null;
    parts.push({ ...like, reference: ref, block, near: null, edge: null, fixed: null, _near: c.near });
    added.push(ref);
  }
  // Keep-outs (the floorplan's and the shape's) are parts with no circuit behind them.
  for (const p of problem.parts) if (p.reference.includes(':')) parts.push({ ...p });
  const index = new Map(parts.map((p, i) => [p.reference, i]));
  const refOf = (i) => problem.parts[i].reference;
  for (const p of parts) {
    const near = p._near ? p._near.split('.') : p.near ? [refOf(p.near[0]), p.near[1]] : null;
    p.near = near && index.has(near[0]) ? [index.get(near[0]), near[1]] : null;
    delete p._near;
  }
  const nets = [];
  for (const [name, pins] of Object.entries(composed.nets)) {
    const ps = pins.map((p) => { const k = p.indexOf('.'); return [index.get(p.slice(0, k)), p.slice(k + 1)]; }).filter(([i]) => i !== undefined);
    if (ps.length < 2 || ps.length > max || skip.has(name)) continue;
    nets.push({ name, pins: ps, weight: 1.0 });
  }
  // A rigid layout keeps only while all its parts are still on the board.
  const groups = problem.groups.flatMap((g) => {
    const anchor = index.get(refOf(g.anchor));
    const members = g.members.map(([i, at, r]) => [index.get(refOf(i)), at, r]);
    return anchor === undefined || members.some(([i]) => i === undefined) ? [] : [{ ...g, anchor, members }];
  });
  const dropped = problem.parts.map((p) => p.reference).filter((r) => !r.includes(':') && !index.has(r));
  // New parts start on the part they sit near, or in the middle of the rest.
  const start = { ...placement };
  const placed = Object.values(placement);
  const mid = placed.length ? { x: placed.reduce((s, q) => s + q.x, 0) / placed.length, y: placed.reduce((s, q) => s + q.y, 0) / placed.length } : { x: 0, y: 0 };
  for (const r of added) {
    const p = parts[index.get(r)];
    const at = p.near ? start[parts[p.near[0]].reference] : null;
    start[r] = { x: (at || mid).x, y: (at || mid).y, rotation: 0 };
  }
  return { problem: { ...problem, parts, nets, groups }, placement: start, added, dropped, missing };
}
