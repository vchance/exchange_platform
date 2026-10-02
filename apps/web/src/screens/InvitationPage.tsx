import type { ErrorCode } from '@exchange/api-client'
import { LAUNCH_CURRENCY } from '@exchange/shared'
import { lazy, Suspense, useEffect, useRef, useState } from 'react'

import { isComplete, useI18n, useSession } from '../app/context'
import { Link } from '../app/Link'
import { navigate } from '../app/router'
import { paths } from '../app/routes'
import { TermsView } from '../components/TermsView'
import { Failure, PageHeading, Written } from '../components/ui'
import { api, failureCode, type InvitationPreview } from '../lib/api'
import { forgetInvitationToken, takeInvitationToken } from '../lib/invitation-token'

const AccountSetup = lazy(() => import('./AccountSetup'))

/**
 * Where an invitation link lands. The proposal can be read without signing
 * in or installing anything; responding to it means signing in and claiming
 * the invitation, which takes the invited party's place in the exchange
 * (DESIGN.md §8). Reading claims nothing.
 */
export function InvitationPage() {
  const { wording, fmt, moment } = useI18n()
  const { account, ready, setAccount } = useSession()
  const w = wording.invitation

  // Read once: taking the token also removes it from the address bar.
  const [token] = useState(takeInvitationToken)
  const [preview, setPreview] = useState<InvitationPreview | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
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
        if (code === 'INVITATION_UNAVAILABLE') forgetInvitationToken()
        setFailure(code)
      },
    )
    return () => {
      cancelled = true
    }
  }, [token])

  // Once the person has asked to respond and has an account that can, claim.
  const able = account !== null && isComplete(account)
  useEffect(() => {
    if (!responding || !able || !token || claiming.current) return
    claiming.current = true
    api.claimInvitation(token).then(
      (exchange) => {
        forgetInvitationToken()
        navigate(paths.exchange(exchange.id), { replace: true })
      },
      (error: unknown) => {
        const code = failureCode(error)
        if (code === 'INVITATION_UNAVAILABLE') forgetInvitationToken()
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

  return (
    <>
      <PageHeading>{w.title}</PageHeading>

      {failure && (
        <>
          {/* The only refusal a claim gives for this reason is opening one's own link. */}
          {failure === 'ACTION_NOT_ALLOWED' ? (
            <p className="notice notice-error" role="alert">
              {w.ownInvitation}
            </p>
          ) : (
            <Failure code={failure} />
          )}
          {failure === 'INVITATION_NOT_FOR_YOU' && account && (
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
      )}

      {!preview && !failure && <p>{wording.common.loading}</p>}

      {preview && (
        <>
          <p>{fmt(w.intro, { name: sender })}</p>
          <p>{w.notBinding}</p>

          <section className="card" aria-label={fmt(wording.home.reference, { code: preview.display_code })}>
            <p className="hint">{fmt(wording.home.reference, { code: preview.display_code })}</p>
            {preview.revision.note && (
              <>
                <h2>{fmt(w.noteHeading, { name: sender })}</h2>
                <Written>{preview.revision.note}</Written>
              </>
            )}
            {/* The preview names neither the exchange's currency nor its timezone. */}
            <TermsView terms={preview.revision.terms} currency={LAUNCH_CURRENCY} you={null} />
            <p className="hint">{fmt(w.expires, { date: moment(preview.revision.expires_at) })}</p>
          </section>

          {preview.bound && <p>{w.bound}</p>}

          {responding && able && <p role="status">{w.opening}</p>}
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
        </>
      )}
    </>
  )
}
