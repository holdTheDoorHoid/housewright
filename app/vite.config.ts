/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';

// Modes: default = Tauri (native core through invoke); `web` = the core as
// WebAssembly in a worker with SQLite on OPFS; `test` = vitest with a fake.
export default defineConfig(({ mode }) => ({
  plugins: [svelte()],
  define: {
    __BACKEND__: JSON.stringify(mode === 'web' ? 'wasm' : mode === 'test' ? 'test' : 'tauri'),
  },
  base: './',
  clearScreen: false,
  server: {
    port: 5173,
    strictPort: true,
    host: process.env.TAURI_DEV_HOST || false,
    fs: { allow: ['..'] },
  },
  worker: { format: 'es' },
  build: {
    target: 'es2022',
    outDir: mode === 'web' ? 'dist-web' : 'dist',
    emptyOutDir: true,
  },
  resolve: process.env.VITEST ? { conditions: ['browser'] } : undefined,
  test: {
    environment: 'jsdom',
    include: ['src/**/*.test.ts'],
  },
}));
