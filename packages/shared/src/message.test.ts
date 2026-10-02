import { expect, test } from 'vitest'

import { formatMessage } from './message'

test('fills in names', () => {
  expect(formatMessage('Waiting for {name}', { name: 'Ana Ruiz' }, 'en')).toBe(
    'Waiting for Ana Ruiz',
  )
  expect(formatMessage('{a} and {b} and {a}', { a: 'x', b: 'y' }, 'en')).toBe('x and y and x')
})

test('leaves what was written inside a value alone', () => {
  expect(formatMessage('From {name}', { name: '{count} #' }, 'en')).toBe('From {count} #')
})

test('chooses the plural form the language uses', () => {
  const message = '{count, plural, one {# item left} other {# items left}}'
  expect(formatMessage(message, { count: 1 }, 'en')).toBe('1 item left')
  expect(formatMessage(message, { count: 2 }, 'en')).toBe('2 items left')
  expect(formatMessage(message, { count: 0 }, 'en')).toBe('0 items left')
})

test('an exact match wins over a plural category', () => {
  const message = '{count, plural, =0 {Nothing left} one {# left} other {# left}}'
  expect(formatMessage(message, { count: 0 }, 'en')).toBe('Nothing left')
})

test('writes numbers the way the language does', () => {
  expect(formatMessage('{count, plural, other {# things}}', { count: 1234 }, 'en')).toBe(
    '1,234 things',
  )
  expect(formatMessage('{count, plural, other {# cosas}}', { count: 12345 }, 'es')).toBe(
    '12.345 cosas',
  )
})

test('placeholders inside a plural branch are filled in too', () => {
  const message = '{count, plural, one {{name} has # item} other {{name} has # items}}'
  expect(formatMessage(message, { count: 3, name: 'Ben' }, 'en')).toBe('Ben has 3 items')
})

test('a placeholder with no value stays visible instead of throwing', () => {
  expect(formatMessage('Hello {name}', {}, 'en')).toBe('Hello {name}')
  expect(formatMessage('Broken {name', { name: 'x' }, 'en')).toBe('Broken {name')
})
