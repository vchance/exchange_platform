import type { ErrorCode } from '@exchange/api-client'
import { useCallback, useEffect, useRef, useState } from 'react'

import type { Actions } from './actions'
import { failureCode, type BlockedPerson, type ExchangeApi } from './api'
import type { ReportReason } from './safety'

/*
 * Reporting and blocking from a screen (DESIGN.md §9). What is asked of the
 * service, in what order, and what the person is then told is the same on the
 * web and in the mobile app, and is decided here; each lays it out with its
 * own components.
 *
 * Nothing here tells the person reported or blocked anything, and a report
 * tells its sender only that it was received.
 */

/** The names of the two panels, among the exchange screen's other panels. */
export const SAFETY_PANELS = { report: 'safety-report', block: 'safety-block' } as const
export type SafetyPanel = keyof typeof SAFETY_PANELS

/** What was just done, to be said once it is. */
export type SafetyOutcome = 'reported' | 'blocked' | 'unblocked'

export interface ExchangeSafety {
  /** Whether this person has blocked the other party. `null` until known. */
  blocked: boolean | null
  /**
   * The other party's name as the service gives it, or `''` until it has. An
   * exchange closed before anything was agreed shows no terms, and so no
   * names; the service still says who the other party was.
   */
  name: string
  busy: boolean
  failure: ErrorCode | null
  outcome: SafetyOutcome | null
  /** Which of the two panels is open, if either. */
  panel: SafetyPanel | null
  /** Opens a panel: nothing is sent until what it says has been read. */
  begin(panel: SafetyPanel): void
  report(reason: ReportReason, details: string | null): void
  block(): void
  unblock(): void
}

/**
 * Report and block on an exchange that has someone on the other side.
 * `actions` is the exchange screen's own runner, so that only one panel is
 * open at a time; `reload` reads the exchange again, because a block
 * withdraws or declines what was waiting to be signed.
 *
 * `onLeft` is given when the person is in the exchange only as a claimant
 * the initiator has not confirmed (DESIGN.md §8). They cannot decline, so a
 * block takes them out of the exchange instead: there is then nothing to
 * read again, and `onLeft` takes them somewhere that still exists.
 */
export function useExchangeSafety(
  api: Pick<ExchangeApi, 'reportExchange' | 'blockStatus' | 'block' | 'unblock'>,
  exchangeId: string,
  actions: Actions,
  reload: () => Promise<unknown>,
  onLeft?: () => void,
): ExchangeSafety {
  const [blocked, setBlocked] = useState<boolean | null>(null)
  const [name, setName] = useState('')
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [outcome, setOutcome] = useState<SafetyOutcome | null>(null)

  useEffect(() => {
    let cancelled = false
    api.blockStatus(exchangeId).then(
      (found) => {
        if (cancelled) return
        setBlocked(found.blocked)
        setName(found.name)
      },
      () => {
        // Not knowing, offer to block: blocking twice changes nothing.
        if (!cancelled) setBlocked(false)
      },
    )
    return () => {
      cancelled = true
    }
  }, [api, exchangeId])

  function begin(panel: SafetyPanel) {
    setFailure(null)
    setOutcome(null)
    actions.open(SAFETY_PANELS[panel])
  }

  async function attempt(work: () => Promise<void>, done: SafetyOutcome, panel: boolean) {
    setBusy(true)
    setFailure(null)
    setOutcome(null)
    try {
      await work()
      if (panel) actions.close()
      setOutcome(done)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setBusy(false)
    }
  }

  return {
    blocked,
    name,
    busy,
    failure,
    outcome,
    panel:
      actions.panel === SAFETY_PANELS.report
        ? 'report'
        : actions.panel === SAFETY_PANELS.block
          ? 'block'
          : null,
    begin,
    report: (reason, details) =>
      void attempt(() => api.reportExchange(exchangeId, reason, details), 'reported', true),
    block: () =>
      void attempt(
        async () => {
          await api.block(exchangeId)
          if (onLeft) {
            onLeft()
            return
          }
          setBlocked(true)
          // Blocking withdraws or declines what was waiting to be signed.
          void reload()
        },
        'blocked',
        true,
      ),
    unblock: () =>
      void attempt(
        async () => {
          await api.unblock(exchangeId)
          setBlocked(false)
        },
        'unblocked',
        false,
      ),
  }
}

export interface InvitationReporting {
  /** Whether the form is open. */
  open: boolean
  busy: boolean
  failure: ErrorCode | null
  /** Set once a report has been received; all its sender is told. */
  sent: boolean
  begin(): void
  cancel(): void
  send(reason: ReportReason, details: string | null): void
}

/**
 * Reporting a proposal from its invitation link, before signing in or
 * without ever doing so. The link's token is the proof of having received
 * the proposal, exactly as it is for reading it; it is sent in the request
 * body and kept nowhere.
 */
export function useInvitationReport(
  api: Pick<ExchangeApi, 'reportInvitation'>,
  token: string,
): InvitationReporting {
  const [open, setOpen] = useState(false)
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [sent, setSent] = useState(false)

  async function send(reason: ReportReason, details: string | null) {
    setBusy(true)
    setFailure(null)
    try {
      await api.reportInvitation(token, reason, details)
      setOpen(false)
      setSent(true)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setBusy(false)
    }
  }

  return {
    open,
    busy,
    failure,
    sent,
    begin() {
      setFailure(null)
      setSent(false)
      setOpen(true)
    },
    cancel: () => setOpen(false),
    send: (reason, details) => void send(reason, details),
  }
}

export interface BlockedPeopleList {
  /** `null` until the list has been read. */
  people: BlockedPerson[] | null
  failure: ErrorCode | null
  /** The exchange through which someone is being unblocked right now. */
  busy: string | null
  /** The name of whoever was just unblocked, as it was shown. */
  unblocked: string | null
  /** Reads the list, or reads it again. */
  load(): void
  /** Unblocks someone; `shownAs` is the name they were listed under. */
  unblock(person: BlockedPerson, shownAs: string): void
}

/**
 * The people this account has blocked, and unblocking each. A person is known
 * here only as an exchange shared with them names them.
 */
export function useBlockedPeople(
  api: Pick<ExchangeApi, 'blockedPeople' | 'unblock'>,
): BlockedPeopleList {
  const [people, setPeople] = useState<BlockedPerson[] | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [busy, setBusy] = useState<string | null>(null)
  const [unblocked, setUnblocked] = useState<string | null>(null)
  // Only the reading asked for last may answer, and none after the screen has gone.
  const asked = useRef(0)
  useEffect(
    () => () => {
      asked.current += 1
    },
    [],
  )

  const load = useCallback(() => {
    const mine = (asked.current += 1)
    api.blockedPeople().then(
      (found) => {
        if (mine !== asked.current) return
        setPeople(found)
        setFailure(null)
      },
      (error: unknown) => {
        if (mine === asked.current) setFailure(failureCode(error))
      },
    )
  }, [api])

  async function unblock(person: BlockedPerson, shownAs: string) {
    setBusy(person.exchange_id)
    setFailure(null)
    setUnblocked(null)
    try {
      await api.unblock(person.exchange_id)
      // A reading still on its way was made before this and would bring them back.
      asked.current += 1
      setPeople(
        (listed) => listed?.filter((other) => other.exchange_id !== person.exchange_id) ?? null,
      )
      setUnblocked(shownAs)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setBusy(null)
    }
  }

  return {
    people,
    failure,
    busy,
    unblocked,
    load,
    unblock: (person, shownAs) => void unblock(person, shownAs),
  }
}
