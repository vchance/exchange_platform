import type { components, ErrorCode, ExchangeView as Exchange } from '@exchange/api-client'
import type { ClosedReason } from '@exchange/shared'
import { useEffect, useRef, useState, type FormEvent } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { Consent } from '../components/Consent'
import { InvitationFor, InvitationLink } from '../components/InvitationLink'
import { Panel } from '../components/Panel'
import { TermsView } from '../components/TermsView'
import { Failure, Notice, PageHeading, Written } from '../components/ui'
import { useActions, type Actions } from '../lib/actions'
import { api, failureCode, type RevisionView, type Slot } from '../lib/api'
import { consentShown } from '../lib/consent'
import { Ending } from './Ending'
import { Fulfillment } from './Fulfillment'
import { History } from './History'

type Status = components['schemas']['Status']

/** How often an open exchange is checked for what the other party has done. */
const CHECK_EVERY_MS = 20_000

interface Props {
  exchange: Exchange
  /** A just-issued invitation token, to show once. */
  issued: string | null
  onIssued(token: string | null): void
  onChange(exchange: Exchange): void
  reload(): Promise<Exchange | null>
}

/**
 * An exchange as one of its two parties sees it: its state, the revision
 * waiting to be signed, the agreement in force and where each contribution
 * stands, and every action open to this party right now. The service decides
 * what is allowed; this offers what should be, and shows the refusal if it
 * was wrong.
 */
export function ExchangeView({ exchange, issued, onIssued, onChange, reload }: Props) {
  const { wording, fmt } = useI18n()
  const w = wording.exchange
  const actions = useActions(exchange, onChange, reload)

  const you = exchange.you
  const other: Slot = you === 'A' ? 'B' : 'A'
  const open = exchange.open_revision ?? null
  const inForce = exchange.in_force_revision ?? null
  const latest = (open ?? inForce)?.terms
  const writtenName = latest ? (other === 'A' ? latest.party_a_name : latest.party_b_name) : ''
  // In a sentence, someone with no name yet is "the other party".
  const otherName = writtenName || wording.party.other
  const active = exchange.state === 'ACTIVE'

  // A newer version found while the person is in the middle of something is
  // held back and offered, not swapped in under them.
  const [newer, setNewer] = useState<Exchange | null>(null)
  const [refreshed, setRefreshed] = useState(false)
  const current = useRef({ exchange, engaged: false })
  useEffect(() => {
    current.current = { exchange, engaged: actions.panel !== null || actions.busy }
  })

  useEffect(() => {
    if (exchange.state === 'CLOSED') return
    let cancelled = false
    async function check() {
      if (document.visibilityState !== 'visible' || current.current.engaged) return
      let found: Exchange
      try {
        found = await api.getExchange(exchange.id)
      } catch {
        return
      }
      if (cancelled || found.version === current.current.exchange.version) return
      if (current.current.engaged) setNewer(found)
      else {
        onChange(found)
        setRefreshed(true)
      }
    }
    const timer = window.setInterval(check, CHECK_EVERY_MS)
    document.addEventListener('visibilitychange', check)
    return () => {
      cancelled = true
      window.clearInterval(timer)
      document.removeEventListener('visibilitychange', check)
    }
  }, [exchange.id, exchange.state, onChange])

  // A refusal with no panel left to show it in, such as one that reloaded the
  // exchange, is shown at the top, and the top is brought into view: the
  // person may be far down the page, looking at what they just pressed.
  const refused = actions.panel === null ? actions.failure : null
  const refusal = useRef<HTMLDivElement>(null)
  useEffect(() => {
    if (refused) refusal.current?.scrollIntoView({ block: 'center' })
  }, [refused])

  const statuses = new Map<string, Status>(
    exchange.contributions.map((contribution) => [contribution.id, contribution.status]),
  )
  const remaining = inForce
    ? inForce.terms.contributions.filter((contribution) => {
        const status = statuses.get(contribution.id)
        return contribution.required && status !== 'ACCEPTED' && status !== 'WAIVED'
      }).length
    : 0

  return (
    <>
      <PageHeading>{writtenName ? fmt(w.title, { name: writtenName }) : w.titleNoName}</PageHeading>
      <p className="tags">
        <span className="tag">
          {exchange.closed_outcome
            ? wording.outcomes[exchange.closed_outcome]
            : wording.states[exchange.state]}
        </span>
        <span className="tag">{fmt(wording.home.reference, { code: exchange.display_code })}</span>
      </p>
      {exchange.closed_reason && exchange.closed_reason in wording.closedReasons && (
        <p>{wording.closedReasons[exchange.closed_reason as ClosedReason]}</p>
      )}

      <div ref={refusal}>
        <Failure code={refused} />
      </div>
      {(actions.done || refreshed) && !newer && <Notice>{w.updated}</Notice>}
      {newer && (
        <div className="notice" role="status">
          <p>{w.newer}</p>
          <button
            type="button"
            onClick={() => {
              actions.close()
              onChange(newer)
              setNewer(null)
            }}
          >
            {w.showLatest}
          </button>
        </div>
      )}

      <Counterparty
        exchange={exchange}
        otherName={otherName}
        actions={actions}
        issued={issued}
        onIssued={onIssued}
        reload={reload}
      />

      {open && (
        <OpenRevision exchange={exchange} revision={open} otherName={otherName} actions={actions} />
      )}

      {inForce && (
        <section className="card" aria-labelledby="agreement-heading">
          <h2 id="agreement-heading">{w.agreementHeading}</h2>
          <p>{w.agreementSigned}</p>
          {active && remaining > 0 && <p>{fmt(w.remaining, { count: remaining })}</p>}
          <TermsView
            terms={inForce.terms}
            currency={exchange.currency}
            timezone={exchange.timezone}
            you={you}
            statuses={statuses}
            footer={(contribution) => (
              <Fulfillment
                contribution={contribution}
                status={statuses.get(contribution.id) ?? 'PENDING'}
                you={you}
                otherName={otherName}
                active={active}
                actions={actions}
              />
            )}
          />
          <p className="hint fingerprint">
            {fmt(wording.terms.fingerprint, { hash: inForce.content_hash })}
          </p>
          {active && !open && (
            <div className="actions">
              <Link className="button" to={paths.revise(exchange.id)}>
                {w.amend}
              </Link>
            </div>
          )}
        </section>
      )}

      {active && <Ending exchange={exchange} otherName={otherName} actions={actions} />}

      <History exchange={exchange} />

      {exchange.state !== 'CLOSED' && (
        <div className="actions">
          <button
            type="button"
            className="link"
            onClick={() => {
              setRefreshed(false)
              void reload()
            }}
          >
            {w.refresh}
          </button>
        </div>
      )}
    </>
  )
}

interface CounterpartyProps {
  exchange: Exchange
  otherName: string
  actions: Actions
  issued: string | null
  onIssued(token: string | null): void
  reload(): Promise<Exchange | null>
}

/**
 * Who is on the other side (DESIGN.md §8). Until someone opens the link the
 * initiator can replace it; once someone has, the initiator confirms it is
 * who they meant before any signature takes effect.
 */
function Counterparty({
  exchange,
  otherName,
  actions,
  issued,
  onIssued,
  reload,
}: CounterpartyProps) {
  const { wording, fmt } = useI18n()
  const w = wording.exchange
  const link = wording.invitationLink
  const initiator = exchange.you === 'A'
  const claimant = exchange.claimant ?? null

  if (exchange.state !== 'NEGOTIATING') return null

  if (initiator && exchange.counterparty === 'UNCLAIMED') {
    return (
      <section className="card" aria-labelledby="invitation-heading">
        <h2 id="invitation-heading">{link.heading}</h2>
        {issued ? <InvitationLink key={issued} token={issued} /> : <p>{link.unclaimed}</p>}
        <p>{link.reissueIntro}</p>
        <div className="actions">
          <button
            type="button"
            aria-expanded={actions.panel === 'reissue'}
            onClick={() => actions.open('reissue')}
          >
            {link.reissue}
          </button>
        </div>
        {actions.panel === 'reissue' && (
          <Reissue exchange={exchange.id} actions={actions} onIssued={onIssued} reload={reload} />
        )}
      </section>
    )
  }

  if (initiator && exchange.counterparty === 'CLAIMED' && claimant) {
    const signed = exchange.open_revision?.accepted_by.includes('B') ?? false
    return (
      <section className="card" aria-labelledby="claimed-heading">
        <h2 id="claimed-heading">{w.claimedHeading}</h2>
        <p>
          {fmt(w.claimedBody, { name: claimant.display_name, identifier: claimant.identifier })}
        </p>
        {signed && <p>{w.claimedSigned}</p>}
        <div className="actions">
          <button
            type="button"
            className="primary"
            disabled={actions.busy}
            onClick={() => void actions.run({ type: 'CONFIRM_COUNTERPARTY' })}
          >
            {w.confirmCounterparty}
          </button>
        </div>
        <p className="hint">{w.notThem}</p>
      </section>
    )
  }

  if (!initiator && exchange.counterparty === 'CLAIMED') {
    const signed = exchange.open_revision?.accepted_by.includes('B') ?? false
    return (
      <p className="notice">
        {fmt(signed ? w.waitingConfirmationSigned : w.waitingConfirmation, { name: otherName })}
      </p>
    )
  }
  return null
}

interface ReissueProps {
  exchange: string
  actions: Actions
  onIssued(token: string | null): void
  reload(): Promise<Exchange | null>
}

function Reissue({ exchange, actions, onIssued, reload }: ReissueProps) {
  const { wording } = useI18n()
  const link = wording.invitationLink
  const [boundTo, setBoundTo] = useState('')
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)

  async function submit(event: FormEvent) {
    event.preventDefault()
    setBusy(true)
    setFailure(null)
    try {
      onIssued(await api.reissueInvitation(exchange, boundTo.trim() || null))
      actions.close()
    } catch (error) {
      const code = failureCode(error)
      setFailure(code)
      // Refused because someone has opened the link in the meantime.
      if (code === 'ACTION_NOT_ALLOWED') await reload()
    } finally {
      setBusy(false)
    }
  }

  return (
    <Panel title={link.reissue}>
      <form noValidate onSubmit={submit}>
        <InvitationFor value={boundTo} onChange={setBoundTo} />
        <Failure code={failure} />
        <div className="actions">
          <button type="submit" className="primary" disabled={busy}>
            {link.reissue}
          </button>
          <button type="button" disabled={busy} onClick={actions.close}>
            {wording.common.cancel}
          </button>
        </div>
      </form>
    </Panel>
  )
}

interface OpenRevisionProps {
  exchange: Exchange
  revision: RevisionView
  otherName: string
  actions: Actions
}

/**
 * The one revision waiting to be signed: a first proposal, a counteroffer,
 * or an amendment to the agreement in force. Its author signed it by sending
 * it; the other party can sign it, decline it, or answer with their own.
 */
function OpenRevision({ exchange, revision, otherName, actions }: OpenRevisionProps) {
  const { wording, fmt, moment, language } = useI18n()
  const w = wording.exchange
  const you = exchange.you
  const yours = revision.author === you
  const youSigned = revision.accepted_by.includes(you)
  const otherSigned = revision.accepted_by.some((slot) => slot !== you)
  const amendment = exchange.state === 'ACTIVE'
  // The initiator is never bound to someone they have not confirmed.
  const blocked = you === 'A' && exchange.counterparty !== 'CONFIRMED'

  return (
    <section className="card" aria-labelledby="open-heading">
      <h2 id="open-heading">{amendment ? w.amendmentHeading : w.proposalHeading}</h2>
      <p className="hint">
        {fmt(w.version, { number: revision.sequence })}
        <br />
        {yours ? w.sentByYou : fmt(w.sentByOther, { name: otherName })}
        <br />
        {fmt(w.expires, { date: moment(revision.expires_at) })}
      </p>

      {revision.note && (
        <>
          <h3>{yours ? w.noteFromYou : fmt(w.noteFromOther, { name: otherName })}</h3>
          <Written>{revision.note}</Written>
        </>
      )}

      <TermsView
        terms={revision.terms}
        currency={exchange.currency}
        timezone={exchange.timezone}
        you={you}
      />
      <p className="hint fingerprint">
        {fmt(wording.terms.fingerprint, { hash: revision.content_hash })}
      </p>

      <ul className="plain">
        <li>{youSigned ? w.signedByYou : w.unsignedByYou}</li>
        <li>
          {otherSigned
            ? fmt(w.signedByOther, { name: otherName })
            : fmt(w.unsignedByOther, { name: otherName })}
        </li>
      </ul>

      {yours && (
        <div className="actions">
          <Link className="button" to={paths.revise(exchange.id)}>
            {w.change}
          </Link>
          <button
            type="button"
            aria-expanded={actions.panel === 'withdraw'}
            disabled={actions.busy}
            onClick={() => actions.open('withdraw')}
          >
            {w.withdraw}
          </button>
        </div>
      )}

      {!yours && !youSigned && (
        <>
          {blocked && <p className="notice">{w.acceptBlocked}</p>}
          <div className="actions">
            {!blocked && (
              <button
                type="button"
                className="primary"
                aria-expanded={actions.panel === 'accept'}
                disabled={actions.busy}
                onClick={() => actions.open('accept')}
              >
                {w.accept}
              </button>
            )}
            <Link className="button" to={paths.revise(exchange.id)}>
              {w.counter}
            </Link>
            <button
              type="button"
              aria-expanded={actions.panel === 'decline'}
              disabled={actions.busy}
              onClick={() => actions.open('decline')}
            >
              {w.decline}
            </button>
          </div>
        </>
      )}

      {actions.panel === 'accept' && (
        <Panel title={w.signHeading}>
          <p>{w.signIntro}</p>
          <Consent
            signLabel={w.accept}
            busy={actions.busy}
            failure={actions.failure}
            onCancel={actions.close}
            onSign={() =>
              // Acceptance names the revision: if the terms have changed since
              // this page showed them, the service refuses (DESIGN.md §6).
              void actions.run({
                type: 'ACCEPT',
                revision: revision.id,
                consent: consentShown(language),
              })
            }
          />
        </Panel>
      )}

      {actions.panel === 'decline' && (
        <Panel title={w.decline}>
          <p>{amendment ? w.declineKeeps : w.declineEnds}</p>
          <Failure code={actions.failure} />
          <div className="actions">
            <button
              type="button"
              className="primary"
              disabled={actions.busy}
              onClick={() => void actions.run({ type: 'DECLINE', revision: revision.id })}
            >
              {w.confirmDecline}
            </button>
            <button type="button" disabled={actions.busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}

      {actions.panel === 'withdraw' && (
        <Panel title={w.withdraw}>
          <p>{amendment ? w.withdrawKeeps : w.withdrawEnds}</p>
          <Failure code={actions.failure} />
          <div className="actions">
            <button
              type="button"
              className="primary"
              disabled={actions.busy}
              onClick={() => void actions.run({ type: 'WITHDRAW', revision: revision.id })}
            >
              {w.confirmWithdraw}
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
