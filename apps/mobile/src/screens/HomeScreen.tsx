import type { ErrorCode, ExchangeSummary } from '@exchange/api-client';
import { failureCode, groupExchanges, labelText } from '@exchange/shared';
import { getCalendars } from 'expo-localization';
import { useFocusEffect, useRouter } from 'expo-router';
import { useCallback, useState } from 'react';
import { Pressable, StyleSheet } from 'react-native';

import {
  Actions,
  Button,
  Card,
  ErrorNote,
  Failure,
  Heading,
  Hint,
  P,
  Screen,
  Tag,
  Tags,
  Written,
} from '../components/ui';
import { useI18n } from '../lib/context';
import { forgetInvitation } from '../lib/invitation';
import { api } from '../lib/session';
import { TOUCH_TARGET } from '../lib/theme';

/** The timezone due dates are read in: that of whoever starts the exchange. */
function deviceTimezone(): string {
  return getCalendars()[0]?.timeZone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
}

/**
 * The signed-in person's exchanges, the way to start one, and the way to
 * open an invitation that did not open by itself. What is in progress comes
 * first, since that is what may be waiting on them; drafts they never sent
 * come next; what is closed is kept but folded away, so it never buries the
 * rest. Within a group, most recently changed first.
 */
export function HomeScreen() {
  const { wording, fmt } = useI18n();
  const router = useRouter();
  const w = wording.home;

  const [exchanges, setExchanges] = useState<ExchangeSummary[] | null>(null);
  const [failure, setFailure] = useState<ErrorCode | null>(null);
  const [starting, setStarting] = useState(false);
  const [tooMany, setTooMany] = useState(false);
  const [refreshing, setRefreshing] = useState(false);
  const [showClosed, setShowClosed] = useState(false);

  const load = useCallback(async () => {
    try {
      setExchanges(await api.listExchanges());
      setFailure(null);
    } catch (error) {
      setFailure(failureCode(error));
    }
  }, []);

  // Coming back from an exchange, the list shows what changed there.
  useFocusEffect(
    useCallback(() => {
      void load();
    }, [load]),
  );

  async function start() {
    setStarting(true);
    setFailure(null);
    setTooMany(false);
    try {
      const exchange = await api.createExchange(deviceTimezone());
      router.push(`/exchanges/${exchange.id}`);
    } catch (error) {
      const code = failureCode(error);
      // Here the limit is on exchanges started today, not on codes.
      if (code === 'TOO_MANY_REQUESTS') setTooMany(true);
      else setFailure(code);
    } finally {
      setStarting(false);
    }
  }

  const groups = exchanges ? groupExchanges(exchanges) : null;

  return (
    <Screen
      refreshing={refreshing}
      onRefresh={() => {
        setRefreshing(true);
        void load().finally(() => setRefreshing(false));
      }}>
      <Heading>{w.title}</Heading>
      <Actions>
        <Button variant="primary" label={w.start} disabled={starting} onPress={() => void start()} />
        <Button
          label={wording.mobile.openInvitation.title}
          onPress={() => {
            // Asking from here means a new one: an invitation looked at earlier is let go.
            forgetInvitation();
            router.push('/invitation');
          }}
        />
      </Actions>
      <Failure code={failure} />
      {tooMany && <ErrorNote>{w.tooManyToday}</ErrorNote>}

      {!exchanges && !failure && <P>{wording.common.loading}</P>}
      {exchanges?.length === 0 && <P>{w.empty}</P>}
      {groups && (
        <>
          <Group heading={w.groupOpen} exchanges={groups.open} />
          <Group heading={w.groupDrafts} exchanges={groups.drafts} />
          {groups.closed.length > 0 && (
            <>
              <Heading level={2}>{w.groupClosed}</Heading>
              <Actions>
                <Button
                  label={
                    showClosed ? w.hideClosed : fmt(w.showClosed, { count: groups.closed.length })
                  }
                  expanded={showClosed}
                  onPress={() => setShowClosed((shown) => !shown)}
                />
              </Actions>
              {showClosed && <Cards exchanges={groups.closed} />}
            </>
          )}
        </>
      )}
    </Screen>
  );
}

function Group({ heading, exchanges }: { heading: string; exchanges: readonly ExchangeSummary[] }) {
  if (exchanges.length === 0) return null;
  return (
    <>
      <Heading level={2}>{heading}</Heading>
      <Cards exchanges={exchanges} />
    </>
  );
}

function Cards({ exchanges }: { exchanges: readonly ExchangeSummary[] }) {
  const { wording, fmt, moment } = useI18n();
  const router = useRouter();
  const w = wording.home;
  return exchanges.map((exchange) => {
    const state = exchange.closed_outcome
      ? wording.outcomes[exchange.closed_outcome]
      : wording.states[exchange.state];
    const title = exchange.other_party_name
      ? fmt(w.withParty, { name: exchange.other_party_name })
      : w.noParty;
    const reference = fmt(w.reference, { code: exchange.display_code });
    const updated = fmt(w.updated, { date: moment(exchange.updated_at) });
    // The name is the other party's own words, and could be written to read
    // like a state or a reference. In the spoken label the product's facts
    // come first and the name last, on one line and without characters that
    // change the direction of text.
    const spokenTitle = exchange.other_party_name
      ? fmt(w.withParty, { name: labelText(exchange.other_party_name) })
      : w.noParty;
    return (
      // One button per exchange, read as one: where it stands, its reference
      // and when it changed, then who it is with, then that it opens.
      <Pressable
        key={exchange.id}
        accessibilityRole="button"
        accessibilityLabel={[state, reference, updated, spokenTitle].join('. ')}
        accessibilityHint={wording.a11y.openExchange}
        onPress={() => router.push(`/exchanges/${exchange.id}`)}
        style={({ pressed }) => [styles.row, pressed && styles.pressed]}>
        <Card>
          {exchange.other_party_name ? <Written>{title}</Written> : <P>{title}</P>}
          <Tags>
            <Tag>{state}</Tag>
          </Tags>
          <Hint>{reference}</Hint>
          <Hint>{updated}</Hint>
        </Card>
      </Pressable>
    );
  });
}

const styles = StyleSheet.create({
  row: { minHeight: TOUCH_TARGET },
  pressed: { opacity: 0.6 },
});
