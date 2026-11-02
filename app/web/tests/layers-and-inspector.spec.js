import { test, expect } from '@playwright/test';
import { waitForAppReady, analyzeCanvasPixels } from './helpers.js';

const LAYER_LABELS = ['Top', 'Bottom', 'Ratsnest', 'Labels', 'Block colours', 'Locks'];

test.describe('Layer checkboxes and parts inspector', () => {
  test('layer checkboxes (including Locks) toggle without errors', async ({ page, isMobile }) => {
    await page.goto('?board=flame-earring');
    await waitForAppReady(page);

    const pageErrors = [];
    const consoleErrors = [];
    page.on('pageerror', (err) => pageErrors.push(err.message || String(err)));
    page.on('console', (msg) => {
      if (msg.type() === 'error') consoleErrors.push(msg.text());
    });

    const openLayerMenuIfNeeded = async () => {
      if (isMobile) {
        const menu = page.locator('details.view-menu');
        const isOpen = await menu.evaluate((el) => el.open);
        if (!isOpen) await menu.locator('summary').click();
      }
    };
    const closeLayerMenuIfNeeded = async () => {
      if (isMobile) {
        const menu = page.locator('details.view-menu');
        const isOpen = await menu.evaluate((el) => el.open);
        if (isOpen) await menu.locator('summary').click();
      }
    };

    const canvas = page.getByLabel('Board floorplan');
    const withLocks = await analyzeCanvasPixels(page, canvas);

    await openLayerMenuIfNeeded();
    const locksBox = page.locator('.toolbar label', { hasText: 'Locks' }).locator('input[type="checkbox"]');
    await expect(locksBox).toBeChecked();
    await locksBox.uncheck();
    await expect(locksBox).not.toBeChecked();
    await closeLayerMenuIfNeeded();

    const withoutLocks = await analyzeCanvasPixels(page, canvas);
    expect(withoutLocks.signature).not.toBe(withLocks.signature);

    await openLayerMenuIfNeeded();
    await locksBox.check();
    await expect(locksBox).toBeChecked();

    for (const label of LAYER_LABELS) {
      const checkbox = page.locator('.toolbar label', { hasText: label }).locator('input[type="checkbox"]');
      await expect(checkbox).toBeVisible();
      await expect(checkbox).toBeChecked();

      await checkbox.uncheck();
      await expect(checkbox).not.toBeChecked();
      await expect(page.locator('.centre .wrap .err')).toHaveCount(0);

      await checkbox.check();
      await expect(checkbox).toBeChecked();
      await expect(page.locator('.centre .wrap .err')).toHaveCount(0);
    }

    expect(pageErrors).toEqual([]);
    expect(consoleErrors).toEqual([]);
  });

  test('parts panel: open, find a part by reference, select it, and inspect details', async ({ page, isMobile }) => {
    await page.goto('?board=rp2350-battery');
    await waitForAppReady(page);

    if (isMobile) {
      const partsBtn = page.getByRole('button', { name: 'Parts', exact: true });
      await expect(partsBtn).toBeVisible();
      await partsBtn.click();
    }

    const inspector = page.locator('aside .insp');
    await expect(inspector).toBeVisible();
    await expect(inspector.locator('h2')).toHaveText('rp2350-battery');

    const findInput = inspector.getByPlaceholder('Find a part: ref, value or block');
    await findInput.fill('U4');

    const hit = inspector.locator('.hits button', { hasText: 'U4' }).first();
    await expect(hit).toBeVisible();
    await hit.click();

    await expect(findInput).toHaveValue('');
    await expect(inspector.locator('h2')).toContainText('U4');
    await expect(inspector.locator('dl')).toContainText('RP2350_60QFN');
    await expect(inspector.locator('dl')).toContainText('RP2350-QFN-60');
    await expect(inspector.locator('dl')).toContainText('mcu');

    await expect(inspector.locator('.pose input[type="number"]')).toHaveCount(2);
    const rotLabel = inspector.locator('.pose .rot');
    const initialRot = await rotLabel.textContent();

    // Rotate 90° and undo with Ctrl+Z.
    const rotateBtn = inspector.getByRole('button', { name: /Rotate 90°/ });
    await expect(rotateBtn).toBeEnabled();
    await rotateBtn.click();
    await expect(rotLabel).not.toHaveText(initialRot);

    await page.evaluate(() => document.activeElement?.blur?.());
    await page.keyboard.press('Control+z');
    await expect(rotLabel).toHaveText(initialRot);

    // Lock disables pose inputs and Rotate; Unlock restores them.
    await inspector.getByRole('button', { name: /^Lock\b/ }).click();
    await expect(inspector.locator('h2 .lock')).toHaveText('locked');
    await expect(rotateBtn).toBeDisabled();
    await expect(inspector.locator('.pose input[type="number"]').first()).toBeDisabled();

    await inspector.getByRole('button', { name: /^Unlock\b/ }).click();
    await expect(rotateBtn).toBeEnabled();
    await expect(inspector.locator('.nets li').first()).toBeVisible();
  });

  test('Select locked and Select free select the expected part sets on flame-earring', async ({ page, isMobile }) => {
    await page.goto('?board=flame-earring');
    await waitForAppReady(page);

    if (isMobile) {
      await page.getByRole('button', { name: 'Parts', exact: true }).click();
    }

    const inspector = page.locator('aside .insp');
    await expect(inspector).toBeVisible();
    await expect(inspector.locator('h2')).toHaveText('flame-earring');

    const selectLockedBtn = inspector.getByRole('button', { name: /^Select locked \(\d+\)$/ });
    const selectFreeBtn = inspector.getByRole('button', { name: /^Select free \(\d+\)$/ });
    await expect(selectLockedBtn).toBeEnabled();
    await expect(selectFreeBtn).toBeEnabled();

    const lockedText = await selectLockedBtn.textContent();
    const lockedCount = Number(lockedText.match(/\((\d+)\)/)[1]);
    expect(lockedCount).toBeGreaterThan(100);

    const freeText = await selectFreeBtn.textContent();
    const freeCount = Number(freeText.match(/\((\d+)\)/)[1]);
    expect(freeCount).toBeGreaterThan(0);

    await selectLockedBtn.click();
    await expect(inspector.locator('h2')).toHaveText(`${lockedCount} parts`);
    await expect(inspector).toContainText('All locked.');
    await expect(inspector.getByRole('button', { name: 'Unlock all', exact: true })).toBeVisible();

    // Clear selection with Escape; on mobile partsOpen remains true from clicking Parts.
    await page.evaluate(() => document.activeElement?.blur?.());
    await page.keyboard.press('Escape');
    await expect(inspector.locator('h2')).toHaveText('flame-earring');

    await selectFreeBtn.click();
    await expect(inspector.locator('h2')).toHaveText(`${freeCount} parts`);
    await expect(inspector).toContainText('All free.');
    await expect(inspector.getByRole('button', { name: 'Lock all', exact: true })).toBeVisible();
  });
});
