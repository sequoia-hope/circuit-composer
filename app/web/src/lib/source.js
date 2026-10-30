// Where the boards come from. With Composer's server behind the page (local or through
// Boardroom), it answers `api/info` and serves the project; with none (GitHub Pages, a static
// server) the page opens the example boards built into the site by app/examples.sh.
// The server doesn't exist yet: every mode is "pages" for now.

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
  const get = (f) => fetch(`examples/${name}/${f}`).then((r) => {
    if (!r.ok) throw new Error(`${name}/${f}: ${r.status}`);
    return r.text();
  });
  const [problem, placement, circuit, shape] = await Promise.all([
    get('problem.json'), get('placement.json'), get('circuit.json'),
    get('shape.json').then(JSON.parse, () => null),
  ]);
  return { problem, placement, circuit: JSON.parse(circuit), shape };
}
