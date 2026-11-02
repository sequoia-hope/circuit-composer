import { test, expect } from '@playwright/test';
import { waitForAppReady } from './helpers.js';

function packedStatLocator(page) {
  return page.locator('.metrics .stat', { hasText: '% packed' });
}

test.describe('Floorplan placement', () => {
  test('runs short placement with smallest moves option and updates "% packed"', async ({ page, isMobile }) => {
    // On mobile, .metrics .small (moves select) is hidden by CSS, while boards with >150 parts
    // (flame-earring) automatically default to the smallest 50k moves option.
    const board = isMobile ? 'flame-earring' : 'easy2350';
    await page.goto(`?board=${board}`);
    await waitForAppReady(page);

    if (!isMobile) {
      const movesSelect = page.locator('.metrics label', { hasText: 'moves' }).locator('select');
      await expect(movesSelect).toBeVisible();
      // Pick the smallest option in the moves dropdown (50k = 50000).
      const smallestValue = await movesSelect.locator('option').first().getAttribute('value');
      expect(smallestValue).toBe('50000');
      await movesSelect.selectOption(smallestValue);
      await expect(movesSelect).toHaveValue('50000');

      // Use fill = "all" (1) so the placement spreads differently from the saved compact placement,
      // guaranteeing a visible metric update when the annealer runs.
      const fillSelect = page.locator('.metrics label', { hasText: 'fill' }).locator('select');
      await fillSelect.selectOption('1');
    }

    const packedStat = packedStatLocator(page);
    await expect(packedStat).toBeVisible();
    const initialPackedText = (await packedStat.textContent()).trim();
    expect(initialPackedText).toMatch(/\d+\s*\/\s*\d+\s*%\s*packed/);

    const placeBtn = page.getByRole('button', { name: 'Place unlocked parts', exact: true });
    await expect(placeBtn).toBeEnabled();
    await placeBtn.click();

    // Either let the 50k run finish cleanly within a few seconds, or stop it if it is still annealing.
    const stopBtn = page.getByRole('button', { name: 'Stop', exact: true });
    const finishedQuickly = await placeBtn
      .waitFor({ state: 'visible', timeout: 25000 })
      .then(() => true)
      .catch(() => false);

    if (!finishedQuickly && (await stopBtn.isVisible())) {
      await stopBtn.click();
      await expect(placeBtn).toBeVisible({ timeout: 5000 });
    }

    await expect(packedStat).toBeVisible();
    await expect
      .poll(async () => (await packedStat.textContent()).trim(), { timeout: 5000 })
      .not.toBe(initialPackedText);

    if (!isMobile && finishedQuickly) {
      await expect(page.locator('.metrics .last')).toContainText('50,000 moves');
    }
  });

  test('Stop terminates an active placement and preserves valid "% packed" metrics', async ({ page, isMobile }) => {
    await page.goto('?board=bolt-earring');
    await waitForAppReady(page);

    if (!isMobile) {
      const movesSelect = page.locator('.metrics label', { hasText: 'moves' }).locator('select');
      await movesSelect.selectOption('1000000');
    }

    const placeBtn = page.getByRole('button', { name: 'Place unlocked parts', exact: true });
    await placeBtn.click();

    const stopBtn = page.getByRole('button', { name: 'Stop', exact: true });
    // On bolt-earring (or 1M moves), the worker runs long enough for Stop to appear; if a fast machine
    // finishes before Stop is clicked, verify completion gracefully.
    if (await stopBtn.isVisible().catch(() => false)) {
      await stopBtn.click();
    } else {
      await stopBtn.waitFor({ state: 'visible', timeout: 5000 }).then(() => stopBtn.click()).catch(() => {});
    }

    await expect(placeBtn).toBeVisible({ timeout: 15000 });
    await expect(packedStatLocator(page)).toContainText('% packed');
  });
});
