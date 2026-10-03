// Exports the app for Expo's web target, for the end-to-end tests: the same
// screens, with the harness's `.web` files, calling the API through the
// harness proxy. The addresses are baked into the bundle, so they are read
// here from the same variables, with the same defaults, as support/env.ts.

import { spawnSync } from 'node:child_process'
import { dirname, resolve } from 'node:path'
import { fileURLToPath } from 'node:url'

const mobileRoot = resolve(dirname(fileURLToPath(import.meta.url)), '../..')
const host = '127.0.0.1'
const proxyPort = process.env.E2E_MOBILE_PROXY_PORT ?? '8203'
const webPort = process.env.E2E_MOBILE_WEB_PORT ?? '5203'
const outputDir = resolve(
  process.env.E2E_MOBILE_EXPORT_DIR ?? resolve(mobileRoot, 'e2e/.output/web'),
)

const result = spawnSync(
  'npx',
  ['expo', 'export', '--platform', 'web', '--output-dir', outputDir, '--clear'],
  {
    cwd: mobileRoot,
    stdio: 'inherit',
    env: {
      ...process.env,
      EXPO_PUBLIC_API_URL: `http://${host}:${proxyPort}`,
      // Invitation links point at the web origin; here that is the harness.
      EXPO_PUBLIC_WEB_URL: `http://${host}:${webPort}`,
    },
  },
)
process.exit(result.status ?? 1)
