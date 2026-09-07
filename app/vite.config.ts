import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

// Tauri expects a fixed dev port; `src-tauri/tauri.conf.json` points at
// 127.0.0.1:1425. 1420 is the reference workbench and 1421 was already taken on this machine.
export default defineConfig({
  plugins: [react()],
  resolve: { dedupe: ['react', 'react-dom'] },
  server: { port: 1425, strictPort: true },
  envPrefix: ['VITE_', 'TAURI_ENV_'],
  build: { outDir: 'dist', emptyOutDir: true, target: 'es2022' },
  clearScreen: false,
});
