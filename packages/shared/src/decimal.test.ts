import { describe, expect, test } from 'vitest'

import {
  decimalForInput,
  formatMoney,
  fractionDigitsOf,
  fromMinorUnits,
  parseDecimal,
  toMinorUnits,
} from './decimal'

describe('parseDecimal', () => {
  test('reads numbers the way English writes them', () => {
    expect(parseDecimal('2', 'en')).toBe('2')
    expect(parseDecimal(' 1.5 ', 'en')).toBe('1.5')
    expect(parseDecimal('1,000', 'en')).toBe('1000')
    expect(parseDecimal('1,234,567.50', 'en')).toBe('1234567.50')
    expect(parseDecimal('007', 'en')).toBe('7')
    expect(parseDecimal('0.5', 'en')).toBe('0.5')
  })

  test('reads numbers the way Spanish writes them', () => {
    expect(parseDecimal('1,5', 'es')).toBe('1.5')
    expect(parseDecimal('1.000', 'es')).toBe('1000')
    expect(parseDecimal('1.234.567,50', 'es')).toBe('1234567.50')
  })

  test('refuses what could be read two ways instead of guessing', () => {
    // A decimal point typed where the language groups thousands.
    expect(parseDecimal('1.5', 'es')).toBeNull()
    expect(parseDecimal('1,5', 'en')).toBeNull()
    expect(parseDecimal('1,00', 'en')).toBeNull()
    expect(parseDecimal('1,0000', 'en')).toBeNull()
  })

  test('refuses anything that is not a positive number', () => {
    for (const text of ['', ' ', '0', '0.00', '-1', '1e3', 'two', '1.2.3', '1.', '.5', '$5']) {
      expect(parseDecimal(text, 'en'), text).toBeNull()
    }
    expect(parseDecimal('1'.repeat(21), 'en')).toBeNull()
  })
})

test('a plain decimal goes back into an input in the language’s form', () => {
  expect(decimalForInput('1.5', 'en')).toBe('1.5')
  expect(decimalForInput('1.5', 'es')).toBe('1,5')
  expect(decimalForInput('400', 'es')).toBe('400')
})

describe('money', () => {
  test('a currency knows its decimal places', () => {
    expect(fractionDigitsOf('USD')).toBe(2)
    expect(fractionDigitsOf('JPY')).toBe(0)
  })

  test('amounts become whole minor units', () => {
    expect(toMinorUnits('400', 2)).toBe(40000)
    expect(toMinorUnits('400.5', 2)).toBe(40050)
    expect(toMinorUnits('0.07', 2)).toBe(7)
    expect(toMinorUnits('19.990', 2)).toBe(1999)
    expect(toMinorUnits('500', 0)).toBe(500)
  })

  test('an amount finer than the currency, zero, or too large is refused', () => {
    expect(toMinorUnits('1.005', 2)).toBeNull()
    expect(toMinorUnits('1.5', 0)).toBeNull()
    expect(toMinorUnits('0', 2)).toBeNull()
    expect(toMinorUnits('90071992547409.92', 2)).toBeNull()
    expect(toMinorUnits('90071992547409.91', 2)).toBe(9007199254740991)
  })

  test('minor units go back to a plain decimal without rounding', () => {
    expect(fromMinorUnits(40000, 2)).toBe('400.00')
    expect(fromMinorUnits(7, 2)).toBe('0.07')
    expect(fromMinorUnits(500, 0)).toBe('500')
    expect(toMinorUnits(fromMinorUnits(123456789, 2), 2)).toBe(123456789)
  })

  test('money is written for the reader’s language', () => {
    expect(formatMoney(40000, 'USD', 'en')).toBe('$400.00')
    expect(formatMoney(40050, 'USD', 'es')).toMatch(/^400,50\s(US\$|USD)$/)
  })
})
