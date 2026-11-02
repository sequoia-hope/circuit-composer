import { test, expect } from '@playwright/test';
import { WORKSPACES, waitForAppReady } from './helpers.js';

async function expectWorkspaceRendered(page, id) {
  const label = WORKSPACES.find((w) => w.id === id).label;
  await expect(
    page.locator('header nav').getByRole('button', { name: label, exact: true }),
  ).toHaveClass(/\bon\b/);

  if (id === 'circuit') {
    await expect(page.getByRole('img', { name: 'Circuit as blocks' })).toBeVisible();
  } else if (id === 'board') {
    await expect(page.getByLabel('Board floorplan')).toBeVisible();
  } else if (id === 'shape') {
    await expect(page.getByLabel('Board shape')).toBeVisible();
  } else if (id === 'copper') {
    await expect(page.locator('.later h2')).toHaveText('Copper');
  } else if (id === 'rules') {
    await expect(page.locator('.later.rules h2')).toHaveText('Rules');
    await expect(page.locator('.later.rules tbody tr').first()).toBeVisible();
  }
}

test.describe('Workspace navigation', () => {
  test('clicking each workspace tab switches the view and updates ?view=', async ({ page }) => {
    await page.goto('./');
    await waitForAppReady(page);

    for (const ws of WORKSPACES) {
      await page.locator('header nav').getByRole('button', { name: ws.label, exact: true }).click();
      await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe(ws.id);
      await expectWorkspaceRendered(page, ws.id);
    }
  });

  test('keys 1-5 switch workspaces and update ?view=', async ({ page }) => {
    await page.goto('./');
    await waitForAppReady(page);
    await page.evaluate(() => document.activeElement?.blur?.());

    // Visit in an order that also tests leaving Shape (key 3 -> key 4) where ShapeView has its own key handler.
    for (const key of ['1', '3', '4', '5', '2']) {
      const ws = WORKSPACES.find((w) => w.key === key);
      await page.keyboard.press(key);
      await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe(ws.id);
      await expectWorkspaceRendered(page, ws.id);
    }
  });

  test('reloading with ?view= restores each workspace', async ({ page }) => {
    await page.goto('?board=easy2350&view=circuit');
    await waitForAppReady(page, { view: 'circuit' });
    await expectWorkspaceRendered(page, 'circuit');

    for (const ws of WORKSPACES) {
      await page.locator('header nav').getByRole('button', { name: ws.label, exact: true }).click();
      await expect.poll(() => new URL(page.url()).searchParams.get('view')).toBe(ws.id);

      await page.reload();
      await waitForAppReady(page, { view: ws.id });
      expect(new URL(page.url()).searchParams.get('view')).toBe(ws.id);
      await expectWorkspaceRendered(page, ws.id);
    }
  });

  test('Circuit block selection carries into Board and Shape outline can be applied', async ({ page, isMobile }) => {
    await page.goto('?board=rp2350-battery&view=circuit');
    await waitForAppReady(page, { view: 'circuit' });

    const svg = page.getByRole('img', { name: 'Circuit as blocks' });
    await expect(svg).toBeVisible();
    const firstNode = svg.locator('g.node').first();
    await expect(firstNode).toBeVisible();
    await firstNode.click();
    await expect(firstNode).toHaveClass(/\bsel\b/);

    // Switch to Board and verify parts from that block are selected in the Inspector.
    await page.locator('header nav').getByRole('button', { name: 'Board', exact: true }).click();
    await expect(page.locator('aside .insp')).toBeVisible();
    await expect(page.locator('aside .insp h2')).not.toHaveText('rp2350-battery');

    // Switch to Shape, add a rectangle outline, and apply it to the board.
    await page.locator('header nav').getByRole('button', { name: 'Shape', exact: true }).click();
    await expect(page.getByLabel('Board shape')).toBeVisible();
    await page.getByRole('button', { name: /Rect/ }).click();

    const applyBtn = page.getByRole('button', { name: 'Use as board outline' });
    await expect(applyBtn).toBeEnabled();
    await applyBtn.click();
    await expect(page.getByRole('button', { name: '✓ Board uses this outline' })).toBeVisible();

    // Verify Dock tabs switch cleanly (and collapse when tapped again on mobile).
    await page.locator('header nav').getByRole('button', { name: 'Board', exact: true }).click();
    const jobsTab = page.getByRole('tab', { name: 'AI jobs' });
    const termTab = page.getByRole('tab', { name: 'Terminal' });
    await jobsTab.click();
    await expect(page.locator('footer .dock .body')).toContainText('AI jobs');
    await termTab.click();
    await expect(page.locator('footer .dock .body')).toContainText('composer');
    if (isMobile) {
      await termTab.click();
      await expect(page.locator('footer .dock .body')).toBeHidden();
    }
  });

  test('embed mode (?embed=1) shows compact workspace bar without board selector', async ({ page }) => {
    await page.goto('?embed=1&board=easy2350');
    await waitForAppReady(page);

    await expect(page.locator('header.bar')).toBeVisible();
    await expect(page.getByLabel('Board', { exact: true })).toHaveCount(0);
    await expect(page.locator('header.bar nav button')).toHaveCount(WORKSPACES.length);
  });
});

test.describe('Board view fit and dock', () => {
  test('no rounding-size overlap is a problem, and closing the dock gives the board the room', async ({ page, isMobile }) => {
    test.skip(isMobile, 'The phone opens with the dock closed');
    await page.goto('?board=flame-earring');
    await waitForAppReady(page);
    const canvas = page.getByLabel('Board floorplan');
    const before = await canvas.boundingBox();

    // No courtyard overlap too small to print is reported as a problem.
    await expect(page.locator('.metrics .stat', { hasText: 'problem' })).toContainText('0 problems');

    const checks = page.getByRole('tab', { name: /Checks/ });
    await expect(checks).toHaveAttribute('aria-selected', 'true');
    await checks.click();
    await expect(checks).toHaveAttribute('aria-selected', 'false');
    await expect(page.locator('footer .dock .body')).toBeHidden();
    await expect.poll(async () => (await canvas.boundingBox()).height).toBeGreaterThan(before.height + 50);
    await checks.click();
    await expect(page.locator('footer .dock .body')).toBeVisible();
  });
});
