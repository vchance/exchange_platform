import type { Account, ErrorCode } from '@yuppers/api-client'

import { formatMoney } from './decimal'
import type { Language } from './language'
import { formatMessage, type MessageValues } from './message'
import type { Wording } from './wording/types'

/** Everything a screen needs to speak the reader's language. */
export interface I18n {
  language: Language
  wording: Wording
  /** Fills in a wording message that has placeholders. */
  fmt(message: string, values: MessageValues): string
  /** A calendar date, `YYYY-MM-DD`, as the language writes dates. */
  day(date: string): string
  /** A moment in time (RFC 3339), in the reader's own timezone. */
  moment(instant: string): string
  money(minor: number, currency: string): string
  /** What to tell the person about a refusal from the service. */
  errorText(code: ErrorCode): string
  setLanguage(language: Language): void
}

/**
 * Dates, numbers and money are written by the platform's own formatting for
 * the reader's language, never by hand (DESIGN.md §4.2).
 */
export function createI18n(
  language: Language,
  wording: Wording,
  setLanguage: (language: Language) => void,
): I18n {
  const dayFormat = new Intl.DateTimeFormat(language, { dateStyle: 'long', timeZone: 'UTC' })
  const momentFormat = new Intl.DateTimeFormat(language, { dateStyle: 'long', timeStyle: 'short' })
  return {
    language,
    wording,
    setLanguage,
    fmt: (message, values) => formatMessage(message, values, language),
    day: (date) => {
      const parsed = new Date(`${date}T00:00:00Z`)
      return Number.isNaN(parsed.getTime()) ? date : dayFormat.format(parsed)
    },
    moment: (instant) => {
      const parsed = new Date(instant)
      return Number.isNaN(parsed.getTime()) ? instant : momentFormat.format(parsed)
    },
    money: (minor, currency) => formatMoney(minor, currency, language),
    // A code this build has no wording for can only come from a newer service.
    errorText: (code) => wording.errors[code] ?? wording.errors.INTERNAL,
  }
}

/** Signing needs a name and a confirmation of age (DESIGN.md §14.1). */
export function isComplete(account: Account): boolean {
  return account.display_name !== '' && account.adult_confirmed
}
