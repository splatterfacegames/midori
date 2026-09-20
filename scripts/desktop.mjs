import { spawn } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { resolve } from 'node:path';

const root = fileURLToPath(new URL('../', import.meta.url));
const mode = process.argv[2] ?? 'dev';
if (!['dev', 'build'].includes(mode)) throw new Error('Use desktop.mjs dev or build');
const children = new Set();
function run(script, args, cwd = root) {
  const child = spawn(process.execPath, [resolve(root, script), ...args], { cwd, stdio: 'inherit', windowsHide: true });
  children.add(child);
  child.once('exit', () => children.delete(child));
  return child;
}
function completion(child) {
  return new Promise((accept, reject) => {
    child.once('error', reject);
    child.once('exit', (code) => code === 0 ? accept() : reject(new Error(`Process exited with ${code}`)));
  });
}
function stop() { for (const child of children) child.kill(); }
process.on('SIGINT', () => { stop(); process.exit(130); });
process.on('SIGTERM', () => { stop(); process.exit(143); });
try {
  if (mode === 'build') {
    await completion(run('node_modules/typescript/bin/tsc', ['--noEmit']));
    await completion(run('node_modules/vite/bin/vite.js', ['build', '--config', 'vite.config.ts']));
  } else {
    run('node_modules/vite/bin/vite.js', ['--config', 'vite.config.ts', '--host', '127.0.0.1'])
      .once('exit', (code) => { if (code) { stop(); process.exitCode = code; } });
  }
  await completion(run('node_modules/@tauri-apps/cli/tauri.js', [mode, ...process.argv.slice(3)], resolve(root, 'crates/midori-desktop')));
} catch (error) {
  console.error(error instanceof Error ? error.message : error);
  process.exitCode = 1;
} finally { stop(); }
