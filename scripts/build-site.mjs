/**
 * Assemble `dist-site/` for https://midori.splatterfacegames.com:
 * - `/`      <- `site/` (landing page, styles, fonts, showcase renders)
 * - `/demo/` <- `dist/` (interactive WebAssembly + WebGL workbench)
 */

import { cpSync, existsSync, mkdirSync, rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const siteSrc = resolve(root, 'site');
const workbenchDist = resolve(root, 'dist');
const outDir = resolve(root, 'dist-site');

if (!existsSync(resolve(workbenchDist, 'index.html'))) {
  console.error('dist/index.html missing — run `npm run build` first');
  process.exit(1);
}

rmSync(outDir, { recursive: true, force: true });
mkdirSync(outDir, { recursive: true });
cpSync(siteSrc, outDir, { recursive: true });
cpSync(workbenchDist, resolve(outDir, 'demo'), { recursive: true });
console.log('Assembled dist-site/ (landing page at / + workbench at /demo/)');
