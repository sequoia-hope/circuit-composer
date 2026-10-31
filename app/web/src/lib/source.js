// Where the boards come from. Opened by Boardroom (`?store=…&board=…`), from the project's store:
// its `.kicad_pcb` and the placement problem Boardroom's snapshot made for it. Otherwise, with
// Composer's server behind the page, it answers `api/info` and serves the project; with none
// (GitHub Pages, a static server) the page opens Boardroom's example projects where their
// snapshot is on the same site, and the example boards built into the site by app/examples.sh.
// The server doesn't exist yet: every mode is "pages" for now.

import { kicad_circuit } from '../core/composer_core.js';
import { browserStore } from './store.js';

/**
 * `fn()`, tried again after a server error (a 5xx: GitHub Pages answers 503 now and then) or a
 * dropped connection: three tries, half a second then a second and a half apart.
 */
async function retrying(fn) {
  for (const wait of [500, 1500, 0]) {
    try {
      return await fn();
    } catch (e) {
      const passing = e instanceof TypeError || /: 5\d\d\b/.test(e?.message ?? '');
      if (!passing || !wait) throw e;
      await new Promise((r) => setTimeout(r, wait));
    }
  }
}

export async function detectMode() {
  try {
    const r = await fetch('api/info', { cache: 'no-store' });
    if (r.ok && (r.headers.get('content-type') || '').includes('json')) return { mode: 'local', info: await r.json() };
  } catch {
    // No server.
  }
  const pages = location.hostname.endsWith('github.io');
  return { mode: pages ? 'pages' : 'static', info: null };
}

export async function listBoards() {
  const r = await fetch('examples/index.json');
  return (await r.json()).boards;
}

export async function loadBoard(name) {
  const get = (f) => retrying(() => fetch(`examples/${name}/${f}`).then((r) => {
    if (!r.ok) throw new Error(`${name}/${f}: ${r.status}`);
    return r.text();
  }));
  const optional = (f) => get(f).then(JSON.parse, () => null);
  const [problem, placement, circuit, shape, loose, packages] = await Promise.all([
    get('problem.json'), get('placement.json'), get('circuit.json'),
    optional('shape.json'), optional('loose.json'), optional('packages.json'),
  ]);
  return { problem, placement, circuit: JSON.parse(circuit), shape, loose, packages };
}

// The recipes, stock and interface types every board's composition shares: fetched once.
let library = null;
export function loadLibrary() {
  library ??= fetch('examples/library.json').then((r) => {
    if (!r.ok) throw new Error(`library.json: ${r.status}`);
    return r.json();
  });
  return library;
}

/** Where a board's files are in a project: Boardroom's layout. */
export function projectPaths(board, variant) {
  return {
    pcb: `hw/${board}/${variant}/${board}.kicad_pcb`,
    problem: `compose/${board}/problem.json`,
    placement: `compose/${board}/placement.json`,
  };
}

/**
 * A board from a project's store: the placement problem (`compose/<board>/problem.json`, which
 * Boardroom's snapshot makes) and the `.kicad_pcb`, which is where the parts are and what the
 * inspector shows. Needs the WASM core initialised. Refuses rather than showing another board.
 */
export async function loadFromStore(store, board, variant) {
  const paths = projectPaths(board, variant);
  const [problem, pcb] = await Promise.all([retrying(() => store.text(paths.problem)), retrying(() => store.read(paths.pcb))]);
  if (problem == null) throw new Error(`Composer can't open ${board} yet: its placement problem (${paths.problem}) isn't in the project`);
  if (!pcb) throw new Error(`Composer can't open ${board} / ${variant}: the project has no ${paths.pcb}`);
  const text = new TextDecoder().decode(pcb.bytes);
  const read = JSON.parse(kicad_circuit(text));
  // The blocks the problem names, for the inspector's colours and groups.
  const pr = JSON.parse(problem);
  const parts = {};
  for (const [ref, p] of Object.entries(read.parts)) parts[ref] = { ...p };
  for (const p of pr.parts) if (p.block && parts[p.reference]) parts[p.reference].block = p.block;
  return {
    problem,
    pcb: { text, rev: pcb.rev },
    circuit: { parts, nets: read.nets, instances: {}, floorplan: {} },
    paths,
  };
}

/**
 * Boardroom's example projects, when its Pages snapshot is on this site (github.io serves
 * Boardroom and Composer from one origin, so they share the browser's store): each board with a
 * placement problem, as `{ project, name, board, variant, snapshot }`. `?boardroom=` names
 * another snapshot folder. None found: [].
 */
export async function boardroomBoards() {
  const asked = new URLSearchParams(location.search).get('boardroom');
  const root = asked ? new URL(asked.endsWith('/') ? asked : `${asked}/`, location.href).href
    : location.hostname.endsWith('github.io') ? new URL('/boardroom/snapshot/', location.href).href : null;
  if (!root) return [];
  try {
    const r = await fetch(`${root}projects.json`, { cache: 'no-cache' });
    if (!r.ok) return [];
    const out = [];
    for (const p of await r.json()) {
      for (const c of p.compose ?? []) {
        if (c.problem) out.push({ project: p.id, name: p.name ?? p.id, board: c.board, variant: c.variant, snapshot: `${root}${p.id}/` });
      }
    }
    return out;
  } catch {
    return [];
  }
}

/** The store standalone Composer opens one of Boardroom's projects through: this tab's workspace. */
export function boardroomStore(b) {
  return browserStore({ project: b.project, snapshot: b.snapshot });
}
