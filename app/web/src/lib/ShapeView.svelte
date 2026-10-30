<script>
  // The Shape workspace: the board's outline built from features (rectangles, circles, slots,
  // polygons, generators), each added or cut away in order; corner rounding and symmetry over
  // the result; mounting holes; keep-outs. The document is shape.json, evaluated by the core
  // (crates/shape), and "Use as board outline" hands the result to the floorplanner.
  import { onMount } from 'svelte';
  import { shape_evaluate, shape_generators, shape_import } from '../core/composer_core.js';

  let { doc: shapeDoc = $bindable(), applied, onapply, onchange } = $props();

  const generators = JSON.parse(shape_generators());
  let selected = $state(shapeDoc.features.length ? { kind: 'feature', i: 0 } : null);
  let result = $state.raw(null);
  let canvas;
  let view = { scale: 6, x: -10, y: -10 };
  let drag = null;
  let undo = [];
  let fitted = false;

  function evaluate() {
    try {
      result = JSON.parse(shape_evaluate(JSON.stringify(shapeDoc)));
    } catch (e) {
      result = { outline: [], cutouts: [], features: [], holes: [], keepouts: [], warnings: [String(e)], area_mm2: 0, bbox: [0, 0, 0, 0] };
    }
    if (!fitted && result.outline.length) { fit(); fitted = true; }
    draw();
  }

  /** Every edit goes through here: remembered for undo, re-evaluated, reported. */
  function edit(fn, remember = true) {
    if (remember) { undo.push(JSON.stringify(shapeDoc)); if (undo.length > 200) undo.shift(); }
    fn(shapeDoc);
    shapeDoc = { ...shapeDoc };
    evaluate();
    onchange?.();
  }

  function undoLast() {
    const s = undo.pop();
    if (!s) return;
    shapeDoc = JSON.parse(s);
    if (selected?.kind === 'feature' && selected.i >= shapeDoc.features.length) selected = null;
    evaluate();
    onchange?.();
  }

  // ---- adding things, at the middle of the view
  function centre() {
    const r = canvas.getBoundingClientRect();
    const [x, y] = toBoard(r.width / 2, r.height / 2);
    return [Math.round(x), Math.round(y)];
  }
  function add(geom, name, op = 'add') {
    edit((d) => d.features.push({ name, op, geom }));
    selected = { kind: 'feature', i: shapeDoc.features.length - 1 };
  }
  function addRect() { const [x, y] = centre(); add({ type: 'rect', x, y, w: 30, h: 20, r: 1, angle: 0 }, `rectangle ${shapeDoc.features.length + 1}`); }
  function addCircle() { const [x, y] = centre(); add({ type: 'circle', x, y, d: 20 }, `circle ${shapeDoc.features.length + 1}`); }
  function addSlot() { const [x, y] = centre(); add({ type: 'slot', x1: x - 10, y1: y, x2: x + 10, y2: y, w: 6 }, `slot ${shapeDoc.features.length + 1}`); }
  function addPolygon() { const [x, y] = centre(); add({ type: 'polygon', points: [[x - 10, y + 8], [x, y - 10], [x + 10, y + 8]] }, `polygon ${shapeDoc.features.length + 1}`); }
  function addGenerator(g) { add({ type: 'generator', name: g.name, params: {} }, g.title.toLowerCase()); }
  function addHole() {
    const [x, y] = centre();
    // Assign first, then push through the document: pushing onto the array just assigned would
    // miss the reactive copy Svelte stores.
    edit((d) => { if (!d.holes) d.holes = []; d.holes.push({ x, y, d: 3.2, plated: false, clearance: 0.5 }); });
    selected = { kind: 'hole', i: shapeDoc.holes.length - 1 };
  }
  function addKeepout() {
    const [x, y] = centre();
    edit((d) => { if (!d.keepouts) d.keepouts = []; d.keepouts.push({ name: `keep-out ${d.keepouts.length + 1}`, side: 'both', geom: { type: 'rect', x, y, w: 8, h: 6, r: 0, angle: 0 } }); });
    selected = { kind: 'keepout', i: shapeDoc.keepouts.length - 1 };
  }
  async function importDrawing(e) {
    const file = e.target.files?.[0];
    e.target.value = '';
    if (!file) return;
    const kind = file.name.toLowerCase().endsWith('.dxf') ? 'dxf' : 'svg';
    try {
      const features = JSON.parse(shape_import(await file.text(), kind));
      edit((d) => d.features.push(...features));
      selected = { kind: 'feature', i: shapeDoc.features.length - features.length };
      fit();
    } catch (err) {
      alert(`${file.name}: ${err}`);
    }
  }
  function remove() {
    if (!selected) return;
    const key = { feature: 'features', hole: 'holes', keepout: 'keepouts' }[selected.kind];
    const i = selected.i;
    edit((d) => d[key].splice(i, 1));
    selected = null;
  }
  function move(dir) {
    if (selected?.kind !== 'feature') return;
    const i = selected.i, j = i + dir;
    if (j < 0 || j >= shapeDoc.features.length) return;
    edit((d) => ([d.features[i], d.features[j]] = [d.features[j], d.features[i]]));
    selected = { kind: 'feature', i: j };
  }

  // ---- the selected thing's geometry, for the property panel and handles
  const item = $derived(
    !selected ? null
      : selected.kind === 'feature' ? shapeDoc.features[selected.i]
      : selected.kind === 'hole' ? shapeDoc.holes?.[selected.i]
      : shapeDoc.keepouts?.[selected.i],
  );
  const geomOf = (it) => (selected?.kind === 'hole' ? null : it?.geom);
  const genOf = (g) => (g?.type === 'generator' ? generators.find((x) => x.name === g.name) : null);

  function setGeom(k, v) {
    edit((d) => {
      const it = selected.kind === 'feature' ? d.features[selected.i] : d.keepouts[selected.i];
      it.geom[k] = v;
    });
  }
  function setParam(k, v) {
    edit((d) => (d.features[selected.i].geom.params[k] = v), !paramDragging);
    paramDragging = true;
  }
  let paramDragging = false;

  // ---- view
  function toBoard(px, py) { return [px / view.scale + view.x, py / view.scale + view.y]; }
  function toScreen(x, y) { return [(x - view.x) * view.scale, (y - view.y) * view.scale]; }
  function fit() {
    if (!canvas || !result) return;
    const r = canvas.getBoundingClientRect();
    let b = result.outline.length ? result.bbox : [-20, -20, 20, 20];
    for (const h of shapeDoc.holes || []) b = [Math.min(b[0], h.x), Math.min(b[1], h.y), Math.max(b[2], h.x), Math.max(b[3], h.y)];
    const pad = 40;
    view.scale = Math.max(0.5, Math.min((r.width - 2 * pad) / Math.max(1, b[2] - b[0]), (r.height - 2 * pad) / Math.max(1, b[3] - b[1])));
    view.x = (b[0] + b[2]) / 2 - r.width / 2 / view.scale;
    view.y = (b[1] + b[3]) / 2 - r.height / 2 / view.scale;
    draw();
  }

  function theme() {
    const s = getComputedStyle(document.documentElement);
    const v = (k) => s.getPropertyValue(k).trim();
    return { canvas: v('--canvas'), grid: v('--grid'), gridMajor: v('--grid-major'), board: v('--board'), edge: v('--edge'), select: v('--select'), muted: v('--muted'), bad: v('--bad'), ink: v('--ink'), lock: v('--lock') };
  }

  // Handles of the selected item: [x, y, onDrag(x, y)].
  function handles() {
    if (!selected || !item) return [];
    if (selected.kind === 'hole') {
      return [[item.x, item.y, (x, y) => { item.x = x; item.y = y; }]];
    }
    const g = item.geom;
    switch (g.type) {
      case 'rect': {
        const a = (g.angle || 0) * Math.PI / 180, c = Math.cos(a), s = Math.sin(a);
        const corner = [g.x + (g.w / 2) * c + (g.h / 2) * s, g.y - (g.w / 2) * s + (g.h / 2) * c];
        return [
          [g.x, g.y, (x, y) => { g.x = x; g.y = y; }],
          [corner[0], corner[1], (x, y) => {
            const dx = x - g.x, dy = y - g.y;
            g.w = Math.max(0.5, 2 * Math.abs(dx * c - dy * s));
            g.h = Math.max(0.5, 2 * Math.abs(dx * s + dy * c));
          }],
        ];
      }
      case 'circle':
        return [[g.x, g.y, (x, y) => { g.x = x; g.y = y; }], [g.x + g.d / 2, g.y, (x, y) => { g.d = Math.max(0.2, 2 * Math.hypot(x - g.x, y - g.y)); }]];
      case 'slot':
        return [[g.x1, g.y1, (x, y) => { g.x1 = x; g.y1 = y; }], [g.x2, g.y2, (x, y) => { g.x2 = x; g.y2 = y; }]];
      case 'polygon':
        return g.points.map((p, k) => [p[0], p[1], (x, y) => { g.points[k] = [x, y]; }]);
      case 'generator': {
        const gen = genOf(g);
        if (!gen) return [];
        const has = (n) => gen.params.some((p) => p.name === n);
        if (!has('centre_x') || !has('centre_y')) return [];
        const val = (n) => g.params[n] ?? gen.params.find((p) => p.name === n).default;
        return [[val('centre_x'), val('centre_y'), (x, y) => { g.params.centre_x = x; g.params.centre_y = y; }]];
      }
    }
    return [];
  }

  function draw() {
    if (!canvas || !result) return;
    const k = window.devicePixelRatio || 1;
    const r = canvas.getBoundingClientRect();
    if (canvas.width !== Math.round(r.width * k)) { canvas.width = Math.round(r.width * k); canvas.height = Math.round(r.height * k); }
    const ctx = canvas.getContext('2d');
    const t = theme();
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = t.canvas;
    ctx.fillRect(0, 0, canvas.width, canvas.height);
    ctx.setTransform(k * view.scale, 0, 0, k * view.scale, -k * view.scale * view.x, -k * view.scale * view.y);
    const px = 1 / view.scale;
    const [x0, y0] = toBoard(0, 0), [x1, y1] = toBoard(r.width, r.height);
    for (const [step, color] of [[1, t.grid], [10, t.gridMajor]]) {
      if (step * view.scale < 6) continue;
      ctx.beginPath();
      for (let x = Math.floor(x0 / step) * step; x <= x1; x += step) { ctx.moveTo(x, y0); ctx.lineTo(x, y1); }
      for (let y = Math.floor(y0 / step) * step; y <= y1; y += step) { ctx.moveTo(x0, y); ctx.lineTo(x1, y); }
      ctx.strokeStyle = color; ctx.lineWidth = px; ctx.stroke();
    }
    // The board: outline less cut-outs, filled with the even-odd rule.
    ctx.beginPath();
    for (const p of [...result.outline, ...result.cutouts]) poly(ctx, p);
    ctx.fillStyle = t.board; ctx.fill('evenodd');
    ctx.strokeStyle = t.edge; ctx.lineWidth = 2 * px; ctx.stroke();
    // Each feature's own outline, faint; the selected one strong.
    result.features.forEach((paths, i) => {
      const f = shapeDoc.features[i];
      if (!paths.length || !f) return;
      const sel = selected?.kind === 'feature' && selected.i === i;
      ctx.beginPath();
      for (const p of paths) poly(ctx, p);
      ctx.setLineDash(f.op === 'subtract' ? [4 * px, 3 * px] : []);
      ctx.strokeStyle = sel ? t.select : f.op === 'subtract' ? t.bad : t.muted;
      ctx.globalAlpha = sel ? 1 : 0.45;
      ctx.lineWidth = (sel ? 1.6 : 1) * px;
      ctx.stroke();
      ctx.globalAlpha = 1;
      ctx.setLineDash([]);
    });
    // Keep-outs, hatched; holes.
    (result.keepouts || []).forEach((kp, i) => {
      const sel = selected?.kind === 'keepout' && selected.i === i;
      ctx.beginPath();
      for (const p of kp.polygon) poly(ctx, p);
      ctx.fillStyle = 'rgba(200, 60, 60, 0.12)'; ctx.fill();
      ctx.strokeStyle = sel ? t.select : t.bad; ctx.lineWidth = (sel ? 1.6 : 1) * px; ctx.setLineDash([3 * px, 2 * px]); ctx.stroke(); ctx.setLineDash([]);
    });
    (shapeDoc.holes || []).forEach((h, i) => {
      const sel = selected?.kind === 'hole' && selected.i === i;
      ctx.beginPath(); ctx.arc(h.x, h.y, h.d / 2 + (h.clearance || 0), 0, 2 * Math.PI);
      ctx.strokeStyle = t.muted; ctx.setLineDash([2 * px, 2 * px]); ctx.lineWidth = px; ctx.stroke(); ctx.setLineDash([]);
      ctx.beginPath(); ctx.arc(h.x, h.y, h.d / 2, 0, 2 * Math.PI);
      ctx.fillStyle = t.canvas; ctx.fill();
      ctx.strokeStyle = sel ? t.select : t.ink; ctx.lineWidth = (sel ? 1.6 : 1) * px; ctx.stroke();
    });
    // Symmetry axis.
    const sym = shapeDoc.symmetry;
    if (sym && sym.kind && sym.kind !== 'none') {
      const [cx, cy] = sym.centre || [0, 0];
      ctx.strokeStyle = t.lock; ctx.lineWidth = px; ctx.setLineDash([6 * px, 4 * px]);
      ctx.beginPath();
      if (sym.kind === 'mirror_x') { ctx.moveTo(cx, y0); ctx.lineTo(cx, y1); }
      else if (sym.kind === 'mirror_y') { ctx.moveTo(x0, cy); ctx.lineTo(x1, cy); }
      else { ctx.arc(cx, cy, 6 * px, 0, 2 * Math.PI); }
      ctx.stroke(); ctx.setLineDash([]);
    }
    // Handles, in screen space.
    ctx.setTransform(k, 0, 0, k, 0, 0);
    for (const [hx, hy] of handles()) {
      const [sx, sy] = toScreen(hx, hy);
      ctx.fillStyle = t.canvas; ctx.strokeStyle = t.select; ctx.lineWidth = 2;
      ctx.beginPath(); ctx.rect(sx - 5, sy - 5, 10, 10); ctx.fill(); ctx.stroke();
    }
    // Overall size along the bottom and right of the outline.
    if (result.outline.length) {
      const b = result.bbox;
      const [ax, ay] = toScreen(b[0], b[3]), [bx, by] = toScreen(b[2], b[1]);
      ctx.fillStyle = t.muted; ctx.font = '11px ui-monospace, monospace'; ctx.textAlign = 'center';
      ctx.fillText(`${(b[2] - b[0]).toFixed(2)} mm`, (ax + bx) / 2, ay + 16);
      ctx.save(); ctx.translate(bx + 16, (ay + by) / 2); ctx.rotate(-Math.PI / 2);
      ctx.fillText(`${(b[3] - b[1]).toFixed(2)} mm`, 0, 0); ctx.restore();
    }
  }

  function poly(ctx, pts) {
    if (!pts.length) return;
    ctx.moveTo(pts[0][0], pts[0][1]);
    for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
    ctx.closePath();
  }

  // ---- picking and dragging
  const at = (e) => { const r = canvas.getBoundingClientRect(); return [e.clientX - r.left, e.clientY - r.top]; };
  const snap = (v, e) => (e.altKey ? v : Math.round(v / 0.1) * 0.1);

  function inside(pts, x, y) {
    let c = false;
    for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
      const [xi, yi] = pts[i], [xj, yj] = pts[j];
      if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) c = !c;
    }
    return c;
  }

  function pick(x, y) {
    for (let i = (shapeDoc.holes || []).length - 1; i >= 0; i--) {
      const h = shapeDoc.holes[i];
      if (Math.hypot(x - h.x, y - h.y) <= h.d / 2 + 0.3) return { kind: 'hole', i };
    }
    for (let i = (result.keepouts || []).length - 1; i >= 0; i--) if (result.keepouts[i].polygon.some((p) => inside(p, x, y))) return { kind: 'keepout', i };
    for (let i = result.features.length - 1; i >= 0; i--) if (result.features[i].some((p) => inside(p, x, y))) return { kind: 'feature', i };
    return null;
  }

  function down(e) {
    canvas.focus();
    const [px, py] = at(e);
    const [x, y] = toBoard(px, py);
    // A handle of the selected item first.
    for (const [hx, hy, set] of handles()) {
      const [sx, sy] = toScreen(hx, hy);
      if (Math.abs(sx - px) <= 7 && Math.abs(sy - py) <= 7) {
        undo.push(JSON.stringify(shapeDoc));
        drag = { kind: 'handle', set };
        canvas.setPointerCapture(e.pointerId);
        return;
      }
    }
    // Shift-click on a polygon's edge adds a point there.
    const g = geomOf(item);
    if (e.shiftKey && g?.type === 'polygon') {
      let best = -1, bestD = 8 / view.scale;
      g.points.forEach((p, k) => {
        const q = g.points[(k + 1) % g.points.length];
        const dx = q[0] - p[0], dy = q[1] - p[1];
        const t = Math.max(0, Math.min(1, ((x - p[0]) * dx + (y - p[1]) * dy) / (dx * dx + dy * dy || 1)));
        const d = Math.hypot(x - p[0] - t * dx, y - p[1] - t * dy);
        if (d < bestD) { bestD = d; best = k; }
      });
      if (best >= 0) { edit((d) => d.features[selected.i].geom.points.splice(best + 1, 0, [snap(x, e), snap(y, e)])); return; }
    }
    const hit = pick(x, y);
    selected = hit;
    if (hit) {
      undo.push(JSON.stringify(shapeDoc));
      drag = { kind: 'move', start: [x, y], before: JSON.stringify(hit.kind === 'hole' ? shapeDoc.holes[hit.i] : (hit.kind === 'feature' ? shapeDoc.features : shapeDoc.keepouts)[hit.i].geom) };
    } else {
      drag = { kind: 'pan', last: [px, py] };
    }
    canvas.setPointerCapture(e.pointerId);
    draw();
  }

  function moveTo(e) {
    if (!drag) return;
    const [px, py] = at(e);
    if (drag.kind === 'pan') {
      view.x -= (px - drag.last[0]) / view.scale;
      view.y -= (py - drag.last[1]) / view.scale;
      drag.last = [px, py];
      draw();
      return;
    }
    const [x, y] = toBoard(px, py);
    if (drag.kind === 'handle') {
      drag.set(snap(x, e), snap(y, e));
    } else {
      const dx = snap(x - drag.start[0], e), dy = snap(y - drag.start[1], e);
      const was = JSON.parse(drag.before);
      if (selected.kind === 'hole') { item.x = was.x + dx; item.y = was.y + dy; }
      else translate(item.geom, was, dx, dy);
    }
    edit(() => {}, false);
  }

  function translate(g, was, dx, dy) {
    switch (g.type) {
      case 'rect': case 'circle': g.x = was.x + dx; g.y = was.y + dy; break;
      case 'slot': g.x1 = was.x1 + dx; g.y1 = was.y1 + dy; g.x2 = was.x2 + dx; g.y2 = was.y2 + dy; break;
      case 'polygon': g.points = was.points.map((p) => [p[0] + dx, p[1] + dy]); break;
      case 'generator': {
        const gen = genOf(g);
        const def = (n) => was.params[n] ?? gen?.params.find((p) => p.name === n)?.default ?? 0;
        if (gen?.params.some((p) => p.name === 'centre_x')) { g.params.centre_x = def('centre_x') + dx; g.params.centre_y = def('centre_y') + dy; }
      }
    }
  }

  function up() {
    if (drag && drag.kind !== 'pan' && undo.length && undo[undo.length - 1] === JSON.stringify(shapeDoc)) undo.pop();
    drag = null;
  }

  function wheel(e) {
    e.preventDefault();
    const [px, py] = at(e);
    const [bx, by] = toBoard(px, py);
    view.scale = Math.min(400, Math.max(0.5, view.scale * Math.exp(-e.deltaY * 0.0015)));
    view.x = bx - px / view.scale;
    view.y = by - py / view.scale;
    draw();
  }

  function key(e) {
    if (e.target.closest('input, textarea, select')) return;
    if ((e.ctrlKey || e.metaKey) && e.key === 'z') { e.preventDefault(); undoLast(); }
    else if ((e.key === 'Delete' || e.key === 'Backspace') && selected) { e.preventDefault(); remove(); }
    else if (e.key === 'f' || e.key === 'F') fit();
    else if (e.key === 'Escape') { selected = null; draw(); }
  }

  onMount(() => {
    evaluate();
    fit();
    const ro = new ResizeObserver(() => draw());
    ro.observe(canvas);
    window.addEventListener('keydown', key);
    return () => { ro.disconnect(); window.removeEventListener('keydown', key); };
  });

  $effect(() => { selected; draw(); });

  function download() {
    const blob = new Blob([JSON.stringify(shapeDoc, null, 1) + '\n'], { type: 'application/json' });
    const a = document.createElement('a');
    a.href = URL.createObjectURL(blob);
    a.download = 'shape.json';
    a.click();
    setTimeout(() => URL.revokeObjectURL(a.href), 1000);
  }

  const num = (v) => Math.round((+v || 0) * 1000) / 1000;
  const FIELDS = {
    rect: [['x', 'x'], ['y', 'y'], ['w', 'width'], ['h', 'height'], ['r', 'corner radius'], ['angle', 'angle °']],
    circle: [['x', 'x'], ['y', 'y'], ['d', 'diameter']],
    slot: [['x1', 'x₁'], ['y1', 'y₁'], ['x2', 'x₂'], ['y2', 'y₂'], ['w', 'width']],
  };
  const symmetry = $derived(shapeDoc.symmetry || { kind: 'none', centre: [0, 0] });
</script>

<div class="shape">
  <div class="list">
    <div class="adds">
      <button onclick={addRect} title="Add a rectangle">▭ Rect</button>
      <button onclick={addCircle} title="Add a circle">◯ Circle</button>
      <button onclick={addSlot} title="Add a slot (a stadium)">⊂⊃ Slot</button>
      <button onclick={addPolygon} title="Add a polygon: drag its points, shift-click an edge to add one">⬠ Polygon</button>
      {#each generators as g}<button onclick={() => addGenerator(g)} title={g.help}>✦ {g.title}</button>{/each}
      <label class="file" title="Import closed shapes from an SVG or DXF drawing">⇪ Import<input type="file" accept=".svg,.dxf" onchange={importDrawing} /></label>
    </div>
    <h3>Outline <span>applied top to bottom</span></h3>
    <ul>
      {#each shapeDoc.features as f, i}
        <li class:sel={selected?.kind === 'feature' && selected.i === i} class:hidden={f.hidden}>
          <button class="op {f.op}" title={f.op === 'subtract' ? 'Cuts away: click to add instead' : 'Adds: click to cut away instead'}
            onclick={() => edit((d) => (d.features[i].op = f.op === 'subtract' ? 'add' : 'subtract'))}>{f.op === 'subtract' ? '−' : '+'}</button>
          <button class="name" onclick={() => (selected = { kind: 'feature', i })}>{f.name} <small>{f.geom.type === 'generator' ? f.geom.name : f.geom.type}</small></button>
          <button class="eye" title={f.hidden ? 'Show' : 'Hide'} onclick={() => edit((d) => (d.features[i].hidden = !f.hidden))}>{f.hidden ? '◌' : '●'}</button>
        </li>
      {:else}
        <li class="empty">No outline yet: the floorplanner sizes the board. Add a shape to set one.</li>
      {/each}
    </ul>
    <h3>Holes and keep-outs</h3>
    <div class="adds">
      <button onclick={addHole}>◎ Hole</button>
      <button onclick={addKeepout}>▨ Keep-out</button>
    </div>
    <ul>
      {#each shapeDoc.holes || [] as h, i}
        <li class:sel={selected?.kind === 'hole' && selected.i === i}><button class="name" onclick={() => (selected = { kind: 'hole', i })}>hole {i + 1} <small>⌀{h.d}</small></button></li>
      {/each}
      {#each shapeDoc.keepouts || [] as k, i}
        <li class:sel={selected?.kind === 'keepout' && selected.i === i}><button class="name" onclick={() => (selected = { kind: 'keepout', i })}>{k.name} <small>{k.side}</small></button></li>
      {/each}
    </ul>
  </div>

  <div class="stage">
    <canvas bind:this={canvas} tabindex="0" onpointerdown={down} onpointermove={moveTo} onpointerup={up} onpointercancel={up} onwheel={wheel} aria-label="Board shape"></canvas>
    <div class="bar">
      {#if result}
        <span><b>{num(result.bbox[2] - result.bbox[0])} × {num(result.bbox[3] - result.bbox[1])}</b> mm</span>
        <span><b>{result.area_mm2.toFixed(1)}</b> mm²</span>
        {#if result.cutouts.length}<span>{result.cutouts.length} cut-out{result.cutouts.length > 1 ? 's' : ''}</span>{/if}
        {#each result.warnings as w}<span class="warn">{w}</span>{/each}
      {/if}
      <span class="grow"></span>
      <button onclick={download}>Download shape.json</button>
      <button class="primary" disabled={!result || result.outline.length !== 1} onclick={() => onapply(result)}
        title="The floorplanner keeps every part inside this outline">{applied ? '✓ Board uses this outline' : 'Use as board outline'}</button>
    </div>
  </div>

  <div class="props">
    {#if selected && item}
      {#if selected.kind === 'hole'}
        <h2>Hole {selected.i + 1}</h2>
        {#each [['x', 'x'], ['y', 'y'], ['d', 'drill ⌀'], ['clearance', 'clearance']] as [k, label]}
          <label class="row">{label}<input type="number" step="0.1" value={num(item[k])} onchange={(e) => edit((d) => (d.holes[selected.i][k] = +e.target.value))} /></label>
        {/each}
        <label class="row">plated<input type="checkbox" checked={item.plated} onchange={(e) => edit((d) => (d.holes[selected.i].plated = e.target.checked))} /></label>
      {:else}
        <input class="title" value={item.name} onchange={(e) => edit((d) => ((selected.kind === 'feature' ? d.features : d.keepouts)[selected.i].name = e.target.value))} />
        {#if selected.kind === 'keepout'}
          <label class="row">side
            <select value={item.side} onchange={(e) => edit((d) => (d.keepouts[selected.i].side = e.target.value))}>
              <option value="both">both</option><option value="front">front</option><option value="back">back</option>
            </select>
          </label>
        {/if}
        {#if FIELDS[item.geom.type]}
          {#each FIELDS[item.geom.type] as [k, label]}
            <label class="row">{label}<input type="number" step="0.1" value={num(item.geom[k])} onchange={(e) => setGeom(k, +e.target.value)} /></label>
          {/each}
        {:else if item.geom.type === 'polygon'}
          <p class="hint">{item.geom.points.length} points. Drag a point to move it; shift-click an edge to add one.</p>
        {:else if genOf(item.geom)}
          {@const gen = genOf(item.geom)}
          <p class="hint">{gen.help}</p>
          {#each gen.params as p}
            {@const v = item.geom.params[p.name] ?? p.default}
            <div class="param">
              <label for="p-{p.name}">{p.name} <small>{p.help}</small></label>
              <input type="number" step={p.step} value={num(v)} onchange={(e) => { paramDragging = false; setParam(p.name, +e.target.value); paramDragging = false; }} />
              <input id="p-{p.name}" type="range" min={p.min} max={p.max} step={p.step} value={v}
                oninput={(e) => setParam(p.name, +e.target.value)} onchange={() => (paramDragging = false)} />
            </div>
          {/each}
          <button onclick={() => edit((d) => (d.features[selected.i].geom.params = {}))}>Reset to defaults</button>
        {/if}
        {#if selected.kind === 'feature'}
          <div class="actions">
            <button onclick={() => move(-1)} title="Earlier">↑</button>
            <button onclick={() => move(1)} title="Later">↓</button>
          </div>
        {/if}
      {/if}
      <div class="actions"><button class="danger" onclick={remove}>Delete <kbd>Del</kbd></button></div>
    {:else}
      <h2>Whole board</h2>
      <label class="row">corner radius<input type="number" min="0" step="0.1" value={num(shapeDoc.corner_radius)} onchange={(e) => edit((d) => (d.corner_radius = Math.max(0, +e.target.value)))} /></label>
      <label class="row">symmetry
        <select value={symmetry.kind} onchange={(e) => edit((d) => (d.symmetry = { centre: symmetry.centre || [0, 0], kind: e.target.value }))}>
          <option value="none">none</option><option value="mirror_x">mirror left–right</option><option value="mirror_y">mirror top–bottom</option><option value="half_turn">half turn</option>
        </select>
      </label>
      {#if symmetry.kind !== 'none'}
        <label class="row">centre x<input type="number" step="0.5" value={num(symmetry.centre[0])} onchange={(e) => edit((d) => (d.symmetry.centre = [+e.target.value, symmetry.centre[1]]))} /></label>
        <label class="row">centre y<input type="number" step="0.5" value={num(symmetry.centre[1])} onchange={(e) => edit((d) => (d.symmetry.centre = [symmetry.centre[0], +e.target.value]))} /></label>
      {/if}
      <p class="hint">Click a shape to edit it; drag it to move it, or its handles to resize. Alt turns off the 0.1 mm snap. Wheel zooms, drag the background to pan, <kbd>F</kbd> fits, <kbd>Ctrl Z</kbd> undoes.</p>
    {/if}
  </div>
</div>

<style>
  .shape { display: grid; grid-template-columns: 220px minmax(0, 1fr) 270px; height: 100%; min-height: 0; }
  .list, .props { overflow-y: auto; padding: 10px 12px; background: var(--panel); }
  .list { border-right: 1px solid var(--line); }
  .props { border-left: 1px solid var(--line); }
  .adds { display: flex; flex-wrap: wrap; gap: 4px; }
  .adds button, .file { font-size: 12px; padding: 3px 7px; }
  .file { border: 1px solid var(--line); border-radius: 6px; cursor: pointer; background: var(--bg); position: relative; overflow: hidden; }
  .file input { position: absolute; inset: 0; opacity: 0; cursor: pointer; }
  h3 { font-size: 11px; text-transform: uppercase; letter-spacing: .06em; color: var(--muted); margin: 14px 0 6px; }
  h3 span { text-transform: none; letter-spacing: 0; font-weight: 400; }
  ul { list-style: none; margin: 0; padding: 0; }
  li { display: flex; align-items: center; gap: 4px; border-radius: 6px; padding: 1px 2px; }
  li.sel { background: var(--hover); box-shadow: inset 2px 0 0 var(--select); }
  li.hidden .name { opacity: 0.45; }
  li.empty { color: var(--muted); font-size: 12px; padding: 6px 2px; }
  li button { all: unset; cursor: pointer; }
  .name { flex: 1; padding: 4px 2px; font-size: 12.5px; overflow: hidden; text-overflow: ellipsis; white-space: nowrap; }
  .name small { color: var(--muted); }
  .op { width: 18px; text-align: center; font-weight: 700; border-radius: 4px; }
  .op.add { color: var(--good); }
  .op.subtract { color: var(--bad); }
  .eye { color: var(--muted); font-size: 11px; padding: 0 4px; }
  .stage { display: flex; flex-direction: column; min-width: 0; min-height: 0; }
  canvas { flex: 1; width: 100%; min-height: 0; display: block; outline: none; touch-action: none; }
  .bar { display: flex; gap: 12px; align-items: center; padding: 6px 12px; border-top: 1px solid var(--line); background: var(--panel); font-size: 12.5px; color: var(--muted); flex-wrap: wrap; }
  .bar b { color: var(--ink); font-variant-numeric: tabular-nums; }
  .warn { color: var(--bad); }
  .grow { flex: 1; }
  h2 { font-size: 15px; margin: 6px 0 10px; }
  .title { width: 100%; font: 600 15px var(--sans); border: 1px solid transparent; background: transparent; color: var(--ink); padding: 3px 4px; border-radius: 5px; margin-bottom: 8px; }
  .title:hover, .title:focus { border-color: var(--line); }
  .row { display: flex; justify-content: space-between; align-items: center; gap: 8px; font-size: 12.5px; color: var(--muted); margin: 5px 0; }
  .row input[type='number'], .row select, .param input[type='number'] { width: 92px; padding: 3px 5px; border: 1px solid var(--line); border-radius: 5px; background: var(--bg); color: var(--ink); font: 12px var(--mono); }
  .param { display: grid; grid-template-columns: 1fr 72px; gap: 2px 6px; margin: 8px 0; align-items: center; }
  .param label { font: 11.5px var(--mono); }
  .param label small { display: block; font: 11px var(--sans); color: var(--muted); }
  .param input[type='number'] { width: 72px; }
  .param input[type='range'] { grid-column: 1 / 3; width: 100%; accent-color: var(--accent); margin: 0; }
  .actions { display: flex; gap: 6px; margin-top: 12px; }
  .danger:hover { border-color: var(--bad); color: var(--bad); }
  .hint { color: var(--muted); font-size: 12px; line-height: 1.45; }
  kbd { font: 10px var(--mono); border: 1px solid var(--line); border-radius: 3px; padding: 0 3px; color: var(--muted); }
  @media (max-width: 820px) { .shape { grid-template-columns: 1fr; grid-template-rows: auto minmax(260px, 1fr) auto; } .list, .props { border: 0; max-height: 200px; } }
</style>
