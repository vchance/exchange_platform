import { defineConfig } from '@playwright/test'

import { phone } from './support/fixtures'
import {
  apiBinary,
  apiEnvironment,
  apiLog,
  apiURL,
  exportDir,
  mobileRoot,
  proxyPort,
  proxyURL,
  repoRoot,
  webPort,
  webURL,
} from './support/env'

/*
 * End-to-end tests of the mobile app's screens, run in Chromium the size of a
 * phone through the browser harness (README, "Running the screens in a
 * browser"): the app exported for the web, calling the real API through the
 * harness proxy, against a real PostgreSQL database with the migrations
 * applied. README, "Mobile end-to-end tests", says how to run them and what
 * they cannot show.
 */
export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  outputDir: '../test-results',
  // Each test signs up its own people, so tests are independent of each other.
  fullyParallel: true,
  forbidOnly: !!process.env.CI,
  retries: process.env.CI ? 1 : 0,
  workers: process.env.CI ? 2 : undefined,
  timeout: 90_000,
  expect: { timeout: 10_000 },
  reporter: [
    ['list'],
    ['html', { open: 'never', outputFolder: '../playwright-report' }],
    ...(process.env.CI ? [['github'] as const] : []),
  ],
  use: {
    ...phone,
    baseURL: webURL,
    locale: 'en-US',
    timezoneId: 'UTC',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'phone', use: { browserName: 'chromium' } }],
  // Each is started unless one already answers there: CI starts its own API
  // and names its log in E2E_MOBILE_API_LOG. The database comes from the
  // environment or `.env`, with the migrations already applied.
  webServer: [
    {
      command: `exec "${apiBinary}" > "${apiLog}" 2>&1`,
      cwd: repoRoot,
      url: `${apiURL}/readyz`,
      env: apiEnvironment(),
      reuseExistingServer: true,
      timeout: 60_000,
    },
    {
      command: 'node scripts/harness-proxy.mjs',
      cwd: mobileRoot,
      url: `${proxyURL}/healthz`,
      env: { API_TARGET: apiURL, HARNESS_ORIGIN: webURL, PORT: String(proxyPort) },
      reuseExistingServer: true,
      timeout: 30_000,
    },
    {
      command: 'node e2e/support/serve-export.mjs',
      cwd: mobileRoot,
      url: webURL,
      env: { EXPORT_DIR: exportDir, PORT: String(webPort) },
      reuseExistingServer: true,
      timeout: 30_000,
    },
  ],
})
