/**
 * Release gate: verify a git tag matches workspace + desktop versions.
 *
 * Usage: node scripts/check-version.mjs v0.2.0   (or 0.2.0)
 * Exits 1 if the tag does not equal `[workspace.package].version` in
 * Cargo.toml, `version` in package.json, `crates/midori-desktop/Cargo.toml`,
 * and `crates/midori-desktop/tauri.conf.json`.
 */

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));

const tag = process.argv[2];
if (!tag) {
  console.error('usage: node scripts/check-version.mjs <tag>');
  process.exit(1);
}
const want = tag.replace(/^v/, '');
if (!/^\d+\.\d+\.\d+(-[0-9A-Za-z.-]+)?$/.test(want)) {
  console.error(`tag "${tag}" is not semver (vMAJOR.MINOR.PATCH[-pre])`);
  process.exit(1);
}

const cargo = readFileSync(resolve(root, 'Cargo.toml'), 'utf8');
const wsSection = cargo.match(/\[workspace\.package\]([\s\S]*?)(?=\n\[|$)/);
const cargoVersion = wsSection?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const npmVersion = JSON.parse(readFileSync(resolve(root, 'package.json'), 'utf8')).version;

const desktopCargo = readFileSync(resolve(root, 'crates/midori-desktop/Cargo.toml'), 'utf8');
const desktopPkg = desktopCargo.match(/\[package\]([\s\S]*?)(?=\n\[|$)/);
const desktopCargoVersion = desktopPkg?.[1].match(/^version\s*=\s*"([^"]+)"/m)?.[1];
const tauriVersion = JSON.parse(
  readFileSync(resolve(root, 'crates/midori-desktop/tauri.conf.json'), 'utf8'),
).version;

const mismatches = [];
if (cargoVersion !== want) mismatches.push(`Cargo.toml workspace: ${cargoVersion}`);
if (npmVersion !== want) mismatches.push(`package.json: ${npmVersion}`);
if (desktopCargoVersion !== want) mismatches.push(`crates/midori-desktop/Cargo.toml: ${desktopCargoVersion}`);
if (tauriVersion !== want) mismatches.push(`crates/midori-desktop/tauri.conf.json: ${tauriVersion}`);

if (mismatches.length) {
  console.error(`version mismatch for tag ${tag}:\n  ${mismatches.join('\n  ')}`);
  process.exit(1);
}
console.log(`version ok: tag ${tag} == ${want} (Cargo.toml + package.json + midori-desktop)`);
