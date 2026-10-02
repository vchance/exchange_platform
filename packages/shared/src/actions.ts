import type { Command, ErrorCode, ExchangeView } from '@exchange/api-client'
import { useCallback, useState } from 'react'

import { failureCode } from './api'

/**
 * Acting on an exchange from its screen. One thing is in progress at a time:
 * a "panel" is an action that has been opened but not sent, because it needs
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

/** The one call the action runner makes. */
export interface CommandSender {
  runCommand(id: string, expectedVersion: number, command: Command): Promise<ExchangeView>
}

export type CommandOutcome =
  | { ok: true; exchange: ExchangeView }
  /** `stale` means someone else changed the exchange first: what is on screen is out of date. */
  | { ok: false; code: ErrorCode; stale: boolean }

/**
 * Sends one command, naming the version it was based on (DESIGN.md §13.4),
 * and says what became of it. It never throws.
 */
export async function sendCommand(
  api: CommandSender,
  exchange: Pick<ExchangeView, 'id' | 'version'>,
  command: Command,
): Promise<CommandOutcome> {
  try {
    return { ok: true, exchange: await api.runCommand(exchange.id, exchange.version, command) }
  } catch (error) {
    const code = failureCode(error)
    return { ok: false, code, stale: code === 'VERSION_CONFLICT' || code === 'STALE_REVISION' }
  }
}

/**
 * Where the focus goes around a panel, for a client that has a focus to
 * move: remembered when a panel opens, put back when it is cancelled.
 */
export interface FocusKeeper {
  remember(): void
  restore(): void
}

export function useActions(
  api: CommandSender,
  exchange: ExchangeView,
  onChange: (exchange: ExchangeView) => void,
  reload: () => Promise<unknown>,
  focus?: FocusKeeper,
): Actions {
  const [panel, setPanel] = useState<string | null>(null)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [done, setDone] = useState(false)

  const open = useCallback(
    (name: string) => {
      focus?.remember()
      setFailure(null)
      setDone(false)
      setPanel(name)
    },
    [focus],
  )

  const close = useCallback(() => {
    setPanel(null)
    setFailure(null)
    focus?.restore()
  }, [focus])

  const { id, version } = exchange
  const run = useCallback(
    async (command: Command) => {
      setBusy(true)
      setFailure(null)
      setDone(false)
      const outcome = await sendCommand(api, { id, version }, command)
      if (outcome.ok) {
        onChange(outcome.exchange)
        setPanel(null)
        setDone(true)
      } else {
        setFailure(outcome.code)
        if (outcome.stale) {
          // Someone else got there first. Show what the exchange is now; the
          // refusal stays on screen to say why nothing happened.
          setPanel(null)
          await reload()
        }
      }
      setBusy(false)
      return outcome.ok
    },
    [api, id, version, onChange, reload],
  )

  return { busy, failure, done, panel, open, close, run }
}
