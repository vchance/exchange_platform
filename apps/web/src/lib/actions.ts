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
    opener?.focus()
  },
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
