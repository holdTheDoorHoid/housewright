/// <reference types="vitest/config" />
import { defineConfig } from 'vite';
import { svelte } from '@sveltejs/vite-plugin-svelte';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import type { Plugin } from 'vite';

// Label reading on the web (D-74): the OCR models from `models/ocr/` are
// served beside the app as `ocr/*.onnx` and fetched by the worker the first
// time a label is read. Only the web build carries them; Android compiles
// them into the native library instead.
const OCR_MODELS = ['text-detection.onnx', 'text-recognition.onnx'];
function ocrModels(): Plugin {
  const file = (n: string) => resolve(import.meta.dirname, '../models/ocr', n);
  return {
    name: 'housewright-ocr-models',
    configureServer(server) {
      server.middlewares.use((req, res, next) => {
        const m = /\/ocr\/([a-z-]+\.onnx)$/.exec(req.url ?? '');
        if (!m || !OCR_MODELS.includes(m[1])) return next();
        res.setHeader('Content-Type', 'application/octet-stream');
        res.end(readFileSync(file(m[1])));
      });
    },
    generateBundle() {
      for (const n of OCR_MODELS) {
        this.emitFile({ type: 'asset', fileName: `ocr/${n}`, source: readFileSync(file(n)) });
      }
    },
  };
}

// Modes: default = Tauri (native core through invoke); `web` = the core as
// WebAssembly in a worker with SQLite on OPFS; `test` = vitest with a fake.
export default defineConfig(({ mode }) => ({
  plugins: mode === 'web' ? [svelte(), ocrModels()] : [svelte()],
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
