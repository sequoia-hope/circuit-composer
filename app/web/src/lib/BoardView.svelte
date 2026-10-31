<script>
  // The Board workspace's canvas, drawn by board-draw (the renderer Composer shares with
  // UltraRoute Studio): the board's mesh comes from the core and only moved parts are rewritten;
  // the grid and Composer's overlay (block colours, ratsnest, locks, selection) are meshes of
  // their own; part references are written on a 2D canvas on top.
  import { onMount } from 'svelte';
  import { Renderer } from 'board-draw';

  let { doc, version, selection, layers, onselect, ondragstart, onchange, ondragend, busy } = $props();

  let canvas, labelCanvas;
  let renderer;
  let view = { cx: 0, cy: 0, zoom: 8 }; // zoom: CSS pixels per mm
  let fitted = null;
  // The view follows the board (fitted again on every resize: a phone turning, a panel opening,
  // Boardroom showing a hidden tab) until it is panned or zoomed by hand.
  let auto = $state(true);
  let drag = null;
  // Pointers down, by id, for two-finger pinch and pan.
  const pts = new Map();
  let pinch = null;
  let cursor = $state(null);
  let error = $state('');
  let queued = false;

  const dpr = () => window.devicePixelRatio || 1;
  const dark = () => {
    const t = document.documentElement.dataset.theme;
    return t ? t === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  };
  function background() {
    const v = getComputedStyle(document.documentElement).getPropertyValue('--canvas').trim();
    const m = v.match(/^#([0-9a-f]{6})$/i);
    if (!m) return [0.06, 0.06, 0.07];
    const n = parseInt(m[1], 16);
    return [(n >> 16) / 255, ((n >> 8) & 255) / 255, (n & 255) / 255];
  }

  function schedule() {
    if (queued) return;
    queued = true;
    requestAnimationFrame(() => { queued = false; draw(); });
  }

  function resize() {
    const r = canvas.getBoundingClientRect();
    for (const c of [canvas, labelCanvas]) {
      c.width = Math.max(1, Math.round(r.width * dpr()));
      c.height = Math.max(1, Math.round(r.height * dpr()));
    }
  }

  function bounds() {
    let b = doc.outline ? doc.outline.reduce((a, p) => [Math.min(a[0], p[0]), Math.min(a[1], p[1]), Math.max(a[2], p[0]), Math.max(a[3], p[1])], [Infinity, Infinity, -Infinity, -Infinity]) : doc.box;
    if (!b) b = [0, 0, 10, 10];
    return b;
  }

  export function fit() {
    const r = canvas.getBoundingClientRect();
    auto = true;
    // Hidden (a Boardroom tab not on show): fitted when it is shown and has a size.
    if (r.width < 1 || r.height < 1) return;
    const b = bounds();
    view.zoom = Math.max(0.5, Math.min((r.width - 64) / Math.max(1, b[2] - b[0]), (r.height - 64) / Math.max(1, b[3] - b[1])));
    view.cx = (b[0] + b[2]) / 2;
    view.cy = (b[1] + b[3]) / 2;
    schedule();
  }

  function draw() {
    if (!renderer || !doc) return;
    if (fitted !== doc.name) { resize(); fitted = doc.name; fit(); return; }
    const k = dpr();
    const rect = { x: 0, y: 0, w: canvas.width, h: canvas.height };
    const v = { cx: view.cx, cy: view.cy, zoom: view.zoom * k };
    const px = 1 / v.zoom;
    const mask = (layers.front ? 1 : 0) | (layers.back ? 2 : 0);
    const d = dark();
    const w2 = rect.w / 2 / v.zoom, h2 = rect.h / 2 / v.zoom;
    try {
      renderer.upload(0, doc.board.grid(px, view.cx - w2, view.cy - h2, view.cx + w2, view.cy + h2, d));
      renderer.upload(1, doc.board.mesh(px, mask));
      const showBox = !doc.outline && !doc.boxIsSet && doc.box;
      renderer.upload(2, doc.board.overlay(px, JSON.stringify({
        selected: selection, ratsnest: layers.ratsnest, blocks: layers.blocks, front: layers.front, back: layers.back,
        dashed_box: showBox ? doc.box : null, dark: d,
      })));
      renderer.clear(background());
      for (const slot of [0, 1, 2]) renderer.draw(slot, v, rect);
      error = '';
    } catch (e) {
      error = String(e);
    }
    drawLabels(k, d);
  }

  function drawLabels(k, d) {
    const ctx = labelCanvas.getContext('2d');
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.clearRect(0, 0, labelCanvas.width, labelCanvas.height);
    if (!layers.labels) return;
    const list = JSON.parse(doc.board.labels(view.zoom, 9, new Int32Array(selection)));
    ctx.setTransform(k, 0, 0, k, 0, 0);
    ctx.textAlign = 'center';
    ctx.textBaseline = 'middle';
    const r = canvas.getBoundingClientRect();
    const sel = new Set(selection);
    for (const l of list) {
      const part = doc.parts[l.i];
      if (!part || part.reference.startsWith('shape:')) continue;
      const sx = (l.x - view.cx) * view.zoom + r.width / 2;
      const sy = (l.y - view.cy) * view.zoom + r.height / 2;
      if (sx < -40 || sy < -20 || sx > r.width + 40 || sy > r.height + 20) continue;
      ctx.font = `${Math.max(9, Math.min(16, l.size * view.zoom))}px ui-monospace, monospace`;
      ctx.fillStyle = sel.has(l.i) ? (d ? '#ff8a4c' : '#d9480f') : part.back ? (d ? 'rgba(170,170,180,.6)' : 'rgba(90,90,100,.6)') : d ? '#e7e6e2' : '#2b2c30';
      ctx.fillText(l.name, sx, sy);
    }
  }

  $effect(() => {
    // Redraw on any change the props carry; the side toggles also change picking.
    version; selection; doc;
    if (doc && renderer) {
      doc.board.set_side_hidden(false, !layers.front);
      doc.board.set_side_hidden(true, !layers.back);
    }
    layers.ratsnest; layers.blocks; layers.labels;
    schedule();
  });

  onMount(() => {
    try {
      renderer = new Renderer(canvas, { yDown: true }); // KiCad's frame: y grows downwards
    } catch (e) {
      error = `${e.message} Composer draws boards with WebGL2.`;
      return;
    }
    const ro = new ResizeObserver(() => { resize(); auto ? fit() : schedule(); });
    ro.observe(canvas);
    const mq = matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', schedule);
    schedule();
    return () => { ro.disconnect(); mq.removeEventListener('change', schedule); };
  });

  function toBoard(e) {
    const r = canvas.getBoundingClientRect();
    return [view.cx + (e.clientX - r.left - r.width / 2) / view.zoom, view.cy + (e.clientY - r.top - r.height / 2) / view.zoom];
  }

  function down(e) {
    canvas.focus();
    pts.set(e.pointerId, [e.clientX, e.clientY]);
    canvas.setPointerCapture(e.pointerId);
    if (pts.size === 2) {
      // A second finger: pinch and pan, and whatever one finger was doing ends.
      if (drag?.kind === 'part' && drag.moved) ondragend();
      drag = null;
      pinch = pinchState();
      return;
    }
    if (pts.size > 2) return;
    const [x, y] = toBoard(e);
    const i = e.button === 0 ? doc.board.pick(x, y) : -1;
    const touch = e.pointerType !== 'mouse';
    if (touch && !(i >= 0 && selection.includes(i))) {
      // A finger pans; it moves a part only once the part is selected, so panning a dense board
      // can't drag what it starts on. A tap selects (or, on nothing, clears the selection).
      drag = { kind: 'pan', last: [e.clientX, e.clientY], tap: i, at: [e.clientX, e.clientY] };
      return;
    }
    if (i >= 0) {
      if (e.shiftKey) {
        const s = new Set(selection);
        s.has(i) ? s.delete(i) : s.add(i);
        onselect([...s]);
      } else if (!selection.includes(i)) {
        onselect([i]);
      }
      const p = doc.parts[i];
      drag = { kind: 'part', i, start: [x, y], pose: [p.pose.x, p.pose.y], moved: false, locked: p.locked };
    } else {
      if (!e.shiftKey && e.button === 0) onselect([]);
      drag = { kind: 'pan', last: [e.clientX, e.clientY] };
    }
  }

  /** The two fingers' midpoint and spread, and the board point under the midpoint. */
  function pinchState() {
    const [a, b] = [...pts.values()];
    const mid = { clientX: (a[0] + b[0]) / 2, clientY: (a[1] + b[1]) / 2 };
    return { spread: Math.max(1, Math.hypot(a[0] - b[0], a[1] - b[1])), zoom: view.zoom, at: toBoard(mid) };
  }

  function move(e) {
    if (pts.has(e.pointerId)) pts.set(e.pointerId, [e.clientX, e.clientY]);
    if (pinch && pts.size >= 2) {
      const [a, b] = [...pts.values()];
      const mid = [(a[0] + b[0]) / 2, (a[1] + b[1]) / 2];
      view.zoom = Math.min(400, Math.max(0.5, (pinch.zoom * Math.hypot(a[0] - b[0], a[1] - b[1])) / pinch.spread));
      const r = canvas.getBoundingClientRect();
      // The board point first under the fingers stays under them.
      view.cx = pinch.at[0] - (mid[0] - r.left - r.width / 2) / view.zoom;
      view.cy = pinch.at[1] - (mid[1] - r.top - r.height / 2) / view.zoom;
      auto = false;
      schedule();
      return;
    }
    const [x, y] = toBoard(e);
    if (e.pointerType === 'mouse') cursor = [x, y];
    if (!drag) return;
    if (drag.kind === 'pan') {
      if (drag.at && Math.hypot(e.clientX - drag.at[0], e.clientY - drag.at[1]) > 8) drag.at = null;
      if (drag.at) return; // still a tap
      view.cx -= (e.clientX - drag.last[0]) / view.zoom;
      view.cy -= (e.clientY - drag.last[1]) / view.zoom;
      drag.last = [e.clientX, e.clientY];
      auto = false;
      schedule();
    } else if (!drag.locked && !busy) {
      const step = e.altKey ? 0 : 0.05;
      const snap = (v) => (step ? Math.round(v / step) * step : v);
      if (!drag.moved) ondragstart();
      doc.board.move_to(drag.i, snap(drag.pose[0] + x - drag.start[0]), snap(drag.pose[1] + y - drag.start[1]));
      drag.moved = true;
      onchange();
    }
  }

  function up(e) {
    pts.delete(e.pointerId);
    if (pinch) {
      // Down to one finger: it pans on from where it is, with no jump.
      pinch = null;
      const rest = [...pts.values()][0];
      drag = rest ? { kind: 'pan', last: rest } : null;
      return;
    }
    if (drag?.kind === 'part' && drag.moved) ondragend();
    if (drag?.kind === 'pan' && drag.at) onselect(drag.tap >= 0 ? [drag.tap] : []);
    drag = null;
  }

  function wheel(e) {
    e.preventDefault();
    const [bx, by] = toBoard(e);
    view.zoom = Math.min(400, Math.max(0.5, view.zoom * Math.exp(-e.deltaY * 0.0015)));
    auto = false;
    const r = canvas.getBoundingClientRect();
    view.cx = bx - (e.clientX - r.left - r.width / 2) / view.zoom;
    view.cy = by - (e.clientY - r.top - r.height / 2) / view.zoom;
    schedule();
  }
</script>

<div class="wrap">
  <canvas
    bind:this={canvas}
    tabindex="0"
    onpointerdown={down}
    onpointermove={move}
    onpointerup={up}
    onpointercancel={up}
    onwheel={wheel}
    oncontextmenu={(e) => e.preventDefault()}
    aria-label="Board floorplan"
  ></canvas>
  <canvas class="labels" bind:this={labelCanvas} aria-hidden="true"></canvas>
  {#if cursor}
    <div class="cursor">x {cursor[0].toFixed(2)} &nbsp;y {cursor[1].toFixed(2)} mm</div>
  {/if}
  {#if error}<div class="err">{error}</div>{/if}
  {#if !auto}<button class="fit" onclick={fit} title="Fit the board in view (F)">Fit</button>{/if}
</div>

<style>
  .wrap { position: relative; width: 100%; height: 100%; min-height: 0; flex: 1; }
  canvas { position: absolute; inset: 0; width: 100%; height: 100%; display: block; outline: none; touch-action: none; }
  canvas.labels { pointer-events: none; }
  .cursor { position: absolute; right: 10px; bottom: 8px; font: 11px var(--mono); color: var(--muted); pointer-events: none; }
  .fit { position: absolute; right: 10px; top: 10px; z-index: 2; padding: 4px 12px; }
  .err { position: absolute; left: 12px; bottom: 8px; color: var(--bad); font-size: 12px; }
</style>
