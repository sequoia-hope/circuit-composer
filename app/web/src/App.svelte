<script>
  import { onMount } from 'svelte';
  import init, { Board } from './core/composer_core.js';
  import { detectMode, listBoards, loadBoard, loadLibrary } from './lib/source.js';
  import BoardView from './lib/BoardView.svelte';
  import CircuitView from './lib/CircuitView.svelte';
  import Inspector from './lib/Inspector.svelte';
  import Dock from './lib/Dock.svelte';
  import ShapeView from './lib/ShapeView.svelte';

  const params = new URLSearchParams(location.search);
  const embed = params.get('embed') === '1';

  const WORKSPACES = [
    ['circuit', 'Circuit'],
    ['board', 'Board'],
    ['shape', 'Shape'],
    ['copper', 'Copper'],
    ['rules', 'Rules'],
  ];

  let mode = $state('static');
  let boards = $state([]);
  let current = $state(params.get('board') || '');
  let workspace = $state(params.get('ws') || 'board');
  let error = $state('');
  let doc = $state.raw(null);
  let circuit = $state.raw(null);
  let composition = $state.raw(null); // { loose, packages, library } for a board with loose.json
  let refIndex = $state.raw({ list: [], of: new Map() });
  let version = $state(0);
  let selection = $state.raw([]);
  let metrics = $state.raw(null);
  let layers = $state({ front: true, back: true, ratsnest: true, labels: true, blocks: true });
  let dockOpen = $state('checks');
  let boardView = $state(null);
  let shapeDoc = $state(null);
  let shapeApplied = $state(false);

  // Annealing.
  let worker = null;
  let anneal = $state.raw(null); // {done, total, warm} while running
  let seed = $state(1);
  let moves = $state(200000);
  let placer = $state('analytic'); // how Place starts: gradient descent, or a random spread
  let lastRun = $state('');
  let history = [];

  onMount(async () => {
    await init();
    mode = (await detectMode()).mode;
    try {
      boards = await listBoards();
    } catch (e) {
      error = `No example boards: ${e.message}. Run app/examples.sh.`;
      return;
    }
    if (!boards.some((b) => b.name === current)) current = (boards.find((b) => b.name === 'rp2350-battery') || boards[0]).name;
    await open(current);
    window.addEventListener('message', fromParent);
  });

  async function open(name) {
    stop();
    error = '';
    try {
      const data = await loadBoard(name);
      const board = new Board(data.problem);
      board.set_placement(data.placement);
      const problem = JSON.parse(data.problem);
      const meta = boards.find((b) => b.name === name) || {};
      circuit = data.circuit;
      composition = data.loose && data.packages ? { loose: data.loose, packages: data.packages, library: await loadLibrary() } : null;
      const list = problem.parts.map((p) => p.reference);
      refIndex = { list, of: new Map(list.map((r, i) => [r, i])) };
      doc = {
        name, note: meta.note || '', board, problem,
        outline: problem.outline_polygon || null,
        box: problem.outline || null, boxIsSet: !!problem.outline,
        margin: problem.options?.margin ?? 0.5,
        parts: [], rats: new Float64Array(0),
      };
      // Big boards start with fewer moves: the analytic start does most of the work.
      moves = problem.parts.length > 150 ? 50000 : 200000;
      shapeDoc = data.shape || shapeFromProblem(problem);
      shapeApplied = false;
      selection = [];
      history = [];
      lastRun = '';
      current = name;
      refresh();
      measure();
      setUrl();
    } catch (e) {
      error = `${name}: ${e.message || e}`;
    }
  }

  /** A board with no shape.json: its shape is the outline its floorplan sets, if any. */
  function shapeFromProblem(problem) {
    if (problem.outline_polygon) {
      return { note: "From the floorplan's outline.", features: [{ name: 'outline', op: 'add', geom: { type: 'polygon', points: problem.outline_polygon } }] };
    }
    if (problem.outline) {
      const [x0, y0, x1, y1] = problem.outline;
      return { note: "From the floorplan's outline.", features: [{ name: 'board', op: 'add', geom: { type: 'rect', x: (x0 + x1) / 2, y: (y0 + y1) / 2, w: x1 - x0, h: y1 - y0, r: 0, angle: 0 } }] };
    }
    return { note: 'No outline: the floorplanner sizes the board.', features: [] };
  }

  /** The shape's outline becomes the floorplanner's. */
  function applyShape(result) {
    doc.board.set_outline(JSON.stringify(result.outline[0]));
    // Keep-outs, holes and cut-outs keep parts away: their bounding boxes, as ccomp does.
    const box = (pts) => pts.reduce((b, p) => [Math.min(b[0], p[0]), Math.min(b[1], p[1]), Math.max(b[2], p[0]), Math.max(b[3], p[1])], [Infinity, Infinity, -Infinity, -Infinity]);
    const ks = [
      ...result.keepouts.map((k) => ({ name: k.name, rect: box(k.polygon.flat()), side: k.side })),
      ...result.cutouts.map((c, i) => ({ name: `cut-out ${i + 1}`, rect: box(c), side: 'both' })),
      ...result.holes.map((h, i) => { const r = h.d / 2 + (h.clearance || 0); return { name: `hole ${i + 1}`, rect: [h.x - r, h.y - r, h.x + r, h.y + r], side: 'both' }; }),
    ];
    doc.board.set_keepouts(JSON.stringify(ks));
    const list = JSON.parse(doc.board.problem()).parts.map((p) => p.reference);
    refIndex = { list, of: new Map(list.map((r, i) => [r, i])) };
    selection = [];
    doc = { ...doc, outline: result.outline[0], box: null, boxIsSet: false };
    refresh();
    measure();
    shapeApplied = true;
  }

  function setUrl() {
    const p = new URLSearchParams(location.search);
    p.set('board', current);
    p.set('ws', workspace);
    history_replace(`?${p}`);
  }
  function history_replace(q) {
    try { window.history.replaceState(null, '', q); } catch { /* sandboxed */ }
  }

  /** Re-read what the scene draws from the core. */
  function refresh() {
    const parts = JSON.parse(doc.board.parts());
    let box = doc.box;
    if (!doc.boxIsSet && !doc.outline) {
      // No outline set: show the one the parts need, as the floorplanner would make it.
      let b = [Infinity, Infinity, -Infinity, -Infinity];
      for (const p of parts) for (const q of p.outline) b = [Math.min(b[0], q[0]), Math.min(b[1], q[1]), Math.max(b[2], q[0]), Math.max(b[3], q[1])];
      box = [b[0] - doc.margin, b[1] - doc.margin, b[2] + doc.margin, b[3] + doc.margin];
    }
    doc = { ...doc, parts, rats: doc.board.ratsnest(), box };
    version++;
  }

  let measureTimer = 0;
  function measure() {
    clearTimeout(measureTimer);
    measureTimer = setTimeout(() => {
      try { metrics = JSON.parse(doc.board.metrics()); } catch (e) { error = String(e); }
    }, 60);
  }

  function remember() {
    history.push(doc.board.poses());
    if (history.length > 100) history.shift();
  }

  function undo() {
    const p = history.pop();
    if (!p || anneal) return;
    doc.board.set_poses(p);
    refresh();
    measure();
  }

  // Selection, shared with Boardroom when embedded.
  function select(ix) {
    selection = [...new Set(ix.filter((i) => i !== undefined && i >= 0))];
    if (window.parent !== window) {
      window.parent.postMessage({ source: 'circuit-composer', type: 'select', board: current, refs: selection.map((i) => refIndex.list[i]) }, '*');
    }
  }
  function fromParent(e) {
    const m = e.data;
    if (m && m.type === 'select' && Array.isArray(m.refs) && (!m.board || m.board === current)) {
      selection = m.refs.map((r) => refIndex.of.get(r)).filter((i) => i !== undefined);
    }
  }

  function changed() {
    refresh();
  }
  function dragEnd() {
    measure();
  }

  function lock(on) {
    remember();
    for (const i of selection) doc.board.set_locked(i, on);
    refresh();
  }
  function rotate() {
    if (selection.length !== 1 || doc.parts[selection[0]].locked) return;
    remember();
    doc.board.rotate(selection[0]);
    refresh();
    measure();
  }
  function pose(i, x, y) {
    remember();
    doc.board.move_to(i, x, y);
    refresh();
    measure();
  }

  function run(warm) {
    if (anneal) return;
    remember();
    const started = performance.now();
    worker = new Worker(new URL('./lib/anneal.worker.js', import.meta.url), { type: 'module' });
    anneal = { done: 0, total: moves, warm };
    worker.onmessage = (e) => {
      const m = e.data;
      doc.board.set_poses(m.poses);
      refresh();
      if (m.type === 'snap') {
        anneal = { done: m.done, total: m.total, warm };
        metrics = JSON.parse(m.metrics);
      } else {
        metrics = JSON.parse(m.placement).metrics;
        lastRun = `${warm ? 'Refined' : placer === 'analytic' ? 'Placed analytically' : 'Placed'} with seed ${seed}, ${moves.toLocaleString()} moves, in ${m.seconds.toFixed(1)} s.`;
        worker.terminate();
        worker = null;
        anneal = null;
      }
    };
    worker.onerror = (e) => {
      error = `Anneal failed: ${e.message}`;
      stop();
    };
    worker.postMessage({ problem: doc.board.problem(), poses: doc.board.poses(), seed, moves, warm, placer, every: Math.max(1000, Math.round(moves / 120)) });
    void started;
  }

  function stop() {
    if (!worker) return;
    worker.terminate();
    worker = null;
    if (anneal) lastRun = `Stopped at ${Math.round((100 * anneal.done) / anneal.total)} %: not legalised yet.`;
    anneal = null;
    measure();
  }

  function download() {
    const blob = new Blob([JSON.stringify(JSON.parse(doc.board.placement()), null, 1) + '\n'], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = `${current}.placement.json`;
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  function key(e) {
    if (e.target.closest('input, textarea, select') || !doc) return;
    if (workspace === 'shape') {
      // The Shape workspace handles its own keys; only the workspace numbers pass.
      if (!e.ctrlKey && !e.metaKey && e.key >= '1' && e.key <= '5') { workspace = WORKSPACES[+e.key - 1][0]; setUrl(); }
      return;
    }
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') { e.preventDefault(); undo(); return; }
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    if (e.key === 'r' || e.key === 'R') rotate();
    else if (e.key === 'l' || e.key === 'L') { if (selection.length) lock(!doc.parts[selection[0]].locked); }
    else if (e.key === 'f' || e.key === 'F') boardView?.fit();
    else if (e.key === 'Escape') select([]);
    else if (e.key >= '1' && e.key <= '5') { workspace = WORKSPACES[+e.key - 1][0]; setUrl(); }
  }

  const fmt = (v, d = 1) => (v === undefined || v === null ? '—' : Number(v).toFixed(d));
  const problems = $derived(metrics ? (metrics.overlaps || 0) + (metrics.edge_misses?.length || 0) + (metrics.separation_misses?.length || 0) : 0);
  const serverOnly = $derived(mode === 'local' ? '' : 'Needs Composer running on your machine (this page is a static demo)');
</script>

<svelte:window onkeydown={key} />

<div class="app" class:embed class:wide={workspace === 'shape'}>
  {#if !embed}
    <header>
      <div class="brand"><span class="logo"></span>Circuit Composer</div>
      <select bind:value={current} onchange={() => open(current)} aria-label="Board">
        {#each boards as b}<option value={b.name}>{b.name}</option>{/each}
      </select>
      <nav>
        {#each WORKSPACES as [id, label], k}
          <button class:on={workspace === id} onclick={() => { workspace = id; setUrl(); }} title="{label} ({k + 1})">{label}</button>
        {/each}
      </nav>
      <div class="right">
        {#if mode !== 'pages'}<a class="debug" href="../../debug.html" title="The project's status and debug page (local only)">debug</a>{/if}
        <span class="mode" title={mode === 'local' ? 'Served by Composer on this machine' : 'No server: the example boards are built into the page'}>{mode === 'local' ? 'Local project' : 'Demo · example boards'}</span>
        <button disabled={!!serverOnly} title={serverOnly || 'Ask Claude to fill the open fields'}>Fill with AI</button>
        <button disabled={!!serverOnly} title={serverOnly || 'Write the placed board and routing.json for UltraRoute'}>Send to UltraRoute</button>
        <button onclick={download} disabled={!doc} title="Download this placement as JSON">Download placement</button>
      </div>
    </header>
  {/if}

  {#if error}<div class="error">{error}</div>{/if}

  <main>
    <section class="centre">
      {#if !doc}
        <div class="empty">Loading…</div>
      {:else if workspace === 'board'}
        <div class="toolbar">
          {#each [['front', 'Top'], ['back', 'Bottom'], ['ratsnest', 'Ratsnest'], ['labels', 'Labels'], ['blocks', 'Block colours']] as [k, label]}
            <label><input type="checkbox" bind:checked={layers[k]} /> {label}</label>
          {/each}
        </div>
        <BoardView bind:this={boardView} {doc} {version} {selection} {layers} busy={!!anneal}
          onselect={select} ondragstart={remember} onchange={changed} ondragend={dragEnd} />
        <div class="metrics">
          <span class="stat"><b>{fmt(metrics?.wire_mm, 0)}</b> mm wire</span>
          <span class="stat"><b>{fmt(doc.box ? doc.box[2] - doc.box[0] : metrics?.board_mm?.[0])} × {fmt(doc.box ? doc.box[3] - doc.box[1] : metrics?.board_mm?.[1])}</b> mm</span>
          <span class="stat" class:bad={problems}><b>{problems}</b> {problems === 1 ? 'problem' : 'problems'}</span>
          <span class="sep"></span>
          {#if anneal}
            <span class="progress"><span style="width:{(100 * anneal.done) / anneal.total}%"></span></span>
            <span class="stat">{anneal.warm ? 'Refining' : 'Placing'}… {Math.round((100 * anneal.done) / anneal.total)} %</span>
            <button onclick={stop}>Stop</button>
          {:else}
            <label class="small" title="How Place starts: gradient descent then a cool anneal, or an anneal from a random spread">start
              <select bind:value={placer}>
                <option value="analytic">analytic</option>
                <option value="sa">random</option>
              </select>
            </label>
            <label class="small">seed <input type="number" min="1" bind:value={seed} /></label>
            <label class="small">moves
              <select bind:value={moves}>
                {#each [50000, 200000, 400000, 1000000] as m}<option value={m}>{m >= 1e6 ? '1 M' : `${m / 1000} k`}</option>{/each}
              </select>
            </label>
            <button onclick={() => run(true)} title="Anneal from the current placement, gently: locked parts stay">Refine</button>
            <button class="primary" onclick={() => run(false)} title="Place every unlocked part from scratch">Place unlocked parts</button>
            <span class="last">{lastRun}</span>
          {/if}
        </div>
      {:else if workspace === 'circuit'}
        {#key current}
          <CircuitView {circuit} parts={doc.parts} {refIndex} {selection} onselect={select}
            loose={composition?.loose} packages={composition?.packages} library={composition?.library} />
        {/key}
      {:else if workspace === 'shape'}
        {#key current}
          <ShapeView bind:doc={shapeDoc} applied={shapeApplied} onapply={applyShape} onchange={() => (shapeApplied = false)} />
        {/key}
      {:else if workspace === 'copper'}
        <div class="later">
          <h2>Copper</h2>
          <p>What must exist before the router runs, and that it may not touch: generated block layouts (the RP2350's core
            buck), pattern generators (the earrings' LED ring fields with their rails), reserved vias, plane and pour assignments,
            and routing regions (<i>must</i>, <i>prefer</i>, <i>avoid</i>, <i>never</i>) per net class, with a preview of the
            <code>routing.json</code> UltraRoute will get.</p>
          <p class="muted">Milestone F. On this board: {doc.problem.groups.length} rigid generated layout{doc.problem.groups.length === 1 ? '' : 's'}
            ({doc.problem.groups.map((g) => g.name).join(', ') || 'none'}), {doc.parts.filter((p) => p.locked).length} locked parts.</p>
        </div>
      {:else}
        <div class="later rules">
          <h2>Rules</h2>
          <p>Net, block and domain intent, each value with its strength and its citations into the research library. Here
            for now: the nets, by size.</p>
          <table>
            <thead><tr><th>Net</th><th>Pins</th><th>Parts</th></tr></thead>
            <tbody>
              {#each Object.entries(circuit.nets).sort((a, b) => b[1].length - a[1].length).slice(0, 200) as [net, pins]}
                <tr><td>{net}</td><td>{pins.length}</td><td class="pins">{pins.slice(0, 14).join(' ')}{pins.length > 14 ? ' …' : ''}</td></tr>
              {/each}
            </tbody>
          </table>
        </div>
      {/if}
    </section>
    <aside>
      {#if doc && circuit}
        <Inspector {doc} {selection} {circuit} onlock={lock} onrotate={rotate} onpose={pose} onselect={select} />
      {/if}
    </aside>
    <footer>
      <Dock {metrics} {mode} {refIndex} onselect={select} bind:open={dockOpen} />
    </footer>
  </main>
</div>

<style>
  .app { display: flex; flex-direction: column; height: 100vh; }
  header { display: flex; align-items: center; gap: 14px; padding: 0 12px; height: 46px; border-bottom: 1px solid var(--line); background: var(--panel); flex: none; }
  .brand { font-weight: 650; display: flex; align-items: center; gap: 8px; white-space: nowrap; }
  .logo { width: 16px; height: 16px; border-radius: 4px; background: var(--accent); box-shadow: inset 4px 4px 0 -1px var(--panel); }
  header select { padding: 4px 6px; border-radius: 6px; border: 1px solid var(--line); background: var(--bg); color: var(--ink); font: inherit; }
  nav { display: flex; gap: 2px; overflow-x: auto; scrollbar-width: none; min-width: 0; }
  nav button { all: unset; cursor: pointer; padding: 6px 11px; border-radius: 6px; color: var(--muted); }
  nav button:hover { background: var(--hover); color: var(--ink); }
  nav button.on { background: var(--hover); color: var(--ink); box-shadow: inset 0 -2px 0 var(--accent); }
  .right { margin-left: auto; display: flex; gap: 6px; align-items: center; }
  .debug { font-size: 11.5px; color: var(--muted); }
  .mode { font-size: 11.5px; color: var(--muted); border: 1px solid var(--line); border-radius: 99px; padding: 2px 9px; white-space: nowrap; }
  .error { background: color-mix(in srgb, var(--bad) 15%, var(--panel)); color: var(--bad); padding: 6px 14px; font-size: 12.5px; border-bottom: 1px solid var(--line); }
  main { flex: 1; min-height: 0; display: grid; grid-template-columns: minmax(0, 1fr) 300px; grid-template-rows: minmax(0, 1fr) 170px; }
  .centre { position: relative; display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  aside { grid-row: 1 / 3; grid-column: 2; border-left: 1px solid var(--line); background: var(--panel); min-height: 0; }
  footer { grid-column: 1; border-top: 1px solid var(--line); background: var(--panel); min-height: 0; }
  .toolbar { position: absolute; top: 8px; left: 10px; z-index: 2; display: flex; gap: 10px; flex-wrap: wrap; font-size: 12px; background: color-mix(in srgb, var(--panel) 88%, transparent); border: 1px solid var(--line); border-radius: 7px; padding: 4px 9px; }
  .toolbar label { display: flex; align-items: center; gap: 4px; cursor: pointer; color: var(--muted); }
  .metrics { flex: none; display: flex; align-items: center; gap: 14px; padding: 6px 12px; border-top: 1px solid var(--line); background: var(--panel); font-size: 12.5px; flex-wrap: wrap; }
  .stat { color: var(--muted); white-space: nowrap; }
  .stat b { color: var(--ink); font-variant-numeric: tabular-nums; }
  .stat.bad b, .stat.bad { color: var(--bad); }
  .sep { flex: 1; }
  .small { color: var(--muted); display: flex; align-items: center; gap: 4px; }
  .small input { width: 52px; }
  .small input, .small select { padding: 3px 5px; border: 1px solid var(--line); border-radius: 5px; background: var(--bg); color: var(--ink); font: 12px var(--mono); }
  .progress { width: 160px; height: 6px; border-radius: 3px; background: var(--hover); overflow: hidden; }
  .progress span { display: block; height: 100%; background: var(--accent); }
  .last { color: var(--muted); font-size: 12px; }
  .empty, .later { padding: 28px 32px; color: var(--muted); max-width: 760px; line-height: 1.55; overflow: auto; }
  .later h2 { color: var(--ink); margin-top: 0; }
  .later.rules { max-width: none; }
  table { border-collapse: collapse; font-size: 12px; }
  th, td { text-align: left; padding: 3px 12px 3px 0; border-bottom: 1px solid var(--line); }
  th { color: var(--muted); font-weight: 600; }
  td { color: var(--ink); }
  .pins { font: 11px var(--mono); color: var(--muted); }
  .wide main { grid-template-columns: minmax(0, 1fr); }
  .wide aside { display: none; }
  .embed main { grid-template-rows: minmax(0, 1fr) 140px; }
  @media (max-width: 820px) {
    main { grid-template-columns: 1fr; grid-template-rows: minmax(0, 1fr) 220px 150px; }
    aside { grid-row: 2; grid-column: 1; border-left: 0; border-top: 1px solid var(--line); }
    footer { grid-row: 3; }
    nav button { padding: 6px 7px; }
    .brand { display: none; }
  }
</style>
