/**
 * Browser smoke test for the Midori workbench.
 *
 * Serves the production build with `vite preview`, drives it with a system
 * Chrome/Chromium via playwright-core (no browser download), and asserts the
 * app boots: engine WASM loads, a tree generates, the LOD preview canvas
 * exists, and no page errors or viewport errors surface.
 *
 * Usage:
 *   npm run build && node scripts/browser-smoke.mjs
 *   CHROME_PATH=/usr/bin/google-chrome node scripts/browser-smoke.mjs
 *
 * Exit 0 = pass, 1 = fail.
 */

import { chromium } from 'playwright-core';
import { spawn } from 'node:child_process';
import { existsSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const PORT = 4173;

function findChrome() {
  if (process.env.CHROME_PATH) return process.env.CHROME_PATH;
  for (const p of [
    '/usr/bin/google-chrome',
    '/usr/bin/google-chrome-stable',
    '/usr/bin/chromium',
    '/usr/bin/chromium-browser',
    '/opt/google/chrome/chrome',
    'C:\\Program Files\\Google\\Chrome\\Application\\chrome.exe',
    '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
  ]) {
    if (existsSync(p)) return p;
  }
  return null;
}

async function waitForServer(url, tries = 60) {
  for (let i = 0; i < tries; i++) {
    try {
      const res = await fetch(url);
      if (res.ok) return;
    } catch {
      /* not up yet */
    }
    await new Promise((r) => setTimeout(r, 500));
  }
  throw new Error(`vite preview did not respond at ${url}`);
}

const chromePath = findChrome();
if (!chromePath) {
  console.error('No Chrome/Chromium binary found — set CHROME_PATH');
  process.exit(1);
}

const server = spawn(
  process.platform === 'win32' ? 'npx.cmd' : 'npx',
  ['vite', 'preview', '--config', 'vite.config.ts', '--port', String(PORT), '--strictPort'],
  { cwd: root, stdio: 'pipe' },
);
let serverLog = '';
server.stdout.on('data', (d) => (serverLog += d));
server.stderr.on('data', (d) => (serverLog += d));

try {
  await waitForServer(`http://127.0.0.1:${PORT}/`);

  const browser = await chromium.launch({
    executablePath: chromePath,
    args: ['--headless=new', '--use-gl=swiftshader', '--no-sandbox'],
  });
  const page = await browser.newPage();
  const pageErrors = [];
  page.on('pageerror', (e) => pageErrors.push(String(e)));
  page.on('console', (m) => {
    if (m.type() === 'error') pageErrors.push(`console: ${m.text()}`);
  });

  await page.goto(`http://127.0.0.1:${PORT}/`);

  // Engine boots, loads WASM, and generates the default species tree.
  const status = page.locator('.midori-header-species');
  await status.waitFor({ timeout: 30_000 });
  const speciesName = (await status.textContent())?.trim() ?? '';
  if (!speciesName || speciesName === 'Midori') {
    throw new Error(`species name never populated: "${speciesName}"`);
  }

  // A WebGL canvas with actual geometry — the LOD preview panel.
  const canvas = page.locator('canvas').first();
  await canvas.waitFor({ timeout: 30_000 });
  const box = await canvas.boundingBox();
  if (!box || box.width < 32 || box.height < 32) {
    throw new Error(`preview canvas missing or degenerate: ${JSON.stringify(box)}`);
  }

  // Status bar reports the generated LOD set.
  const statusText = (await page.locator('.midori-status-right').textContent()) ?? '';
  if (!/LOD|WASM/.test(statusText)) {
    throw new Error(`status bar never reported generation: "${statusText}"`);
  }

  const inlineError = page.locator('.midori-error-inline');
  if ((await inlineError.count()) > 0) {
    throw new Error(`viewport error surfaced: ${await inlineError.textContent()}`);
  }

  const fatal = pageErrors.filter(
    (e) => !/favicon|Failed to load resource.*404|downloadable font/i.test(e),
  );
  if (fatal.length) throw new Error(`page errors: ${fatal.join(' | ')}`);

  await browser.close();
  console.log(`browser-smoke ok: species "${speciesName}", canvas ${box.width}x${box.height}`);
} catch (error) {
  console.error('browser-smoke FAILED:', error.message ?? error);
  if (serverLog.trim()) console.error('--- vite preview log ---\n' + serverLog.trim());
  process.exitCode = 1;
} finally {
  server.kill('SIGTERM');
}
