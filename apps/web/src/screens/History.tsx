import type { ExchangeView as Exchange } from '@yuppers/api-client'
import type { HistoryReading } from '@yuppers/shared'

import { useEffect, useRef } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { EventList } from '../components/EventList'
import { Failure } from '../components/ui'
import { announce } from '../lib/announce'
import { focusLost } from '../lib/focus'

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
  const { wording, moment, fmt } = useI18n()
  const w = wording.record
  const { page, failure, readEarlier, readingEarlier } = reading

  // Earlier entries arrive above the ones already shown, out of sight of
  // whoever asked for them, so how many came is said. When there are no more
  // to read, the button goes, and the keyboard goes to the heading. (A
  // button that is disabled while it works loses the focus in some browsers.)
  const heading = useRef<HTMLHeadingElement>(null)
  const earlier = useRef<HTMLButtonElement>(null)
  const before = useRef<number | null>(null)
  const count = page?.events.length ?? 0
  useEffect(() => {
    if (readingEarlier || before.current === null) return
    const added = count - before.current
    before.current = null
    if (added > 0) announce(fmt(wording.a11y.earlierAdded, { count: added }))
    if (focusLost()) (earlier.current ?? heading.current)?.focus()
  }, [readingEarlier, count, fmt, wording.a11y.earlierAdded])

  return (
    <section aria-labelledby="history-heading">
      <h2 id="history-heading" tabIndex={-1} ref={heading}>
        {w.historyHeading}
      </h2>
      <Failure code={failure} />
      {!page && !failure && <p>{wording.common.loading}</p>}
      {page && page.events.length === 0 && <p>{w.historyEmpty}</p>}
      {page && page.earlier != null && (
        <div className="actions">
          <button
            type="button"
            ref={earlier}
            disabled={readingEarlier}
            onClick={() => {
              before.current = count
              void readEarlier()
            }}
          >
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
