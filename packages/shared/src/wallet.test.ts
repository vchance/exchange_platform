import { expect, test, vi } from 'vitest'

import { ApiFailure } from './api'
import { browserWallet, walletLink, walletOffered, walletPlatforms, type WalletApi } from './wallet'

const AGENTS = {
  iphone:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Mobile/15E148 Safari/604.1',
  iphoneChrome:
    'Mozilla/5.0 (iPhone; CPU iPhone OS 19_0 like Mac OS X) AppleWebKit/605.1.15 (KHTML, like Gecko) CriOS/140.0 Mobile/15E148 Safari/604.1',
  ipadDesktop:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Safari/605.1.15',
  macSafari:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/19.0 Safari/605.1.15',
  macChrome:
    'Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36',
  macFirefox: 'Mozilla/5.0 (Macintosh; Intel Mac OS X 10.15; rv:140.0) Gecko/20100101 Firefox/140.0',
  android:
    'Mozilla/5.0 (Linux; Android 16; Pixel 9) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Mobile Safari/537.36',
  windows:
    'Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/140.0.0.0 Safari/537.36',
  linux: 'Mozilla/5.0 (X11; Linux x86_64; rv:140.0) Gecko/20100101 Firefox/140.0',
}

test('the device decides the wallet: Apple on Apple devices, Google on Android, else none', () => {
  expect(browserWallet(AGENTS.iphone)).toBe('APPLE')
  expect(browserWallet(AGENTS.iphoneChrome)).toBe('APPLE')
  expect(browserWallet(AGENTS.ipadDesktop, 5)).toBe('APPLE')
  expect(browserWallet(AGENTS.macSafari, 0)).toBe('APPLE')
  expect(browserWallet(AGENTS.macChrome)).toBeNull()
  expect(browserWallet(AGENTS.macFirefox)).toBeNull()
  expect(browserWallet(AGENTS.android)).toBe('GOOGLE')
  expect(browserWallet(AGENTS.windows)).toBeNull()
  expect(browserWallet(AGENTS.linux)).toBeNull()
  expect(browserWallet('')).toBeNull()
})

test('a pass is offered for an agreement in force, on a platform the service has', () => {
  const both = ['APPLE', 'GOOGLE'] as const
  expect(walletOffered({ state: 'ACTIVE' }, both, 'APPLE')).toBe('APPLE')
  expect(walletOffered({ state: 'ACTIVE' }, both, 'GOOGLE')).toBe('GOOGLE')
  expect(walletOffered({ state: 'ACTIVE' }, ['GOOGLE'], 'APPLE')).toBeNull()
  expect(walletOffered({ state: 'ACTIVE' }, [], 'GOOGLE')).toBeNull()
  expect(walletOffered({ state: 'ACTIVE' }, both, null)).toBeNull()
  for (const state of ['DRAFT', 'NEGOTIATING', 'CLOSED'] as const) {
    expect(walletOffered({ state }, both, 'APPLE'), state).toBeNull()
  }
})

function fakeApi(meta: () => Promise<unknown>): WalletApi & { calls: string[] } {
  const calls: string[] = []
  return {
    calls,
    meta: vi.fn(meta) as unknown as WalletApi['meta'],
    appleWalletLink: async (id: string) => {
      calls.push(`apple ${id}`)
      return { url: 'https://app.test/v1/wallet/apple/pass?token=t', expires_at: null }
    },
    googleWalletLink: async (id: string) => {
      calls.push(`google ${id}`)
      return { url: 'https://pay.google.com/gp/v/save/jwt' }
    },
  }
}

test('the platforms are asked for once, and asked again after a failure', async () => {
  const api = fakeApi(async () => ({ wallet_platforms: ['GOOGLE'] }))
  expect(await walletPlatforms(api)).toEqual(['GOOGLE'])
  expect(await walletPlatforms(api)).toEqual(['GOOGLE'])
  expect(api.meta).toHaveBeenCalledTimes(1)

  let fail = true
  const flaky = fakeApi(async () => {
    if (fail) throw new ApiFailure('SERVICE_UNAVAILABLE', true)
    return { wallet_platforms: ['APPLE'] }
  })
  expect(await walletPlatforms(flaky)).toEqual([])
  fail = false
  expect(await walletPlatforms(flaky)).toEqual(['APPLE'])

  // A service from before Wallet passes names none.
  expect(await walletPlatforms(fakeApi(async () => ({})))).toEqual([])
})

test('the link comes from the platform the button is for', async () => {
  const api = fakeApi(async () => ({}))
  expect((await walletLink(api, 'x1', 'APPLE')).url).toContain('/v1/wallet/apple/pass')
  expect((await walletLink(api, 'x1', 'GOOGLE')).url).toContain('pay.google.com')
  expect(api.calls).toEqual(['apple x1', 'google x1'])
})
