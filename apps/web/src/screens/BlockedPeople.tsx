import { useBlockedPeople } from '@exchange/shared'
import { useEffect, useRef } from 'react'

import { useI18n } from '../app/context'
import { Link } from '../app/Link'
import { paths } from '../app/routes'
import { Failure, Written } from '../components/ui'
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
  const { people, failure, busy, unblocked, load, unblock } = useBlockedPeople(safetyApi)
  const announced = useRef<HTMLParagraphElement>(null)

  useEffect(load, [load])

  // The button that was pressed leaves with its entry.
  useEffect(() => {
    if (unblocked !== null) announced.current?.focus()
  }, [unblocked])

  const nameOf = (person: BlockedPerson) => person.name || wording.party.other

  return (
    <section aria-labelledby="blocked-heading">
      <h2 id="blocked-heading">{w.blockedHeading}</h2>
      <Failure code={failure} />
      {unblocked !== null && (
        <p className="notice" tabIndex={-1} ref={announced}>
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
                {/* An exchange this person has left names whom they blocked,
                    and is not theirs to open. */}
                {person.left ? (
                  <p>
                    <Written inline>{nameOf(person)}</Written>
                  </p>
                ) : (
                  <Link to={paths.exchange(person.exchange_id)} className="card-link">
                    <Written inline>{nameOf(person)}</Written>
                  </Link>
                )}
                <p className="hint">
                  {fmt(wording.home.reference, { code: person.display_code })}
                  <br />
                  {person.left && (
                    <>
                      {wording.claimant.blockedAfterLeaving}
                      <br />
                    </>
                  )}
                  {fmt(w.blockedSince, { date: moment(person.blocked_at) })}
                </p>
                <div className="actions">
                  <button
                    type="button"
                    disabled={busy !== null}
                    onClick={() => unblock(person, nameOf(person))}
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
