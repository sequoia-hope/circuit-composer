import { expect } from '@playwright/test';

export const EXAMPLE_BOARDS = [
  'bolt-earring',
  'easy2350',
  'flame-earring',
  'rp2350-battery',
];

export const WORKSPACES = [
  { id: 'circuit', label: 'Circuit', key: '1' },
  { id: 'board', label: 'Board', key: '2' },
  { id: 'shape', label: 'Shape', key: '3' },
  { id: 'copper', label: 'Copper', key: '4' },
  { id: 'rules', label: 'Rules', key: '5' },
];

/**
 * Attach listeners for console errors, uncaught page errors, network failures, and HTTP >= 400.
 */
export function attachDiagnostics(page) {
  const consoleErrors = [];
  const pageErrors = [];
  const failedRequests = [];
  const badResponses = [];

  page.on('console', (msg) => {
    if (msg.type() === 'error') {
      consoleErrors.push(msg.text());
    }
  });
  page.on('pageerror', (err) => {
    pageErrors.push(err.message || String(err));
  });
  page.on('requestfailed', (req) => {
    failedRequests.push(`${req.method()} ${req.url()} (${req.failure()?.errorText || 'failed'})`);
  });
  page.on('response', (res) => {
    if (res.status() >= 400) {
      badResponses.push(`${res.status()} ${res.url()}`);
    }
  });

  return { consoleErrors, pageErrors, failedRequests, badResponses };
}

/**
 * Wait until WASM initialization and board loading complete.
 */
export async function waitForAppReady(page, { view = 'board' } = {}) {
  await expect(page.locator('header')).toBeVisible();
  await expect(page.locator('.empty')).toHaveCount(0, { timeout: 15000 });
  await expect(page.locator('.error')).toHaveCount(0);
  if (view === 'board') {
    await expect(page.getByLabel('Board floorplan')).toBeVisible();
    await expect(page.locator('.metrics')).toContainText('% packed', { timeout: 10000 });
  }
}

/**
 * Capture a compositor screenshot of `locator` and compute pixel statistics in the browser.
 */
export async function analyzeCanvasPixels(page, locator) {
  // Let pending requestAnimationFrame redraws settle before screenshotting.
  await page.evaluate(() => new Promise((r) => requestAnimationFrame(() => requestAnimationFrame(r))));
  const pngBuffer = await locator.screenshot();
  const b64 = pngBuffer.toString('base64');

  return page.evaluate(async (encoded) => {
    const bytes = Uint8Array.from(atob(encoded), (c) => c.charCodeAt(0));
    const bmp = await createImageBitmap(new Blob([bytes], { type: 'image/png' }));
    const off = new OffscreenCanvas(bmp.width, bmp.height);
    const ctx = off.getContext('2d');
    ctx.drawImage(bmp, 0, 0);
    const { data, width, height } = ctx.getImageData(0, 0, bmp.width, bmp.height);

    // Find dominant quantized background color and overall color diversity.
    const counts = new Map();
    const buckets = new Set();
    let lumaSum = 0;
    const total = width * height;

    for (let i = 0; i < data.length; i += 4) {
      const r = data[i];
      const g = data[i + 1];
      const b = data[i + 2];
      lumaSum += 0.2126 * r + 0.7152 * g + 0.0722 * b;
      const q = ((r >> 3) << 10) | ((g >> 3) << 5) | (b >> 3);
      counts.set(q, (counts.get(q) || 0) + 1);
      buckets.add(((r >> 4) << 8) | ((g >> 4) << 4) | (b >> 4));
    }

    let bestKey = 0;
    let bestCount = -1;
    for (const [k, c] of counts) {
      if (c > bestCount) {
        bestCount = c;
        bestKey = k;
      }
    }
    const bgR = ((bestKey >> 10) & 31) * 8 + 4;
    const bgG = ((bestKey >> 5) & 31) * 8 + 4;
    const bgB = (bestKey & 31) * 8 + 4;

    let nonBgPixels = 0;
    for (let i = 0; i < data.length; i += 4) {
      const dist =
        Math.abs(data[i] - bgR) + Math.abs(data[i + 1] - bgG) + Math.abs(data[i + 2] - bgB);
      if (dist > 24) nonBgPixels++;
    }

    // 16x16 downsample signature to compare distinct renders.
    const sig = [];
    for (let sy = 0; sy < 16; sy++) {
      for (let sx = 0; sx < 16; sx++) {
        const px = Math.min(width - 1, Math.floor(((sx + 0.5) * width) / 16));
        const py = Math.min(height - 1, Math.floor(((sy + 0.5) * height) / 16));
        const idx = (py * width + px) * 4;
        sig.push(`${data[idx] >> 3},${data[idx + 1] >> 3},${data[idx + 2] >> 3}`);
      }
    }

    return {
      width,
      height,
      totalPixels: total,
      nonBgPixels,
      nonBgRatio: total ? nonBgPixels / total : 0,
      uniqueBuckets: buckets.size,
      meanLuma: total ? lumaSum / total : 0,
      dominantRgb: [bgR, bgG, bgB],
      signature: sig.join(';'),
    };
  }, b64);
}
