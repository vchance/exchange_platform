import type { ErrorCode, ExchangeView as Exchange } from '@exchange/api-client';
import {
  consentShown,
  failureCode,
  otherPartyName,
  remainingRequired,
  statusesOf,
  useActions,
  type Actions as ExchangeActions,
  type ClosedReason,
  type RevisionView,
} from '@exchange/shared';
import { useIsFocused, useRouter } from 'expo-router';
import { useEffect, useRef, useState } from 'react';
import { AppState, StyleSheet, Text, type ScrollView } from 'react-native';

import { Consent } from '../components/Consent';
import { InvitationFor, InvitationLink } from '../components/InvitationLink';
import { OtherPartyLeft } from '../components/OtherPartyLeft';
import { TermsView } from '../components/TermsView';
import {
  Actions,
  Button,
  Card,
  Failure,
  Heading,
  Hint,
  Lines,
  Notice,
  P,
  Panel,
  Screen,
  Tag,
  Tags,
  Written,
} from '../components/ui';
import { useI18n } from '../lib/context';
import { api } from '../lib/session';
import { type, useColors } from '../lib/theme';
import { Ending } from './Ending';
import { Fulfillment } from './Fulfillment';

/** How often an open exchange is checked for what the other party has done. */
const CHECK_EVERY_MS = 20_000;

interface Props {
  exchange: Exchange;
  /** A just-issued invitation token, to show once. */
  issued: string | null;
  onIssued(token: string | null): void;
  onChange(exchange: Exchange): void;
  reload(): Promise<Exchange | null>;
}

/**
 * An exchange as one of its two parties sees it: its state, the revision
 * waiting to be signed, the agreement in force and where each contribution
 * stands, and every action open to this party right now. The service decides
 * what is allowed; this offers what should be, and shows the refusal if it
 * was wrong.
 */
export function ExchangeView({ exchange, issued, onIssued, onChange, reload }: Props) {
  const { wording, fmt } = useI18n();
  const router = useRouter();
  const w = wording.exchange;
  const actions = useActions(api, exchange, onChange, reload);

  const you = exchange.you;
  const open = exchange.open_revision ?? null;
  const inForce = exchange.in_force_revision ?? null;
  const writtenName = otherPartyName(exchange);
  // In a sentence, someone with no name yet is "the other party".
  const otherName = writtenName || wording.party.other;
  const active = exchange.state === 'ACTIVE';
  const revise = () => router.push(`/exchanges/${exchange.id}/revise`);

  // A newer version found while the person is in the middle of something is
  // held back and offered, not swapped in under them.
  const [newer, setNewer] = useState<Exchange | null>(null);
  const [refreshed, setRefreshed] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const current = useRef({ exchange, engaged: false });
  useEffect(() => {
    current.current = { exchange, engaged: actions.panel !== null || actions.busy };
  });

  // While this screen is the one in front and the app is open, look now and
  // then for what the other party has done. Every change needs the service,
  // so a stale screen is only ever a refusal away from being corrected.
  const closed = exchange.state === 'CLOSED';
  const exchangeId = exchange.id;
  const focused = useIsFocused();
  useEffect(() => {
    if (closed || !focused) return;
    let cancelled = false;
    async function check() {
      if (AppState.currentState !== 'active' || current.current.engaged) return;
      let found: Exchange;
      try {
        found = await api.getExchange(exchangeId);
      } catch {
        return;
      }
      if (cancelled || found.version === current.current.exchange.version) return;
      if (current.current.engaged) setNewer(found);
      else {
        onChange(found);
        setRefreshed(true);
      }
    }
    const timer = setInterval(() => void check(), CHECK_EVERY_MS);
    const subscription = AppState.addEventListener('change', (state) => {
      if (state === 'active') void check();
    });
    return () => {
      cancelled = true;
      clearInterval(timer);
      subscription.remove();
    };
  }, [exchangeId, closed, focused, onChange]);

  // A refusal with no panel left to show it in, such as one that reloaded the
  // exchange, is shown at the top, and the top is brought into view: the
  // person may be far down the screen, looking at what they just pressed.
  const refused = actions.panel === null ? actions.failure : null;
  const scroll = useRef<ScrollView>(null);
  useEffect(() => {
    if (refused) scroll.current?.scrollTo({ y: 0 });
  }, [refused]);

  const statuses = statusesOf(exchange);
  const remaining = remainingRequired(exchange);

  return (
    <Screen
      scroll={scroll}
      refreshing={refreshing}
      onRefresh={() => {
        setRefreshing(true);
        setRefreshed(false);
        void reload().finally(() => setRefreshing(false));
      }}>
      <Heading>{writtenName ? fmt(w.title, { name: writtenName }) : w.titleNoName}</Heading>
      <Tags>
        <Tag>
          {exchange.closed_outcome
            ? wording.outcomes[exchange.closed_outcome]
            : wording.states[exchange.state]}
        </Tag>
        <Tag>{fmt(wording.home.reference, { code: exchange.display_code })}</Tag>
      </Tags>
      {exchange.closed_reason && exchange.closed_reason in wording.closedReasons ? (
        <P>{wording.closedReasons[exchange.closed_reason as ClosedReason]}</P>
      ) : null}

      <Failure code={refused} />
      {(actions.done || refreshed) && !newer && <Notice>{w.updated}</Notice>}
      {newer && (
        <Notice>
          <P>{w.newer}</P>
          <Actions>
            <Button
              label={w.showLatest}
              onPress={() => {
                actions.close();
                onChange(newer);
                setNewer(null);
              }}
            />
          </Actions>
        </Notice>
      )}

      <OtherPartyLeft exchange={exchange} otherName={otherName} />
      <Counterparty
        exchange={exchange}
        otherName={otherName}
        actions={actions}
        issued={issued}
        onIssued={onIssued}
        reload={reload}
      />

      {open && (
        <OpenRevision
          exchange={exchange}
          revision={open}
          otherName={otherName}
          actions={actions}
          onRevise={revise}
        />
      )}

      {inForce && (
        <Card>
          <Heading level={2}>{w.agreementHeading}</Heading>
          <P>{w.agreementSigned}</P>
          {active && remaining > 0 && <P>{fmt(w.remaining, { count: remaining })}</P>}
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
          <Fingerprint hash={inForce.content_hash} />
          {active && !open && (
            <Actions>
              <Button label={w.amend} onPress={revise} />
            </Actions>
          )}
        </Card>
      )}

      {active && <Ending exchange={exchange} otherName={otherName} actions={actions} />}

      <Actions>
        {!closed && (
          <Button
            variant="link"
            label={w.refresh}
            onPress={() => {
              setRefreshed(false);
              void reload();
            }}
          />
        )}
        {/* Opened from a link, there is no screen underneath to go back to. */}
        {!router.canGoBack() && (
          <Button
            variant="link"
            label={wording.common.goHome}
            onPress={() => router.replace('/')}
          />
        )}
      </Actions>
    </Screen>
  );
}

/** The hash of the signed terms, which a signature is bound to (DESIGN.md §6). */
function Fingerprint({ hash }: { hash: string }) {
  const { wording, fmt } = useI18n();
  const colors = useColors();
  return (
    <Text selectable style={[type.hint, styles.fingerprint, { color: colors.muted }]}>
      {fmt(wording.terms.fingerprint, { hash })}
    </Text>
  );
}

interface CounterpartyProps {
  exchange: Exchange;
  otherName: string;
  actions: ExchangeActions;
  issued: string | null;
  onIssued(token: string | null): void;
  reload(): Promise<Exchange | null>;
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
  const { wording, fmt } = useI18n();
  const w = wording.exchange;
  const link = wording.invitationLink;
  const initiator = exchange.you === 'A';
  const claimant = exchange.claimant ?? null;

  if (exchange.state !== 'NEGOTIATING') return null;

  if (initiator && exchange.counterparty === 'UNCLAIMED') {
    return (
      <Card>
        <Heading level={2}>{link.heading}</Heading>
        {issued ? <InvitationLink key={issued} token={issued} /> : <P>{link.unclaimed}</P>}
        <P>{link.reissueIntro}</P>
        <Actions>
          <Button
            label={link.reissue}
            expanded={actions.panel === 'reissue'}
            onPress={() => actions.open('reissue')}
          />
        </Actions>
        {actions.panel === 'reissue' && (
          <Reissue exchange={exchange.id} actions={actions} onIssued={onIssued} reload={reload} />
        )}
      </Card>
    );
  }

  if (initiator && exchange.counterparty === 'CLAIMED' && claimant) {
    const signed = exchange.open_revision?.accepted_by.includes('B') ?? false;
    return (
      <Card>
        <Heading level={2}>{w.claimedHeading}</Heading>
        <P>{fmt(w.claimedBody, { name: claimant.display_name, identifier: claimant.identifier })}</P>
        {signed && <P>{w.claimedSigned}</P>}
        <Actions>
          <Button
            variant="primary"
            label={w.confirmCounterparty}
            disabled={actions.busy}
            onPress={() => void actions.run({ type: 'CONFIRM_COUNTERPARTY' })}
          />
        </Actions>
        <Hint>{w.notThem}</Hint>
      </Card>
    );
  }

  if (!initiator && exchange.counterparty === 'CLAIMED') {
    const signed = exchange.open_revision?.accepted_by.includes('B') ?? false;
    return (
      <Notice quiet>
        {fmt(signed ? w.waitingConfirmationSigned : w.waitingConfirmation, { name: otherName })}
      </Notice>
    );
  }
  return null;
}

interface ReissueProps {
  exchange: string;
  actions: ExchangeActions;
  onIssued(token: string | null): void;
  reload(): Promise<Exchange | null>;
}

function Reissue({ exchange, actions, onIssued, reload }: ReissueProps) {
  const { wording } = useI18n();
  const link = wording.invitationLink;
  const [boundTo, setBoundTo] = useState('');
  const [busy, setBusy] = useState(false);
  const [failure, setFailure] = useState<ErrorCode | null>(null);

  async function submit() {
    setBusy(true);
    setFailure(null);
    try {
      onIssued(await api.reissueInvitation(exchange, boundTo.trim() || null));
      actions.close();
    } catch (error) {
      const code = failureCode(error);
      setFailure(code);
      // Refused because someone has opened the link in the meantime.
      if (code === 'ACTION_NOT_ALLOWED') await reload();
    } finally {
      setBusy(false);
    }
  }

  return (
    <Panel title={link.reissue}>
      <InvitationFor value={boundTo} onChange={setBoundTo} />
      <Failure code={failure} />
      <Actions>
        <Button
          variant="primary"
          label={link.reissue}
          disabled={busy}
          onPress={() => void submit()}
        />
        <Button label={wording.common.cancel} disabled={busy} onPress={actions.close} />
      </Actions>
    </Panel>
  );
}

interface OpenRevisionProps {
  exchange: Exchange;
  revision: RevisionView;
  otherName: string;
  actions: ExchangeActions;
  onRevise(): void;
}

/**
 * The one revision waiting to be signed: a first proposal, a counteroffer,
 * or an amendment to the agreement in force. Its author signed it by sending
 * it; the other party can sign it, decline it, or answer with their own.
 */
function OpenRevision({ exchange, revision, otherName, actions, onRevise }: OpenRevisionProps) {
  const { wording, fmt, moment, language } = useI18n();
  const w = wording.exchange;
  const you = exchange.you;
  const yours = revision.author === you;
  const youSigned = revision.accepted_by.includes(you);
  const otherSigned = revision.accepted_by.some((slot) => slot !== you);
  const amendment = exchange.state === 'ACTIVE';
  // The initiator is never bound to someone they have not confirmed.
  const blocked = you === 'A' && exchange.counterparty !== 'CONFIRMED';

  return (
    <Card>
      <Heading level={2}>{amendment ? w.amendmentHeading : w.proposalHeading}</Heading>
      <Lines>
        <Hint>{fmt(w.version, { number: revision.sequence })}</Hint>
        <Hint>{yours ? w.sentByYou : fmt(w.sentByOther, { name: otherName })}</Hint>
        <Hint>{fmt(w.expires, { date: moment(revision.expires_at) })}</Hint>
      </Lines>

      {revision.note ? (
        <>
          <Heading level={3}>
            {yours ? w.noteFromYou : fmt(w.noteFromOther, { name: otherName })}
          </Heading>
          <Written>{revision.note}</Written>
        </>
      ) : null}

      {/* The complete terms come before any way to sign them (DESIGN.md §14.1). */}
      <TermsView
        terms={revision.terms}
        currency={exchange.currency}
        timezone={exchange.timezone}
        you={you}
      />
      <Fingerprint hash={revision.content_hash} />

      <Lines>
        <P>{youSigned ? w.signedByYou : w.unsignedByYou}</P>
        <P>
          {otherSigned
            ? fmt(w.signedByOther, { name: otherName })
            : fmt(w.unsignedByOther, { name: otherName })}
        </P>
      </Lines>

      {yours && (
        <Actions>
          <Button label={w.change} onPress={onRevise} />
          <Button
            label={w.withdraw}
            expanded={actions.panel === 'withdraw'}
            disabled={actions.busy}
            onPress={() => actions.open('withdraw')}
          />
        </Actions>
      )}

      {!yours && !youSigned && (
        <>
          {blocked && <Notice quiet>{w.acceptBlocked}</Notice>}
          <Actions>
            {!blocked && (
              <Button
                variant="primary"
                label={w.accept}
                expanded={actions.panel === 'accept'}
                disabled={actions.busy}
                onPress={() => actions.open('accept')}
              />
            )}
            <Button label={w.counter} onPress={onRevise} />
            <Button
              label={w.decline}
              expanded={actions.panel === 'decline'}
              disabled={actions.busy}
              onPress={() => actions.open('decline')}
            />
          </Actions>
        </>
      )}

      {actions.panel === 'accept' && (
        <Panel title={w.signHeading}>
          <P>{w.signIntro}</P>
          <Consent
            signLabel={w.accept}
            busy={actions.busy}
            failure={actions.failure}
            onCancel={actions.close}
            onSign={() =>
              // Acceptance names the revision: if the terms have changed since
              // this screen showed them, the service refuses (DESIGN.md §6).
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
          <P>{amendment ? w.declineKeeps : w.declineEnds}</P>
          <Failure code={actions.failure} />
          <Actions>
            <Button
              variant="primary"
              label={w.confirmDecline}
              disabled={actions.busy}
              onPress={() => void actions.run({ type: 'DECLINE', revision: revision.id })}
            />
            <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
          </Actions>
        </Panel>
      )}

      {actions.panel === 'withdraw' && (
        <Panel title={w.withdraw}>
          <P>{amendment ? w.withdrawKeeps : w.withdrawEnds}</P>
          <Failure code={actions.failure} />
          <Actions>
            <Button
              variant="primary"
              label={w.confirmWithdraw}
              disabled={actions.busy}
              onPress={() => void actions.run({ type: 'WITHDRAW', revision: revision.id })}
            />
            <Button label={wording.common.cancel} disabled={actions.busy} onPress={actions.close} />
          </Actions>
        </Panel>
      )}
    </Card>
  );
}

const styles = StyleSheet.create({
  fingerprint: { fontVariant: ['tabular-nums'] },
});
