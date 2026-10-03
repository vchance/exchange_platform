import type { ExchangeView } from '@exchange/api-client'
import { useActions as useSharedActions, type Actions, type FocusKeeper } from '@exchange/shared'

import { api } from './api'

export type { Actions }

// Where the keyboard was when a panel opened, to put it back on cancel.
let opener: HTMLElement | null = null
const focus: FocusKeeper = {
  remember() {
    opener = document.activeElement instanceof HTMLElement ? document.activeElement : null
  },
  restore() {
    restoreFocus()
  },
}

/**
 * Puts the keyboard back on whatever opened the last panel, if it is still
 * on the page. Says whether it could.
 */
export function restoreFocus(): boolean {
  if (!opener?.isConnected) return false
  opener.focus()
  return true
}

/**
 * Acting on an exchange from its page. The runner itself is shared with the
 * mobile app; what the web adds is keeping track of the keyboard focus.
 */
export function useActions(
  exchange: ExchangeView,
  onChange: (exchange: ExchangeView) => void,
  reload: () => Promise<unknown>,
): Actions {
  return useSharedActions(api, exchange, onChange, reload, focus)
}
