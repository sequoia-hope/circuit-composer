import { execFileSync } from 'node:child_process';
import { defineConfig, devices } from '@playwright/test';

function resolveBaseUrl() {
  const envPort = process.env.PORT?.trim();
  if (envPort) {
    const n = Number(envPort);
    if (!/^\d+$/.test(envPort) || !Number.isInteger(n) || n < 1 || n > 0xffff) {
      throw new Error(`Invalid PORT environment variable: ${JSON.stringify(process.env.PORT)}`);
    }
    return `http://localhost:${envPort}/app/dist/`;
  }

  let rawUrl = '';
  try {
    rawUrl = execFileSync('proj', ['url', 'circuit-composer'], {
      encoding: 'utf8',
      stdio: ['ignore', 'pipe', 'pipe'],
    }).trim();
  } catch (err) {
    const detail = err?.stderr?.toString().trim() || err?.message || String(err);
    throw new Error(
      `Cannot determine Circuit Composer URL: PORT is not set and \`proj url circuit-composer\` failed (${detail}).`,
    );
  }

  if (!rawUrl) {
    throw new Error(
      'Cannot determine Circuit Composer URL: PORT is not set and `proj url circuit-composer` returned empty output.',
    );
  }

  let root;
  try {
    root = new URL(rawUrl.endsWith('/') ? rawUrl : `${rawUrl}/`);
  } catch {
    throw new Error(`\`proj url circuit-composer\` returned an invalid URL: ${JSON.stringify(rawUrl)}`);
  }

  return new URL('app/dist/', root).href;
}

const reservedCores = process.env.RESERVED_CORES
  ? Number.parseInt(process.env.RESERVED_CORES, 10)
  : Number.NaN;

export default defineConfig({
  testDir: 'tests',
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: 0,
  workers: Number.isInteger(reservedCores) && reservedCores > 0 ? reservedCores : undefined,
  reporter: 'list',
  use: {
    baseURL: resolveBaseUrl(),
    browserName: 'chromium',
    screenshot: 'only-on-failure',
    trace: 'retain-on-failure',
    launchOptions: {
      executablePath: '/usr/bin/google-chrome',
    },
  },
  projects: [
    {
      name: 'desktop',
      use: {
        viewport: { width: 1440, height: 900 },
      },
    },
    {
      name: 'mobile',
      use: {
        ...devices['Pixel 7'],
        viewport: { width: 412, height: 915 },
        isMobile: true,
        hasTouch: true,
      },
    },
  ],
});
