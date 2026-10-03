import type { components, ErrorCode, ExchangeView as Exchange } from '@yuppers/api-client'
import { leaveExchange } from '@yuppers/shared'
import { useState } from 'react'

import { useI18n } from '../app/context'
import { navigate } from '../app/router'
import { paths } from '../app/routes'
import { Panel } from '../components/Panel'
import { Failure } from '../components/ui'
import type { Actions } from '../lib/actions'
import { api } from '../lib/api'

/*
 * Someone opened an invitation that named nobody, and the initiator has not
 * yet said whether it is the person they invited (DESIGN.md §8). The
 * initiator confirms them or removes them; until then the claimant can sign
 * or leave, and nothing else.
 */

const REJECT = 'claimant-reject'
const LEAVE = 'claimant-leave'

interface ConfirmProps {
  exchange: Exchange
  claimant: components['schemas']['Claimant']
  actions: Actions
  /** The claimant has been removed, and the place is free again. */
  onRejected(): void
}

/**
 * The initiator's question: is this who you invited? Yes confirms them. No
 * removes them, which is said in full before it is done, because it cannot
 * be undone and voids anything they signed.
 */
export function ConfirmClaimant({ exchange, claimant, actions, onRejected }: ConfirmProps) {
  const { wording, fmt } = useI18n()
  const w = wording.exchange
  const c = wording.claimant
  const signed = exchange.open_revision?.accepted_by.includes('B') ?? false

  async function reject() {
    if (await actions.run({ type: 'REJECT_COUNTERPARTY' })) onRejected()
  }

  return (
    <section className="card" aria-labelledby="claimed-heading">
      <h2 id="claimed-heading">{w.claimedHeading}</h2>
      <p>{fmt(w.claimedBody, { name: claimant.display_name, identifier: claimant.identifier })}</p>
      {signed && <p>{w.claimedSigned}</p>}
      <p className="hint">{w.notThem}</p>
      <div className="actions">
        <button
          type="button"
          className="primary"
          disabled={actions.busy}
          onClick={() => void actions.run({ type: 'CONFIRM_COUNTERPARTY' })}
        >
          {w.confirmCounterparty}
        </button>
        <button
          type="button"
          aria-expanded={actions.panel === REJECT}
          disabled={actions.busy}
          onClick={() => actions.open(REJECT)}
        >
          {c.reject}
        </button>
      </div>

      {actions.panel === REJECT && (
        <Panel title={c.rejectTitle}>
          <p>{fmt(c.rejectRemoves, { name: claimant.display_name })}</p>
          <p>{c.rejectVoids}</p>
          <p>{c.rejectKeeps}</p>
          <p>{c.rejectQuiet}</p>
          <Failure code={actions.failure} />
          <div className="actions">
            <button
              type="button"
              className="primary"
              disabled={actions.busy}
              onClick={() => void reject()}
            >
              {c.confirmReject}
            </button>
            <button type="button" disabled={actions.busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}
    </section>
  )
}

interface WaitingProps {
  exchange: Exchange
  /** The initiator, as the proposal names them. */
  otherName: string
  actions: Actions
}

/**
 * What a claimant is told while the initiator has not confirmed them: that
 * they can sign, that they cannot yet decline or propose changes, and that
 * they can leave. Leaving is their only way out, so it is always offered.
 */
export function ClaimantWaiting({ exchange, otherName, actions }: WaitingProps) {
  const { wording, fmt } = useI18n()
  const w = wording.exchange
  const c = wording.claimant
  const signed = exchange.open_revision?.accepted_by.includes('B') ?? false
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)

  async function leave() {
    setBusy(true)
    setFailure(null)
    const refused = await leaveExchange(api, exchange.id)
    // The exchange no longer exists for this person; there is nothing here
    // to come back to.
    if (refused === null) navigate(paths.home, { replace: true })
    else {
      setFailure(refused)
      setBusy(false)
    }
  }

  return (
    <div className="notice">
      <p>
        {fmt(signed ? w.waitingConfirmationSigned : w.waitingConfirmation, { name: otherName })}
      </p>
      <p>{c.limits}</p>
      <div className="actions">
        <button
          type="button"
          aria-expanded={actions.panel === LEAVE}
          disabled={busy || actions.busy}
          onClick={() => {
            setFailure(null)
            actions.open(LEAVE)
          }}
        >
          {c.leave}
        </button>
      </div>

      {actions.panel === LEAVE && (
        <Panel title={c.leave}>
          <p>{fmt(c.leaveText, { name: otherName })}</p>
          {signed && <p>{c.leaveVoids}</p>}
          <Failure code={failure} />
          <div className="actions">
            <button type="button" className="primary" disabled={busy} onClick={() => void leave()}>
              {c.confirmLeave}
            </button>
            <button type="button" disabled={busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}
    </div>
  )
}
