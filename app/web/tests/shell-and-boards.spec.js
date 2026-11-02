import { test, expect } from '@playwright/test';
import {
  EXAMPLE_BOARDS,
  WORKSPACES,
  attachDiagnostics,
  waitForAppReady,
  analyzeCanvasPixels,
} from './helpers.js';

test.describe('App shell and example boards', () => {
  test('header, workspace tabs, and board selector are visible on load', async ({ page, isMobile }) => {
    const diag = attachDiagnostics(page);
    await page.goto('./');
    await waitForAppReady(page);

    await expect(page.locator('header')).toBeVisible();

    const boardSelect = page.getByLabel('Board', { exact: true });
    await expect(boardSelect).toBeVisible();
    await expect(boardSelect).toHaveValue('rp2350-battery');

    const navButtons = page.locator('header nav button');
    await expect(navButtons).toHaveCount(WORKSPACES.length);
    for (const ws of WORKSPACES) {
      const btn = page.locator('header nav').getByRole('button', { name: ws.label, exact: true });
      await expect(btn).toBeVisible();
    }
    await expect(
      page.locator('header nav').getByRole('button', { name: 'Board', exact: true }),
    ).toHaveClass(/\bon\b/);

    if (!isMobile) {
      await expect(page.locator('header .brand')).toContainText('Circuit Composer');
      await expect(page.locator('header .mode')).toContainText('Demo · example boards');
      await expect(page.getByRole('button', { name: 'Download placement' })).toBeEnabled();
    }

    expect(diag.pageErrors).toEqual([]);
    expect(diag.failedRequests).toEqual([]);
  });

  test('loads every board with no errors and no failed requests but the server probe', async ({ page }) => {
    // detectMode() asks api/info whether Composer's server is behind the page; on a static server
    // that is a 404 by design, and Chrome logs it. Nothing else may fail.
    const probe = (s) => /\/api\/info\b/.test(s);
    const diag = attachDiagnostics(page);
    for (const board of EXAMPLE_BOARDS) {
      await page.goto(`?board=${board}`);
      await waitForAppReady(page);
    }

    expect(diag.pageErrors).toEqual([]);
    expect(diag.failedRequests).toEqual([]);
    expect(diag.badResponses.filter((r) => !probe(r))).toEqual([]);
    const probes = diag.badResponses.filter(probe).length;
    // Each 404 logs one console error; those are the probe's.
    expect(diag.consoleErrors.filter((e) => !/status of 404/.test(e))).toEqual([]);
    expect(diag.consoleErrors.length).toBe(probes);
  });

  test('each example board opens from the selector and draws non-blank pixels', async ({ page }) => {
    const diag = attachDiagnostics(page);
    await page.goto('./');
    await waitForAppReady(page);

    const boardSelect = page.getByLabel('Board', { exact: true });
    const options = await boardSelect.locator('option').allTextContents();
    expect(options).toEqual(EXAMPLE_BOARDS);

    const signatures = new Set();

    for (const board of EXAMPLE_BOARDS) {
      await boardSelect.selectOption(board);
      await expect.poll(() => new URL(page.url()).searchParams.get('board')).toBe(board);
      await waitForAppReady(page);

      const canvas = page.getByLabel('Board floorplan');
      await expect(canvas).toBeVisible();
      await expect(page.locator('.centre .wrap .err')).toHaveCount(0);

      const dims = await canvas.evaluate((el) => ({ w: el.width, h: el.height }));
      expect(dims.w).toBeGreaterThan(100);
      expect(dims.h).toBeGreaterThan(100);

      const stats = await analyzeCanvasPixels(page, canvas);
      expect(stats.nonBgRatio, `${board} should draw non-blank pixels`).toBeGreaterThan(0.01);
      expect(stats.uniqueBuckets, `${board} should draw multiple colors`).toBeGreaterThanOrEqual(5);
      signatures.add(stats.signature);
    }

    expect(signatures.size).toBe(EXAMPLE_BOARDS.length);
    expect(diag.pageErrors).toEqual([]);
    expect(diag.failedRequests).toEqual([]);
  });

  test('Drawn / Solved floorplan toggle works on boards with loose composition', async ({ page }) => {
    await page.goto('?board=rp2350-battery');
    await waitForAppReady(page);

    const whichGroup = page.getByRole('group', { name: 'Which board to floorplan' });
    await expect(whichGroup).toBeVisible();

    const drawnBtn = whichGroup.getByRole('button', { name: 'Drawn', exact: true });
    const solvedBtn = whichGroup.getByRole('button', { name: 'Solved', exact: true });
    await expect(drawnBtn).toHaveClass(/\bon\b/);
    await expect(solvedBtn).toBeEnabled();

    await solvedBtn.click();
    await expect(solvedBtn).toHaveClass(/\bon\b/);
    await expect.poll(() => new URL(page.url()).searchParams.get('fp')).toBe('solved');

    const stats = await analyzeCanvasPixels(page, page.getByLabel('Board floorplan'));
    expect(stats.nonBgRatio).toBeGreaterThan(0.01);

    await drawnBtn.click();
    await expect(drawnBtn).toHaveClass(/\bon\b/);
    await expect.poll(() => new URL(page.url()).searchParams.get('fp')).toBeNull();
  });
});
