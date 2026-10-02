import type { ErrorCode } from '@exchange/api-client'
import { useEffect, useRef, useState } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { Failure, Written } from '../components/ui'
import { failureCode } from '../lib/api'
import { safetyApi, type BlockedPerson } from '../lib/safety'

/**
 * The people this account has blocked, and the way to unblock each
 * (DESIGN.md §9). A person is shown as an exchange shared with them names
 * them, with that exchange's reference; the page never learns who they are
 * beyond that.
 */
export function BlockedPeople() {
  const { wording, fmt, moment } = useI18n()
  const w = wording.safety
  const [people, setPeople] = useState<BlockedPerson[] | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  /** The exchange through which someone is being unblocked right now. */
  const [busy, setBusy] = useState<string | null>(null)
  /** The name of whoever was just unblocked. */
  const [unblocked, setUnblocked] = useState<string | null>(null)
  const announced = useRef<HTMLParagraphElement>(null)

  useEffect(() => {
    let cancelled = false
    safetyApi.blockedPeople().then(
      (found) => {
        if (!cancelled) setPeople(found)
      },
      (error: unknown) => {
        if (!cancelled) setFailure(failureCode(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [])

  // The button that was pressed leaves with its entry.
  useEffect(() => {
    if (unblocked !== null) announced.current?.focus()
  }, [unblocked])

  const nameOf = (person: BlockedPerson) => person.name || wording.party.other

  async function unblock(person: BlockedPerson) {
    setBusy(person.exchange_id)
    setFailure(null)
    setUnblocked(null)
    try {
      await safetyApi.unblock(person.exchange_id)
      setPeople(
        (listed) => listed?.filter((other) => other.exchange_id !== person.exchange_id) ?? null,
      )
      setUnblocked(nameOf(person))
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setBusy(null)
    }
  }

  return (
    <section aria-labelledby="blocked-heading">
      <h2 id="blocked-heading">{w.blockedHeading}</h2>
      <Failure code={failure} />
      {unblocked !== null && (
        <p className="notice" role="status" tabIndex={-1} ref={announced}>
          {fmt(w.unblocked, { name: unblocked })}
        </p>
      )}

      {!people && !failure && <p>{wording.common.loading}</p>}
      {people?.length === 0 && <p>{w.blockedEmpty}</p>}
      {people && people.length > 0 && (
        <>
          <p>{w.blockedIntro}</p>
          <ul className="plain">
            {people.map((person) => (
              <li key={person.exchange_id} className="card">
                <Link to={paths.exchange(person.exchange_id)} className="card-link">
                  <Written inline>{nameOf(person)}</Written>
                </Link>
                <p className="hint">
                  {fmt(wording.home.reference, { code: person.display_code })}
                  <br />
                  {fmt(w.blockedSince, { date: moment(person.blocked_at) })}
                </p>
                <div className="actions">
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => void unblock(person)}
                  >
                    {fmt(w.unblock, { name: nameOf(person) })}
                  </button>
                </div>
              </li>
            ))}
          </ul>
        </>
      )}
    </section>
  )
}
