import type { ExchangeView as Exchange } from '@exchange/api-client'
import type { HistoryReading } from '@exchange/shared'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { EventList } from '../components/EventList'
import { Failure } from '../components/ui'

interface Props {
  exchange: Exchange
  /** The history as read so far; the exchange view reads it, since it names the parties too. */
  reading: HistoryReading
  /** The ids of the money contributions, spoken of in words for paying and receiving. */
  money: ReadonlySet<string>
}

/**
 * What has happened in an exchange so far, at the foot of its page, with
 * what each party wrote along the way: a message sent with terms, a note on
 * a delivery, the reason for a dispute, a statement about closing. The latest
 * comes first to hand; what came before is a press away. It is there for
 * every exchange that has been sent, including one that closed without
 * agreement, and leads to the full record.
 */
export function History({ exchange, reading, money }: Props) {
  const { wording, moment } = useI18n()
  const w = wording.record
  const { page, failure, readEarlier, readingEarlier } = reading

  return (
    <section aria-labelledby="history-heading">
      <h2 id="history-heading">{w.historyHeading}</h2>
      <Failure code={failure} />
      {!page && !failure && <p>{wording.common.loading}</p>}
      {page && page.events.length === 0 && <p>{w.historyEmpty}</p>}
      {page && page.earlier != null && (
        <div className="actions">
          <button type="button" disabled={readingEarlier} onClick={() => void readEarlier()}>
            {readingEarlier ? wording.common.loading : w.historyEarlier}
          </button>
        </div>
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
      <div className="actions">
        <Link className="button" to={paths.record(exchange.id)}>
          {w.open}
        </Link>
      </div>
      <p className="hint">{w.openHint}</p>
    </section>
  )
}
