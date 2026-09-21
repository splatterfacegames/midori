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

// Embedded file!()/panic paths leak the build machine's checkout and cargo
// home into the wasm data section. Remap both to fixed prefixes so the bundle
// is byte-identical across machines (the wasm-repro CI leg depends on it).
const home = process.env.HOME ?? process.env.USERPROFILE ?? '';
const remap = [
  `--remap-path-prefix=${resolve(root)}=/midori`,
  ...(home ? [`--remap-path-prefix=${resolve(home)}/.cargo=/cargo`] : []),
];
// When the rust-src component is installed, rustc embeds real toolchain
// paths (~/.rustup/toolchains/...) instead of virtual /rustc/<commit> ones.
// Remap the sysroot src dir so both cases produce identical bytes.
const v = spawnSync('rustc', ['-vV'], { encoding: 'utf8' });
const s = spawnSync('rustc', ['--print', 'sysroot'], { encoding: 'utf8' });
const sysroot = s.stdout?.trim();
const commit = v.stdout?.match(/^commit-hash: ([0-9a-f]+)$/m)?.[1];
if (sysroot && commit) {
  remap.push(`--remap-path-prefix=${sysroot}/lib/rustlib/src/rust=/rustc/${commit}`);
}
const rustflags = [process.env.RUSTFLAGS ?? '', ...remap].filter(Boolean).join(' ');

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
  // --no-opt: wasm-pack's bundled wasm-opt (binaryen 117) emits different
  // bytes across machines; skipping it keeps the committed bundle
  // reproducible (the wasm-repro CI leg depends on it). The release tarball
  // is optimized separately — release.yml runs a pinned wasm-opt -O2.
  ['build', 'crates/midori-wasm', '--target', 'web', '--release', '--no-opt', '--out-dir', outDir],
  { cwd: root, stdio: 'inherit', env: { ...process.env, RUSTFLAGS: rustflags } },
);
if (result.error) throw result.error;
if (result.status !== 0) process.exit(result.status ?? 1);

// wasm-pack emits a .gitignore that would hide the committed bundle.
rmSync(resolve(outDir, '.gitignore'), { force: true });
