// TEST HARNESS ONLY. Serves the mobile app's web export (`npx expo export
// --platform web`) for the end-to-end tests: each file as it is, and the
// app's page for any other address, so the router takes a deep link such as
// `/en/i#…` or `/exchanges/{id}` the way a device takes one it was handed.
//
//   EXPORT_DIR=apps/mobile/e2e/.output/web PORT=5203 node apps/mobile/e2e/support/serve-export.mjs
//
// This machine only; never deployed.

import { readFile, stat } from 'node:fs/promises'
import http from 'node:http'
import { extname, join, normalize, resolve, sep } from 'node:path'

const root = resolve(process.env.EXPORT_DIR ?? 'dist')
const port = Number(process.env.PORT ?? 5203)

const types = {
  '.html': 'text/html; charset=utf-8',
  '.js': 'text/javascript; charset=utf-8',
  '.json': 'application/json',
  '.css': 'text/css; charset=utf-8',
  '.png': 'image/png',
  '.jpg': 'image/jpeg',
  '.svg': 'image/svg+xml',
  '.ico': 'image/x-icon',
  '.ttf': 'font/ttf',
  '.woff': 'font/woff',
  '.woff2': 'font/woff2',
}

async function fileFor(pathname) {
  const wanted = normalize(join(root, decodeURIComponent(pathname)))
  if (wanted !== root && !wanted.startsWith(root + sep)) return null
  try {
    if ((await stat(wanted)).isFile()) return wanted
  } catch {
    // Not a file: the app's page answers for it.
  }
  return null
}

const server = http.createServer(async (request, response) => {
  try {
    const { pathname } = new URL(request.url ?? '/', 'http://localhost')
    const file = (await fileFor(pathname)) ?? join(root, 'index.html')
    const body = await readFile(file)
    response.writeHead(200, {
      'Content-Type': types[extname(file)] ?? 'application/octet-stream',
      'Cache-Control': 'no-cache',
    })
    response.end(body)
  } catch {
    response.writeHead(500)
    response.end()
  }
})

server.listen(port, '127.0.0.1', () => {
  console.log(`mobile web export: http://127.0.0.1:${port} from ${root}`)
})
