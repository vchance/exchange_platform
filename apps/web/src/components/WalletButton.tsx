import type { ExchangeView as Exchange } from '@yuppers/api-client'
import { browserWallet, useWalletButton, type WalletPlatform } from '@yuppers/shared'
import { useMemo } from 'react'

import { useI18n } from '../app/context'
import { api } from '../lib/api'
import { Failure } from './ui'

/** The browser's own wallet, if its device has one. */
function thisDevice(): WalletPlatform | null {
  if (typeof navigator === 'undefined') return null
  return browserWallet(navigator.userAgent, navigator.maxTouchPoints ?? 0)
}

/** Goes to the link: Safari opens an Apple pass in Wallet, Android Google's. */
function follow(url: string): void {
  window.location.assign(url)
}

interface Props {
  exchange: Exchange
  /** For tests: the device's wallet and how a link is followed. */
  device?: WalletPlatform | null
  open?: (url: string) => void
}

/**
 * "Add to Apple Wallet" on an Apple device, "Add to Google Wallet" on
 * Android, for an agreement in force (DESIGN.md §11, §13.5); nothing where
 * neither wallet applies or the service issues no passes for it. A plain
 * button in the product's own words: Apple's and Google's badge artwork
 * comes under their brand terms, which the owner accepts when the accounts
 * exist (docs/wallet.md).
 */
export function WalletButton({ exchange, device, open = follow }: Props) {
  const { wording, fmt } = useI18n()
  const detected = useMemo(() => (device === undefined ? thisDevice() : device), [device])
  const wallet = useWalletButton(api, exchange, detected, open)
  if (wallet.platform === null) return null
  const w = wording.wallet
  return (
    <div className="wallet">
      <p className="hint">{fmt(w.intro, { productName: wording.productName })}</p>
      <div className="actions">
        <button type="button" disabled={wallet.busy} onClick={() => void wallet.add()}>
          {wallet.busy ? w.adding : wallet.platform === 'APPLE' ? w.addToApple : w.addToGoogle}
        </button>
      </div>
      <Failure code={wallet.failure} />
    </div>
  )
}
