<script>
  // The board's loose composition, solved in the browser: each connection's pins as the solver
  // chose them (or as pinned here), what the interface rules added, and the recipes' parameters.
  // Every change solves and composes again; the block graph beside it shows the result.
  import { onMount } from 'svelte';
  import { recipe_expand } from '../core/composer_core.js';
  import { solve, drawnPort, natural } from './compose.js';

  let { loose: initial, packages, library, drawn, onresult } = $props();

  // A copy to edit: the parent mounts the panel afresh for each board.
  // svelte-ignore state_referenced_locally
  let loose = $state.raw(structuredClone(initial));
  let result = $state.raw(null);
  let error = $state('');
  let busy = $state(false);
  let ms = $state(0);

  onMount(run);

  function run() {
    busy = true;
    // Let the page paint "Solving…" first: the bolt's 15 connections take a moment.
    setTimeout(() => {
      const t = performance.now();
      try {
        result = solve(loose, packages, library, drawn);
        error = '';
      } catch (e) {
        result = null;
        error = String(e.message || e);
      }
      ms = performance.now() - t;
      busy = false;
      onresult(result?.composed ? { composed: result.composed, composition: result.solved.composition } : null);
    }, 0);
  }

  /** An instance's interface: its package's, or its recipe's. */
  function iface(from) {
    const [inst, name] = from.split('.');
    const i = loose.instances[inst];
    if (!i) return null;
    return i.recipe ? library.recipes[i.recipe]?.interfaces?.[name] : packages[i.package]?.ports?.interfaces?.[name];
  }

  const partnerCircuit = (inst) => {
    const i = loose.instances[inst];
    return i && !i.recipe ? packages[i.package]?.circuits?.[i.variant] : null;
  };

  /** Each connection, each of its device's signals, and the partner pin the solver gave it. */
  const rows = $derived.by(() => {
    const pinout = result?.solved?.pinout || [];
    return loose.connect.map((c, k) => {
      const label = `${c.from} -> ${c.to}`;
      const partner = c.to.split('.')[0];
      const pc = partnerCircuit(partner);
      const inst = c.from.split('.')[0];
      const pins = iface(c.from)?.pins;
      const sigs = pins ? Object.entries(pins) : [['IO', c.from.split('.')[1]]];
      return {
        k, c, label, partner,
        options: pc ? Object.entries(pc.ports).filter(([, p]) => p.kind === 'signal').map(([n]) => n).sort(natural) : [],
        sigs: sigs.map(([sig, port]) => {
          const row = pinout.find((r) => r.connection === label && r.device === `${inst}.${port}`);
          const got = row?.partner.split('.')[1] ?? null;
          const was = row && result.composed && pc ? drawnPort(row, result.composed, drawn, pc) : null;
          return { sig, port, row, got, was, pinned: c.pins?.[sig] ?? '' };
        }),
      };
    });
  });
  const differ = $derived(rows.flatMap((r) => r.sigs).filter((s) => s.was && s.got && s.was !== s.got).length);

  function pin(k, sig, value) {
    const connect = loose.connect.map((c, i) => {
      if (i !== k) return c;
      const pins = { ...(c.pins || {}) };
      if (value) pins[sig] = value;
      else delete pins[sig];
      const { pins: _, ...rest } = c;
      return Object.keys(pins).length ? { ...rest, pins } : rest;
    });
    loose = { ...loose, connect };
    run();
  }

  /** Pin every connection where the drawn board has it, or free them all. */
  function pinAll(drawnToo) {
    const connect = loose.connect.map((c, k) => {
      const { pins: _, ...rest } = c;
      if (!drawnToo) return rest;
      const pins = {};
      for (const s of rows[k].sigs) if (s.was) pins[s.sig] = s.was;
      return Object.keys(pins).length ? { ...rest, pins } : rest;
    });
    loose = { ...loose, connect };
    run();
  }

  // Recipes: each instance's parameters, and what they expand to.
  const recipes = $derived(Object.entries(loose.instances).filter(([, i]) => i.recipe).map(([name, i]) => {
    const r = library.recipes[i.recipe];
    let x = null, why = '';
    try {
      x = JSON.parse(recipe_expand(JSON.stringify(r), JSON.stringify(i.params || {}), JSON.stringify(library.stock)));
    } catch (e) {
      why = String(e.message || e);
    }
    return { name, i, r, x, why };
  }));

  function param(name, key, raw) {
    const i = loose.instances[name];
    const params = { ...(i.params || {}) };
    const v = raw.trim();
    if (v === '') delete params[key];
    else params[key] = /^-?\d+(\.\d+)?(e-?\d+)?$/i.test(v) ? Number(v) : v;
    loose = { ...loose, instances: { ...loose.instances, [name]: { ...i, params } } };
    run();
  }

  function reset() {
    loose = structuredClone(initial);
    run();
  }

  const s = $derived(result?.solved);
</script>

<div class="panel">
  <div class="head">
    <h3>Connections</h3>
    <span class="grow"></span>
    <button onclick={() => pinAll(true)} disabled={busy || !result?.composed} title="Pin every connection where the drawn board has it">Drawn pins</button>
    <button onclick={() => pinAll(false)} disabled={busy} title="Let the solver choose every pin">Free all</button>
    <button onclick={reset} disabled={busy} title="Back to the board's loose.json">Reset</button>
  </div>
  <p class="status" class:bad={error || (s && !s.ok)}>
    {#if busy}Solving…
    {:else if error}{error}
    {:else if s?.ok}
      {loose.connect.length} connections placed, cost {s.cost.toFixed(2)}, {s.exhaustive ? 'every assignment searched' : 'stopped at the node limit'}, {Math.round(ms)} ms.
      {#if result.composed && !result.composed.ok}<br />Composing refused: {result.composed.errors.slice(0, 3).join('; ')}{/if}
      {#if differ}<br /><b>{differ}</b> pin{differ === 1 ? '' : 's'} differ from the drawn board (marked).{:else if result.composed?.ok}<br />Every pin as the drawn board has it.{/if}
    {:else if s}Refused.{/if}
  </p>
  {#if s?.errors?.length}<ul class="msgs bad">{#each s.errors as m}<li>{m}</li>{/each}</ul>{/if}

  <table>
    <thead><tr><th>From</th><th>Pin</th><th>Via</th></tr></thead>
    <tbody>
      {#each rows as r (r.k)}
        {#each r.sigs as g, j}
          <tr class:first={j === 0} class:moved={g.was && g.got && g.was !== g.got}>
            <td>{#if j === 0}<b>{r.c.from}</b>{/if}{#if r.sigs.length > 1}<span class="sig"> {g.sig}</span>{/if}
              {#if j === 0 && r.c.name}<div class="net">{r.c.name}</div>{/if}</td>
            <td>
              <select value={g.pinned} onchange={(e) => pin(r.k, g.sig, e.target.value)} disabled={busy} title={g.pinned ? 'Pinned here' : 'The solver chooses'}>
                <option value="">{g.got && !g.pinned ? `${g.got} (auto)` : 'auto'}</option>
                {#each r.options as o}<option value={o}>{o}</option>{/each}
              </select>
              {#if g.was && g.got && g.was !== g.got}<div class="was">drawn: {g.was}</div>{/if}
            </td>
            <td class="via">{g.row ? `${g.row.via}${g.row.function ? ` · ${g.row.function}` : ''}` : '—'}</td>
          </tr>
        {/each}
      {/each}
    </tbody>
  </table>

  {#if s?.added?.length}<h4>Added by the rules</h4><ul class="msgs">{#each s.added as m}<li>{m}</li>{/each}</ul>{/if}
  {#if s?.warnings?.length}<h4>Warnings</h4><ul class="msgs warn">{#each s.warnings as m}<li>{m}</li>{/each}</ul>{/if}

  <h4>Recipes</h4>
  {#each recipes as q (q.name)}
    <details>
      <summary><b>{q.name}</b> <span class="muted">{q.i.recipe}</span></summary>
      <p class="muted small">{q.r?.summary}</p>
      {#if q.r && Object.keys(q.r.params || {}).length}
        <div class="params">
          {#each Object.entries(q.r.params) as [k, p]}
            <label title={p.note || ''}>{k}{p.unit ? ` (${p.unit})` : ''}
              {#if p.choices}
                <select value={q.i.params?.[k] ?? p.default} onchange={(e) => param(q.name, k, e.target.value)} disabled={busy}>
                  {#each Object.keys(p.choices) as c}<option value={c}>{c}</option>{/each}
                </select>
              {:else}
                <input value={q.i.params?.[k] ?? ''} placeholder={String(p.default ?? '')} onchange={(e) => param(q.name, k, e.target.value)} disabled={busy} />
              {/if}
            </label>
          {/each}
        </div>
      {:else}<p class="muted small">No parameters.</p>{/if}
      {#if q.why}<p class="bad small">{q.why}</p>
      {:else if q.x}
        <p class="small parts">{Object.entries(q.x.parts).slice(0, 8).map(([role, p]) => `${role} ${p.value}`).join(' · ')}{Object.keys(q.x.parts).length > 8 ? ` · and ${Object.keys(q.x.parts).length - 8} more` : ''}</p>
        {#if Object.keys(q.x.chosen || {}).length}<p class="small muted">Chosen: {Object.entries(q.x.chosen).map(([role, c]) => `${role} ${c}`).join('; ')}</p>{/if}
      {/if}
    </details>
  {/each}
</div>

<style>
  .panel { height: 100%; overflow: auto; padding: 10px 12px 20px; border-left: 1px solid var(--line); background: var(--panel); font-size: 12.5px; }
  .head { display: flex; align-items: center; gap: 6px; }
  .head button { padding: 2px 8px; font-size: 12px; }
  h3 { margin: 0; font-size: 14px; }
  h4 { margin: 14px 0 4px; font-size: 12.5px; }
  .grow { flex: 1; }
  .status { color: var(--muted); margin: 8px 0; line-height: 1.45; }
  .status b { color: var(--select); }
  .bad { color: var(--bad) !important; }
  .muted { color: var(--muted); }
  .small { font-size: 11.5px; margin: 4px 0; }
  table { border-collapse: collapse; width: 100%; }
  th { text-align: left; color: var(--muted); font-weight: 600; padding: 2px 4px 4px 0; }
  td { padding: 2px 4px 2px 0; vertical-align: top; }
  tr.first td { border-top: 1px solid var(--line); padding-top: 5px; }
  tr.moved select { border-color: var(--select); }
  .sig, .net { font: 11px var(--mono); color: var(--muted); }
  .was { font: 11px var(--mono); color: var(--select); }
  .via { font: 11px var(--mono); color: var(--muted); white-space: nowrap; }
  select, input { font: 11.5px var(--mono); padding: 2px 4px; border: 1px solid var(--line); border-radius: 5px; background: var(--bg); color: var(--ink); max-width: 165px; }
  .msgs { margin: 4px 0; padding-left: 16px; color: var(--ink); }
  .msgs li { margin: 2px 0; }
  .msgs.warn li { color: var(--lock); }
  details { border-top: 1px solid var(--line); padding: 5px 0; }
  summary { cursor: pointer; }
  .params { display: grid; grid-template-columns: 1fr; gap: 4px; margin: 4px 0; }
  .params label { display: flex; justify-content: space-between; align-items: center; gap: 8px; color: var(--muted); }
  .parts { font-family: var(--mono); color: var(--ink); }
</style>
