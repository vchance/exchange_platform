import type { ErrorCode } from '@exchange/api-client'
import { lazy, Suspense, useEffect, useRef, useState } from 'react'

import { isComplete, useI18n, useSession } from '../app/context'
import { Link } from '../app/Link'
import { navigate } from '../app/router'
import { invitationToken, paths } from '../app/routes'
import { TermsView } from '../components/TermsView'
import { ErrorNote, Failure, PageHeading, Written } from '../components/ui'
import { useAnnouncement } from '../lib/announce'
import { api, failureCode, type InvitationPreview } from '../lib/api'
import { forgetInvitationToken, takeInvitationToken } from '../lib/invitation-token'
import { InvitationReport } from './InvitationReport'

const AccountSetup = lazy(() => import('./AccountSetup'))

/**
 * Where an invitation link lands. The proposal can be read without signing
 * in or installing anything; responding to it means signing in and claiming
 * the invitation, which takes the invited party's place in the exchange
 * (DESIGN.md §8). Reading claims nothing.
 *
 * A second link pasted into a tab already showing this page changes only
 * the fragment, so the browser does not load the page again. The token is
 * taken again then, and a different one shows its own proposal from the
 * start, with nothing kept from the one before.
 */
export function InvitationPage() {
  // Taking the token also removes it from the address bar.
  const [token, setToken] = useState(takeInvitationToken)
  useEffect(() => {
    const taken = () => {
      // A fragment that is not a token, or no fragment at all, leaves the
      // page as it is: taking the token is what emptied it.
      if (!invitationToken(window.location.hash)) return
      setToken(takeInvitationToken())
    }
    window.addEventListener('hashchange', taken)
    return () => window.removeEventListener('hashchange', taken)
  }, [])
  return <Invitation key={token ?? ''} token={token} />
}

function Invitation({ token }: { token: string | null }) {
  const { wording, fmt, moment } = useI18n()
  const { account, ready, setAccount } = useSession()
  const w = wording.invitation

  const [preview, setPreview] = useState<InvitationPreview | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [spent, setSpent] = useState(false)
  const [responding, setResponding] = useState(false)
  const claiming = useRef(false)

  useEffect(() => {
    if (!token) return
    let cancelled = false
    api.previewInvitation(token).then(
      (found) => {
        if (!cancelled) setPreview(found)
      },
      (error: unknown) => {
        if (cancelled) return
        const code = failureCode(error)
        if (code === 'INVITATION_UNAVAILABLE') setSpent(true)
        else setFailure(code)
      },
    )
    return () => {
      cancelled = true
    }
  }, [token])

  const able = account !== null && isComplete(account)
  useAnnouncement(responding && able ? w.opening : null)

  // A link that no longer shows its proposal has usually been used, and
  // people come back to the message it arrived in. Claiming again with the
  // account that already claimed it changes nothing and answers with the
  // exchange, so the person who used the link is taken to it; anyone else
  // gets the same refusal as before.
  useEffect(() => {
    if (!spent || !ready || !token) return
    if (!able) {
      forgetInvitationToken(token)
      return
    }
    if (claiming.current) return
    claiming.current = true
    api.claimInvitation(token).then(
      (exchange) => {
        forgetInvitationToken(token)
        navigate(paths.exchange(exchange.id), { replace: true })
      },
      (error: unknown) => {
        forgetInvitationToken(token)
        claiming.current = false
        setFailure(failureCode(error))
      },
    )
  }, [spent, ready, able, token])

  // Once the person has asked to respond and has an account that can, claim.
  useEffect(() => {
    if (!responding || !able || !token || claiming.current) return
    claiming.current = true
    api.claimInvitation(token).then(
      (exchange) => {
        forgetInvitationToken(token)
        navigate(paths.exchange(exchange.id), { replace: true })
      },
      (error: unknown) => {
        const code = failureCode(error)
        if (code === 'INVITATION_UNAVAILABLE') forgetInvitationToken(token)
        claiming.current = false
        setResponding(false)
        setFailure(code)
      },
    )
  }, [responding, able, token])

  if (!token) {
    return (
      <>
        <PageHeading>{w.missingTitle}</PageHeading>
        <p>{w.missing}</p>
      </>
    )
  }

  const sender = preview?.revision.terms.party_a_name ?? ''

  // A link that cannot be read is refused at the top of an otherwise empty
  // page. A claim is refused next to the button that asked for it, which is
  // below the whole proposal.
  const refused: ErrorCode | null =
    failure ?? (spent && ready && !able ? 'INVITATION_UNAVAILABLE' : null)
  const refusal = refused && (
    <>
      {/* The only refusal a claim gives for this reason is opening one's own link. */}
      {refused === 'ACTION_NOT_ALLOWED' ? (
        <ErrorNote>{w.ownInvitation}</ErrorNote>
      ) : (
        <Failure code={refused} />
      )}
      {refused === 'INVITATION_UNAVAILABLE' && !account && (
        <p>
          {w.alreadyResponded} <Link to={paths.home}>{wording.signIn.title}</Link>
        </p>
      )}
      {refused === 'INVITATION_NOT_FOR_YOU' && account && (
        <p>
          <button
            type="button"
            onClick={() => {
              setFailure(null)
              api.signOut().then(
                () => setAccount(null),
                () => setAccount(null),
              )
            }}
          >
            {wording.nav.signOut}
          </button>
        </p>
      )}
      {account && (
        <p>
          <Link to={paths.home}>{wording.common.goHome}</Link>
        </p>
      )}
    </>
  )

  return (
    <>
      <PageHeading>{w.title}</PageHeading>

      {!preview && refusal}
      {!preview && !refused && <p>{wording.common.loading}</p>}

      {preview && (
        <>
          {/* An invitation that names nobody can be opened by whoever holds
              the link, so its sender has to confirm them before they can do
              more than sign (DESIGN.md §8). */}
          {preview.bound ? (
            <p>{fmt(able ? w.introSignedIn : w.intro, { name: sender })}</p>
          ) : (
            <p>
              {fmt(
                able ? wording.claimant.invitationIntroSignedIn : wording.claimant.invitationIntro,
                { name: sender },
              )}
            </p>
          )}
          <p>{w.notBinding}</p>

          <section
            className="card"
            aria-label={fmt(wording.home.reference, { code: preview.display_code })}
          >
            <p className="hint">{fmt(wording.home.reference, { code: preview.display_code })}</p>
            {preview.revision.note && (
              <>
                <h2>{fmt(w.noteHeading, { name: sender })}</h2>
                <Written>{preview.revision.note}</Written>
              </>
            )}
            <TermsView
              terms={preview.revision.terms}
              currency={preview.currency}
              timezone={preview.timezone}
              you={null}
              level={2}
            />
            <p className="hint">{fmt(w.expires, { date: moment(preview.revision.expires_at) })}</p>
          </section>

          {preview.bound && <p>{w.bound}</p>}
          {refusal}

          {responding && able && <p>{w.opening}</p>}
          {responding && !able && (
            <Suspense fallback={<p>{wording.common.loading}</p>}>
              <AccountSetup headingLevel="h2" />
            </Suspense>
          )}
          {!responding && ready && (
            <div className="actions">
              <button type="button" className="primary" onClick={() => setResponding(true)}>
                {account && able ? fmt(w.respondAs, { name: account.display_name }) : w.respond}
              </button>
            </div>
          )}
          <InvitationReport token={token} />
        </>
      )}
    </>
  )
}
