/**
 * Numbers people type, and money.
 *
 * The API takes a quantity as a plain decimal such as `1.5` and money as a
 * whole number of minor units. People write numbers the way their language
 * does (`1,5` in Spanish), so what is typed is read by the language's rules,
 * and anything that could be read two ways is refused instead of guessed at.
 */

/**
 * The one currency exchanges are in at launch (DESIGN.md §4). An exchange
 * names its own currency and that is what to use; this is for the invitation
 * preview, which does not yet say.
 */
export const LAUNCH_CURRENCY = 'USD'

interface Separators {
  group: string
  decimal: string
}

/**
 * How a language separates thousands and decimals, read off a number it has
 * formatted. `formatToParts` would say so directly, but the JavaScript engine
 * in the mobile apps does not have it on iOS, so the same number is formatted
 * with and without grouping and the two are compared.
 */
function separators(language: string): Separators {
  const grouped = new Intl.NumberFormat(language, { useGrouping: 'always' }).format(1234567.5)
  const plain = new Intl.NumberFormat(language, { useGrouping: false }).format(1234567.5)

  // The number ends in its one decimal digit; the decimal separator is before it.
  const decimal = plain.length >= 2 ? plain[plain.length - 2] : '.'
  // The first character the grouped form has that the plain one lacks.
  let group = ''
  for (let index = 0; index < grouped.length; index += 1) {
    if (grouped[index] !== plain[index]) {
      group = grouped[index]
      break
    }
  }
  return { group, decimal }
}

/**
 * Reads a positive number typed in `language` and returns it in plain form
 * (`1234.5`), or `null` if it is not one. Group separators are accepted only
 * where they belong, so `1.5` typed in a language that writes `1,5` is
 * refused, not read as fifteen.
 */
export function parseDecimal(text: string, language: string): string | null {
  const { group, decimal } = separators(language)
  // Some languages group with a space, which people type as an ordinary one.
  const spaced = /^\s$/u.test(group)
  const typed = text.trim().replace(/[  ]/g, ' ')
  if (typed === '') return null

  const [whole, fraction, ...extra] = typed.split(decimal)
  if (extra.length > 0) return null
  if (fraction !== undefined && !/^\d+$/.test(fraction)) return null

  let digits: string
  const groups = whole.split(spaced ? ' ' : group || '\u0000')
  if (groups.length === 1) {
    digits = whole
  } else {
    const [first, ...others] = groups
    const wellGrouped = /^\d{1,3}$/.test(first) && others.every((part) => /^\d{3}$/.test(part))
    if (!wellGrouped) return null
    digits = groups.join('')
  }
  if (!/^\d+$/.test(digits)) return null

  const plain = digits.replace(/^0+(?=\d)/, '') + (fraction === undefined ? '' : `.${fraction}`)
  // The API's limit on a quantity, and zero is not a quantity.
  if (plain.length > 20 || !/[1-9]/.test(plain)) return null
  return plain
}

/** A plain decimal as someone would type it in `language`, for filling an input. */
export function decimalForInput(plain: string, language: string): string {
  return plain.replace('.', separators(language).decimal)
}

/** How many decimal places a currency has: 2 for USD, 0 for JPY. */
export function fractionDigitsOf(currency: string): number {
  return (
    new Intl.NumberFormat('en', { style: 'currency', currency }).resolvedOptions()
      .maximumFractionDigits ?? 2
  )
}

/**
 * A plain decimal amount as whole minor units: `400.5` in USD is `40050`.
 * `null` if it has more decimal places than the currency, or is too large to
 * be carried exactly.
 */
export function toMinorUnits(plain: string, fractionDigits: number): number | null {
  const [whole, fraction = ''] = plain.split('.')
  if (!/^\d+$/.test(whole) || !/^\d*$/.test(fraction)) return null
  if (/[1-9]/.test(fraction.slice(fractionDigits))) return null
  const minor = Number(whole + fraction.slice(0, fractionDigits).padEnd(fractionDigits, '0'))
  return Number.isSafeInteger(minor) && minor > 0 ? minor : null
}

/** Whole minor units as a plain decimal: `40050` in USD is `400.50`. */
export function fromMinorUnits(minor: number, fractionDigits: number): string {
  if (fractionDigits === 0) return String(minor)
  const digits = String(minor).padStart(fractionDigits + 1, '0')
  return `${digits.slice(0, -fractionDigits)}.${digits.slice(-fractionDigits)}`
}

/** Money as the reader's language writes it. */
export function formatMoney(minor: number, currency: string, language: string): string {
  const digits = fractionDigitsOf(currency)
  return new Intl.NumberFormat(language, { style: 'currency', currency }).format(
    minor / 10 ** digits,
  )
}
