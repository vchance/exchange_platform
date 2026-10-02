import type { ErrorCode, ExchangeSummary } from '@exchange/api-client'
import { useEffect, useState } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { navigate } from '../app/router'
import { paths } from '../app/routes'
import { Failure, PageHeading, Written } from '../components/ui'
import { api, failureCode } from '../lib/api'

/** The signed-in person's exchanges, most recently changed first, and the way to start one. */
export default function HomePage() {
  const { wording, fmt, moment } = useI18n()
  const w = wording.home

  const [exchanges, setExchanges] = useState<ExchangeSummary[] | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [starting, setStarting] = useState(false)
  const [tooMany, setTooMany] = useState(false)

  useEffect(() => {
    let cancelled = false
    api.listExchanges().then(
      (found) => {
        if (!cancelled) setExchanges(found)
      },
      (error: unknown) => {
        if (!cancelled) setFailure(failureCode(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

  async function start() {
    setStarting(true)
    setFailure(null)
    setTooMany(false)
    try {
      // Due dates are read in the timezone of whoever starts the exchange.
      const timezone = Intl.DateTimeFormat().resolvedOptions().timeZone
      const exchange = await api.createExchange(timezone)
      navigate(paths.exchange(exchange.id))
    } catch (error) {
      const code = failureCode(error)
      // Here the limit is on exchanges started today, not on codes.
      if (code === 'TOO_MANY_REQUESTS') setTooMany(true)
      else setFailure(code)
      setStarting(false)
    }
  }

  return (
    <>
      <PageHeading>{w.title}</PageHeading>
      <div className="actions">
        <button type="button" className="primary" disabled={starting} onClick={start}>
          {w.start}
        </button>
      </div>
      <Failure code={failure} />
      {tooMany && (
        <p className="notice notice-error" role="alert">
          {w.tooManyToday}
        </p>
      )}

      {!exchanges && !failure && <p>{wording.common.loading}</p>}
      {exchanges?.length === 0 && <p>{w.empty}</p>}
      {exchanges && exchanges.length > 0 && (
        <ul className="plain cards">
          {exchanges.map((exchange) => (
            <li key={exchange.id} className="card">
              <Link to={paths.exchange(exchange.id)} className="card-link">
                {exchange.other_party_name ? (
                  <Written inline>{fmt(w.withParty, { name: exchange.other_party_name })}</Written>
                ) : (
                  w.noParty
                )}
              </Link>
              <p>
                <span className="tag">
                  {exchange.closed_outcome
                    ? wording.outcomes[exchange.closed_outcome]
                    : wording.states[exchange.state]}
                </span>
              </p>
              <p className="hint">
                {fmt(w.reference, { code: exchange.display_code })}
                <br />
                {fmt(w.updated, { date: moment(exchange.updated_at) })}
              </p>
            </li>
          ))}
        </ul>
      )}
    </>
  )
}
