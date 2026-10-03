import { expect, test } from 'vitest'

import type { ApiClient } from '@yuppers/api-client'

import { createExchangeApi } from './api'
import { clientHeader, compareVersions, isClientTooOld, parseVersion } from './client-version'

test('a client names itself as kind and version', () => {
  expect(clientHeader({ name: 'ios', version: '1.4.0' })).toBe('ios/1.4.0')
})

test('a client may add the commit it was built from, which the version check ignores', () => {
  const commit = '0123456789abcdef0123456789abcdef01234567'
  expect(clientHeader({ name: 'web', version: '0.1.0', commit })).toBe('web/0.1.0+0123456')
  expect(clientHeader({ name: 'web', version: '0.1.0', commit: 'unknown' })).toBe('web/0.1.0')
  expect(clientHeader({ name: 'web', version: '0.1.0', commit: null })).toBe('web/0.1.0')
  const minimums = { web: '2.1', ios: null, android: null }
  expect(isClientTooOld(minimums, { name: 'web', version: '2.0.5', commit })).toBe(true)
  expect(isClientTooOld(minimums, { name: 'web', version: '2.1.0', commit })).toBe(false)
})

test('versions are dotted whole numbers compared part by part', () => {
  expect(parseVersion('1.4.0')).toEqual([1, 4, 0])
  expect(parseVersion(' 2 ')).toEqual([2])
  for (const bad of ['', 'v1', '1..0', '1.0-beta', 'one']) expect(parseVersion(bad), bad).toBeNull()
  expect(compareVersions([1, 4], [1, 4, 0])).toBe(0)
  expect(compareVersions([1, 10], [1, 4])).toBeGreaterThan(0)
  expect(compareVersions([1, 3, 9], [1, 4])).toBeLessThan(0)
})

test('a client is too old only below a minimum the service names for its kind', () => {
  const minimums = { web: '2.1', ios: '1.4.0', android: null }
  expect(isClientTooOld(minimums, { name: 'ios', version: '1.3.9' })).toBe(true)
  expect(isClientTooOld(minimums, { name: 'ios', version: '1.4' })).toBe(false)
  expect(isClientTooOld(minimums, { name: 'ios', version: '1.10.0' })).toBe(false)
  expect(isClientTooOld(minimums, { name: 'web', version: '2.0.5' })).toBe(true)
  expect(isClientTooOld(minimums, { name: 'android', version: '0.0.1' })).toBe(false)
  // Nothing is required unless said, and what cannot be read is not refused.
  expect(isClientTooOld({ web: null, ios: null, android: null }, { name: 'web', version: '0' })).toBe(false)
  expect(isClientTooOld(minimums, { name: 'ios', version: 'dev' })).toBe(false)
})

test('a client names its build on every request, unless it cannot read its version', async () => {
  const sent: (Record<string, string> | undefined)[] = []
  const call = async (_path: string, init: { headers?: Record<string, string> } = {}) => {
    sent.push(init.headers)
    return { data: {}, response: { ok: true, status: 200 } }
  }
  const client = { GET: call } as unknown as ApiClient
  const session = { delivery: 'TOKEN', token: () => null } as const
  const header = async (identity?: { name: 'ios'; version: string }) => {
    await createExchangeApi({ client, session, identity }).meta()
    return sent.at(-1)?.['X-Client-Version']
  }
  expect(await header({ name: 'ios', version: '1.4.0' })).toBe('ios/1.4.0')
  expect(await header({ name: 'ios', version: '' })).toBeUndefined()
  expect(await header({ name: 'ios', version: 'unknown' })).toBeUndefined()
  expect(await header(undefined)).toBeUndefined()
})
