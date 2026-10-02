import { afterAll, beforeAll, expect, test, vi } from 'vitest'

import { decimalForInput, formatMoney, fractionDigitsOf, parseDecimal } from './decimal'
import { todayIn } from './fulfillment'
import { createI18n } from './i18n'
import { wordingFor } from './language'
import { formatMessage } from './message'

/*
 * The mobile apps run on Hermes, whose `Intl` is narrower than a browser's:
 * it has no `Intl.PluralRules`, and `Intl.NumberFormat.prototype.formatToParts`
 * is missing on iOS, where calling it takes the app down. Nothing here may
 * depend on either. These tests take both away, and `formatToParts` on dates
 * too, and run the shared formatting the screens use.
 */

const pluralRules = Intl.PluralRules

beforeAll(() => {
  const missing = () => {
    throw new Error('not available on the mobile engine')
  }
  vi.spyOn(Intl.NumberFormat.prototype, 'formatToParts').mockImplementation(missing)
  vi.spyOn(Intl.DateTimeFormat.prototype, 'formatToParts').mockImplementation(missing)
  Reflect.deleteProperty(Intl, 'PluralRules')
})

afterAll(() => {
  vi.restoreAllMocks()
  Object.defineProperty(Intl, 'PluralRules', {
    value: pluralRules,
    configurable: true,
    writable: true,
  })
})

test('numbers are still read and written the way the language does', () => {
  expect(parseDecimal('1,234.5', 'en')).toBe('1234.5')
  expect(parseDecimal('1.234,5', 'es')).toBe('1234.5')
  expect(parseDecimal('1.5', 'es')).toBeNull()
  expect(decimalForInput('1.5', 'es')).toBe('1,5')
  expect(fractionDigitsOf('USD')).toBe(2)
  expect(formatMoney(40050, 'USD', 'en')).toBe('$400.50')
})

test('today in a timezone is still found', () => {
  const instant = new Date('2026-11-01T03:30:00Z')
  expect(todayIn('America/Chicago', instant)).toBe('2026-10-31')
  expect(todayIn('Asia/Tokyo', instant)).toBe('2026-11-01')
})

test('a plural message falls back to the form every message has instead of failing', () => {
  const message = '{count, plural, =0 {None left} one {# item left} other {# items left}}'
  expect(formatMessage(message, { count: 0 }, 'en')).toBe('None left')
  expect(formatMessage(message, { count: 3 }, 'en')).toBe('3 items left')
  expect(formatMessage(message, { count: 1 }, 'en')).toBe('1 items left')
})

test('dates and refusals are still told in the reader’s language', () => {
  const es = createI18n('es', wordingFor('es'), () => {})
  expect(es.day('2026-10-02')).toBe('2 de octubre de 2026')
  expect(es.errorText('VERSION_CONFLICT')).toBe(wordingFor('es').errors.VERSION_CONFLICT)
})
