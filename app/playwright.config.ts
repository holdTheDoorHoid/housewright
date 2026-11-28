import { defineConfig } from '@playwright/test';

// End-to-end tests of the web build (the core as WebAssembly, SQLite on OPFS).
// Uses the installed Google Chrome so no browser download is needed; set
// PW_CHANNEL=chromium in CI after `npx playwright install chromium`.
export default defineConfig({
  testDir: './tests',
  timeout: 60_000,
  use: {
    baseURL: 'http://localhost:4173',
    channel: process.env.PW_CHANNEL ?? 'chrome',
    headless: true,
  },
  webServer: {
    command: 'npx vite preview --mode web --port 4173 --strictPort',
    url: 'http://localhost:4173',
    reuseExistingServer: false,
  },
});
