import type { ExchangeView } from '@exchange/api-client';
import { NOTE_MAX_CHARS, type Actions as ExchangeActions } from '@exchange/shared';
import { useState } from 'react';

import { Actions, Button, Failure, Heading, Notice, P, Panel, TextField } from '../components/ui';
import { useI18n } from '../lib/context';

interface Props {
  exchange: ExchangeView;
  otherName: string;
  actions: ExchangeActions;
}

/**
 * The ways out of an agreement in force that is not simply completed
 * (DESIGN.md §5.3): ending by agreement, which either party proposes and the
 * other accepts, and closing without agreement, which either party can ask
 * for alone. Every exchange can end; neither way needs the platform to judge
 * anything.
 */
export function Ending({ exchange, otherName, actions }: Props) {
  const { wording, fmt, moment } = useI18n();
  const w = wording.exchange;
  const you = exchange.you;
  const endBy = exchange.end_proposed_by ?? null;
  const closeBy = exchange.close_requested_by ?? null;
  const [stated, setStated] = useState(false);

  // Agreeing to end answers the other party's proposal, and only that. A close
  // request is not an offer to release everything, so it cannot be "agreed
  // to": ending by agreement during one still starts with a proposal.
  const invited = endBy !== null && endBy !== you;
  const requestedOn = exchange.close_requested_at ? moment(exchange.close_requested_at) : '';

  return (
    <>
      <Heading level={2}>{w.endingHeading}</Heading>

      {endBy === you && <P>{fmt(w.endProposedByYou, { name: otherName })}</P>}
      {endBy !== null && endBy !== you && <P>{fmt(w.endProposedByOther, { name: otherName })}</P>}
      {closeBy === you && <P>{fmt(w.closeRequestedByYou, { date: requestedOn })}</P>}
      {closeBy !== null && closeBy !== you && (
        <P>{fmt(w.closeRequestedByOther, { name: otherName, date: requestedOn })}</P>
      )}
      {/* The service works out when the window ends, so both parties see one date. */}
      {closeBy !== null && exchange.close_request_lapses_at ? (
        <P>{fmt(w.closeRequestLapses, { date: moment(exchange.close_request_lapses_at) })}</P>
      ) : null}
      {stated && closeBy !== null && <Notice>{w.statementAdded}</Notice>}

      <Actions>
        {invited && (
          <Button
            variant="primary"
            label={w.agreeEnd}
            expanded={actions.panel === 'agree-end'}
            disabled={actions.busy}
            onPress={() => actions.open('agree-end')}
          />
        )}
        {endBy !== null && (
          <Button
            label={endBy === you ? w.cancelEnd : w.declineEnd}
            disabled={actions.busy}
            onPress={() => void actions.run({ type: 'CANCEL_END' })}
          />
        )}
        {endBy === null && (
          <Button
            label={w.proposeEnd}
            expanded={actions.panel === 'propose-end'}
            disabled={actions.busy}
            onPress={() => actions.open('propose-end')}
          />
        )}
        {closeBy === null && (
          <Button
            label={w.requestClose}
            expanded={actions.panel === 'request-close'}
            disabled={actions.busy}
            onPress={() => actions.open('request-close')}
          />
        )}
        {closeBy === you && (
          <Button
            label={w.retractClose}
            disabled={actions.busy}
            onPress={() => void actions.run({ type: 'RETRACT_CLOSE' })}
          />
        )}
        {closeBy !== null && (
          <Button
            label={w.addStatement}
            expanded={actions.panel === 'statement'}
            disabled={actions.busy}
            onPress={() => {
              setStated(false);
              actions.open('statement');
            }}
          />
        )}
      </Actions>

      {actions.panel === 'agree-end' && (
        <Panel title={w.agreeEnd}>
          <P>{w.agreeEndText}</P>
          <Failure code={actions.failure} />
          <Actions>
            <Button
              variant="primary"
              label={w.confirmAgreeEnd}
              disabled={actions.busy}
              onPress={() => void actions.run({ type: 'ACCEPT_END' })}
            />
            <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
          </Actions>
        </Panel>
      )}

      {actions.panel === 'propose-end' && (
        <Panel title={w.proposeEnd}>
          <P>{fmt(w.proposeEndText, { name: otherName })}</P>
          <Failure code={actions.failure} />
          <Actions>
            <Button
              variant="primary"
              label={w.sendEndProposal}
              disabled={actions.busy}
              onPress={() => void actions.run({ type: 'PROPOSE_END' })}
            />
            <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
          </Actions>
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
            const added = await actions.run({ type: 'ADD_STATEMENT', note: note ?? '' });
            setStated(added);
            return added;
          }}
        />
      )}
    </>
  );
}

interface StatementPanelProps {
  title: string;
  text: string | null;
  label: string;
  submitLabel: string;
  required: boolean;
  actions: ExchangeActions;
  onSubmit(note: string | null): Promise<boolean>;
}

function StatementPanel(props: StatementPanelProps) {
  const { wording } = useI18n();
  const { actions } = props;
  const [note, setNote] = useState('');
  const [missing, setMissing] = useState(false);

  function submit() {
    const written = note.trim();
    if (props.required && written === '') {
      setMissing(true);
      return;
    }
    void props.onSubmit(written === '' ? null : written);
  }

  return (
    <Panel title={props.title}>
      {props.text ? <P>{props.text}</P> : null}
      <TextField
        label={props.label}
        hint={wording.exchange.noteRecord}
        error={missing ? wording.exchange.noteRequired : null}
        multiline
        maxLength={NOTE_MAX_CHARS}
        value={note}
        onChangeText={(next) => {
          setNote(next);
          setMissing(false);
        }}
      />
      <Failure code={actions.failure} />
      <Actions>
        <Button
          variant="primary"
          label={props.submitLabel}
          disabled={actions.busy}
          onPress={submit}
        />
        <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
      </Actions>
    </Panel>
  );
}
