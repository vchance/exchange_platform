import type { ExchangeView as Exchange } from '@exchange/api-client';
import type { HistoryReading } from '@exchange/shared';
import { useRouter } from 'expo-router';

import { EventList } from '../components/EventList';
import { Actions, Button, Failure, Heading, Hint, P } from '../components/ui';
import { useI18n } from '../lib/context';

interface Props {
  exchange: Exchange;
  /** The history as read so far; the exchange screen reads it, since it names the parties too. */
  reading: HistoryReading;
  /** The ids of the money contributions, spoken of in words for paying and receiving. */
  money: ReadonlySet<string>;
}

/**
 * What has happened in an exchange so far, at the foot of its screen, with
 * what each party wrote along the way: a message sent with terms, a note on
 * a delivery, the reason for a dispute, a statement about closing. The latest
 * comes first to hand; what came before is a press away. It is there for
 * every exchange that has been sent, including one that closed without
 * agreement, and leads to the full record.
 */
export function History({ exchange, reading, money }: Props) {
  const { wording, moment } = useI18n();
  const router = useRouter();
  const w = wording.record;
  const { page, failure, readEarlier, readingEarlier } = reading;

  return (
    <>
      <Heading level={2}>{w.historyHeading}</Heading>
      <Failure code={failure} />
      {!page && !failure && <P>{wording.common.loading}</P>}
      {page && page.events.length === 0 && <P>{w.historyEmpty}</P>}
      {page && page.earlier != null && (
        <Actions>
          <Button
            label={readingEarlier ? wording.common.loading : w.historyEarlier}
            disabled={readingEarlier}
            onPress={() => void readEarlier()}
          />
        </Actions>
      )}
      {page && page.events.length > 0 && (
        <EventList
          events={page.events}
          parties={page.parties}
          reader={page.you}
          when={moment}
          money={money}
        />
      )}
      <Actions>
        <Button label={w.open} onPress={() => router.push(`/exchanges/${exchange.id}/record`)} />
      </Actions>
      <Hint>{wording.mobile.record.openHint}</Hint>
    </>
  );
}
