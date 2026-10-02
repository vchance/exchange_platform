import type { ErrorCode, ExchangeView as Exchange } from '@exchange/api-client'
import type { HistoryPage } from '@exchange/shared'
import { useEffect, useState } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { EventList } from '../components/EventList'
import { Failure } from '../components/ui'
import { api, failureCode } from '../lib/api'

/**
 * What has happened in an exchange so far, at the foot of its page, with
 * what each party wrote along the way: a message sent with terms, a note on
 * a delivery, the reason for a dispute, a statement about closing. It is
 * there for every exchange that has been sent, including one that closed
 * without agreement, and leads to the full record.
 */
export function History({ exchange }: { exchange: Exchange }) {
  const { wording, moment } = useI18n()
  const w = wording.record
  const [page, setPage] = useState<HistoryPage | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)

  // Every change to an exchange adds to its history, so it is read again
  // whenever the exchange on screen is a newer version. Until the new one
  // arrives, the one already shown stays.
  useEffect(() => {
    let cancelled = false
    api.history(exchange.id).then(
      (found) => {
        if (cancelled) return
        setPage(found)
        setFailure(null)
      },
      (error: unknown) => {
        if (!cancelled) setFailure(failureCode(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [exchange.id, exchange.version])

  return (
    <section aria-labelledby="history-heading">
      <h2 id="history-heading">{w.historyHeading}</h2>
      <Failure code={failure} />
      {!page && !failure && <p>{wording.common.loading}</p>}
      {page && page.events.length === 0 && <p>{w.historyEmpty}</p>}
      {page && page.earlier != null && <p className="hint">{w.historyEarlier}</p>}
      {page && page.events.length > 0 && (
        <EventList events={page.events} parties={page.parties} reader={page.you} when={moment} />
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
