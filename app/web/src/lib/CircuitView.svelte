<script>
  // The Circuit workspace: the board as blocks (package and recipe instances, or the board's own
  // groups of parts) joined by the nets between them. A board with a loose composition
  // (loose.json) is also solved and composed here, live: the Connections panel shows and edits
  // its pins and recipes, and the graph can show the solved board instead of the drawn one. Open
  // fields for the AI to fill come in milestone D.
  import Connections from './Connections.svelte';

  let { circuit: drawn, parts, refIndex, selection, onselect, loose = null, start = null, packages = null, library = null, onsolved = () => {} } = $props();

  // The solved board, once the panel has one, and which of the two the graph shows.
  let live = $state.raw(null);
  let show = $state('solved');
  const circuit = $derived(show === 'solved' && live
    ? { parts: live.composed.parts, nets: live.composed.nets, instances: live.composition.instances }
    : drawn);

  // A part's block: its package instance, else the floorplan's block, else the circuit's; with
  // none, its designator's letters (LEDs, capacitors...), marked as such.
  const fpBlock = $derived(new Map(parts.map((p) => [p.reference, p.block])));
  const groupOf = (ref, p) => p.instance || fpBlock.get(ref) || p.block || `${ref.match(/^[A-Za-z]+/)?.[0] || '?'}*`;

  // A portrait screen (a phone) lays the graph out tall rather than wide, so the blocks keep a
  // readable size when it is scaled to fit.
  let gw = $state(0), gh = $state(0);
  const portrait = $derived(gh > gw * 1.1);

  const graph = $derived.by(() => {
    const tall = portrait;
    const groups = new Map();
    for (const [ref, p] of Object.entries(circuit.parts)) {
      const g = groupOf(ref, p);
      if (!groups.has(g)) groups.set(g, { name: g, refs: [], inst: circuit.instances?.[g], byRef: g.endsWith('*') });
      groups.get(g).refs.push(ref);
    }
    // Edges: nets that join two groups; a net touching many groups (a rail) is counted once per
    // pair but drawn faint.
    const edges = new Map();
    const rails = [];
    for (const [net, pins] of Object.entries(circuit.nets)) {
      const gs = [...new Set(pins.map((p) => p.split('.')[0]).filter((r) => circuit.parts[r]).map((r) => groupOf(r, circuit.parts[r])))];
      if (gs.length < 2) continue;
      if (gs.length > 4) { rails.push({ net, groups: gs }); continue; }
      for (let a = 0; a < gs.length; a++)
        for (let b = a + 1; b < gs.length; b++) {
          const k = [gs[a], gs[b]].sort().join('|');
          if (!edges.has(k)) edges.set(k, { a: gs[a], b: gs[b], nets: [] });
          edges.get(k).nets.push(net);
        }
    }
    const nodes = [...groups.values()];
    for (const d of nodes) {
      d.sub = d.inst?.recipe ? `recipe ${d.inst.recipe} · ${d.refs.length} part${d.refs.length === 1 ? '' : 's'}`
        : d.inst ? `${d.inst.package} · ${d.inst.variant}`
        : `${d.refs.length} part${d.refs.length === 1 ? '' : 's'}${d.byRef ? ', by designator' : ''}`;
      d.w = Math.max(120, d.name.length * 10 + 30, d.sub.length * 7 + 28);
    }
    layout(nodes, [...edges.values()], tall);
    return { nodes, edges: [...edges.values()], rails };
  });

  function layout(nodes, edges, tall) {
    // A few hundred steps of a spring layout from an ellipse: small graphs, done once per board.
    // Tall: the ellipse stands up and the pull to the middle is stronger across than down.
    const n = nodes.length;
    const [rx, ry, gx, gy] = tall ? [140, 320, 0.008, 0.001] : [260, 180, 0.002, 0.002];
    nodes.forEach((d, i) => { d.x = Math.cos((2 * Math.PI * i) / n) * rx; d.y = Math.sin((2 * Math.PI * i) / n) * ry; });
    const idx = new Map(nodes.map((d, i) => [d.name, i]));
    for (let it = 0; it < 400; it++) {
      const f = nodes.map(() => [0, 0]);
      for (let i = 0; i < n; i++)
        for (let j = i + 1; j < n; j++) {
          const dx = nodes[i].x - nodes[j].x, dy = nodes[i].y - nodes[j].y;
          const d2 = Math.max(100, dx * dx + dy * dy);
          const k = 60000 / d2;
          f[i][0] += (dx / Math.sqrt(d2)) * k; f[i][1] += (dy / Math.sqrt(d2)) * k;
          f[j][0] -= (dx / Math.sqrt(d2)) * k; f[j][1] -= (dy / Math.sqrt(d2)) * k;
        }
      for (const e of edges) {
        const i = idx.get(e.a), j = idx.get(e.b);
        const dx = nodes[j].x - nodes[i].x, dy = nodes[j].y - nodes[i].y;
        const d = Math.max(1, Math.hypot(dx, dy));
        const k = (d - 170) * 0.02 * Math.min(4, e.nets.length);
        f[i][0] += (dx / d) * k; f[i][1] += (dy / d) * k;
        f[j][0] -= (dx / d) * k; f[j][1] -= (dy / d) * k;
      }
      const cool = 1 - it / 400;
      nodes.forEach((d, i) => { d.x += Math.max(-20, Math.min(20, f[i][0])) * cool - d.x * gx; d.y += Math.max(-20, Math.min(20, f[i][1])) * cool - d.y * gy; });
    }
    // Then push apart any boxes that still overlap, along the shorter way out.
    for (let it = 0; it < 200; it++) {
      let moved = false;
      for (let i = 0; i < n; i++)
        for (let j = i + 1; j < n; j++) {
          const a = nodes[i], b = nodes[j];
          const ox = (a.w + b.w) / 2 + 24 - Math.abs(a.x - b.x), oy = 52 + 24 - Math.abs(a.y - b.y);
          if (ox <= 0 || oy <= 0) continue;
          moved = true;
          if (ox * (tall ? 3 : 1) < oy) { const s = (a.x < b.x ? -1 : 1) * ox / 2; a.x += s; b.x -= s; }
          else { const s = (a.y < b.y ? -1 : 1) * oy / 2; a.y += s; b.y -= s; }
        }
      if (!moved) break;
    }
  }

  const byName = $derived(new Map(graph.nodes.map((d) => [d.name, d])));
  const selectedRefs = $derived(new Set(selection.map((i) => refIndex.list[i])));
  const box = $derived.by(() => {
    const xs = graph.nodes.map((d) => d.x), ys = graph.nodes.map((d) => d.y);
    const pad = 40 + Math.max(...graph.nodes.map((d) => d.w)) / 2;
    return [Math.min(...xs) - pad, Math.min(...ys) - pad, Math.max(...xs) - Math.min(...xs) + 2 * pad, Math.max(...ys) - Math.min(...ys) + 2 * pad];
  });
  const view = $derived(box.join(' '));
  // Fitted, unless that would shrink the labels past reading (a big graph in a small panel):
  // then drawn at the least readable scale, and the panel scrolls.
  const MIN_SCALE = 0.55;
  const tooSmall = $derived(gw > 0 && Math.min(gw / box[2], (gh - 34) / box[3]) < MIN_SCALE);

  let hoverEdge = $state(null);

  function pick(node) {
    onselect(node.refs.map((r) => refIndex.of.get(r)).filter((i) => i !== undefined));
  }
</script>

<div class="wrap" class:split={loose && library}>
 <div class="graph" bind:clientWidth={gw} bind:clientHeight={gh}>
  {#if loose && library}
    <div class="which" role="group" aria-label="Which board the graph shows">
      <button class:on={show === 'drawn'} onclick={() => (show = 'drawn')} title="The board's own circuit">Drawn</button>
      <button class:on={show === 'solved'} onclick={() => (show = 'solved')} disabled={!live} title="loose.json, solved and composed in the browser">Solved</button>
    </div>
  {/if}
  <div class="scroll" class:pan={tooSmall}>
  <svg viewBox={view} role="img" aria-label="Circuit as blocks"
    style={tooSmall ? `width:${box[2] * MIN_SCALE}px;height:${box[3] * MIN_SCALE}px` : ''}>
    {#each graph.edges as e}
      {@const a = byName.get(e.a)}
      {@const b = byName.get(e.b)}
      <line x1={a.x} y1={a.y} x2={b.x} y2={b.y} class="edge" stroke-width={1 + Math.min(6, e.nets.length)}
        onpointerenter={() => (hoverEdge = e)} onpointerleave={() => (hoverEdge = null)} role="presentation" />
    {/each}
    {#each graph.nodes as d}
      {@const sel = d.refs.some((r) => selectedRefs.has(r))}
      {@const sub = d.sub}
      {@const w = d.w}
      <g transform="translate({d.x},{d.y})" class="node" class:sel onclick={() => pick(d)} role="button" tabindex="0"
        onkeydown={(e) => e.key === 'Enter' && pick(d)}>
        <rect x={-w / 2} y="-26" width={w} height="52" rx="8" />
        <text y="-6" class="name">{d.name}</text>
        <text y="13" class="sub">{sub}</text>
      </g>
    {/each}
  </svg>
  </div>
  <div class="legend">
    {#if hoverEdge}
      <b>{hoverEdge.a} ↔ {hoverEdge.b}</b>: {hoverEdge.nets.slice(0, 12).join(', ')}{hoverEdge.nets.length > 12 ? ` and ${hoverEdge.nets.length - 12} more` : ''}
    {:else}
      {graph.nodes.length} blocks, {graph.edges.length} links. Rails across many blocks: {graph.rails.map((r) => r.net).join(', ') || 'none'}.
      Click a block to select its parts; they light up in Board too.
    {/if}
  </div>
 </div>
  {#if loose && library}
    <Connections {loose} {start} {packages} {library} {drawn} onresult={(r) => { live = r; onsolved(r); }} />
  {/if}
</div>

<style>
  .wrap { position: relative; width: 100%; height: 100%; background: var(--canvas); }
  .wrap.split { display: grid; grid-template-columns: minmax(0, 1fr) minmax(300px, 380px); }
  .graph { position: relative; height: 100%; min-width: 0; }
  .which { position: absolute; top: 8px; left: 10px; z-index: 2; display: flex; }
  .which button { border-radius: 0; padding: 3px 10px; font-size: 12px; }
  .which button:first-child { border-radius: 6px 0 0 6px; }
  .which button:last-child { border-radius: 0 6px 6px 0; border-left: 0; }
  .which button.on { background: var(--hover); box-shadow: inset 0 -2px 0 var(--accent); }
  @media (max-width: 820px) { .wrap.split { grid-template-columns: 1fr; grid-template-rows: minmax(0, 1fr) minmax(0, 1fr); } }
  .scroll { height: calc(100% - 34px); }
  .scroll.pan { overflow: auto; }
  svg { width: 100%; height: 100%; display: block; }
  .edge { stroke: var(--rats); stroke-linecap: round; opacity: 0.7; }
  .edge:hover { stroke: var(--select); opacity: 1; }
  .node { cursor: pointer; outline: none; }
  .node:focus-visible rect { stroke: var(--accent); stroke-width: 3; }
  .node rect { fill: var(--panel); stroke: var(--line-strong); stroke-width: 1.5; }
  .node.sel rect { stroke: var(--select); stroke-width: 3; }
  .node:hover rect { stroke: var(--accent); }
  .name { text-anchor: middle; font: 600 15px var(--sans); fill: var(--ink); }
  .sub { text-anchor: middle; font: 11px var(--mono); fill: var(--muted); }
  .legend { height: 34px; padding: 8px 14px; font-size: 12px; color: var(--muted); border-top: 1px solid var(--line); overflow: hidden; white-space: nowrap; text-overflow: ellipsis; }
  .legend b { color: var(--ink); }
</style>
