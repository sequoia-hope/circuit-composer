<script>
  // The right-hand panel: what is selected, and what can be done to it.
  let { doc, selection, circuit, onlock, onrotate, onpose, onselect } = $props();

  const parts = $derived(selection.map((i) => doc.parts[i]).filter(Boolean));
  const one = $derived(parts.length === 1 ? parts[0] : null);
  const info = $derived(one ? circuit.parts[one.reference] || {} : {});
  const nets = $derived.by(() => {
    if (!one) return [];
    const out = [];
    for (const [net, pins] of Object.entries(circuit.nets)) {
      const mine = pins.filter((p) => p.split('.')[0] === one.reference).map((p) => p.split('.')[1]);
      if (mine.length) out.push({ net, pins: mine, others: pins.length - mine.length });
    }
    return out.sort((a, b) => a.net.localeCompare(b.net));
  });

  let filter = $state('');
  const matches = $derived.by(() => {
    const f = filter.trim().toLowerCase();
    if (!f) return [];
    return doc.parts.map((p, i) => ({ p, i, c: circuit.parts[p.reference] || {} }))
      .filter(({ p, c }) => p.reference.toLowerCase().includes(f) || (c.value || '').toLowerCase().includes(f) || (p.block || '').toLowerCase().includes(f))
      .slice(0, 40);
  });

  function num(v) {
    return Math.round(v * 1000) / 1000;
  }
</script>

<div class="insp">
  <input class="find" placeholder="Find a part: ref, value or block" bind:value={filter} />
  {#if matches.length}
    <ul class="hits">
      {#each matches as m}
        <li><button onclick={() => { onselect([m.i]); filter = ''; }}><b>{m.p.reference}</b> {m.c.value || ''} <span>{m.p.block || ''}</span></button></li>
      {/each}
    </ul>
  {/if}

  {#if one}
    <h2>{one.reference} <span class="side">{one.back ? 'bottom' : 'top'}</span></h2>
    <dl>
      <dt>Value</dt><dd>{info.value || '—'}</dd>
      <dt>Footprint</dt><dd class="fp">{info.footprint || '—'}</dd>
      {#if info.instance}<dt>Instance</dt><dd>{info.instance} · role {info.role}</dd>{/if}
      <dt>Block</dt><dd>{one.block || info.block || '—'}</dd>
      {#if one.group}<dt>Rigid group</dt><dd>{one.group} <small>(moves as one)</small></dd>{/if}
    </dl>
    <div class="pose">
      <label>x <input type="number" step="0.05" value={num(one.pose.x)} disabled={one.locked} onchange={(e) => onpose(selection[0], +e.target.value, one.pose.y)} /></label>
      <label>y <input type="number" step="0.05" value={num(one.pose.y)} disabled={one.locked} onchange={(e) => onpose(selection[0], one.pose.x, +e.target.value)} /></label>
      <span class="rot">{num(one.pose.rotation)}°</span>
    </div>
    <div class="actions">
      <button onclick={() => onlock(!one.locked)}>{one.locked ? 'Unlock' : 'Lock'} <kbd>L</kbd></button>
      <button onclick={onrotate} disabled={one.locked}>Rotate 90° <kbd>R</kbd></button>
    </div>
    <p class="hint">{one.locked ? 'Locked: the floorplanner leaves it where it is, and so does dragging.' : 'Free: the floorplanner may move it. Drag to place it; lock it to keep it.'}</p>
    {#if nets.length}
      <h3>Nets</h3>
      <ul class="nets">
        {#each nets as n}<li><b>{n.net}</b> <span>pin {n.pins.join(', ')}{n.others ? ` · ${n.others} more` : ''}</span></li>{/each}
      </ul>
    {/if}
  {:else if parts.length > 1}
    <h2>{parts.length} parts</h2>
    <p class="refs">{parts.map((p) => p.reference).join(' ')}</p>
    <div class="actions">
      <button onclick={() => onlock(true)}>Lock all</button>
      <button onclick={() => onlock(false)}>Unlock all</button>
    </div>
  {:else}
    <h2>{doc.name}</h2>
    <p class="hint">{doc.note}</p>
    <dl>
      <dt>Parts</dt><dd>{doc.parts.length} ({doc.parts.filter((p) => p.locked).length} locked, {doc.parts.filter((p) => p.back).length} on the bottom)</dd>
      <dt>Nets</dt><dd>{Object.keys(circuit.nets).length}</dd>
      <dt>Outline</dt><dd>{doc.outline ? 'shaped (set by the mechanical design)' : doc.boxIsSet ? 'rectangle (set)' : "the floorplanner's to choose"}</dd>
    </dl>
    <p class="hint">Click a part to inspect it, shift-click to add to the selection, drag to move. Wheel zooms, drag the background to pan, <kbd>F</kbd> fits.</p>
  {/if}
</div>

<style>
  .insp { padding: 12px 14px 20px; overflow-y: auto; height: 100%; }
  .find { width: 100%; padding: 6px 8px; border: 1px solid var(--line); border-radius: 6px; background: var(--bg); color: var(--ink); font: inherit; }
  .hits { list-style: none; margin: 4px 0 10px; padding: 0; border: 1px solid var(--line); border-radius: 6px; max-height: 220px; overflow: auto; }
  .hits button { all: unset; display: block; width: 100%; box-sizing: border-box; padding: 4px 8px; cursor: pointer; font-size: 12px; }
  .hits button:hover { background: var(--hover); }
  .hits span { color: var(--muted); float: right; }
  h2 { font-size: 16px; margin: 14px 0 8px; }
  h2 .side { font-size: 11px; font-weight: 500; color: var(--muted); border: 1px solid var(--line); border-radius: 99px; padding: 1px 7px; vertical-align: 2px; }
  h3 { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); margin: 16px 0 6px; }
  dl { display: grid; grid-template-columns: 84px 1fr; gap: 4px 8px; margin: 0; font-size: 12.5px; }
  dt { color: var(--muted); }
  dd { margin: 0; overflow-wrap: anywhere; }
  .fp { font: 11px var(--mono); }
  .pose { display: flex; gap: 8px; align-items: center; margin-top: 12px; font-size: 12px; }
  .pose input { width: 76px; padding: 3px 5px; border: 1px solid var(--line); border-radius: 5px; background: var(--bg); color: var(--ink); font: 12px var(--mono); }
  .rot { font: 12px var(--mono); color: var(--muted); }
  .actions { display: flex; gap: 6px; margin-top: 10px; flex-wrap: wrap; }
  .hint { color: var(--muted); font-size: 12px; line-height: 1.45; }
  .nets { list-style: none; margin: 0; padding: 0; font-size: 12px; }
  .nets li { padding: 2px 0; border-bottom: 1px solid var(--line); }
  .nets span { color: var(--muted); }
  .refs { font: 12px var(--mono); color: var(--muted); line-height: 1.6; }
  kbd { font: 10px var(--mono); border: 1px solid var(--line); border-radius: 3px; padding: 0 3px; color: var(--muted); }
</style>
