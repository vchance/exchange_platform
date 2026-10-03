import { defineConfig, devices } from '@playwright/test'

import { apiBinary, apiEnvironment, apiLog, baseURL, port, repoRoot } from './support/env'

/*
 * End-to-end tests: the built web app, served by the real API, against a
 * real PostgreSQL database with the migrations applied. README, "End-to-end
 * tests", says how to run them.
 */
export default defineConfig({
  testDir: '.',
  testMatch: '**/*.spec.ts',
  outputDir: '../test-results',
  // Each test signs up its own people, so tests are independent of each
  // other. The tests in one file share a worker, so a file can start
  // something once for all of them.
  fullyParallel: false,
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
    baseURL,
    locale: 'en-US',
    timezoneId: 'UTC',
    trace: 'retain-on-failure',
    screenshot: 'only-on-failure',
  },
  projects: [{ name: 'chromium', use: { ...devices['Desktop Chrome'] } }],
  // The API, serving the built web app, unless one already answers there:
  // CI starts its own and names its log in E2E_API_LOG. The database comes
  // from the environment or `.env`, with the migrations already applied.
  webServer: {
    command: `exec "${apiBinary}" > "${apiLog}" 2>&1`,
    cwd: repoRoot,
    url: `${baseURL}/readyz`,
    env: apiEnvironment(port),
    reuseExistingServer: true,
    timeout: 60_000,
  },
})
