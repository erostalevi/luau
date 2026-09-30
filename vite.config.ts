import { defineConfig } from 'vitest/config';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { fileURLToPath } from 'node:url';

const host = process.env.TAURI_DEV_HOST;

export default defineConfig({
  plugins: [svelte()],
  clearScreen: false,
  resolve: { alias: { $lib: fileURLToPath(new URL('./src/lib', import.meta.url)) } },
  server: {
    port: 1420,
    strictPort: true,
    host: host || false,
    hmr: host ? { protocol: 'ws', host, port: 1421 } : undefined,
    watch: { ignored: ['**/src-tauri/**', '**/crates/**', '**/target/**'] },
  },
  build: {
    target: ['es2022', 'safari16', 'chrome110'],
    minify: 'esbuild',
    sourcemap: false,
    chunkSizeWarningLimit: 1500,
  },
  test: { environment: 'jsdom', include: ['src/**/*.test.ts'] },
});
