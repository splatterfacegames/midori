/**
 * Build midori-wasm into apps/desktop/src/wasm via wasm-pack.
 *
 * The generated package is committed so `npm ci && npm run build` works on
 * machines without a Rust/WASM toolchain; rerun this script after changing
 * midori-core or midori-wasm sources.
 */

import { spawnSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const outDir = resolve(root, 'apps/desktop/src/wasm');

const result = spawnSync(
  'wasm-pack',
  ['build', 'crates/midori-wasm', '--target', 'web', '--release', '--out-dir', outDir],
  { cwd: root, stdio: 'inherit' },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// wasm-pack emits a .gitignore that would hide the committed bundle.
rmSync(resolve(outDir, '.gitignore'), { force: true });
