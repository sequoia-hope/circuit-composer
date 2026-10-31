<script>
  // Along the bottom: checks, AI jobs, and the composition terminal.
  // `collapsible` (a phone): the open tab's button closes it again, leaving only the tabs.
  let { metrics, mode, refIndex, onselect, open = $bindable('checks'), collapsible = false } = $props();
  const pick = (tab) => (open = collapsible && open === tab ? null : tab);

  // Findings from the floorplanner's metrics: each names the parts it concerns.
  const findings = $derived.by(() => {
    if (!metrics) return [];
    const out = [];
    const refsIn = (s) => (s.match(/[A-Za-z_][A-Za-z0-9_]*/g) || []).filter((t) => refIndex.of.has(t));
    for (const s of metrics.overlap_pairs || []) out.push({ level: 'error', kind: 'overlap', text: s, refs: refsIn(s) });
    for (const s of metrics.edge_misses || []) out.push({ level: 'error', kind: 'edge', text: s, refs: refsIn(s) });
    for (const s of metrics.separation_misses || []) out.push({ level: 'error', kind: 'separation', text: s, refs: refsIn(s) });
    return out;
  });

  const server = $derived(mode === 'local');
</script>

<div class="dock">
  <div class="tabs" role="tablist">
    <button class:on={open === 'checks'} onclick={() => pick('checks')} role="tab">Checks {#if findings.length}<span class="n">{findings.length}</span>{:else if metrics}<span class="ok">✓</span>{/if}</button>
    <button class:on={open === 'jobs'} onclick={() => pick('jobs')} role="tab">AI jobs</button>
    <button class:on={open === 'terminal'} onclick={() => pick('terminal')} role="tab">Terminal</button>
  </div>
  <div class="body" hidden={!open}>
    {#if open === 'checks'}
      {#if !metrics}
        <p class="muted">Checking…</p>
      {:else if !findings.length}
        <p class="muted">The floorplan is legal: no courtyard overlaps, every edge connector on its edge and facing out, every separation kept.</p>
      {:else}
        <ul>
          {#each findings as f}
            <li><button onclick={() => onselect(f.refs.map((r) => refIndex.of.get(r)))}><span class="lvl {f.level}">{f.kind}</span> {f.text}</button></li>
          {/each}
        </ul>
      {/if}
    {:else if open === 'jobs'}
      <p class="muted">
        {#if server}No AI jobs yet.{:else}AI jobs fill in what a person leaves open: a block with no part chosen, a net with no class.
        They run your own Claude Code through Boardroom's job queue, so they need Composer running on your machine;
        this page is a static demo with no server behind it.{/if}
      </p>
    {:else}
      <div class="term">
        <div class="term-line"><span class="prompt">composer</span> <span class="muted">$</span> <span class="cursor"></span></div>
        <p class="muted">
          The composition terminal is a real Claude Code session in the project folder, with the circuit-composer
          skill loaded. Say "make this a 2 A power net" with parts selected, and its edits land on the canvas as they
          happen. It runs on Boardroom's server, so it {server ? 'connects once Boardroom is running' : 'is not available on this static page'}.
        </p>
      </div>
    {/if}
  </div>
</div>

<style>
  .dock { display: flex; flex-direction: column; height: 100%; }
  .tabs { display: flex; gap: 2px; padding: 0 8px; border-bottom: 1px solid var(--line); flex: none; }
  .tabs button { all: unset; cursor: pointer; padding: 7px 10px 6px; font-size: 12px; color: var(--muted); border-bottom: 2px solid transparent; }
  .tabs button.on { color: var(--ink); border-color: var(--accent); }
  .n { background: var(--bad); color: #fff; border-radius: 99px; padding: 0 6px; font-size: 10.5px; margin-left: 3px; }
  .ok { color: var(--good); margin-left: 3px; }
  .body { overflow: auto; padding: 8px 14px; flex: 1; font-size: 12.5px; }
  ul { list-style: none; margin: 0; padding: 0; }
  li button { all: unset; cursor: pointer; display: block; padding: 3px 0; font: 12px var(--mono); }
  li button:hover { color: var(--accent); }
  .lvl { font: 600 10.5px var(--sans); text-transform: uppercase; padding: 1px 6px; border-radius: 4px; margin-right: 6px; }
  .lvl.error { background: color-mix(in srgb, var(--bad) 20%, transparent); color: var(--bad); }
  .muted { color: var(--muted); margin: 4px 0; line-height: 1.5; max-width: 900px; }
  .term { font: 12px var(--mono); }
  .term-line { background: var(--term); color: var(--term-ink); padding: 6px 10px; border-radius: 6px; margin-bottom: 6px; }
  .prompt { color: var(--good); }
  .cursor { display: inline-block; width: 7px; height: 13px; background: var(--term-ink); vertical-align: -2px; animation: blink 1s steps(2) infinite; }
  @keyframes blink { 50% { opacity: 0; } }
</style>
