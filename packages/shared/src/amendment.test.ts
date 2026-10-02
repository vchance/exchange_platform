import type { components, RevisionTerms } from '@exchange/api-client'
import { expect, test } from 'vitest'

import { amendmentEffects, amendmentRefused, draftEffects, type ItemEffect } from './amendment'
import { buildTerms, draftFromTerms } from './draft'

type Contribution = components['schemas']['ContributionDto']
type Status = components['schemas']['Status']

/*
 * The same cases as the tests of `backend/src/domain/amendment.rs`, with the
 * same fixtures: the client's prediction must come out where the service's
 * rule does. `fence_job()` there is a repair by A due on a date and a payment
 * by B due once the repair is confirmed; `contribution(n, ...)` is described
 * as "Contribution n".
 */

const id = (n: number) => `00000000-0000-0000-0000-${String(n).padStart(12, '0')}`

function contribution(
  n: number,
  from: 'A' | 'B',
  type: Contribution['type'],
  due: Contribution['due'],
  amountMinor: number | null = null,
): Contribution {
  return {
    id: id(n),
    from,
    type,
    description: `Contribution ${n}`,
    quantity: null,
    due,
    completion_criteria: null,
    required: true,
    amount_minor: amountMinor,
  }
}

function fenceJob(): RevisionTerms {
  return {
    party_a_name: 'Ana',
    party_b_name: 'Ben',
    terms: 'Repair the back fence.',
    contributions: [
      contribution(1, 'A', 'SERVICE', { kind: 'DATE', date: '2026-11-01' }),
      contribution(2, 'B', 'MONEY', { kind: 'AFTER_CONTRIBUTION', contribution: id(1) }, 50_000),
    ],
  }
}

const statuses = (pairs: [number, Status][]) => new Map(pairs.map(([n, status]) => [id(n), status]))

/** What the service's `effects` answers with: each contribution's status, or a refusal. */
function outcome(effects: ItemEffect[]): Record<string, Status | null> | 'LOCKED' | 'REUSED' {
  const refused = effects.find((item) => item.effect === 'LOCKED' || item.effect === 'REUSED')
  if (refused) return refused.effect as 'LOCKED' | 'REUSED'
  return Object.fromEntries(effects.map((item) => [item.id, item.status]))
}

const effects = (current: Map<string, Status>, proposed: RevisionTerms) =>
  amendmentEffects(fenceJob(), current, proposed.contributions)

test('untouched contributions keep their status', () => {
  const current = statuses([
    [1, 'CLAIMED'],
    [2, 'PENDING'],
  ])
  const proposed = fenceJob()
  proposed.terms = 'Repair the back fence and the gate.'
  const found = effects(current, proposed)
  expect(outcome(found)).toEqual({ [id(1)]: 'CLAIMED', [id(2)]: 'PENDING' })
  expect(found.map((item) => item.effect)).toEqual(['UNTOUCHED', 'UNTOUCHED'])
})

test('a changed contribution goes back to pending', () => {
  for (const status of ['CLAIMED', 'DISPUTED', 'WAIVED', 'PENDING'] as const) {
    const current = statuses([
      [1, status],
      [2, 'PENDING'],
    ])
    const proposed = fenceJob()
    proposed.contributions[0].description = 'Repair and paint the fence'
    const found = effects(current, proposed)
    expect(outcome(found), status).toEqual({ [id(1)]: 'PENDING', [id(2)]: 'PENDING' })
    expect(found[0].effect, status).toBe('CHANGED')
  }
})

test('a dropped contribution is removed and a new one starts pending', () => {
  const current = statuses([
    [1, 'CLAIMED'],
    [2, 'PENDING'],
  ])
  const proposed = fenceJob()
  proposed.contributions.splice(1, 1)
  proposed.contributions.push(contribution(3, 'B', 'ITEM', { kind: 'ON_AGREEMENT' }))
  const found = effects(current, proposed)
  expect(outcome(found)).toEqual({
    [id(1)]: 'CLAIMED',
    [id(2)]: 'REMOVED',
    [id(3)]: 'PENDING',
  })
  expect(found.map((item) => [item.effect, item.description])).toEqual([
    ['UNTOUCHED', 'Contribution 1'],
    ['NEW', 'Contribution 3'],
    // A removed contribution is still named, as the agreement wrote it.
    ['REMOVED', 'Contribution 2'],
  ])
})

test('an accepted contribution cannot be changed or removed', () => {
  const current = statuses([
    [1, 'ACCEPTED'],
    [2, 'PENDING'],
  ])
  const changed = fenceJob()
  changed.contributions[0].required = false
  expect(outcome(effects(current, changed))).toBe('LOCKED')
  expect(amendmentRefused(effects(current, changed))).toBe(true)

  const removed = fenceJob()
  removed.contributions.splice(0, 1)
  removed.contributions[0].due = { kind: 'ON_AGREEMENT' }
  const found = effects(current, removed)
  expect(outcome(found)).toBe('LOCKED')
  expect(found.find((item) => item.id === id(1))?.effect).toBe('LOCKED')
})

test('an accepted contribution can be carried over and adjusted for', () => {
  const current = statuses([
    [1, 'ACCEPTED'],
    [2, 'PENDING'],
  ])
  const proposed = fenceJob()
  proposed.contributions.push(contribution(3, 'B', 'MONEY', { kind: 'ON_AGREEMENT' }, 5_000))
  const found = effects(current, proposed)
  expect(outcome(found)).toEqual({
    [id(1)]: 'ACCEPTED',
    [id(2)]: 'PENDING',
    [id(3)]: 'PENDING',
  })
  expect(amendmentRefused(found)).toBe(false)
})

test('a removed contribution’s id is retired', () => {
  const inForce = fenceJob()
  inForce.contributions.splice(1, 1)
  const current = statuses([
    [1, 'PENDING'],
    [2, 'REMOVED'],
  ])
  const found = amendmentEffects(inForce, current, fenceJob().contributions)
  expect(outcome(found)).toBe('REUSED')
})

test('reordering alone changes no status', () => {
  const current = statuses([
    [1, 'CLAIMED'],
    [2, 'DISPUTED'],
  ])
  const proposed = fenceJob()
  proposed.contributions.reverse()
  const found = effects(current, proposed)
  expect(outcome(found)).toEqual({ [id(1)]: 'CLAIMED', [id(2)]: 'DISPUTED' })
  expect(found.every((item) => item.effect === 'UNTOUCHED')).toBe(true)
})

test('what the composer predicts from the working copy is what it will send', () => {
  const inForce = fenceJob()
  const current = statuses([
    [1, 'CLAIMED'],
    [2, 'ACCEPTED'],
  ])
  const draft = draftFromTerms(inForce, 'r1', 2)

  // Untouched, the working copy predicts no change.
  expect(draftEffects(draft, inForce, current, 2).map((item) => item.effect)).toEqual([
    'UNTOUCHED',
    'UNTOUCHED',
  ])

  // A field edited and put back is untouched again; whitespace around a
  // description is not a change, since what is signed is trimmed.
  draft.contributions[0].description = ' Contribution 1 '
  expect(draftEffects(draft, inForce, current, 2)[0].effect).toBe('UNTOUCHED')
  draft.contributions[0].description = 'Contribution 1, painted'
  expect(draftEffects(draft, inForce, current, 2)[0].effect).toBe('CHANGED')

  // Touching the accepted payment is what the service will refuse.
  draft.contributions[1].amount = '600'
  const predicted = draftEffects(draft, inForce, current, 2)
  expect(predicted[1].effect).toBe('LOCKED')

  // The prediction agrees with the terms `buildTerms` actually produces.
  const built = buildTerms(draft, 2, inForce)
  expect(built.ok).toBe(true)
  if (built.ok) {
    expect(amendmentEffects(inForce, current, built.terms.contributions)).toEqual(predicted)
  }
})
