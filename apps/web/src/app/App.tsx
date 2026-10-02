import type { Account, ErrorCode } from '@exchange/api-client'
import { directionOf, languages, pickLanguage, type Language, type Wording } from '@exchange/shared'
import { lazy, Suspense, useCallback, useEffect, useMemo, useState, type ReactNode } from 'react'

import { Failure, PageHeading } from '../components/ui'
import { api, failureCode, onSignedOut } from '../lib/api'
import { InvitationPage } from '../screens/InvitationPage'
import {
  createI18n,
  I18nContext,
  isComplete,
  SessionContext,
  useI18n,
  useSession,
  type Session,
} from './context'
import { Link } from './Link'
import { usePathname } from './router'
import { matchRoute, paths } from './routes'
import { loadWording, rememberLanguage } from './wording'

// The invitation page is the front door and has a size budget, so it is the
// only screen in the first bundle. Everything else loads when it is needed
// (DESIGN.md §13.5).
const AccountSetup = lazy(() => import('../screens/AccountSetup'))
const HomePage = lazy(() => import('../screens/HomePage'))
const AccountPage = lazy(() => import('../screens/AccountPage'))
const ExchangePage = lazy(() => import('../screens/ExchangePage'))
const RecordPage = lazy(() => import('../screens/RecordPage'))

interface Props {
  initialLanguage: Language
  initialWording: Wording
}

export function App({ initialLanguage, initialWording }: Props) {
  const [account, setAccountState] = useState<Account | null>(null)
  const [ready, setReady] = useState(false)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [attempt, setAttempt] = useState(0)

  // The account's language once signed in, this device's before that
  // (DESIGN.md §4.2). The device takes up the language of whoever signs in,
  // so signing out leaves the screen in the language it was in.
  const [deviceChoice, setDeviceChoice] = useState(initialLanguage)
  const setAccount = useCallback((next: Account | null) => {
    setAccountState(next)
    if (!next) return
    const language = pickLanguage([next.language])
    rememberLanguage(language)
    setDeviceChoice(language)
  }, [])

  useEffect(() => {
    let cancelled = false
    api.me().then(
      (found) => {
        if (cancelled) return
        setAccount(found)
        setFailure(null)
        setReady(true)
      },
      (error: unknown) => {
        if (cancelled) return
        setFailure(failureCode(error))
        setReady(true)
      },
    )
    return () => {
      cancelled = true
    }
  }, [attempt, setAccount])

  // A session can end at any time: it expires, or is signed out elsewhere.
  useEffect(() => onSignedOut(() => setAccount(null)), [setAccount])

  // The wording, `lang`, `dir` and every format change together, once the
  // new language's wording has arrived.
  const [shown, setShown] = useState({ language: initialLanguage, wording: initialWording })
  const wanted = account ? pickLanguage([account.language]) : deviceChoice

  useEffect(() => {
    if (wanted === shown.language) return
    let cancelled = false
    loadWording(wanted).then(
      (wording) => {
        if (!cancelled) setShown({ language: wanted, wording })
      },
      () => {
        // Offline, most likely. The language already on screen stays.
      },
    )
    return () => {
      cancelled = true
    }
  }, [wanted, shown.language])

  useEffect(() => {
    document.documentElement.lang = shown.language
    document.documentElement.dir = directionOf(shown.language)
  }, [shown.language])

  const signedIn = account !== null
  const setLanguage = useCallback(
    (language: Language) => {
      rememberLanguage(language)
      setDeviceChoice(language)
      if (signedIn) {
        // The preference belongs to the account and follows it to other devices.
        api.updateMe({ language }).then(setAccount, () => {})
      }
    },
    [signedIn, setAccount],
  )

  const i18n = useMemo(
    () => createI18n(shown.language, shown.wording, setLanguage),
    [shown, setLanguage],
  )
  const session = useMemo<Session>(
    () => ({
      ready,
      failure,
      account,
      setAccount,
      retry: () => {
        setReady(false)
        setAttempt((count) => count + 1)
      },
    }),
    [ready, failure, account, setAccount],
  )

  return (
    <I18nContext value={i18n}>
      <SessionContext value={session}>
        <Shell />
      </SessionContext>
    </I18nContext>
  )
}

function Shell() {
  const { wording, language, setLanguage } = useI18n()
  const { account } = useSession()
  const pathname = usePathname()
  const route = matchRoute(pathname)
  const current = (name: string) => (route.name === name ? 'page' : undefined)

  let page: ReactNode
  switch (route.name) {
    case 'invitation':
      page = <InvitationPage />
      break
    case 'home':
      page = (
        <Gate>
          <HomePage />
        </Gate>
      )
      break
    case 'account':
      page = (
        <Gate>
          <AccountPage />
        </Gate>
      )
      break
    case 'exchange':
    case 'revise':
      page = (
        <Gate>
          <ExchangePage key={route.id} id={route.id} revising={route.name === 'revise'} />
        </Gate>
      )
      break
    case 'record':
      page = (
        <Gate>
          <RecordPage key={route.id} id={route.id} />
        </Gate>
      )
      break
    default:
      page = <NotFound />
  }

  return (
    <>
      <a className="skip" href="#content">
        {wording.common.skipToContent}
      </a>
      <header className="site">
        <Link to={paths.home} className="brand">
          {wording.productName}
        </Link>
        {account && isComplete(account) && (
          <nav aria-label={wording.nav.label}>
            <Link to={paths.home} aria-current={current('home')}>
              {wording.nav.exchanges}
            </Link>
            <Link to={paths.account} aria-current={current('account')}>
              {wording.nav.account}
            </Link>
          </nav>
        )}
        <label className="language">
          <span className="visually-hidden">{wording.nav.language}</span>
          <select
            value={language}
            onChange={(event) => setLanguage(event.target.value as Language)}
          >
            {languages.map((info) => (
              <option key={info.code} value={info.code} lang={info.code}>
                {info.name}
              </option>
            ))}
          </select>
        </label>
      </header>
      <main id="content" tabIndex={-1}>
        <Suspense fallback={<p>{wording.common.loading}</p>}>{page}</Suspense>
      </main>
    </>
  )
}

/** Shows a page only to a signed-in account that is ready to act; otherwise, the way to become one. */
function Gate({ children }: { children: ReactNode }) {
  const { wording } = useI18n()
  const { ready, failure, account, retry } = useSession()

  if (!ready) return <p>{wording.common.loading}</p>
  if (failure) {
    return (
      <>
        <Failure code={failure} />
        <button type="button" onClick={retry}>
          {wording.common.tryAgain}
        </button>
      </>
    )
  }
  if (!account || !isComplete(account)) return <AccountSetup />
  return children
}

function NotFound() {
  const { wording } = useI18n()
  return (
    <>
      <PageHeading>{wording.common.notFoundTitle}</PageHeading>
      <p>{wording.common.notFoundBody}</p>
      <p>
        <Link to={paths.home}>{wording.common.goHome}</Link>
      </p>
    </>
  )
}
