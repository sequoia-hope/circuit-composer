<script>
  // The Board workspace's canvas: draws the floorplan through the scene, picks and drags parts.
  import { onMount } from 'svelte';
  import { Scene } from './scene.js';

  let { doc, version, selection, layers, onselect, ondragstart, onchange, ondragend, busy } = $props();

  let canvas;
  let scene;
  let fitted = null;
  let drag = null; // {kind: 'part'|'pan', i, start: [bx, by], pose: [x, y], moved}

  function theme() {
    const s = getComputedStyle(document.documentElement);
    const v = (k) => s.getPropertyValue(k).trim();
    return {
      canvas: v('--canvas'), grid: v('--grid'), gridMajor: v('--grid-major'), board: v('--board'), edge: v('--edge'),
      select: v('--select'), lock: v('--lock'), pin: v('--pin'), pinBack: v('--pin-back'), rats: v('--rats'),
      label: v('--label'), labelBack: v('--label-back'),
    };
  }

  function draw() {
    if (!scene || !doc) return;
    scene.setParts(doc.parts);
    scene.setRatsnest(doc.rats);
    scene.setOutline(doc.outline, doc.box, doc.boxIsSet);
    scene.selected = new Set(selection);
    Object.assign(scene.layers, layers);
    if (fitted !== doc.name) {
      scene.resize();
      scene.fit();
      fitted = doc.name;
    }
    scene.render(theme());
  }

  $effect(() => {
    // Redraw on any change the props carry.
    version; selection; layers; doc;
    draw();
  });

  onMount(() => {
    scene = new Scene(canvas);
    const ro = new ResizeObserver(() => { scene.resize(); draw(); });
    ro.observe(canvas);
    const mq = matchMedia('(prefers-color-scheme: dark)');
    mq.addEventListener('change', draw);
    return () => { ro.disconnect(); mq.removeEventListener('change', draw); };
  });

  export function fit() {
    scene.fit();
    draw();
  }

  const at = (e) => {
    const r = canvas.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };

  function down(e) {
    canvas.focus();
    const [px, py] = at(e);
    const i = e.button === 0 ? scene.pick(px, py) : -1;
    if (i >= 0) {
      if (e.shiftKey) {
        const s = new Set(selection);
        s.has(i) ? s.delete(i) : s.add(i);
        onselect([...s]);
      } else if (!selection.includes(i)) {
        onselect([i]);
      }
      const p = doc.parts[i];
      drag = { kind: 'part', i, start: scene.toBoard(px, py), pose: [p.pose.x, p.pose.y], moved: false, locked: p.locked };
    } else {
      if (!e.shiftKey && e.button === 0) onselect([]);
      drag = { kind: 'pan', last: [px, py] };
    }
    canvas.setPointerCapture(e.pointerId);
  }

  function move(e) {
    const [px, py] = at(e);
    if (!drag) {
      const h = scene.pick(px, py);
      if (h !== scene.hover) { scene.hover = h; draw(); }
      cursor = scene.toBoard(px, py);
      return;
    }
    if (drag.kind === 'pan') {
      scene.pan(px - drag.last[0], py - drag.last[1]);
      drag.last = [px, py];
      draw();
    } else if (!drag.locked && !busy) {
      const [bx, by] = scene.toBoard(px, py);
      const step = e.altKey ? 0 : 0.05;
      const snap = (v) => (step ? Math.round(v / step) * step : v);
      if (!drag.moved) ondragstart();
      doc.board.move_to(drag.i, snap(drag.pose[0] + bx - drag.start[0]), snap(drag.pose[1] + by - drag.start[1]));
      drag.moved = true;
      onchange();
    }
  }

  function up() {
    if (drag?.kind === 'part' && drag.moved) ondragend();
    drag = null;
  }

  function wheel(e) {
    e.preventDefault();
    const [px, py] = at(e);
    scene.zoomAt(px, py, Math.exp(-e.deltaY * 0.0015));
    draw();
  }

  let cursor = $state(null);
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
  {#if cursor}
    <div class="cursor">x {cursor[0].toFixed(2)} &nbsp;y {cursor[1].toFixed(2)} mm</div>
  {/if}
</div>

<style>
  .wrap { position: relative; width: 100%; height: 100%; }
  canvas { width: 100%; height: 100%; display: block; outline: none; cursor: default; touch-action: none; }
  .cursor { position: absolute; right: 10px; bottom: 8px; font: 11px var(--mono); color: var(--muted); pointer-events: none; }
</style>
