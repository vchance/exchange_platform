import type { ExchangeView } from '@exchange/api-client'
import { expect, test } from 'vitest'

import { sendCommand } from './actions'
import { ApiFailure } from './api'

const ID = '0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70'
const after = { id: ID, version: 8 } as ExchangeView

test('a command names the version it was based on', async () => {
  const sent: unknown[] = []
  const api = {
    runCommand: async (...args: unknown[]) => {
      sent.push(args)
      return after
    },
  }
  const outcome = await sendCommand(api, { id: ID, version: 7 }, { type: 'PROPOSE_END' })
  expect(sent).toEqual([[ID, 7, { type: 'PROPOSE_END' }]])
  expect(outcome).toEqual({ ok: true, exchange: after })
})

test('a refusal because the exchange moved on says the screen is out of date', async () => {
  for (const code of ['VERSION_CONFLICT', 'STALE_REVISION'] as const) {
    const api = { runCommand: () => Promise.reject(new ApiFailure(code)) }
    expect(await sendCommand(api, after, { type: 'ACCEPT_END' })).toEqual({
      ok: false,
      code,
      stale: true,
    })
  }
})

test('any other refusal leaves what is on screen standing', async () => {
  for (const code of ['TOO_MANY_REQUESTS', 'WRONG_ACTOR', 'SERVICE_UNAVAILABLE'] as const) {
    const api = { runCommand: () => Promise.reject(new ApiFailure(code)) }
    expect(await sendCommand(api, after, { type: 'ACCEPT_END' })).toEqual({
      ok: false,
      code,
      stale: false,
    })
  }
})

test('something that is not a refusal from the service is reported as a fault here', async () => {
  const api = { runCommand: () => Promise.reject(new TypeError('boom')) }
  expect(await sendCommand(api, after, { type: 'ACCEPT_END' })).toEqual({
    ok: false,
    code: 'INTERNAL',
    stale: false,
  })
})
