import { expect, test } from 'vitest'

import { idempotencyKeys } from './idempotency'

function counted() {
  let next = 0
  return idempotencyKeys(() => `key-${(next += 1)}`)
}

test('every attempt gets a key of its own', () => {
  const keys = counted()
  const first = keys.keyFor('accept')
  keys.answered('accept')
  // The same request made again later is a new attempt, not a retry.
  expect(keys.keyFor('accept')).not.toBe(first)
})

test('a request that went unanswered is retried with the same key', () => {
  const keys = counted()
  const key = keys.keyFor('accept')
  keys.unanswered('accept', key)

  expect(keys.keyFor('accept')).toBe(key)
  // A different request never borrows it.
  expect(keys.keyFor('decline')).not.toBe(key)

  keys.answered('accept')
  expect(keys.keyFor('accept')).not.toBe(key)
})

test('keys are unguessable by default', () => {
  const keys = idempotencyKeys()
  expect(keys.keyFor('a')).toMatch(/^[0-9a-f-]{36}$/)
  expect(keys.keyFor('a')).not.toBe(keys.keyFor('a'))
})
