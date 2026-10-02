import type { components } from '@exchange/api-client';
import {
  moveCommand,
  movesFor,
  noteFor,
  NOTE_MAX_CHARS,
  type Actions as ExchangeActions,
  type Move,
  type Slot,
} from '@exchange/shared';
import { useState } from 'react';

import { Actions, Button, Failure, P, Panel, TextField } from '../components/ui';
import { useI18n } from '../lib/context';

type Contribution = components['schemas']['ContributionDto'];
type Status = components['schemas']['Status'];

interface Props {
  contribution: Contribution;
  status: Status;
  you: Slot;
  /** The other party's name, for telling the person who an action affects. */
  otherName: string;
  /** Whether the agreement is still in force; a closed one is only read. */
  active: boolean;
  actions: ExchangeActions;
}

/**
 * Where one contribution stands and what the reader can do about it: the
 * provider marks it delivered, the recipient confirms, disputes or waives
 * (DESIGN.md §5.2). A claim is never a confirmation, and the two are worded
 * differently so neither party mistakes one for the other.
 */
export function Fulfillment({ contribution, status, you, otherName, active, actions }: Props) {
  const { wording } = useI18n();
  const w = wording.exchange;
  const role = contribution.from === you ? 'PROVIDER' : 'RECIPIENT';
  const moves = active ? movesFor(status, role) : [];
  const panelOf = (move: Move) => `move:${contribution.id}:${move}`;
  const opened = moves.find((move) => actions.panel === panelOf(move));

  return (
    <>
      <P style={{ fontWeight: '600' }}>{wording.contributionStatus[status]}</P>
      {moves.length > 0 && (
        <Actions>
          {moves.map((move) => (
            <Button
              key={move}
              label={w.moves[move]}
              expanded={opened === move}
              disabled={actions.busy}
              onPress={() => actions.open(panelOf(move))}
            />
          ))}
        </Actions>
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
  );
}

interface MovePanelProps {
  move: Move;
  contribution: string;
  otherName: string;
  actions: ExchangeActions;
}

/** The second look before a move is sent, with the note it takes or needs. */
function MovePanel({ move, contribution, otherName, actions }: MovePanelProps) {
  const { wording, fmt } = useI18n();
  const w = wording.exchange;
  const [note, setNote] = useState('');
  const [missing, setMissing] = useState(false);
  const { takes, label } = noteFor(move);

  function submit() {
    const command = moveCommand(move, contribution, note);
    if (command) void actions.run(command);
    else setMissing(true);
  }

  return (
    <Panel title={w.moves[move]}>
      <P>{fmt(w.moveText[move], { name: otherName })}</P>
      {takes && (
        <TextField
          label={w[label]}
          hint={w.noteRecord}
          error={missing ? w.noteRequired : null}
          multiline
          maxLength={NOTE_MAX_CHARS}
          value={note}
          onChangeText={(next) => {
            setNote(next);
            setMissing(false);
          }}
        />
      )}
      <Failure code={actions.failure} />
      <Actions>
        <Button
          variant="primary"
          label={w.moves[move]}
          disabled={actions.busy}
          onPress={submit}
        />
        <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
      </Actions>
    </Panel>
  );
}
