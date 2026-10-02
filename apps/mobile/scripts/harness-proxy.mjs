// TEST HARNESS ONLY. Lets the app's screens, run in a browser with Expo's web
// target, call a local API. A browser refuses a call to another origin unless
// that origin allows it, and the service deliberately allows none: its web
// client is served from its own origin, and the apps are not browsers. This
// stands between the two on a development machine and adds the permission.
// It is never deployed and the apps never use it.
//
//   API_TARGET=http://127.0.0.1:8080 HARNESS_ORIGIN=http://localhost:8081 \
//     node scripts/harness-proxy.mjs
//
// It passes requests through untouched and logs nothing.

import http from 'node:http'

const target = new URL(process.env.API_TARGET ?? 'http://127.0.0.1:8080')
const origin = process.env.HARNESS_ORIGIN ?? 'http://localhost:8081'
const port = Number(process.env.PORT ?? 5185)

const cors = {
  'Access-Control-Allow-Origin': origin,
  'Access-Control-Allow-Headers': 'authorization, content-type, idempotency-key, x-client-version',
  'Access-Control-Allow-Methods': 'GET, POST, PUT, PATCH, DELETE',
  'Access-Control-Max-Age': '600',
  Vary: 'Origin',
}

const server = http.createServer((request, response) => {
  if (request.method === 'OPTIONS') {
    response.writeHead(204, cors)
    response.end()
    return
  }
  // The service sees a plain client, as it would from a phone: no browser origin.
  const { origin: _origin, referer: _referer, ...headers } = request.headers
  const upstream = http.request(
    {
      host: target.hostname,
      port: target.port,
      path: request.url,
      method: request.method,
      headers: { ...headers, host: target.host },
    },
    (reply) => {
      response.writeHead(reply.statusCode ?? 502, { ...reply.headers, ...cors })
      reply.pipe(response)
    },
  )
  upstream.on('error', () => {
    response.writeHead(502, cors)
    response.end()
  })
  request.pipe(upstream)
})

// This machine only.
server.listen(port, '127.0.0.1', () => {
  console.log(`harness proxy: http://127.0.0.1:${port} -> ${target.origin}, for ${origin}`)
})
