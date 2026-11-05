import { test, expect } from '@playwright/test';
import { waitForAppReady } from './helpers.js';

const LOCKED_WAFFLE_SHAPE = {
  thickness: 1.6,
  features: [],
  corner_radius: 0,
  symmetry: { kind: 'none', centre: [0, 0] },
  holes: [
    { id: 'waffle:h1', x: -18, y: 0, d: 2.7, plated: true, clearance: 0.25 },
    { id: 'waffle:h2', x: 18, y: 0, d: 2.7, plated: true, clearance: 0.25 },
  ],
  keepouts: [],
  sync: {
    driver: 'waffle',
    revision: 'sha256:3e74fcbaa25453aee16b256cafc50254722c7f8c17e157c3090133db85d29369',
    base: 'sha256:3e74fcbaa25453aee16b256cafc50254722c7f8c17e157c3090133db85d29369',
    synced: {
      outline: {
        elements: [
          {
            id: 'waffle:outer-disc',
            kind: {
              type: 'circle',
              center: { x: 0, y: 0 },
              diameter_nm: 50000000,
            },
          },
        ],
      },
      cutouts: [
        {
          elements: [
            {
              id: 'waffle:center-bore',
              kind: {
                type: 'circle',
                center: { x: 0, y: 0 },
                diameter_nm: 12000000,
              },
            },
          ],
        },
      ],
      holes: [
        { id: 'waffle:h1', center: { x: -18000000, y: 0 }, diameter_nm: 2700000, plated: true, clearance_nm: 250000 },
        { id: 'waffle:h2', center: { x: 18000000, y: 0 }, diameter_nm: 2700000, plated: true, clearance_nm: 250000 },
      ],
    },
    recipe_aside: {
      features: [
        { name: 'draft_rect', op: 'add', geom: { type: 'rect', x: 0, y: 0, w: 30, h: 45, r: 2, angle: 0 } },
      ],
      corner_radius: 0,
      symmetry: { kind: 'none', centre: [0, 0] },
    },
  },
};

test.describe('Shape workspace synced lock and take-over', () => {
  test('shows locked driver banner, disables editing, warns on unlock, and takes over cleanly', async ({ page }) => {
    await page.route('**/examples/easy2350/shape.json', (route) =>
      route.fulfill({
        status: 200,
        contentType: 'application/json',
        body: JSON.stringify(LOCKED_WAFFLE_SHAPE),
      }),
    );

    await page.goto('?board=easy2350&view=shape');
    await waitForAppReady(page, { view: 'shape' });
    await expect(page.getByLabel('Board shape')).toBeVisible();

    const statusBox = page.locator('.shape .lock-box');
    await expect(statusBox).toBeVisible();
    await expect(statusBox).toContainText('Outline controlled by Waffle Iron');

    // Editing controls are disabled while locked, while floorplanner outline application still works.
    await expect(page.getByRole('button', { name: /Rect/ })).toBeDisabled();
    await expect(page.getByRole('button', { name: /Circle/ })).toBeDisabled();
    await expect(page.getByRole('button', { name: /Hole/ })).toBeDisabled();
    await expect(page.getByRole('button', { name: 'Use as board outline' })).toBeEnabled();
    await expect(page.locator('.shape .bar')).toContainText('1850.0 mm²');
    await expect(page.locator('.shape .bar')).toContainText('1 cut-out');

    // Clicking Unlock reveals the warning naming Waffle Iron and the set-aside recipe.
    await statusBox.getByRole('button', { name: 'Unlock…' }).click();
    await expect(statusBox.locator('.lock-warn')).toContainText('Waffle Iron');
    await expect(statusBox.locator('.lock-warn')).toContainText('draft_rect');

    // Confirming Take over unlocks the document and creates the path recipe keeping the shape.
    await statusBox.getByRole('button', { name: 'Take over' }).click();
    await expect(statusBox).toHaveCount(0);
    await expect(page.getByRole('button', { name: /Rect/ })).toBeEnabled();
    await expect(page.locator('.shape .list ul').first()).toContainText('outline');
    await expect(page.locator('.shape .list ul').first()).toContainText('cutout 1');
    await expect(page.locator('.shape .bar')).toContainText('1850.0 mm²');
  });
});
