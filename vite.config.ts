import { defineConfig } from 'vite';
import react from '@vitejs/plugin-react';
import { fileURLToPath, URL } from 'node:url';

const repoRoot = fileURLToPath(new URL('.', import.meta.url));

export default defineConfig({
  root: 'apps/desktop',
  plugins: [react()],
  resolve: {
    dedupe: ['react', 'react-dom', 'three', 'lucide-react'],
  },
  server: {
    port: 1420,
    strictPort: true,
    // Presets are imported from ../../presets/species via ?raw.
    fs: { allow: [repoRoot] },
  },
  build: {
    outDir: '../../dist',
    emptyOutDir: true,
    // The 1.1 MB midori_wasm binary is emitted as its own asset; it is the
    // engine, not splittable JS. 1.2 MB covers it plus app code.
    chunkSizeWarningLimit: 1200,
  },
  clearScreen: false,
});
