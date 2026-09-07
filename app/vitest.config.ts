import { defineConfig, mergeConfig } from 'vitest/config';
// The extensionless specifier is what tsc wants; Vite warns that its native
// config loader will one day want the extension. tsc is the one that fails the
// build today.
import viteConfig from './vite.config';

// jsdom, not a browser. These tests cover contract decoding, the slice state
// machine and how the two are wired. They are not evidence that the native
// application launches, that the shared shell renders, or that a GLB draws.
export default mergeConfig(
  viteConfig,
  defineConfig({
    test: {
      environment: 'jsdom',
      include: ['src/**/*.test.ts', 'src/**/*.test.tsx'],
    },
  }),
);
