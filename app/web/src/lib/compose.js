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
