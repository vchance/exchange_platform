import type { ExchangeView, RevisionTerms } from '@exchange/api-client'
import { expect, test } from 'vitest'

import {
  isOverdue,
  moveCommand,
  movesFor,
  noteFor,
  otherPartyName,
  otherSlot,
  remainingRequired,
  statusesOf,
  todayIn,
} from './fulfillment'

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

test('a dispute and a second claim must say something; a first claim may; nothing else does', () => {
  expect(noteFor('DISPUTE')).toEqual({ takes: true, needs: true, label: 'reasonLabel' })
  expect(noteFor('RECLAIM')).toEqual({ takes: true, needs: true, label: 'remedyLabel' })
  expect(noteFor('CLAIM')).toEqual({ takes: true, needs: false, label: 'noteLabel' })
  for (const move of ['RETRACT_CLAIM', 'CONFIRM', 'WAIVE'] as const) {
    expect(noteFor(move)).toMatchObject({ takes: false, needs: false })
  }
})

test('a move becomes the command the service takes', () => {
  const id = '11111111-1111-4111-8111-111111111111'
  expect(moveCommand('CLAIM', id, '  Left it on the porch ')).toEqual({
    type: 'CONTRIBUTION',
    contribution: id,
    action: 'CLAIM',
    note: 'Left it on the porch',
  })
  expect(moveCommand('CLAIM', id, '')).toMatchObject({ action: 'CLAIM', note: null })
  // Marking delivered again is sent as a claim.
  expect(moveCommand('RECLAIM', id, 'Replaced the broken board')).toMatchObject({
    action: 'CLAIM',
    note: 'Replaced the broken board',
  })
  expect(moveCommand('CONFIRM', id, 'ignored')).toMatchObject({ action: 'CONFIRM', note: null })
})

test('a move that must say why is not sent without it', () => {
  const id = '11111111-1111-4111-8111-111111111111'
  expect(moveCommand('DISPUTE', id, '   ')).toBeNull()
  expect(moveCommand('RECLAIM', id, '')).toBeNull()
  expect(moveCommand('DISPUTE', id, 'Two boards are loose')).toMatchObject({
    action: 'DISPUTE',
    note: 'Two boards are loose',
  })
})

const REPAIR = '11111111-1111-4111-8111-111111111111'
const PAYMENT = '22222222-2222-4222-8222-222222222222'
const TIP = '33333333-3333-4333-8333-333333333333'

const terms: RevisionTerms = {
  party_a_name: 'Ana Ruiz',
  party_b_name: 'Ben Ortiz',
  terms: '',
  contributions: [
    { id: REPAIR, from: 'A', type: 'SERVICE', description: 'Repair', due: { kind: 'ON_AGREEMENT' }, required: true },
    { id: PAYMENT, from: 'B', type: 'MONEY', description: 'Payment', due: { kind: 'ON_AGREEMENT' }, required: true, amount_minor: 40000 },
    { id: TIP, from: 'B', type: 'OTHER', description: 'Lemonade', due: { kind: 'ON_AGREEMENT' }, required: false },
  ],
}

function exchange(patch: Partial<ExchangeView>): ExchangeView {
  return {
    id: '0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70',
    version: 3,
    state: 'ACTIVE',
    you: 'A',
    counterparty: 'CONFIRMED',
    currency: 'USD',
    timezone: 'America/Chicago',
    display_code: 'ABCD-1234',
    contributions: [],
    ...patch,
  }
}

const revision = {
  id: 'r',
  sequence: 1,
  author: 'A' as const,
  accepted_by: ['A' as const, 'B' as const],
  content_hash: 'ab'.repeat(32),
  expires_at: '2026-10-16T12:00:00Z',
  terms,
}

test('the other party is named as the latest terms write them', () => {
  expect(otherSlot('A')).toBe('B')
  expect(otherSlot('B')).toBe('A')
  expect(otherPartyName(exchange({ in_force_revision: revision }))).toBe('Ben Ortiz')
  expect(otherPartyName(exchange({ you: 'B', in_force_revision: revision }))).toBe('Ana Ruiz')
  // Terms on the table are newer than the agreement in force.
  const renamed = { ...revision, terms: { ...terms, party_b_name: 'Benjamin Ortiz' } }
  expect(otherPartyName(exchange({ in_force_revision: revision, open_revision: renamed }))).toBe(
    'Benjamin Ortiz',
  )
  expect(otherPartyName(exchange({ state: 'DRAFT' }))).toBe('')
})

test('what remains is every required contribution not yet accepted or waived', () => {
  const pending = exchange({
    in_force_revision: revision,
    contributions: [
      { id: REPAIR, status: 'CLAIMED' },
      { id: PAYMENT, status: 'PENDING' },
      { id: TIP, status: 'PENDING' },
    ],
  })
  expect(statusesOf(pending).get(REPAIR)).toBe('CLAIMED')
  expect(remainingRequired(pending)).toBe(2)
  const nearly = exchange({
    in_force_revision: revision,
    contributions: [
      { id: REPAIR, status: 'ACCEPTED' },
      { id: PAYMENT, status: 'DISPUTED' },
      { id: TIP, status: 'PENDING' },
    ],
  })
  expect(remainingRequired(nearly)).toBe(1)
  const done = exchange({
    in_force_revision: revision,
    contributions: [
      { id: REPAIR, status: 'ACCEPTED' },
      { id: PAYMENT, status: 'WAIVED' },
      { id: TIP, status: 'PENDING' },
    ],
  })
  expect(remainingRequired(done)).toBe(0)
  expect(remainingRequired(exchange({ state: 'NEGOTIATING' }))).toBe(0)
})
