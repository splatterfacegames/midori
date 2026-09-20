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
  build: { outDir: '../../dist', emptyOutDir: true },
  clearScreen: false,
});
