import type { ExchangeView as Exchange } from '@exchange/api-client';
import { useHistory } from '@exchange/shared';
import { useRouter } from 'expo-router';

import { EventList } from '../components/EventList';
import { Actions, Button, Failure, Heading, Hint, P } from '../components/ui';
import { useI18n } from '../lib/context';
import { api } from '../lib/session';

/**
 * What has happened in an exchange so far, at the foot of its screen, with
 * what each party wrote along the way: a message sent with terms, a note on
 * a delivery, the reason for a dispute, a statement about closing. It is
 * there for every exchange that has been sent, including one that closed
 * without agreement, and leads to the full record.
 */
export function History({ exchange }: { exchange: Exchange }) {
  const { wording, moment } = useI18n();
  const router = useRouter();
  const w = wording.record;
  // Read again whenever the exchange on screen is a newer version.
  const { page, failure } = useHistory(api, exchange);

  return (
    <>
      <Heading level={2}>{w.historyHeading}</Heading>
      <Failure code={failure} />
      {!page && !failure && <P>{wording.common.loading}</P>}
      {page && page.events.length === 0 && <P>{w.historyEmpty}</P>}
      {page && page.earlier != null && <Hint>{w.historyEarlier}</Hint>}
      {page && page.events.length > 0 && (
        <EventList events={page.events} parties={page.parties} reader={page.you} when={moment} />
      )}
      <Actions>
        <Button label={w.open} onPress={() => router.push(`/exchanges/${exchange.id}/record`)} />
      </Actions>
      <Hint>{wording.mobile.record.openHint}</Hint>
    </>
  );
}
