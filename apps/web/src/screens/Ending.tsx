import type { ExchangeView } from '@exchange/api-client'
import { NOTE_MAX_CHARS } from '@exchange/shared'
import { useState, type FormEvent } from 'react'

import { useI18n } from '../app/context'
import { Panel } from '../components/Panel'
import { Failure, Field, Notice } from '../components/ui'
import type { Actions } from '../lib/actions'

interface Props {
  exchange: ExchangeView
  otherName: string
  actions: Actions
}

/**
 * The ways out of an agreement in force that is not simply completed
 * (DESIGN.md §5.3): ending by agreement, which either party proposes and the
 * other accepts, and closing without agreement, which either party can ask
 * for alone. Every exchange can end; neither way needs the platform to judge
 * anything.
 */
export function Ending({ exchange, otherName, actions }: Props) {
  const { wording, fmt, moment } = useI18n()
  const w = wording.exchange
  const you = exchange.you
  const endBy = exchange.end_proposed_by ?? null
  const closeBy = exchange.close_requested_by ?? null
  const [stated, setStated] = useState(false)

  // Agreeing to end answers the other party's proposal or their close request.
  const invited = (endBy !== null && endBy !== you) || (closeBy !== null && closeBy !== you)
  const requestedOn = exchange.close_requested_at ? moment(exchange.close_requested_at) : ''

  return (
    <section aria-labelledby="ending-heading">
      <h2 id="ending-heading">{w.endingHeading}</h2>

      {endBy === you && <p>{fmt(w.endProposedByYou, { name: otherName })}</p>}
      {endBy !== null && endBy !== you && <p>{fmt(w.endProposedByOther, { name: otherName })}</p>}
      {closeBy === you && <p>{fmt(w.closeRequestedByYou, { date: requestedOn })}</p>}
      {closeBy !== null && closeBy !== you && (
        <p>{fmt(w.closeRequestedByOther, { name: otherName, date: requestedOn })}</p>
      )}
      {stated && <Notice>{w.statementAdded}</Notice>}

      <div className="actions">
        {invited && (
          <button
            type="button"
            className="primary"
            aria-expanded={actions.panel === 'agree-end'}
            disabled={actions.busy}
            onClick={() => actions.open('agree-end')}
          >
            {w.agreeEnd}
          </button>
        )}
        {endBy !== null && (
          <button
            type="button"
            disabled={actions.busy}
            onClick={() => void actions.run({ type: 'CANCEL_END' })}
          >
            {endBy === you ? w.cancelEnd : w.declineEnd}
          </button>
        )}
        {endBy === null && (
          <button
            type="button"
            aria-expanded={actions.panel === 'propose-end'}
            disabled={actions.busy}
            onClick={() => actions.open('propose-end')}
          >
            {w.proposeEnd}
          </button>
        )}
        {closeBy === null && (
          <button
            type="button"
            aria-expanded={actions.panel === 'request-close'}
            disabled={actions.busy}
            onClick={() => actions.open('request-close')}
          >
            {w.requestClose}
          </button>
        )}
        {closeBy === you && (
          <button
            type="button"
            disabled={actions.busy}
            onClick={() => void actions.run({ type: 'RETRACT_CLOSE' })}
          >
            {w.retractClose}
          </button>
        )}
        {closeBy !== null && (
          <button
            type="button"
            aria-expanded={actions.panel === 'statement'}
            disabled={actions.busy}
            onClick={() => {
              setStated(false)
              actions.open('statement')
            }}
          >
            {w.addStatement}
          </button>
        )}
      </div>

      {actions.panel === 'agree-end' && (
        <Panel title={w.agreeEnd}>
          <p>{w.agreeEndText}</p>
          <Failure code={actions.failure} />
          <div className="actions">
            <button
              type="button"
              className="primary"
              disabled={actions.busy}
              onClick={() => void actions.run({ type: 'ACCEPT_END' })}
            >
              {w.confirmAgreeEnd}
            </button>
            <button type="button" disabled={actions.busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}

      {actions.panel === 'propose-end' && (
        <Panel title={w.proposeEnd}>
          <p>{fmt(w.proposeEndText, { name: otherName })}</p>
          <Failure code={actions.failure} />
          <div className="actions">
            <button
              type="button"
              className="primary"
              disabled={actions.busy}
              onClick={() => void actions.run({ type: 'PROPOSE_END' })}
            >
              {w.sendEndProposal}
            </button>
            <button type="button" disabled={actions.busy} onClick={actions.close}>
              {wording.common.cancel}
            </button>
          </div>
        </Panel>
      )}

      {actions.panel === 'request-close' && (
        <StatementPanel
          title={w.requestClose}
          text={fmt(w.requestCloseText, { name: otherName })}
          label={w.statementLabel}
          submitLabel={w.sendCloseRequest}
          required={false}
          actions={actions}
          onSubmit={(note) => actions.run({ type: 'REQUEST_CLOSE', note })}
        />
      )}

      {actions.panel === 'statement' && (
        <StatementPanel
          title={w.addStatement}
          text={null}
          label={w.statementRequiredLabel}
          submitLabel={w.sendStatement}
          required
          actions={actions}
          onSubmit={async (note) => {
            const added = await actions.run({ type: 'ADD_STATEMENT', note: note ?? '' })
            setStated(added)
            return added
          }}
        />
      )}
    </section>
  )
}

interface StatementPanelProps {
  title: string
  text: string | null
  label: string
  submitLabel: string
  required: boolean
  actions: Actions
  onSubmit(note: string | null): Promise<boolean>
}

function StatementPanel(props: StatementPanelProps) {
  const { wording } = useI18n()
  const { actions } = props
  const [note, setNote] = useState('')
  const [missing, setMissing] = useState(false)

  function submit(event: FormEvent) {
    event.preventDefault()
    const written = note.trim()
    if (props.required && written === '') {
      setMissing(true)
      return
    }
    void props.onSubmit(written === '' ? null : written)
  }

  return (
    <Panel title={props.title}>
      <form noValidate onSubmit={submit}>
        {props.text && <p>{props.text}</p>}
        <Field
          label={props.label}
          hint={wording.exchange.noteRecord}
          error={missing ? wording.exchange.noteRequired : null}
        >
          {(control) => (
            <textarea
              {...control}
              rows={3}
              maxLength={NOTE_MAX_CHARS}
              value={note}
              onChange={(event) => {
                setNote(event.target.value)
                setMissing(false)
              }}
            />
          )}
        </Field>
        <Failure code={actions.failure} />
        <div className="actions">
          <button type="submit" className="primary" disabled={actions.busy}>
            {props.submitLabel}
          </button>
          <button type="button" disabled={actions.busy} onClick={actions.close}>
            {wording.common.cancel}
          </button>
        </div>
      </form>
    </Panel>
  )
}
