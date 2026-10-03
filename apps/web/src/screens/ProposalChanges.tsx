import type { ExchangeView } from '@yuppers/api-client'
import {
  fieldChangeText,
  proposalChanges,
  statusWording,
  useProposalBase,
  type RevisionView,
} from '@yuppers/shared'
import { useId, useMemo } from 'react'

import { useI18n } from '../app/context'
import { Written } from '../components/ui'
import { api } from '../lib/api'

interface Props {
  exchange: ExchangeView
  /** The proposal waiting for the reader's signature. */
  revision: RevisionView
}

/**
 * What a proposal changes, shown to the person asked to sign it, above the
 * way to sign: each item added, removed or changed field by field, against
 * the agreement in force for an amendment (with where each item would then
 * stand), or against the version a counteroffer answers. The complete terms
 * above remain what is signed.
 */
export function ProposalChanges({ exchange, revision }: Props) {
  const i18n = useI18n()
  const { wording, fmt } = i18n
  const w = wording.proposalChanges
  const base = useProposalBase(api, exchange)
  const heading = useId()
  const changes = useMemo(
    () => (base ? proposalChanges(base.terms, revision.terms, base.statuses) : null),
    [base, revision.terms],
  )
  if (!base || !changes) return null

  const amendment = base.against === 'IN_FORCE'
  const terms = { before: base.terms, after: revision.terms }
  // For an amendment every item is shown, with where it would stand; for a
  // counteroffer only what differs, and how many items did not.
  const shown = amendment ? changes.items : changes.items.filter((item) => item.kind !== 'UNCHANGED')
  const unchanged = changes.items.length - shown.length
  const anyItemChanged = changes.items.some((item) => item.kind !== 'UNCHANGED')

  return (
    <section className="proposal-changes" aria-labelledby={heading}>
      <h3 id={heading}>{w.heading}</h3>
      <p className="hint">
        {fmt(amendment ? w.againstInForce : w.againstPrevious, { number: base.sequence })}
      </p>
      {changes.names.map((name) => (
        <div key={name.slot} className="change">
          <p className="label">{w.fields.name}</p>
          <p>
            {w.was} <Written inline>{name.before}</Written>
          </p>
          <p>
            {w.now} <Written inline>{name.after}</Written>
          </p>
        </div>
      ))}
      {changes.termsChanged && <p>{w.termsChanged}</p>}
      {!anyItemChanged && <p>{w.nothing}</p>}
      {shown.length > 0 && (
        <ul className="plain">
          {shown.map((item) => (
            <li key={item.id} className="contribution">
              <p className="tags">
                <span className="tag">{w.kinds[item.kind]}</span>
              </p>
              <Written>{item.description}</Written>
              {item.fields.map((change) => {
                const text = fieldChangeText(change, i18n, exchange.currency, terms)
                return text.sentence ? (
                  <p key={change.field}>{text.sentence}</p>
                ) : (
                  <div key={change.field} className="change">
                    <p className="label">{text.label}</p>
                    <p>
                      {w.was} <Written inline>{text.before}</Written>
                    </p>
                    <p>
                      {w.now} <Written inline>{text.after}</Written>
                    </p>
                  </div>
                )
              })}
              {/* The one consequence nobody would guess (DESIGN.md §7): the tag says the rest. */}
              {item.effect === 'CHANGED' && <p>{wording.composer.effects.CHANGED}</p>}
              {item.status && item.status !== 'REMOVED' && (
                <p className="status">
                  {fmt(w.statusAfter, { status: statusWording(wording, item.status, item.money) })}
                </p>
              )}
            </li>
          ))}
        </ul>
      )}
      {unchanged > 0 && anyItemChanged && <p>{fmt(w.unchangedCount, { count: unchanged })}</p>}
    </section>
  )
}
