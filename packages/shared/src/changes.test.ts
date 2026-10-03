import type { components, RevisionTerms } from '@exchange/api-client'
import { describe, expect, test } from 'vitest'

import { contributionChanges, fieldChangeText, proposalChanges } from './changes'
import { createI18n } from './i18n'
import { wordingFor } from './language'

type Contribution = components['schemas']['ContributionDto']
type Status = components['schemas']['Status']

const BIKE = '00000000-0000-4000-8000-000000000011'
const PAY = '00000000-0000-4000-8000-000000000012'
const LOCK = '00000000-0000-4000-8000-000000000013'

const bike: Contribution = {
  id: BIKE,
  from: 'A',
  type: 'ITEM',
  description: 'A blue bicycle',
  due: { kind: 'DATE', date: '2026-11-01' },
  required: true,
}
const pay: Contribution = {
  id: PAY,
  from: 'B',
  type: 'MONEY',
  description: 'Payment for the bicycle',
  due: { kind: 'AFTER_CONTRIBUTION', contribution: BIKE },
  required: true,
  amount_minor: 30000,
}

function terms(contributions: Contribution[], fields: Partial<RevisionTerms> = {}): RevisionTerms {
  return { party_a_name: 'Ana', party_b_name: 'Ben', terms: 'As agreed.', contributions, ...fields }
}

const en = createI18n('en', wordingFor('en'), () => {})

describe('one item, field by field', () => {
  test('nothing differs', () => {
    expect(contributionChanges(pay, { ...pay })).toEqual([])
    // An absent field and an empty one are the same thing.
    expect(contributionChanges(bike, { ...bike, completion_criteria: null, quantity: null })).toEqual([])
  })

  test('each field that differs, in reading order', () => {
    const after: Contribution = {
      ...pay,
      description: 'Final payment',
      amount_minor: 36000,
      due: { kind: 'DATE', date: '2026-11-15' },
      completion_criteria: 'Cash or transfer',
      required: false,
    }
    expect(contributionChanges(pay, after).map((change) => change.field)).toEqual([
      'description',
      'amount',
      'due',
      'criteria',
      'required',
    ])
    expect(contributionChanges(pay, after)[1]).toEqual({ field: 'amount', before: 30000, after: 36000 })
  })

  test('quantity, provider and kind', () => {
    const after: Contribution = { ...bike, from: 'B', type: 'OTHER', quantity: { amount: '2', unit: 'boxes' } }
    expect(contributionChanges(bike, after)).toEqual([
      { field: 'from', before: 'A', after: 'B' },
      { field: 'type', before: 'ITEM', after: 'OTHER' },
      { field: 'quantity', before: null, after: { amount: '2', unit: 'boxes' } },
    ])
  })
})

describe('a whole proposal', () => {
  test('a counteroffer: changed, added and removed items, names and terms', () => {
    const before = terms([bike, pay])
    const after = terms(
      [{ ...pay, amount_minor: 36000 }, { ...bike, id: LOCK, description: 'A lock' }],
      { party_b_name: 'Benjamin', terms: 'As agreed, with a lock.' },
    )
    const changes = proposalChanges(before, after)
    expect(changes.names).toEqual([{ slot: 'B', before: 'Ben', after: 'Benjamin' }])
    expect(changes.termsChanged).toBe(true)
    expect(changes.items.map((item) => [item.description, item.kind])).toEqual([
      ['Payment for the bicycle', 'CHANGED'],
      ['A lock', 'ADDED'],
      ['A blue bicycle', 'REMOVED'],
    ])
    expect(changes.items[0].money).toBe(true)
    expect(changes.items[0].effect).toBeUndefined()
  })

  test('an amendment: what it does to each item, from the same rule as the composer', () => {
    const before = terms([bike, pay])
    const statuses = new Map<string, Status>([
      [BIKE, 'CLAIMED'],
      [PAY, 'PENDING'],
    ])
    const after = terms([{ ...bike, description: 'A blue bicycle, new chain' }, pay])
    const changes = proposalChanges(before, after, statuses)
    expect(changes.items.map((item) => [item.kind, item.effect, item.status])).toEqual([
      ['CHANGED', 'CHANGED', 'PENDING'],
      ['UNCHANGED', 'UNTOUCHED', 'PENDING'],
    ])
    const removed = proposalChanges(before, terms([bike]), statuses)
    expect(removed.items[1]).toMatchObject({ kind: 'REMOVED', effect: 'REMOVED', status: 'REMOVED' })
  })
})

describe('in words', () => {
  const before = terms([bike, pay])
  const after = terms([bike, pay], { party_a_name: 'Ana R.' })
  const say = (change: Parameters<typeof fieldChangeText>[0]) =>
    fieldChangeText(change, en, 'USD', { before, after })

  test('what the product says is one sentence', () => {
    expect(say({ field: 'amount', before: 30000, after: 36000 }).sentence).toBe(
      'Amount: was $300.00, now $360.00.',
    )
    expect(say({ field: 'required', before: true, after: false }).sentence).toBe(
      'Required or optional: was Required, now Optional.',
    )
    expect(
      say({ field: 'due', before: { kind: 'ON_AGREEMENT' }, after: { kind: 'DATE', date: '2026-11-15' } })
        .sentence,
    ).toBe('When it’s due: was Due when the agreement is signed, now Due November 15, 2026.')
    expect(
      say({ field: 'due', before: { kind: 'AFTER_CONTRIBUTION', contribution: BIKE }, after: { kind: 'ON_AGREEMENT' } })
        .before,
    ).toBe('Due once “A blue bicycle” is confirmed')
  })

  test('what the parties wrote is kept apart, to be shown as theirs', () => {
    const description = say({ field: 'description', before: 'Old', after: 'New' })
    expect(description).toEqual({ label: 'Description', sentence: null, before: 'Old', after: 'New' })
    expect(say({ field: 'criteria', before: null, after: 'Done' }).before).toBe('nothing')
    expect(say({ field: 'from', before: 'A', after: 'B' })).toMatchObject({ before: 'Ana', after: 'Ben' })
    expect(say({ field: 'quantity', before: null, after: { amount: '2', unit: 'kg' } }).after).toBe('2 kg')
  })
})
