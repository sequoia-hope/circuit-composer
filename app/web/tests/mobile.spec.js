import { test, expect } from '@playwright/test';
import { WORKSPACES, waitForAppReady } from './helpers.js';

test.describe('Mobile layout and ergonomics', () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(!isMobile, 'Mobile-only layout checks');
  });

  test('nothing overflows the viewport horizontally across workspaces and panels', async ({ page }) => {
    await page.goto('?board=rp2350-battery');
    await waitForAppReady(page);

    const checkNoHorizontalOverflow = async (context) => {
      const metrics = await page.evaluate(() => ({
        innerWidth: window.innerWidth,
        docScrollWidth: document.documentElement.scrollWidth,
        bodyScrollWidth: document.body.scrollWidth,
      }));
      expect(
        metrics.docScrollWidth,
        `document.scrollWidth <= innerWidth in ${context}`,
      ).toBeLessThanOrEqual(metrics.innerWidth);
      expect(
        metrics.bodyScrollWidth,
        `body.scrollWidth <= innerWidth in ${context}`,
      ).toBeLessThanOrEqual(metrics.innerWidth);
    };

    await checkNoHorizontalOverflow('Board default');

    // Open mobile View menu, Parts panel, and Dock tabs.
    await page.locator('details.view-menu summary').click();
    await checkNoHorizontalOverflow('Board with View menu open');
    await page.locator('details.view-menu summary').click();

    await page.getByRole('button', { name: 'Parts', exact: true }).click();
    await expect(page.locator('aside .insp')).toBeVisible();
    await checkNoHorizontalOverflow('Board with Parts panel open');

    await page.getByRole('tab', { name: /Checks/ }).click();
    await checkNoHorizontalOverflow('Board with Checks dock open');

    for (const ws of WORKSPACES) {
      await page.locator('header nav').getByRole('button', { name: ws.label, exact: true }).click();
      await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe(ws.id);
      await checkNoHorizontalOverflow(`Workspace ${ws.label}`);
    }
  });

  test('header wraps as designed and all workspace tabs are reachable', async ({ page }) => {
    await page.goto('./');
    await waitForAppReady(page);

    // Desktop-only chrome is hidden on narrow screens.
    await expect(page.locator('header .brand')).toBeHidden();
    await expect(page.locator('header .mode')).toBeHidden();
    await expect(page.getByRole('button', { name: 'Download placement' })).toBeHidden();

    const boardSelect = page.getByLabel('Board', { exact: true });
    const nav = page.locator('header nav');
    await expect(boardSelect).toBeVisible();
    await expect(nav).toBeVisible();

    const selectBox = await boardSelect.boundingBox();
    const navBox = await nav.boundingBox();
    expect(selectBox).not.toBeNull();
    expect(navBox).not.toBeNull();
    // Header wraps: workspace <nav> sits on its own row below the board selector.
    expect(navBox.y).toBeGreaterThanOrEqual(selectBox.y + selectBox.height - 2);

    const viewportWidth = page.viewportSize().width;
    for (const ws of WORKSPACES) {
      const btn = nav.getByRole('button', { name: ws.label, exact: true });
      await expect(btn).toBeVisible();
      const box = await btn.boundingBox();
      expect(box).not.toBeNull();
      expect(box.x).toBeGreaterThanOrEqual(0);
      expect(box.x + box.width).toBeLessThanOrEqual(viewportWidth);

      await btn.tap();
      await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe(ws.id);
    }
  });

  test('mobile interactive tap targets are at least 32px', async ({ page }) => {
    await page.goto('?board=rp2350-battery');
    await waitForAppReady(page);

    const targets = [
      ['Board selector', page.getByLabel('Board', { exact: true })],
      ...WORKSPACES.map((ws) => [
        `Tab ${ws.label}`,
        page.locator('header nav').getByRole('button', { name: ws.label, exact: true }),
      ]),
      ['View menu summary', page.locator('details.view-menu summary')],
      ['Drawn button', page.getByRole('group', { name: 'Which board to floorplan' }).getByRole('button', { name: 'Drawn' })],
      ['Solved button', page.getByRole('group', { name: 'Which board to floorplan' }).getByRole('button', { name: 'Solved' })],
      ['Parts button', page.getByRole('button', { name: 'Parts', exact: true })],
      ['Refine button', page.getByRole('button', { name: 'Refine', exact: true })],
      ['Place unlocked parts button', page.getByRole('button', { name: 'Place unlocked parts', exact: true })],
      ['Dock Checks tab', page.getByRole('tab', { name: /Checks/ })],
      ['Dock AI jobs tab', page.getByRole('tab', { name: 'AI jobs' })],
      ['Dock Terminal tab', page.getByRole('tab', { name: 'Terminal' })],
    ];

    const undersized = [];
    for (const [name, locator] of targets) {
      await expect(locator).toBeVisible();
      const box = await locator.boundingBox();
      if (!box || box.height < 32 || box.width < 32) {
        undersized.push(
          `${name}: ${box ? `${box.width.toFixed(1)}x${box.height.toFixed(1)}px` : 'null'}`,
        );
      }
    }

    expect(undersized, `Controls smaller than 32px: ${undersized.join(', ')}`).toEqual([]);
  });
});

test.describe('Circuit graph on a phone', () => {
  test.beforeEach(({ isMobile }) => {
    test.skip(!isMobile, 'Mobile-only layout checks');
  });

  test('block names stay readable: at least 8 px on screen', async ({ page }) => {
    for (const board of ['flame-earring', 'bolt-earring']) {
      await page.goto(`?board=${board}&view=circuit`);
      await waitForAppReady(page, { view: 'circuit' });
      const name = page.getByRole('img', { name: 'Circuit as blocks' }).locator('text.name').first();
      await expect(name).toBeVisible();
      const h = (await name.boundingBox()).height;
      expect(h, `${board}: a block name is ${h.toFixed(1)} px tall`).toBeGreaterThanOrEqual(8);
    }
  });
});
