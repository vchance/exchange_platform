import type { components } from '@exchange/api-client'
import { moveCommand, movesFor, noteFor, NOTE_MAX_CHARS, type Move } from '@exchange/shared'
import { useState, type FormEvent } from 'react'

import { useI18n } from '../app/context'
import { Panel } from '../components/Panel'
import { Failure, Field } from '../components/ui'
import type { Actions } from '../lib/actions'
import type { Slot } from '../lib/api'

type Contribution = components['schemas']['ContributionDto']
type Status = components['schemas']['Status']

interface Props {
  contribution: Contribution
  status: Status
  you: Slot
  /** The other party's name, for telling the person who an action affects. */
  otherName: string
  /** Whether the agreement is still in force; a closed one is only read. */
  active: boolean
  actions: Actions
}

/**
 * Where one contribution stands and what the reader can do about it: the
 * provider marks it delivered, the recipient confirms, disputes or waives
 * (DESIGN.md §5.2). A claim is never a confirmation, and the two are worded
 * differently so neither party mistakes one for the other.
 */
export function Fulfillment({ contribution, status, you, otherName, active, actions }: Props) {
  const { wording } = useI18n()
  const w = wording.exchange
  const role = contribution.from === you ? 'PROVIDER' : 'RECIPIENT'
  const moves = active ? movesFor(status, role) : []
  const panelOf = (move: Move) => `move:${contribution.id}:${move}`
  const opened = moves.find((move) => actions.panel === panelOf(move))

  return (
    <>
      <p className="status">{wording.contributionStatus[status]}</p>
      {moves.length > 0 && (
        <div className="actions">
          {moves.map((move) => (
            <button
              key={move}
              type="button"
              aria-expanded={opened === move}
              disabled={actions.busy}
              onClick={() => actions.open(panelOf(move))}
            >
              {w.moves[move]}
            </button>
          ))}
        </div>
      )}
      {opened && (
        <MovePanel
          key={opened}
          move={opened}
          contribution={contribution.id}
          otherName={otherName}
          actions={actions}
        />
      )}
    </>
  )
}

interface MovePanelProps {
  move: Move
  contribution: string
  otherName: string
  actions: Actions
}

function MovePanel({ move, contribution, otherName, actions }: MovePanelProps) {
  const { wording, fmt } = useI18n()
  const w = wording.exchange
  const [note, setNote] = useState('')
  const [missing, setMissing] = useState(false)

  const { takes: takesNote, label } = noteFor(move)

  function submit(event: FormEvent) {
    event.preventDefault()
    const command = moveCommand(move, contribution, note)
    if (command) void actions.run(command)
    else setMissing(true)
  }

  return (
    <Panel title={w.moves[move]}>
      <form noValidate onSubmit={submit}>
        <p>{fmt(w.moveText[move], { name: otherName })}</p>
        {takesNote && (
          <Field label={w[label]} hint={w.noteRecord} error={missing ? w.noteRequired : null}>
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
        )}
        <Failure code={actions.failure} />
        <div className="actions">
          <button type="submit" className="primary" disabled={actions.busy}>
            {w.moves[move]}
          </button>
          <button type="button" disabled={actions.busy} onClick={actions.close}>
            {wording.common.cancel}
          </button>
        </div>
      </form>
    </Panel>
  )
}
