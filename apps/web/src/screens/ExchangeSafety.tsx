import type { ExchangeView } from '@yuppers/api-client'
import {
  blockLeavesAgreement,
  CLOSE_AFTER_BLOCK_PANEL,
  hasOtherParty,
  isUnconfirmedClaimant,
  offersCloseAfterBlock,
  useExchangeSafety,
} from '@yuppers/shared'
import { useEffect, useRef } from 'react'

import { useI18n } from '../app/context'
import { navigate } from '../app/router'
import { paths } from '../app/routes'
import { Panel } from '../components/Panel'
import { ReportForm } from '../components/ReportForm'
import { Failure } from '../components/ui'
import type { Actions } from '../lib/actions'
import { safetyApi } from '../lib/safety'
import { RequestClosePanel } from './Ending'

interface Props {
  exchange: ExchangeView
  otherName: string
  actions: Actions
  reload(): Promise<unknown>
}

/**
 * Report and block, on every view of an exchange that has someone on the
 * other side (DESIGN.md §9). Each is said in full before it is done: what
 * happens, and that the other party is not told.
 *
 * A block leaves an agreement in force standing, and the person blocked can
 * still act in it. Blocking from one says so, and once the block is made,
 * closing without agreement is offered right here. It opens the same request
 * to close as "Ending the agreement" does, after the block rather than as
 * part of it: the block is silent and immediate, while a request to close
 * is seen by the other party, has its own window and its own statement, and
 * deserves its own confirmation (DESIGN.md §5.3).
 */
export function ExchangeSafety(props: Props) {
  const { exchange } = props
  // Until someone has joined there is nobody to report and nobody to block.
  if (!hasOtherParty(exchange)) return null
  return <Controls {...props} />
}

function Controls({ exchange, otherName, actions, reload }: Props) {
  const { wording, fmt } = useI18n()
  const w = wording.safety

  // Someone the initiator has not confirmed cannot decline. Blocking takes
  // them out of the exchange instead (DESIGN.md §8, §9), and it is gone for
  // them. The block is listed, and can be lifted, with the account.
  const leaves = isUnconfirmedClaimant(exchange)
  const safety = useExchangeSafety(
    safetyApi,
    exchange.id,
    actions,
    reload,
    leaves ? () => navigate(paths.account) : undefined,
  )
  const { blocked, busy, failure, outcome } = safety
  // An exchange closed before anything was agreed shows no terms, and so no
  // names; the service still says who the other party was.
  const name = { name: safety.name || otherName }
  const announced = useRef<HTMLParagraphElement>(null)

  // What just happened takes the focus, because the button that did it may
  // no longer be on the page.
  useEffect(() => {
    if (outcome) announced.current?.focus()
  }, [outcome])

  const waiting = busy || actions.busy
  const offerClose = offersCloseAfterBlock(exchange, blocked)

  return (
    <section aria-labelledby="safety-heading">
      <h2 id="safety-heading">{w.heading}</h2>

      {blocked && outcome !== 'blocked' && <p>{fmt(w.blocked, name)}</p>}
      {outcome && (
        <p className="notice" tabIndex={-1} ref={announced}>
          {outcome === 'reported' && w.reportSent}
          {outcome === 'blocked' && fmt(w.blocked, name)}
          {outcome === 'unblocked' && fmt(w.unblocked, name)}
        </p>
      )}
      {offerClose && (
        <>
          <p>{fmt(w.blockedInForce, name)}</p>
          <div className="actions">
            <button
              type="button"
              aria-expanded={actions.panel === CLOSE_AFTER_BLOCK_PANEL}
              disabled={waiting}
              onClick={() => actions.open(CLOSE_AFTER_BLOCK_PANEL)}
            >
              {wording.exchange.requestClose}
            </button>
          </div>
          {actions.panel === CLOSE_AFTER_BLOCK_PANEL && (
            <RequestClosePanel otherName={name.name} actions={actions} />
          )}
        </>
      )}

      <div className="actions">
        <button
          type="button"
          aria-expanded={safety.panel === 'report'}
          disabled={waiting}
          onClick={() => safety.begin('report')}
        >
          {w.report}
        </button>
        {blocked === false && (
          <button
            type="button"
            aria-expanded={safety.panel === 'block'}
            disabled={waiting}
            onClick={() => safety.begin('block')}
          >
            {fmt(w.block, name)}
          </button>
        )}
        {blocked === true && (
          <button type="button" disabled={waiting} onClick={safety.unblock}>
            {fmt(w.unblock, name)}
          </button>
        )}
      </div>
      {safety.panel === null && <Failure code={failure} />}

      {safety.panel === 'report' && (
        <Panel title={w.report}>
          <ReportForm
            intro={fmt(w.reportIntro, name)}
            busy={busy}
            failure={failure}
            onSend={safety.report}
            onCancel={actions.close}
          />
        </Panel>
      )}

      {safety.panel === 'block' && (
        <Panel title={fmt(w.block, name)}>
          <p>{fmt(w.blockStops, name)}</p>
          {leaves ? (
            <p>{wording.claimant.blockLeaves}</p>
          ) : (
            <>
              <p>{w.blockEnds}</p>
              <p>{w.blockKeeps}</p>
              {blockLeavesAgreement(exchange) && (
                <>
                  <p>{fmt(w.blockInForce, name)}</p>
                  <p>{fmt(w.blockThenClose, name)}</p>
                </>
              )}
            </>
          )}
          <p>{fmt(w.blockQuiet, name)}</p>
          <Failure code={failure} />
          <div className="actions">
            <button type="button" className="primary" disabled={busy} onClick={safety.block}>
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
