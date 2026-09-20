/**
 * Build midori-wasm into apps/desktop/src/wasm via wasm-pack.
 *
 * The generated package is committed so `npm ci && npm run build` works on
 * machines without a Rust/WASM toolchain; rerun this script after changing
 * midori-core or midori-wasm sources.
 *
 * wasm-pack is pinned (WASM_PACK_VERSION) because the emitted JS glue and
 * wasm-opt output can differ between versions; CI rebuilds the bundle and
 * diffs it against the commit, so a version drift shows up as a dirty tree.
 */

import { spawnSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const outDir = resolve(root, 'apps/desktop/src/wasm');

const WASM_PACK_VERSION = '0.15.0';

const version = spawnSync('wasm-pack', ['--version'], { encoding: 'utf8' });
if (version.error) {
  console.error(`wasm-pack ${WASM_PACK_VERSION} is required (not on PATH)`);
  process.exit(1);
}
const found = version.stdout.trim().split(/\s+/).pop();
if (found !== WASM_PACK_VERSION) {
  console.error(`wasm-pack ${WASM_PACK_VERSION} required, found ${found}`);
  process.exit(1);
}

const result = spawnSync(
  'wasm-pack',
  ['build', 'crates/midori-wasm', '--target', 'web', '--release', '--out-dir', outDir],
  { cwd: root, stdio: 'inherit' },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// wasm-pack emits a .gitignore that would hide the committed bundle.
rmSync(resolve(outDir, '.gitignore'), { force: true });
