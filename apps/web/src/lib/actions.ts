import type { Command, ErrorCode, ExchangeView } from '@exchange/api-client'
import { useCallback, useRef, useState } from 'react'

import { api, failureCode } from './api'

/**
 * Acting on an exchange from its page. One thing is in progress at a time: a
 * "panel" is an action that has been opened but not sent, because it needs
 * something first: a reason, the consent step, or a second look at something
 * that cannot be undone.
 */
export interface Actions {
  busy: boolean
  /** Why the last action was refused, until the next one is tried. */
  failure: ErrorCode | null
  /** Set after an action succeeds, so the change can be announced. */
  done: boolean
  /** Which panel is open, by a name its owner chooses. */
  panel: string | null
  open(panel: string): void
  close(): void
  /** Sends a command based on the version on screen. Resolves to whether it took effect. */
  run(command: Command): Promise<boolean>
}

export function useActions(
  exchange: ExchangeView,
  onChange: (exchange: ExchangeView) => void,
  reload: () => Promise<unknown>,
): Actions {
  const [panel, setPanel] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [done, setDone] = useState(false)
  // Where the keyboard was when a panel opened, to put it back on cancel.
  const opener = useRef<HTMLElement | null>(null)

  const open = useCallback((name: string) => {
    opener.current = document.activeElement instanceof HTMLElement ? document.activeElement : null
    setFailure(null)
    setDone(false)
    setPanel(name)
  }, [])

  const close = useCallback(() => {
    setPanel(null)
    setFailure(null)
    opener.current?.focus()
  }, [])

  const { id, version } = exchange
  const run = useCallback(
    async (command: Command) => {
      setBusy(true)
      setFailure(null)
      setDone(false)
      try {
        // Every change names the version it was based on (DESIGN.md §13.4).
        onChange(await api.runCommand(id, version, command))
        setPanel(null)
        setDone(true)
        return true
      } catch (error) {
        const code = failureCode(error)
        setFailure(code)
        if (code === 'VERSION_CONFLICT' || code === 'STALE_REVISION') {
          // Someone else got there first. Show what the exchange is now; the
          // refusal stays on screen to say why nothing happened.
          setPanel(null)
          await reload()
        }
        return false
      } finally {
        setBusy(false)
      }
    },
    [id, version, onChange, reload],
  )

  return { busy, failure, done, panel, open, close, run }
}
