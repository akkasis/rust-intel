import { defineConfig, devices } from "@playwright/test";

// e2e по UI в браузерном mock-режиме (фикстуры из демо-сида Rust).
export default defineConfig({
  testDir: "e2e",
  timeout: 30_000,
  use: { baseURL: "http://127.0.0.1:4173", viewport: { width: 1366, height: 768 } },
  webServer: { command: "npx vite build && npx vite preview --port 4173 --strictPort --host 127.0.0.1", url: "http://127.0.0.1:4173", reuseExistingServer: !process.env.CI, timeout: 120_000 },
  projects: [{ name: "chromium", use: { ...devices["Desktop Chrome"], viewport: { width: 1366, height: 768 } } }],
});
