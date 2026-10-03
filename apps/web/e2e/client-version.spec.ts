import { spawn, type ChildProcess } from 'node:child_process'
import { openSync, readFileSync } from 'node:fs'
import { dirname, resolve } from 'node:path'

import { expect, test } from './support/fixtures'
import { apiBinary, apiEnvironment, apiLog, outdatedPort, repoRoot, webRoot } from './support/env'
import { en } from './support/wording'

/*
 * A service that no longer accepts changes from this build of the web app
 * (README, "A client that is too old"). It is an API process of its own,
 * started for this file, with a minimum above the web app's version; the
 * database is the same one.
 */

const origin = `http://127.0.0.1:${outdatedPort}`
const version = (
  JSON.parse(readFileSync(resolve(webRoot, 'package.json'), 'utf8')) as { version: string }
).version

let api: ChildProcess | null = null

test.beforeAll(async () => {
  const log = openSync(resolve(dirname(apiLog), 'outdated-api.log'), 'w')
  api = spawn(apiBinary, [], {
    cwd: repoRoot,
    env: { ...process.env, ...apiEnvironment(outdatedPort), MIN_CLIENT_VERSION_WEB: '999.0.0' },
    stdio: ['ignore', log, log],
  })
  const deadline = Date.now() + 30_000
  while (Date.now() < deadline) {
    if (api.exitCode !== null) throw new Error(`the API exited with ${api.exitCode}`)
    try {
      if ((await fetch(`${origin}/readyz`)).ok) return
    } catch {
      // Not listening yet.
    }
    await new Promise((settle) => setTimeout(settle, 200))
  }
  throw new Error(`the API on ${origin} did not become ready`)
})

test.afterAll(async () => {
  if (!api || api.exitCode !== null) return
  const exited = new Promise((settle) => api!.once('exit', settle))
  api.kill('SIGTERM')
  await exited
})

test('a build older than the service accepts shows the update page, and its changes are refused', async ({
  page,
}) => {
  const named = page.waitForRequest((request) => request.url().endsWith('/v1/meta'))
  await page.goto(`${origin}/`)

  // The page says it must be reloaded, and offers nothing else.
  await expect(page.getByRole('heading', { name: en.errors.CLIENT_TOO_OLD, level: 1 })).toBeVisible()
  await expect(page.getByText(en.service.outdatedWeb)).toBeVisible()
  await expect(page.getByRole('button', { name: en.service.reload })).toBeVisible()
  await expect(page.getByRole('heading', { name: en.signIn.title })).toHaveCount(0)
  await expect(page.getByRole('button', { name: en.signIn.sendCode })).toHaveCount(0)

  // The app names its build on every request.
  const header = (await named).headers()['x-client-version']
  expect(header).toBe(`web/${version}`)

  // Reading is still allowed; a change from that build is refused.
  const meta = await page.request.get(`${origin}/v1/meta`, { headers: { 'X-Client-Version': header } })
  expect(meta.status()).toBe(200)
  expect((await meta.json()).minimum_client_versions.web).toBe('999.0.0')

  const change = await page.request.post(`${origin}/v1/auth/codes`, {
    headers: { 'X-Client-Version': header, Origin: origin },
    data: { identifier: 'outdated@example.test' },
  })
  expect(change.status()).toBe(426)
  expect((await change.json()).code).toBe('CLIENT_TOO_OLD')
})
