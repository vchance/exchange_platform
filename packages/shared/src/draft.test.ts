import type { RevisionTerms } from '@exchange/api-client'
import { describe, expect, test } from 'vitest'

import {
  buildTerms,
  draftFromTerms,
  emptyDraft,
  newContribution,
  readDraft,
  type Draft,
  type DraftContribution,
} from './draft'

const REPAIR = '11111111-1111-4111-8111-111111111111'
const PAYMENT = '22222222-2222-4222-8222-222222222222'
const EXTRA = '33333333-3333-4333-8333-333333333333'

/** The fence job from the backend tests: a repair by A, paid for by B once it is accepted. */
const fenceJob: RevisionTerms = {
  party_a_name: 'Ana Ruiz',
  party_b_name: 'Ben Ortiz',
  terms: 'Repair the back fence.',
  contributions: [
    {
      id: REPAIR,
      from: 'A',
      type: 'SERVICE',
      description: 'Repair the back fence',
      quantity: null,
      due: { kind: 'ON_AGREEMENT' },
      completion_criteria: null,
      required: true,
      amount_minor: null,
    },
    {
      id: PAYMENT,
      from: 'B',
      type: 'MONEY',
      description: 'Payment on completion',
      quantity: null,
      due: { kind: 'AFTER_CONTRIBUTION', contribution: REPAIR },
      completion_criteria: null,
      required: true,
      amount_minor: 40000,
    },
  ],
}

function fenceDraft(): Draft {
  return {
    format: 1,
    base: null,
    partyA: ' Ana Ruiz ',
    partyB: 'Ben Ortiz',
    terms: 'Repair the back fence.',
    note: '  ',
    contributions: [
      { ...newContribution(REPAIR, 'A'), type: 'SERVICE', description: ' Repair the back fence ' },
      {
        ...newContribution(PAYMENT, 'B'),
        type: 'MONEY',
        description: 'Payment on completion',
        amount: '400',
        due: { kind: 'AFTER_CONTRIBUTION', contribution: REPAIR },
      },
    ],
  }
}

function problemsOf(draft: Draft) {
  const built = buildTerms(draft, 2)
  return built.ok ? [] : built.problems
}

function withItem(change: Partial<DraftContribution>): Draft {
  const draft = fenceDraft()
  draft.contributions[0] = { ...draft.contributions[0], ...change }
  return draft
}

describe('buildTerms', () => {
  test('builds exactly the revision the API takes', () => {
    expect(buildTerms(fenceDraft(), 2)).toEqual({ ok: true, terms: fenceJob, note: null })
  })

  test('sends a quantity, a date, criteria and a note when they are filled in', () => {
    const draft = withItem({
      type: 'ITEM',
      quantity: '1.5',
      unit: ' boxes ',
      criteria: ' Delivered to the door ',
      due: { kind: 'DATE', date: '2026-11-01' },
      required: false,
    })
    draft.note = ' See you Saturday. '
    const built = buildTerms(draft, 2)
    expect(built.ok && built.note).toBe('See you Saturday.')
    expect(built.ok && built.terms.contributions[0]).toEqual({
      id: REPAIR,
      from: 'A',
      type: 'ITEM',
      description: 'Repair the back fence',
      quantity: { amount: '1.5', unit: 'boxes' },
      due: { kind: 'DATE', date: '2026-11-01' },
      completion_criteria: 'Delivered to the door',
      required: false,
      amount_minor: null,
    })
  })

  test('only money carries an amount, and money carries no quantity', () => {
    const built = buildTerms(withItem({ type: 'ITEM', amount: '25', quantity: '2' }), 2)
    expect(built.ok && built.terms.contributions[0].amount_minor).toBeNull()

    const money = buildTerms(withItem({ type: 'MONEY', amount: '25.5', quantity: '2' }), 2)
    expect(money.ok && money.terms.contributions[0]).toMatchObject({
      amount_minor: 2550,
      quantity: null,
    })
  })

  test('says what is missing, and where', () => {
    const empty = emptyDraft('')
    expect(problemsOf(empty)).toEqual([
      { code: 'PARTY_NAME_MISSING', field: 'partyA' },
      { code: 'PARTY_NAME_MISSING', field: 'partyB' },
      { code: 'NO_CONTRIBUTIONS', field: 'contributions' },
    ])

    expect(problemsOf(withItem({ description: '  ' }))).toEqual([
      { code: 'DESCRIPTION_MISSING', field: 'description', contribution: REPAIR },
    ])
    expect(problemsOf(withItem({ type: 'MONEY', amount: '' }))).toEqual([
      { code: 'AMOUNT_MISSING', field: 'amount', contribution: REPAIR },
    ])
    for (const amount of [null, '1.005']) {
      expect(problemsOf(withItem({ type: 'MONEY', amount }))).toEqual([
        { code: 'AMOUNT_INVALID', field: 'amount', contribution: REPAIR },
      ])
    }
    expect(problemsOf(withItem({ quantity: null }))).toEqual([
      { code: 'QUANTITY_INVALID', field: 'quantity', contribution: REPAIR },
    ])
    expect(problemsOf(withItem({ quantity: '', unit: 'boxes' }))).toEqual([
      { code: 'QUANTITY_INVALID', field: 'quantity', contribution: REPAIR },
    ])
    for (const date of ['', '2026-02-30', '11/01/2026']) {
      expect(problemsOf(withItem({ due: { kind: 'DATE', date } }))).toEqual([
        { code: 'DATE_MISSING', field: 'date', contribution: REPAIR },
      ])
    }
  })

  test('needs at least one required contribution', () => {
    const draft = fenceDraft()
    draft.contributions = draft.contributions.map((item) => ({ ...item, required: false }))
    expect(problemsOf(draft)).toEqual([
      { code: 'NO_REQUIRED_CONTRIBUTION', field: 'contributions' },
    ])
  })

  test('a note is limited in characters, not bytes', () => {
    const draft = fenceDraft()
    draft.note = 'é'.repeat(1000)
    expect(problemsOf(draft)).toEqual([])
    draft.note = '😀'.repeat(1001)
    expect(problemsOf(draft)).toEqual([{ code: 'NOTE_TOO_LONG', field: 'note' }])
  })

  test('a contribution cannot wait on nothing, on itself, or in a circle', () => {
    for (const contribution of ['', EXTRA, REPAIR]) {
      expect(problemsOf(withItem({ due: { kind: 'AFTER_CONTRIBUTION', contribution } }))).toEqual([
        { code: 'DEPENDENCY_MISSING', field: 'after', contribution: REPAIR },
      ])
    }

    // The repair waits on the payment, which waits on the repair.
    const circle = withItem({ due: { kind: 'AFTER_CONTRIBUTION', contribution: PAYMENT } })
    expect(problemsOf(circle)).toEqual([
      { code: 'DEPENDENCY_CYCLE', field: 'after', contribution: REPAIR },
      { code: 'DEPENDENCY_CYCLE', field: 'after', contribution: PAYMENT },
    ])
  })
})

describe('starting from a revision', () => {
  test('a working copy made from terms builds back into the same terms', () => {
    const draft = draftFromTerms(fenceJob, 'revision-1', 2)
    expect(draft.base).toBe('revision-1')
    expect(draft.contributions[1].amount).toBe('400.00')
    expect(buildTerms(draft, 2, fenceJob)).toEqual({ ok: true, terms: fenceJob, note: null })
  })

  test('an untouched contribution is carried over character for character', () => {
    // Text the service stored as written, which a rebuild would tidy up. An
    // amendment that altered an accepted contribution would be refused.
    const awkward: RevisionTerms = {
      ...fenceJob,
      contributions: [
        {
          ...fenceJob.contributions[0],
          completion_criteria: ' Gate closes on its own. ',
          quantity: { amount: '2', unit: ' panels' },
        },
        fenceJob.contributions[1],
      ],
    }
    const draft = draftFromTerms(awkward, 'revision-1', 2)
    draft.contributions.push({ ...newContribution(EXTRA, 'B'), description: 'Lunch' })

    const built = buildTerms(draft, 2, awkward)
    expect(built.ok && built.terms.contributions.slice(0, 2)).toEqual(awkward.contributions)
    expect(built.ok && built.terms.contributions[2].id).toBe(EXTRA)
  })

  test('a changed contribution is rebuilt from what was typed', () => {
    const draft = draftFromTerms(fenceJob, 'revision-1', 2)
    draft.contributions[1].amount = '450'
    const built = buildTerms(draft, 2, fenceJob)
    expect(built.ok && built.terms.contributions[1].amount_minor).toBe(45000)
    expect(built.ok && built.terms.contributions[0]).toBe(fenceJob.contributions[0])
  })

  test('removing what an untouched contribution waits on is still caught', () => {
    const draft = draftFromTerms(fenceJob, 'revision-1', 2)
    draft.contributions = [draft.contributions[1]]
    const built = buildTerms(draft, 2, fenceJob)
    expect(!built.ok && built.problems).toEqual([
      { code: 'DEPENDENCY_MISSING', field: 'after', contribution: PAYMENT },
    ])
  })
})

describe('readDraft', () => {
  test('reads back what was saved', () => {
    const draft = fenceDraft()
    expect(readDraft(JSON.parse(JSON.stringify(draft)))).toEqual(draft)
  })

  test('something unrecognizable is no draft at all', () => {
    for (const stored of [null, undefined, 'text', 7, {}, { format: 2, contributions: [] }]) {
      expect(readDraft(stored)).toBeNull()
    }
    expect(readDraft({ format: 1, contributions: [{ description: 'no id' }] })).toBeNull()
  })

  test('missing pieces come back empty, and a number that was not one comes back blank', () => {
    const read = readDraft({
      format: 1,
      contributions: [{ id: REPAIR, from: 'B', type: 'NONSENSE', quantity: null, due: {} }],
    })
    expect(read).toEqual({
      format: 1,
      base: null,
      partyA: '',
      partyB: '',
      terms: '',
      note: '',
      contributions: [newContribution(REPAIR, 'B')],
    })
  })
})
