// A placeholder for `board-draw`, the board renderer Composer will share with UltraRoute Studio
// (docs/app-spec.md). It takes what board-draw's scene will take (an outline, parts with their
// courtyards and pins in board millimetres, the ratsnest, a selection) and draws it with Canvas
// 2D. When board-draw lands, this file is replaced and nothing else changes.

const dpr = () => window.devicePixelRatio || 1;

function hue(name) {
  let h = 0;
  for (const c of name || '') h = (h * 31 + c.charCodeAt(0)) >>> 0;
  return h % 360;
}

export class Scene {
  constructor(canvas) {
    this.canvas = canvas;
    this.ctx = canvas.getContext('2d');
    this.view = { scale: 8, x: 0, y: 0 }; // screen = (board - (x, y)) * scale
    this.parts = [];
    this.rats = new Float64Array(0);
    this.outline = null; // polygon [[x, y], ...]
    this.box = null; // [x0, y0, x1, y1]: a rectangle outline, or the floorplanner's
    this.boxIsSet = false;
    this.selected = new Set();
    this.hover = -1;
    this.highlightNets = null;
    this.layers = { front: true, back: true, ratsnest: true, pins: true, labels: true, blocks: true };
  }

  setOutline(polygon, box, boxIsSet) {
    this.outline = polygon;
    this.box = box;
    this.boxIsSet = boxIsSet;
  }

  setParts(parts) {
    this.parts = parts;
  }

  setRatsnest(seg) {
    this.rats = seg;
  }

  resize() {
    const r = this.canvas.getBoundingClientRect();
    this.canvas.width = Math.max(1, Math.round(r.width * dpr()));
    this.canvas.height = Math.max(1, Math.round(r.height * dpr()));
  }

  bounds() {
    let b = [Infinity, Infinity, -Infinity, -Infinity];
    const add = (x, y) => (b = [Math.min(b[0], x), Math.min(b[1], y), Math.max(b[2], x), Math.max(b[3], y)]);
    for (const p of this.outline || []) add(p[0], p[1]);
    if (this.box) { add(this.box[0], this.box[1]); add(this.box[2], this.box[3]); }
    for (const p of this.parts) for (const q of p.outline) add(q[0], q[1]);
    return b[0] === Infinity ? [0, 0, 10, 10] : b;
  }

  fit() {
    const b = this.bounds();
    const w = this.canvas.width / dpr(), h = this.canvas.height / dpr();
    const pad = 32;
    const s = Math.min((w - 2 * pad) / Math.max(1, b[2] - b[0]), (h - 2 * pad) / Math.max(1, b[3] - b[1]));
    this.view.scale = Math.max(0.5, s);
    this.view.x = (b[0] + b[2]) / 2 - w / 2 / this.view.scale;
    this.view.y = (b[1] + b[3]) / 2 - h / 2 / this.view.scale;
  }

  toBoard(px, py) {
    return [px / this.view.scale + this.view.x, py / this.view.scale + this.view.y];
  }

  zoomAt(px, py, factor) {
    const [bx, by] = this.toBoard(px, py);
    this.view.scale = Math.min(400, Math.max(0.5, this.view.scale * factor));
    this.view.x = bx - px / this.view.scale;
    this.view.y = by - py / this.view.scale;
  }

  pan(dpx, dpy) {
    this.view.x -= dpx / this.view.scale;
    this.view.y -= dpy / this.view.scale;
  }

  /** The part under a screen point: a front part before a back one, then the smallest. */
  pick(px, py) {
    const [x, y] = this.toBoard(px, py);
    let best = -1, bestKey = Infinity;
    this.parts.forEach((p, i) => {
      if ((p.back && !this.layers.back) || (!p.back && !this.layers.front)) return;
      if (!inside(p.outline, x, y)) return;
      const key = (p.back ? 1e6 : 0) + area(p.outline);
      if (key < bestKey) { bestKey = key; best = i; }
    });
    return best;
  }

  render(theme) {
    const { ctx, view } = this;
    const k = dpr();
    const W = this.canvas.width, H = this.canvas.height;
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    ctx.fillStyle = theme.canvas;
    ctx.fillRect(0, 0, W, H);
    ctx.setTransform(k * view.scale, 0, 0, k * view.scale, -k * view.scale * view.x, -k * view.scale * view.y);
    const px = 1 / view.scale; // one screen pixel in mm

    // Grid: 1 mm when it is wide enough to read, 10 mm always.
    const [x0, y0] = this.toBoard(0, 0), [x1, y1] = this.toBoard(W / k, H / k);
    for (const [step, color] of [[1, theme.grid], [10, theme.gridMajor]]) {
      if (step * view.scale < 6) continue;
      ctx.beginPath();
      for (let x = Math.floor(x0 / step) * step; x <= x1; x += step) { ctx.moveTo(x, y0); ctx.lineTo(x, y1); }
      for (let y = Math.floor(y0 / step) * step; y <= y1; y += step) { ctx.moveTo(x0, y); ctx.lineTo(x1, y); }
      ctx.strokeStyle = color;
      ctx.lineWidth = px;
      ctx.stroke();
    }

    // The board.
    ctx.beginPath();
    if (this.outline) poly(ctx, this.outline);
    else if (this.box) ctx.rect(this.box[0], this.box[1], this.box[2] - this.box[0], this.box[3] - this.box[1]);
    ctx.fillStyle = theme.board;
    ctx.fill();
    ctx.strokeStyle = theme.edge;
    ctx.lineWidth = 2 * px;
    ctx.setLineDash(this.outline || this.boxIsSet ? [] : [6 * px, 4 * px]);
    ctx.stroke();
    ctx.setLineDash([]);

    // Back parts first, faint and dashed; then front parts.
    for (const side of [true, false]) {
      if ((side && !this.layers.back) || (!side && !this.layers.front)) continue;
      this.parts.forEach((p, i) => {
        if (p.back !== side) return;
        const sel = this.selected.has(i);
        const h = hue(this.layers.blocks ? p.block || p.group || '' : '');
        ctx.beginPath();
        poly(ctx, p.outline);
        ctx.fillStyle = side ? `hsla(${h}, 40%, 55%, 0.10)` : `hsla(${h}, 55%, 55%, ${this.layers.blocks && p.block ? 0.32 : 0.18})`;
        ctx.fill();
        ctx.lineWidth = (sel ? 2.5 : i === this.hover ? 1.8 : 1) * px;
        ctx.strokeStyle = sel ? theme.select : side ? `hsla(${h}, 30%, 60%, 0.6)` : `hsla(${h}, 55%, 62%, 0.95)`;
        ctx.setLineDash(side ? [3 * px, 2 * px] : []);
        ctx.stroke();
        ctx.setLineDash([]);
        // A padlock where it can be read, or on what is selected: a board of locked LEDs would
        // otherwise be all padlocks.
        if (p.locked && (sel || view.scale > 14)) {
          const c = centre(p.outline);
          const r = Math.min(0.6, 4 * px);
          ctx.fillStyle = theme.lock;
          ctx.fillRect(c[0] - r, c[1] - r * 0.4, 2 * r, 1.4 * r);
          ctx.beginPath();
          ctx.arc(c[0], c[1] - r * 0.4, r * 0.7, Math.PI, 0);
          ctx.strokeStyle = theme.lock;
          ctx.lineWidth = r * 0.35;
          ctx.stroke();
        }
        if (this.layers.pins && view.scale > 5) {
          ctx.fillStyle = side ? theme.pinBack : theme.pin;
          const r = Math.min(0.22, Math.max(0.08, 1.5 * px));
          for (const q of Object.values(p.pins)) { ctx.beginPath(); ctx.arc(q[0], q[1], r, 0, 2 * Math.PI); ctx.fill(); }
        }
      });
    }

    // Ratsnest.
    if (this.layers.ratsnest && this.rats.length) {
      ctx.beginPath();
      for (let j = 0; j < this.rats.length; j += 4) { ctx.moveTo(this.rats[j], this.rats[j + 1]); ctx.lineTo(this.rats[j + 2], this.rats[j + 3]); }
      ctx.strokeStyle = theme.rats;
      ctx.lineWidth = px;
      ctx.stroke();
    }

    // Labels, in screen space so they stay legible.
    if (this.layers.labels && view.scale > 3) {
      ctx.setTransform(k, 0, 0, k, 0, 0);
      ctx.font = `${view.scale > 14 ? 12 : 10}px ui-monospace, monospace`;
      ctx.textAlign = 'center';
      ctx.textBaseline = 'middle';
      this.parts.forEach((p, i) => {
        if ((p.back && !this.layers.back) || (!p.back && !this.layers.front)) return;
        const w = (Math.hypot(p.outline[1][0] - p.outline[0][0], p.outline[1][1] - p.outline[0][1])) * view.scale;
        if (w < 22 && !this.selected.has(i)) return;
        const c = centre(p.outline);
        ctx.fillStyle = this.selected.has(i) ? theme.select : p.back ? theme.labelBack : theme.label;
        ctx.fillText(p.reference, (c[0] - view.x) * view.scale, (c[1] - view.y) * view.scale);
      });
    }
  }
}

function poly(ctx, pts) {
  ctx.moveTo(pts[0][0], pts[0][1]);
  for (let i = 1; i < pts.length; i++) ctx.lineTo(pts[i][0], pts[i][1]);
  ctx.closePath();
}

function centre(pts) {
  let x = 0, y = 0;
  for (const p of pts) { x += p[0]; y += p[1]; }
  return [x / pts.length, y / pts.length];
}

function area(pts) {
  let a = 0;
  for (let i = 0; i < pts.length; i++) {
    const p = pts[i], q = pts[(i + 1) % pts.length];
    a += p[0] * q[1] - q[0] * p[1];
  }
  return Math.abs(a / 2);
}

function inside(pts, x, y) {
  let c = false;
  for (let i = 0, j = pts.length - 1; i < pts.length; j = i++) {
    const [xi, yi] = pts[i], [xj, yj] = pts[j];
    if ((yi > y) !== (yj > y) && x < ((xj - xi) * (y - yi)) / (yj - yi) + xi) c = !c;
  }
  return c;
}
