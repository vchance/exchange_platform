import type { ExchangeView } from '@yuppers/api-client';
import {
  blockLeavesAgreement,
  CLOSE_AFTER_BLOCK_PANEL,
  hasOtherParty,
  isUnconfirmedClaimant,
  offersCloseAfterBlock,
  useExchangeSafety,
  type Actions as ExchangeActions,
} from '@yuppers/shared';
import { useRouter } from 'expo-router';

import { HelpLink } from '../components/HelpLink';
import { ReportForm } from '../components/ReportForm';
import { Actions, Button, Failure, Heading, Notice, P, Panel } from '../components/ui';
import { useI18n } from '../lib/context';
import { api } from '../lib/session';
import { RequestClosePanel } from './Ending';

interface Props {
  exchange: ExchangeView;
  otherName: string;
  actions: ExchangeActions;
  reload(): Promise<unknown>;
}

/**
 * Report and block, on every view of an exchange that has someone on the
 * other side (DESIGN.md §9). Each is said in full before it is done: what
 * happens, and that the other party is not told.
 *
 * A block leaves an agreement in force standing, and the person blocked can
 * still act in it. Blocking from one says so, and once the block is made,
 * closing without agreement is offered right here, opening the same request
 * to close as "Ending the agreement" does. It comes after the block, not as
 * part of it: the other party sees a request to close, which has its own
 * window and statement and deserves its own confirmation (DESIGN.md §5.3).
 */
export function ExchangeSafety(props: Props) {
  // Until someone has joined there is nobody to report and nobody to block.
  if (!hasOtherParty(props.exchange)) return null;
  return <Controls {...props} />;
}

function Controls({ exchange, otherName, actions, reload }: Props) {
  const { wording, fmt } = useI18n();
  const w = wording.safety;

  const router = useRouter();
  // Someone the initiator has not confirmed cannot decline. Blocking takes
  // them out of the exchange instead (DESIGN.md §8, §9), and it is gone for
  // them. The block is listed, and can be lifted, with the account.
  const leaves = isUnconfirmedClaimant(exchange);
  const safety = useExchangeSafety(
    api,
    exchange.id,
    actions,
    reload,
    leaves ? () => router.dismissTo('/') : undefined,
  );
  const { blocked, busy, failure, outcome } = safety;
  // An exchange closed before anything was agreed shows no terms, and so no
  // names; the service still says who the other party was.
  const name = { name: safety.name || otherName };
  const waiting = busy || actions.busy;
  const offerClose = offersCloseAfterBlock(exchange, blocked);

  return (
    <>
      <Heading level={2}>{w.heading}</Heading>

      {blocked && outcome !== 'blocked' && <P>{fmt(w.blocked, name)}</P>}
      {/* What just happened is read out: the button that did it may be gone. */}
      {outcome === 'reported' && <Notice>{w.reportSent}</Notice>}
      {outcome === 'blocked' && <Notice>{fmt(w.blocked, name)}</Notice>}
      {outcome === 'unblocked' && <Notice>{fmt(w.unblocked, name)}</Notice>}
      {offerClose && (
        <>
          <P>{fmt(w.blockedInForce, name)}</P>
          <Actions>
            <Button
              label={wording.exchange.requestClose}
              expanded={actions.panel === CLOSE_AFTER_BLOCK_PANEL}
              disabled={waiting}
              onPress={() => actions.open(CLOSE_AFTER_BLOCK_PANEL)}
            />
          </Actions>
          {actions.panel === CLOSE_AFTER_BLOCK_PANEL && (
            <RequestClosePanel otherName={name.name} actions={actions} />
          )}
        </>
      )}

      <Actions>
        <Button
          label={w.report}
          expanded={safety.panel === 'report'}
          disabled={waiting}
          onPress={() => safety.begin('report')}
        />
        {blocked === false && (
          <Button
            label={fmt(w.block, name)}
            expanded={safety.panel === 'block'}
            disabled={waiting}
            onPress={() => safety.begin('block')}
          />
        )}
        {blocked === true && (
          <Button label={fmt(w.unblock, name)} disabled={waiting} onPress={safety.unblock} />
        )}
      </Actions>
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
          {/* All of what a block does comes before the button that does it. */}
          <P>{fmt(w.blockStops, name)}</P>
          {leaves ? (
            <P>{wording.claimant.blockLeaves}</P>
          ) : (
            <>
              <P>{w.blockEnds}</P>
              <P>{w.blockKeeps}</P>
              {blockLeavesAgreement(exchange) && (
                <>
                  <P>{fmt(w.blockInForce, name)}</P>
                  <P>{fmt(w.blockThenClose, name)}</P>
                </>
              )}
            </>
          )}
          <P>{fmt(w.blockQuiet, name)}</P>
          <HelpLink place="blocking" />
          <Failure code={failure} />
          <Actions>
            <Button
              variant="primary"
              label={fmt(w.confirmBlock, name)}
              disabled={busy}
              onPress={safety.block}
            />
            <Button label={wording.common.cancel} disabled={busy} onPress={actions.close} />
          </Actions>
        </Panel>
      )}
    </>
  );
}
