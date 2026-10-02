import { fileURLToPath } from 'node:url'

import react from '@vitejs/plugin-react'
import { defineConfig } from 'vitest/config'

import { entryPagesPlugin } from './build/entry-pages.ts'

// The Rust service in development. In production the web app is static files
// served from the same origin as the API. Set API_PROXY_TARGET when the
// service listens somewhere other than its default address; whatever origin
// this dev server runs on must also be the service's WEB_ORIGIN, or it will
// not honor the session cookie.
const api = process.env.API_PROXY_TARGET ?? 'http://127.0.0.1:8080'

const wording = fileURLToPath(new URL('../../packages/shared/wording', import.meta.url))

// https://vite.dev/config/
export default defineConfig({
  plugins: [react(), entryPagesPlugin(wording)],
  server: {
    proxy: {
      '/v1': api,
      '/healthz': api,
      '/readyz': api,
    },
  },
  test: {
    include: ['src/**/*.test.ts', 'build/**/*.test.ts'],
  },
})
