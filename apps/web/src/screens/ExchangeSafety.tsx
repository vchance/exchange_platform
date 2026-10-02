import type { ErrorCode, ExchangeView } from '@exchange/api-client'
import type { ReportReason } from '@exchange/shared'
import { useEffect, useRef, useState } from 'react'

import { useI18n } from '../app/context'
import { Panel } from '../components/Panel'
import { ReportForm } from '../components/ReportForm'
import { Failure } from '../components/ui'
import type { Actions } from '../lib/actions'
import { failureCode } from '../lib/api'
import { safetyApi } from '../lib/safety'

interface Props {
  exchange: ExchangeView
  otherName: string
  actions: Actions
  reload(): Promise<unknown>
}

const REPORT = 'safety-report'
const BLOCK = 'safety-block'

type Outcome = 'reported' | 'blocked' | 'unblocked'

/**
 * Report and block, on every view of an exchange that has someone on the
 * other side (DESIGN.md §9). Each is said in full before it is done: what
 * happens, and that the other party is not told.
 */
export function ExchangeSafety(props: Props) {
  const { exchange } = props
  // Until someone has joined there is nobody to report and nobody to block.
  if (exchange.state === 'DRAFT' || exchange.counterparty === 'UNCLAIMED') return null
  return <Controls {...props} />
}

function Controls({ exchange, otherName, actions, reload }: Props) {
  const { wording, fmt } = useI18n()
  const w = wording.safety

  // Whether this person has blocked the other party. `null` until known.
  const [blocked, setBlocked] = useState<boolean | null>(null)
  // An exchange closed before anything was agreed shows no terms, and so no
  // names; the service still says who the other party was.
  const [written, setWritten] = useState('')
  const name = { name: written || otherName }
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [outcome, setOutcome] = useState<Outcome | null>(null)
  const announced = useRef<HTMLParagraphElement>(null)

  useEffect(() => {
    let cancelled = false
    safetyApi.blockStatus(exchange.id).then(
      (found) => {
        if (cancelled) return
        setBlocked(found.blocked)
        setWritten(found.name)
      },
      () => {
        // Not knowing, offer to block: blocking twice changes nothing.
        if (!cancelled) setBlocked(false)
      },
    )
    return () => {
      cancelled = true
    }
  }, [exchange.id])

  // What just happened takes the focus, because the button that did it may
  // no longer be on the page.
  useEffect(() => {
    if (outcome) announced.current?.focus()
  }, [outcome])

  function begin(panel: string) {
    setFailure(null)
    setOutcome(null)
    actions.open(panel)
  }

  async function attempt(work: () => Promise<void>, done: Outcome, panel: boolean) {
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

  const report = (reason: ReportReason, details: string | null) =>
    void attempt(() => safetyApi.reportExchange(exchange.id, reason, details), 'reported', true)

  const block = () =>
    void attempt(
      async () => {
        await safetyApi.block(exchange.id)
        setBlocked(true)
        // Blocking withdraws or declines what was waiting to be signed.
        void reload()
      },
      'blocked',
      true,
    )

  const unblock = () =>
    void attempt(
      async () => {
        await safetyApi.unblock(exchange.id)
        setBlocked(false)
      },
      'unblocked',
      false,
    )

  const waiting = busy || actions.busy
  const mine = actions.panel === REPORT || actions.panel === BLOCK

  return (
    <section aria-labelledby="safety-heading">
      <h2 id="safety-heading">{w.heading}</h2>

      {blocked && outcome !== 'blocked' && <p>{fmt(w.blocked, name)}</p>}
      {outcome && (
        <p className="notice" role="status" tabIndex={-1} ref={announced}>
          {outcome === 'reported' && w.reportSent}
          {outcome === 'blocked' && fmt(w.blocked, name)}
          {outcome === 'unblocked' && fmt(w.unblocked, name)}
        </p>
      )}

      <div className="actions">
        <button
          type="button"
          aria-expanded={actions.panel === REPORT}
          disabled={waiting}
          onClick={() => begin(REPORT)}
        >
          {w.report}
        </button>
        {blocked === false && (
          <button
            type="button"
            aria-expanded={actions.panel === BLOCK}
            disabled={waiting}
            onClick={() => begin(BLOCK)}
          >
            {fmt(w.block, name)}
          </button>
        )}
        {blocked === true && (
          <button type="button" disabled={waiting} onClick={unblock}>
            {fmt(w.unblock, name)}
          </button>
        )}
      </div>
      {!mine && <Failure code={failure} />}

      {actions.panel === REPORT && (
        <Panel title={w.report}>
          <ReportForm
            intro={fmt(w.reportIntro, name)}
            busy={busy}
            failure={failure}
            onSend={report}
            onCancel={actions.close}
          />
        </Panel>
      )}

      {actions.panel === BLOCK && (
        <Panel title={fmt(w.block, name)}>
          <p>{fmt(w.blockStops, name)}</p>
          <p>{w.blockEnds}</p>
          <p>{w.blockKeeps}</p>
          <p>{fmt(w.blockQuiet, name)}</p>
          <Failure code={failure} />
          <div className="actions">
            <button type="button" className="primary" disabled={busy} onClick={block}>
              {fmt(w.confirmBlock, name)}
            </button>
            <button type="button" disabled={busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}
    </section>
  )
}
