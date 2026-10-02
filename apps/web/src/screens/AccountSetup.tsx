import type { ErrorCode } from '@exchange/api-client'
import { useEffect, useRef, useState, type FormEvent } from 'react'

import { isComplete, useI18n, useSession } from '../app/context'
import { Failure, Field, Notice, PageHeading } from '../components/ui'
import { api, failureCode } from '../lib/api'
import { ProfileForm } from './ProfileForm'

/**
 * Everything between "not signed in" and "ready to act": signing in with a
 * one-time code, then, for a new account, the profile. It shows whichever
 * step is next and nothing once both are done, so it can stand in for any
 * page that needs an account.
 */
export default function AccountSetup({ headingLevel = 'h1' }: { headingLevel?: 'h1' | 'h2' }) {
  const { wording } = useI18n()
  const { account } = useSession()
  const heading = (text: string) =>
    headingLevel === 'h1' ? <PageHeading>{text}</PageHeading> : <h2>{text}</h2>

  if (!account) {
    return (
      <section>
        {heading(wording.signIn.title)}
        <SignIn />
      </section>
    )
  }
  if (!isComplete(account)) {
    return (
      <section>
        {heading(wording.profile.firstTitle)}
        <p>{wording.profile.firstIntro}</p>
        <ProfileForm account={account} first />
      </section>
    )
  }
  return null
}

/**
 * Signing in: an email address or phone number, then the six-digit code sent
 * to it. The first time, this creates the account. The service answers a
 * request for a code the same way whether or not an account exists, and so
 * does this form.
 */
function SignIn() {
  const { wording, fmt, language } = useI18n()
  const { setAccount } = useSession()
  const w = wording.signIn

  const [identifier, setIdentifier] = useState('')
  const [sentTo, setSentTo] = useState<string | null>(null)
  const [code, setCode] = useState('')
  const [busy, setBusy] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [resent, setResent] = useState(false)
  const codeInput = useRef<HTMLInputElement>(null)

  // Each step starts with the focus on the one thing it asks for.
  useEffect(() => {
    if (sentTo) codeInput.current?.focus()
  }, [sentTo])

  async function requestCode(to: string, again: boolean) {
    setBusy(true)
    setFailure(null)
    setResent(false)
    try {
      await api.requestCode(to)
      setSentTo(to)
      setResent(again)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setBusy(false)
    }
  }

  async function signIn(event: FormEvent) {
    event.preventDefault()
    if (!sentTo) return
    setBusy(true)
    setFailure(null)
    setResent(false)
    try {
      // The language on screen becomes a new account's language.
      const session = await api.signIn(sentTo, code.trim(), language)
      setAccount(session.account)
    } catch (error) {
      setFailure(failureCode(error))
      setBusy(false)
    }
  }

  if (!sentTo) {
    return (
      // The two steps are separate forms with separate fields, so a browser
      // offering to fill in the code is not looking at the address field.
      <form
        key="identifier"
        noValidate
        onSubmit={(event) => {
          event.preventDefault()
          void requestCode(identifier.trim(), false)
        }}
      >
        <p>{w.intro}</p>
        <Field label={w.identifierLabel} hint={w.identifierHint}>
          {(control) => (
            <input
              {...control}
              type="text"
              inputMode="email"
              autoComplete="username"
              autoCapitalize="none"
              spellCheck={false}
              value={identifier}
              onChange={(event) => setIdentifier(event.target.value)}
            />
          )}
        </Field>
        <Failure code={failure} />
        <div className="actions">
          <button type="submit" className="primary" disabled={busy}>
            {w.sendCode}
          </button>
        </div>
      </form>
    )
  }

  return (
    <form key="code" noValidate onSubmit={signIn}>
      <p>{fmt(w.codeSent, { identifier: sentTo })}</p>
      <Field label={w.codeLabel} hint={w.codeHint}>
        {(control) => (
          <input
            {...control}
            ref={codeInput}
            type="text"
            inputMode="numeric"
            autoComplete="one-time-code"
            maxLength={6}
            className="code"
            value={code}
            onChange={(event) => setCode(event.target.value)}
          />
        )}
      </Field>
      <Failure code={failure} />
      {resent && <Notice>{w.resent}</Notice>}
      <div className="actions">
        <button type="submit" className="primary" disabled={busy}>
          {w.submit}
        </button>
        <button type="button" disabled={busy} onClick={() => void requestCode(sentTo, true)}>
          {w.resend}
        </button>
        <button
          type="button"
          className="link"
          disabled={busy}
          onClick={() => {
            setSentTo(null)
            setCode('')
            setFailure(null)
            setResent(false)
          }}
        >
          {w.changeIdentifier}
        </button>
      </div>
    </form>
  )
}
