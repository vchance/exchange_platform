import react from '@vitejs/plugin-react'
import { defineConfig } from 'vite'

// The Rust service in development. In production the web app is static files
// served from the same origin as the API.
const api = 'http://127.0.0.1:8080'

// https://vite.dev/config/
export default defineConfig({
  plugins: [react()],
  server: {
    proxy: {
      '/v1': api,
      '/healthz': api,
      '/readyz': api,
    },
  },
})
