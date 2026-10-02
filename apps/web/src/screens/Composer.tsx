import type { components, ErrorCode, ExchangeView as Exchange } from '@exchange/api-client'
import {
  buildTerms,
  decimalForInput,
  draftFromTerms,
  emptyDraft,
  fractionDigitsOf,
  newContribution,
  NOTE_MAX_CHARS,
  parseDecimal,
  readDraft,
  toMinorUnits,
  type Draft,
  type DraftContribution,
  type DraftDue,
  type Problem,
  type ProblemField,
} from '@exchange/shared'
import { useEffect, useMemo, useRef, useState } from 'react'

import { useI18n, useSession } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { Consent } from '../components/Consent'
import { InvitationFor } from '../components/InvitationLink'
import { TermsView } from '../components/TermsView'
import { Field, PageHeading, Written, type ControlProps } from '../components/ui'
import { api, failureCode, type RevisionSent, type Slot } from '../lib/api'
import { consentShown } from '../lib/consent'

type ContributionType = components['schemas']['ContributionType']

const TYPES: readonly ContributionType[] = ['ITEM', 'SERVICE', 'TASK', 'MONEY', 'OTHER']

/** How long after the last keystroke the working copy is saved. */
const SAVE_AFTER_MS = 700

interface Props {
  exchange: Exchange
  reload(): Promise<Exchange | null>
  onSent(sent: RevisionSent): void
}

/**
 * Writing terms and sending them: a first proposal on a draft, a counteroffer
 * during negotiation, or an amendment to an agreement in force. The three
 * differ only in what they start from.
 *
 * The working copy saves itself as a draft, which is private and binds
 * nobody. Sending is a separate, deliberate step, because sending signs
 * (DESIGN.md §6, §14.1): it shows the complete terms as they will be sent,
 * then the consent wording, and does nothing until the person agrees.
 */
export default function Composer(props: Props) {
  const { wording } = useI18n()
  const { exchange } = props
  const base = exchange.open_revision ?? exchange.in_force_revision ?? null

  if (exchange.state === 'CLOSED' || (exchange.state !== 'DRAFT' && !base)) {
    return (
      <>
        <PageHeading>{wording.composer.titleCounter}</PageHeading>
        <p>{wording.composer.notAvailable}</p>
        <p>
          <Link to={paths.exchange(exchange.id)}>{wording.exchange.titleNoName}</Link>
        </p>
      </>
    )
  }
  return <Editor {...props} />
}

function Editor({ exchange, reload, onSent }: Props) {
  const { wording, fmt, language, money } = useI18n()
  const { account } = useSession()
  const w = wording.composer

  const you = exchange.you
  const other: Slot = you === 'A' ? 'B' : 'A'
  const kind = exchange.state === 'DRAFT' ? 'first' : exchange.state === 'ACTIVE' ? 'amend' : 'counter'
  // What a counteroffer or an amendment starts from: the terms on the table.
  const base = exchange.open_revision ?? exchange.in_force_revision ?? null
  const digits = fractionDigitsOf(exchange.currency)

  const [draft, setDraft] = useState<Draft>(() => {
    const stored = readDraft(exchange.draft)
    if (stored) return stored
    return base
      ? draftFromTerms(base.terms, base.id, digits)
      : emptyDraft(account?.display_name ?? '')
  })
  // Bumped to rebuild the inputs when the whole working copy is replaced.
  const [generation, setGeneration] = useState(0)
  const [step, setStep] = useState<'edit' | 'sign'>('edit')
  const [returned, setReturned] = useState(false)
  const [checked, setChecked] = useState(false)
  const [conflict, setConflict] = useState(false)
  const [boundTo, setBoundTo] = useState('')
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [saveState, setSaveState] = useState<'idle' | 'saving' | 'saved' | 'failed'>('idle')
  const [added, setAdded] = useState<string | null>(null)

  // The working copy was started from terms that have since been replaced.
  const stale = base !== null && draft.base !== base.id

  // ---- Saving the working copy ----------------------------------------------

  const latest = useRef(draft)
  const dirty = useRef(false)
  const sent = useRef(false)
  const timer = useRef<number | undefined>(undefined)
  const saving = useRef<Promise<void> | null>(null)
  const exchangeId = exchange.id

  function schedule() {
    window.clearTimeout(timer.current)
    timer.current = window.setTimeout(() => void save(), SAVE_AFTER_MS)
  }

  // One save at a time, so an older copy can never land after a newer one.
  async function save() {
    if (saving.current || !dirty.current || sent.current) return
    dirty.current = false
    setSaveState('saving')
    let saved = true
    saving.current = api.saveDraft(exchangeId, latest.current).catch(() => {
      saved = false
    })
    await saving.current
    saving.current = null
    if (!saved) {
      dirty.current = true
      setSaveState('failed')
    } else if (dirty.current) schedule()
    else setSaveState('saved')
  }

  function edit(next: Draft) {
    latest.current = next
    dirty.current = true
    setDraft(next)
    schedule()
  }

  // Leaving the page keeps what was typed in the last moment.
  useEffect(
    () => () => {
      window.clearTimeout(timer.current)
      if (dirty.current && !sent.current) {
        api.saveDraft(exchangeId, latest.current).catch(() => {})
      }
    },
    [exchangeId],
  )

  // ---- Editing -----------------------------------------------------------------

  const change = (patch: Partial<Draft>) => edit({ ...latest.current, ...patch })
  const changeItem = (id: string, patch: Partial<DraftContribution>) =>
    change({
      contributions: latest.current.contributions.map((item) =>
        item.id === id ? { ...item, ...patch } : item,
      ),
    })

  function addItem(from: Slot) {
    const id = crypto.randomUUID()
    change({ contributions: [...latest.current.contributions, newContribution(id, from)] })
    setAdded(id)
  }

  // A new item starts with the keyboard in it.
  useEffect(() => {
    if (added) document.getElementById(`${added}-description`)?.focus()
  }, [added])

  const built = useMemo(() => buildTerms(draft, digits, base?.terms), [draft, digits, base])
  const problems = checked && !built.ok ? built.problems : []
  const exampleNumber = decimalForInput('1.5', language)
  const exampleAmount = decimalForInput('25.50', language)

  function problemText(problem: Problem): string {
    const message = w.problems[problem.code]
    if (problem.code === 'NOTE_TOO_LONG') return fmt(message, { max: NOTE_MAX_CHARS })
    if (problem.code === 'QUANTITY_INVALID') return fmt(message, { example: exampleNumber })
    if (problem.code === 'AMOUNT_INVALID') return fmt(message, { example: exampleAmount })
    return message
  }

  function errorFor(field: ProblemField, contribution?: string): string | null {
    const found = problems.find(
      (problem) => problem.field === field && problem.contribution === contribution,
    )
    return found ? problemText(found) : null
  }

  function fieldId(problem: Problem): string {
    if (problem.contribution) return `${problem.contribution}-${problem.field}`
    if (problem.field === 'partyA') return 'party-A'
    if (problem.field === 'partyB') return 'party-B'
    if (problem.field === 'note') return 'note'
    return 'add-yours'
  }

  function review() {
    setChecked(true)
    setConflict(false)
    setFailure(null)
    if (built.ok) {
      setStep('sign')
      setReturned(true)
      return
    }
    // The keyboard goes to the first thing to fix, once it has been marked.
    const first = fieldId(built.problems[0])
    window.setTimeout(() => document.getElementById(first)?.focus())
  }

  // ---- Sending -------------------------------------------------------------------

  async function send() {
    if (!built.ok) return
    setBusy(true)
    setFailure(null)
    // The service drops the working copy when the revision is sent. A save
    // still on its way must not put it back afterwards.
    window.clearTimeout(timer.current)
    dirty.current = false
    await saving.current
    try {
      const result = await api.sendRevision(exchange.id, {
        expected_version: exchange.version,
        terms: built.terms,
        note: built.note,
        consent: consentShown(language),
        invitation: kind === 'first' ? { bound_to: boundTo.trim() || null } : null,
      })
      sent.current = true
      onSent(result)
    } catch (error) {
      const code = failureCode(error)
      dirty.current = true
      if (code === 'VERSION_CONFLICT') {
        // The exchange moved on. Nothing was sent; show what it is now and
        // keep what was written.
        await reload()
        setConflict(true)
        setStep('edit')
      } else setFailure(code)
      setBusy(false)
    }
  }

  const title = kind === 'first' ? w.titleFirst : kind === 'amend' ? w.titleAmend : w.titleCounter

  if (step === 'sign' && built.ok) {
    return (
      <>
        <PageHeading key="sign" step>
          {w.signTitle}
        </PageHeading>
        <p>{w.signIntro}</p>
        {built.note && (
          <section>
            <h2>{w.yourNote}</h2>
            <Written>{built.note}</Written>
            <p className="hint">{w.noteHint}</p>
          </section>
        )}
        <section className="card">
          <TermsView
            terms={built.terms}
            currency={exchange.currency}
            timezone={exchange.timezone}
            you={you}
          />
        </section>
        {kind === 'first' && <InvitationFor value={boundTo} onChange={setBoundTo} />}
        <Consent
          signLabel={w.signAndSend}
          busy={busy}
          failure={failure}
          onSign={() => void send()}
          onCancel={() => setStep('edit')}
          cancelLabel={w.backToEdit}
        />
      </>
    )
  }

  // An accepted contribution is locked: an amendment may not touch it.
  const locked = new Set(
    exchange.state === 'ACTIVE'
      ? exchange.contributions.filter((item) => item.status === 'ACCEPTED').map((item) => item.id)
      : [],
  )
  const nameOf = (slot: Slot) => (slot === 'A' ? draft.partyA : draft.partyB)
  const setName = (slot: Slot, name: string) =>
    change(slot === 'A' ? { partyA: name } : { partyB: name })
  const general = problems.filter((problem) => problem.field === 'contributions')

  return (
    <>
      {/* Coming back from the signing step, the keyboard starts from the top again. */}
      <PageHeading key="edit" step={returned}>
        {title}
      </PageHeading>
      <p>{kind === 'first' ? w.introFirst : kind === 'amend' ? w.introAmend : w.introCounter}</p>

      {conflict && (
        <p className="notice notice-error" role="alert">
          {w.conflict}
        </p>
      )}
      {stale && base && (
        <div className="notice" role="status">
          <p>{w.staleDraft}</p>
          <div className="actions">
            <button type="button" onClick={() => change({ base: base.id })}>
              {w.staleDraftKeep}
            </button>
            <button
              type="button"
              onClick={() => {
                edit(draftFromTerms(base.terms, base.id, digits))
                setGeneration((count) => count + 1)
                setChecked(false)
              }}
            >
              {w.staleDraftDiscard}
            </button>
          </div>
        </div>
      )}
      {problems.length > 0 && (
        <p className="notice notice-error" role="alert">
          {fmt(w.problemsSummary, { count: problems.length })}
        </p>
      )}

      <form
        noValidate
        key={generation}
        onSubmit={(event) => {
          event.preventDefault()
          review()
        }}
      >
        <fieldset>
          <legend>{w.partiesLegend}</legend>
          <Field label={w.yourName} id={`party-${you}`} error={errorFor(you === 'A' ? 'partyA' : 'partyB')}>
            {(control) => (
              <input
                {...control}
                type="text"
                maxLength={100}
                value={nameOf(you)}
                onChange={(event) => setName(you, event.target.value)}
              />
            )}
          </Field>
          <Field
            label={w.otherName}
            id={`party-${other}`}
            error={errorFor(other === 'A' ? 'partyA' : 'partyB')}
          >
            {(control) => (
              <input
                {...control}
                type="text"
                maxLength={100}
                value={nameOf(other)}
                onChange={(event) => setName(other, event.target.value)}
              />
            )}
          </Field>
        </fieldset>

        <Field label={w.termsLabel} hint={w.termsHint}>
          {(control) => (
            <textarea
              {...control}
              rows={5}
              value={draft.terms}
              onChange={(event) => change({ terms: event.target.value })}
            />
          )}
        </Field>

        <h2>{w.itemsHeading}</h2>
        {draft.contributions.map((item, index) => {
          const number = index + 1
          const minor =
            item.type === 'MONEY' && item.amount ? toMinorUnits(item.amount, digits) : null
          return (
            <fieldset key={item.id} disabled={locked.has(item.id)}>
              <legend>{fmt(w.itemLegend, { number })}</legend>
              {locked.has(item.id) && <p className="notice">{w.locked}</p>}

              <div className="pair">
                <Field label={w.fromLabel}>
                  {(control) => (
                    <select
                      {...control}
                      value={item.from}
                      onChange={(event) => changeItem(item.id, { from: event.target.value as Slot })}
                    >
                      <option value={you}>{wording.party.you}</option>
                      <option value={other}>{wording.party.other}</option>
                    </select>
                  )}
                </Field>
                <Field label={w.typeLabel}>
                  {(control) => (
                    <select
                      {...control}
                      value={item.type}
                      onChange={(event) =>
                        changeItem(item.id, { type: event.target.value as ContributionType })
                      }
                    >
                      {TYPES.map((type) => (
                        <option key={type} value={type}>
                          {wording.contributionTypes[type]}
                        </option>
                      ))}
                    </select>
                  )}
                </Field>
              </div>

              <Field
                label={w.descriptionLabel}
                id={`${item.id}-description`}
                error={errorFor('description', item.id)}
              >
                {(control) => (
                  <textarea
                    {...control}
                    rows={2}
                    value={item.description}
                    onChange={(event) => changeItem(item.id, { description: event.target.value })}
                  />
                )}
              </Field>

              {item.type === 'MONEY' ? (
                <Field
                  label={fmt(w.amountLabel, { currency: exchange.currency })}
                  id={`${item.id}-amount`}
                  error={errorFor('amount', item.id)}
                >
                  {(control) => (
                    <>
                      <DecimalInput
                        {...control}
                        value={item.amount}
                        onChange={(amount) => changeItem(item.id, { amount })}
                      />
                      {/* What the typed number will be signed as. */}
                      {minor !== null && (
                        <p className="hint" role="status">
                          {fmt(w.amountPreview, { amount: money(minor, exchange.currency) })}
                        </p>
                      )}
                    </>
                  )}
                </Field>
              ) : (
                <div className="pair">
                  <Field
                    label={w.quantityLabel}
                    id={`${item.id}-quantity`}
                    error={errorFor('quantity', item.id)}
                  >
                    {(control) => (
                      <DecimalInput
                        {...control}
                        value={item.quantity}
                        onChange={(quantity) => changeItem(item.id, { quantity })}
                      />
                    )}
                  </Field>
                  <Field label={w.unitLabel} hint={w.unitHint}>
                    {(control) => (
                      <input
                        {...control}
                        type="text"
                        maxLength={40}
                        value={item.unit}
                        onChange={(event) => changeItem(item.id, { unit: event.target.value })}
                      />
                    )}
                  </Field>
                </div>
              )}

              <Field label={w.dueLabel}>
                {(control) => (
                  <select
                    {...control}
                    value={item.due.kind}
                    onChange={(event) =>
                      changeItem(item.id, { due: dueOf(event.target.value as DraftDue['kind']) })
                    }
                  >
                    <option value="ON_AGREEMENT">{w.dueOnAgreement}</option>
                    <option value="DATE">{w.dueOnDate}</option>
                    <option value="AFTER_CONTRIBUTION">{w.dueAfter}</option>
                  </select>
                )}
              </Field>
              {item.due.kind === 'DATE' && (
                <Field label={w.dateLabel} id={`${item.id}-date`} error={errorFor('date', item.id)}>
                  {(control) => (
                    <input
                      {...control}
                      type="date"
                      value={item.due.kind === 'DATE' ? item.due.date : ''}
                      onChange={(event) =>
                        changeItem(item.id, { due: { kind: 'DATE', date: event.target.value } })
                      }
                    />
                  )}
                </Field>
              )}
              {item.due.kind === 'AFTER_CONTRIBUTION' && (
                <Field label={w.afterLabel} id={`${item.id}-after`} error={errorFor('after', item.id)}>
                  {(control) => (
                    <select
                      {...control}
                      value={item.due.kind === 'AFTER_CONTRIBUTION' ? item.due.contribution : ''}
                      onChange={(event) =>
                        changeItem(item.id, {
                          due: { kind: 'AFTER_CONTRIBUTION', contribution: event.target.value },
                        })
                      }
                    >
                      <option value="">{w.afterChoose}</option>
                      {draft.contributions.map((candidate, position) =>
                        candidate.id === item.id ? null : (
                          <option key={candidate.id} value={candidate.id}>
                            {candidate.description.trim()
                              ? fmt(w.itemOption, {
                                  number: position + 1,
                                  description: candidate.description.trim(),
                                })
                              : fmt(w.itemOptionBlank, { number: position + 1 })}
                          </option>
                        ),
                      )}
                    </select>
                  )}
                </Field>
              )}

              <Field label={w.criteriaLabel}>
                {(control) => (
                  <textarea
                    {...control}
                    rows={2}
                    value={item.criteria}
                    onChange={(event) => changeItem(item.id, { criteria: event.target.value })}
                  />
                )}
              </Field>

              <label className="check">
                <input
                  type="checkbox"
                  checked={item.required}
                  onChange={(event) => changeItem(item.id, { required: event.target.checked })}
                />
                <span>{w.requiredLabel}</span>
              </label>

              <div className="actions">
                <button
                  type="button"
                  onClick={() =>
                    change({
                      contributions: latest.current.contributions.filter(
                        (candidate) => candidate.id !== item.id,
                      ),
                    })
                  }
                >
                  {fmt(w.remove, { number })}
                </button>
              </div>
            </fieldset>
          )
        })}

        {general.map((problem) => (
          <p key={problem.code} className="field-error" id="items-error">
            {problemText(problem)}
          </p>
        ))}
        <div className="actions">
          <button
            type="button"
            id="add-yours"
            aria-describedby={general.length > 0 ? 'items-error' : undefined}
            onClick={() => addItem(you)}
          >
            {w.addYours}
          </button>
          <button type="button" onClick={() => addItem(other)}>
            {w.addTheirs}
          </button>
        </div>

        <Field label={w.noteLabel} hint={w.noteHint} id="note" error={errorFor('note')}>
          {(control) => (
            <textarea
              {...control}
              rows={3}
              value={draft.note}
              onChange={(event) => change({ note: event.target.value })}
            />
          )}
        </Field>

        <p className="hint" role="status">
          {saveState === 'saving' && w.saving}
          {saveState === 'saved' && w.saved}
          {saveState === 'failed' && w.saveFailed}
        </p>

        <div className="actions">
          <button type="submit" className="primary" disabled={stale}>
            {w.review}
          </button>
          <Link className="button" to={kind === 'first' ? paths.home : paths.exchange(exchange.id)}>
            {kind === 'first' ? wording.nav.exchanges : wording.common.cancel}
          </Link>
        </div>
      </form>
    </>
  )
}

function dueOf(kind: DraftDue['kind']): DraftDue {
  if (kind === 'DATE') return { kind, date: '' }
  if (kind === 'AFTER_CONTRIBUTION') return { kind, contribution: '' }
  return { kind: 'ON_AGREEMENT' }
}

interface DecimalInputProps extends ControlProps {
  /** A plain decimal, empty for none, `null` when what is typed is not a number. */
  value: string | null
  onChange(value: string | null): void
}

/**
 * A number typed the way the reader's language writes numbers. What is typed
 * stays on screen as typed; the working copy gets the plain form, or `null`
 * while it cannot be read as a number.
 */
function DecimalInput({ value, onChange, ...control }: DecimalInputProps) {
  const { language } = useI18n()
  const [text, setText] = useState(() => (value ? decimalForInput(value, language) : ''))
  return (
    <input
      {...control}
      type="text"
      inputMode="decimal"
      autoComplete="off"
      value={text}
      onChange={(event) => {
        const typed = event.target.value
        setText(typed)
        onChange(typed.trim() === '' ? '' : parseDecimal(typed, language))
      }}
    />
  )
}
