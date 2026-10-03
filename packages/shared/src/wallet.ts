import type { ErrorCode, ExchangeView } from '@yuppers/api-client'
import { useCallback, useEffect, useRef, useState } from 'react'

import { failureCode, type ExchangeApi, type WalletLink, type WalletPlatform } from './api'

/*
 * Wallet passes (DESIGN.md §11, §13.5): which wallet button to show, and
 * what pressing it does. The service decides which platforms it issues
 * passes for (`GET /v1/meta`); the device decides which one fits. A pass is
 * offered once the agreement is in force, and never where neither wallet
 * applies.
 */

export type WalletApi = Pick<ExchangeApi, 'meta' | 'appleWalletLink' | 'googleWalletLink'>

/**
 * Which wallet a browser's device has, from what it says of itself: Apple
 * Wallet on an iPhone, iPad or iPod (every browser there is Safari
 * underneath) and in Safari on a Mac, which adds passes to Wallet; Google
 * Wallet on Android. Anything else has neither.
 */
export function browserWallet(userAgent: string, touchPoints = 0): WalletPlatform | null {
  if (/Android/i.test(userAgent)) return 'GOOGLE'
  if (/iPhone|iPad|iPod/.test(userAgent)) return 'APPLE'
  if (/Macintosh/.test(userAgent)) {
    // An iPad asks for the desktop page and calls itself a Mac; its touch
    // screen gives it away.
    if (touchPoints > 1) return 'APPLE'
    const safari =
      /Version\/[\d.]+.*Safari\//.test(userAgent) &&
      !/Chrome|Chromium|CriOS|Edg\/|Firefox|FxiOS|OPR\//.test(userAgent)
    if (safari) return 'APPLE'
  }
  return null
}

/**
 * The wallet to offer for an exchange: the device's, when the service issues
 * passes for it, once something is agreed and while it is in force.
 */
export function walletOffered(
  exchange: Pick<ExchangeView, 'state'>,
  platforms: readonly WalletPlatform[],
  device: WalletPlatform | null,
): WalletPlatform | null {
  if (exchange.state !== 'ACTIVE' || device === null) return null
  return platforms.includes(device) ? device : null
}

const asked = new WeakMap<WalletApi, Promise<WalletPlatform[]>>()

/**
 * The platforms the service issues passes for, asked once per client and
 * remembered. None when the service cannot say, or is too old to know.
 */
export function walletPlatforms(api: WalletApi): Promise<WalletPlatform[]> {
  let found = asked.get(api)
  if (!found) {
    found = api.meta().then(
      (meta) => meta.wallet_platforms ?? [],
      () => {
        // Asked again next time rather than remembered as none.
        asked.delete(api)
        return []
      },
    )
    asked.set(api, found)
  }
  return found
}

/** The link that adds the caller's pass to `platform`'s wallet. */
export function walletLink(
  api: WalletApi,
  exchange: string,
  platform: WalletPlatform,
): Promise<WalletLink> {
  return platform === 'APPLE' ? api.appleWalletLink(exchange) : api.googleWalletLink(exchange)
}

export interface WalletButton {
  /** The wallet to offer, or `null` for no button. */
  platform: WalletPlatform | null
  busy: boolean
  failure: ErrorCode | null
  /** Gets the link and hands it to the client's way of opening it. */
  add(): Promise<void>
}

/**
 * The wallet button for an exchange on this device. `open` is the client's
 * way of following the link: the page goes to it in a browser, the app hands
 * it to the system, which opens Safari for an Apple pass and Google Wallet
 * or the browser for a Google one.
 */
export function useWalletButton(
  api: WalletApi,
  exchange: Pick<ExchangeView, 'id' | 'state'>,
  device: WalletPlatform | null,
  open: (url: string) => void | Promise<void>,
): WalletButton {
  const [platforms, setPlatforms] = useState<WalletPlatform[]>([])
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const pressed = useRef(false)
  const active = exchange.state === 'ACTIVE'

  useEffect(() => {
    // Nothing to ask where no button could be shown.
    if (device === null || !active) return
    let cancelled = false
    void walletPlatforms(api).then((found) => {
      if (!cancelled) setPlatforms(found)
    })
    return () => {
      cancelled = true
    }
  }, [api, device, active])

  const platform = walletOffered(exchange, platforms, device)
  const id = exchange.id
  const add = useCallback(async () => {
    if (platform === null || pressed.current) return
    pressed.current = true
    setBusy(true)
    setFailure(null)
    try {
      const link = await walletLink(api, id, platform)
      await open(link.url)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      pressed.current = false
      setBusy(false)
    }
  }, [api, id, platform, open])

  return { platform, busy, failure, add }
}
