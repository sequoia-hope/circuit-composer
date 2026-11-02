import { test, expect } from '@playwright/test';
import { waitForAppReady, analyzeCanvasPixels } from './helpers.js';

test.describe('Color schemes (light and dark)', () => {
  test('light colorScheme applies light theme tokens and draws on a light canvas', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('?board=easy2350');
    await waitForAppReady(page);

    const vars = await page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const body = getComputedStyle(document.body);
      return {
        canvas: root.getPropertyValue('--canvas').trim(),
        bg: root.getPropertyValue('--bg').trim(),
        bodyBg: body.backgroundColor,
      };
    });

    expect(vars.canvas).toBe('#f7f6f2');
    expect(vars.bg).toBe('#f4f3ef');
    expect(vars.bodyBg).toBe('rgb(244, 243, 239)');

    const stats = await analyzeCanvasPixels(page, page.getByLabel('Board floorplan'));
    expect(stats.meanLuma).toBeGreaterThan(180);
    expect(stats.nonBgRatio).toBeGreaterThan(0.01);
  });

  test('dark colorScheme applies dark theme tokens and draws on a dark canvas', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'dark' });
    await page.goto('?board=easy2350');
    await waitForAppReady(page);

    const vars = await page.evaluate(() => {
      const root = getComputedStyle(document.documentElement);
      const body = getComputedStyle(document.body);
      return {
        canvas: root.getPropertyValue('--canvas').trim(),
        bg: root.getPropertyValue('--bg').trim(),
        bodyBg: body.backgroundColor,
      };
    });

    expect(vars.canvas).toBe('#0e0f11');
    expect(vars.bg).toBe('#111214');
    expect(vars.bodyBg).toBe('rgb(17, 18, 20)');

    const stats = await analyzeCanvasPixels(page, page.getByLabel('Board floorplan'));
    expect(stats.meanLuma).toBeLessThan(80);
    expect(stats.nonBgRatio).toBeGreaterThan(0.01);
  });

  test('switching colorScheme dynamically redraws the WebGL2 board canvas', async ({ page }) => {
    await page.emulateMedia({ colorScheme: 'light' });
    await page.goto('?board=rp2350-battery');
    await waitForAppReady(page);

    const canvas = page.getByLabel('Board floorplan');
    const lightStats = await analyzeCanvasPixels(page, canvas);
    expect(lightStats.meanLuma).toBeGreaterThan(180);

    await page.emulateMedia({ colorScheme: 'dark' });
    await expect
      .poll(async () => (await analyzeCanvasPixels(page, canvas)).meanLuma, { timeout: 5000 })
      .toBeLessThan(80);

    const darkStats = await analyzeCanvasPixels(page, canvas);
    expect(darkStats.nonBgRatio).toBeGreaterThan(0.01);
  });
});
