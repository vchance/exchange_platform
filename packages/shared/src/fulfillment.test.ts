import { expect, test } from 'vitest'

import { isOverdue, movesFor, todayIn } from './fulfillment'

test('offers each party the actions in the table in DESIGN.md §5.2', () => {
  expect(movesFor('PENDING', 'PROVIDER')).toEqual(['CLAIM'])
  expect(movesFor('CLAIMED', 'PROVIDER')).toEqual(['RETRACT_CLAIM'])
  expect(movesFor('DISPUTED', 'PROVIDER')).toEqual(['RECLAIM'])

  expect(movesFor('PENDING', 'RECIPIENT')).toEqual(['CONFIRM', 'WAIVE'])
  expect(movesFor('CLAIMED', 'RECIPIENT')).toEqual(['CONFIRM', 'DISPUTE', 'WAIVE'])
  expect(movesFor('DISPUTED', 'RECIPIENT')).toEqual(['CONFIRM', 'WAIVE'])
})

test('offers nothing once a contribution is settled', () => {
  for (const status of ['ACCEPTED', 'WAIVED', 'REMOVED'] as const) {
    expect(movesFor(status, 'PROVIDER')).toEqual([])
    expect(movesFor(status, 'RECIPIENT')).toEqual([])
  }
})

test('today is read in the exchange’s timezone', () => {
  const instant = new Date('2026-11-01T03:30:00Z')
  expect(todayIn('UTC', instant)).toBe('2026-11-01')
  expect(todayIn('America/Chicago', instant)).toBe('2026-10-31')
  expect(todayIn('Asia/Tokyo', instant)).toBe('2026-11-01')
})

test('overdue means nothing delivered and the due date gone by', () => {
  const due = { kind: 'DATE', date: '2026-10-31' } as const
  expect(isOverdue('PENDING', due, '2026-11-01')).toBe(true)
  // Due today is not overdue yet.
  expect(isOverdue('PENDING', due, '2026-10-31')).toBe(false)
  expect(isOverdue('CLAIMED', due, '2026-11-01')).toBe(false)
  expect(isOverdue('PENDING', { kind: 'ON_AGREEMENT' }, '2026-11-01')).toBe(false)
})
