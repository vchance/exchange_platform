import type { Account, ErrorCode } from '@exchange/api-client'
import type { I18n } from '@exchange/shared'
import { createContext, useContext } from 'react'

// How a screen speaks the reader's language is the same on every client.
export { createI18n, isComplete } from '@exchange/shared'
export type { I18n }

export const I18nContext = createContext<I18n | null>(null)

export function useI18n(): I18n {
  const value = useContext(I18nContext)
  if (!value) throw new Error('useI18n outside the app')
  return value
}

export interface Session {
  /** `false` until the service has said whether anyone is signed in. */
  ready: boolean
  /** Set when the service could not be asked. */
  failure: ErrorCode | null
  account: Account | null
  setAccount(account: Account | null): void
  retry(): void
}

export const SessionContext = createContext<Session | null>(null)

export function useSession(): Session {
  const value = useContext(SessionContext)
  if (!value) throw new Error('useSession outside the app')
  return value
}
