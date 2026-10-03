import type { ExchangeView } from '@yuppers/api-client'
import { expect, test } from 'vitest'

import { createI18n } from './i18n'
import { wordingFor } from './language'
import { formatMessage } from './message'
import {
  TROUBLE_SITUATIONS,
  troubleOffered,
  troublePanel,
  troubleRoute,
  troubleSituationOf,
  type TroubleRoute,
} from './trouble'

const BIKE = '00000000-0000-4000-8000-000000000011'
const PAY = '00000000-0000-4000-8000-000000000012'

function exchange(fields: Partial<ExchangeView> = {}): ExchangeView {
  return {
    id: 'e',
    version: 3,
    state: 'ACTIVE',
    you: 'A',
    counterparty: 'CONFIRMED',
    currency: 'USD',
    timezone: 'UTC',
    display_code: 'TEST-0001',
    in_force_revision: {
      id: 'r',
      sequence: 1,
      author: 'A',
      accepted_by: ['A', 'B'],
      content_hash: '',
      expires_at: '',
      terms: {
        party_a_name: 'Ana',
        party_b_name: 'Ben',
        terms: '',
        contributions: [
          { id: BIKE, from: 'A', type: 'ITEM', description: 'A bicycle', due: { kind: 'ON_AGREEMENT' }, required: true },
          { id: PAY, from: 'B', type: 'MONEY', description: 'Payment', due: { kind: 'ON_AGREEMENT' }, required: true, amount_minor: 100 },
        ],
      },
    },
    contributions: [
      { id: BIKE, status: 'PENDING' },
      { id: PAY, status: 'PENDING' },
    ],
    ...fields,
  }
}

const ways = (route: TroubleRoute) =>
  route.offers.map((offer) => [offer.way, offer.panel, offer.items.map((item) => item.panel)])

test('offered only on an agreement in force', () => {
  expect(troubleOffered(exchange())).toBe(true)
  expect(troubleOffered(exchange({ state: 'NEGOTIATING' }))).toBe(false)
  expect(troubleOffered(exchange({ state: 'CLOSED' }))).toBe(false)
  expect(troubleRoute(exchange({ state: 'CLOSED' }), 'BOTH_STOP')).toEqual({ offers: [], notes: [] })
})

test('they have not done their part: waive what they owe, end together, or close alone', () => {
  expect(ways(troubleRoute(exchange(), 'THEY_HAVENT'))).toEqual([
    ['WAIVE', null, [`move:${PAY}:WAIVE`]],
    ['PROPOSE_END', 'propose-end', []],
    ['REQUEST_CLOSE', 'request-close', []],
  ])
  // Something they marked delivered can be disputed too.
  const claimed = exchange({
    contributions: [
      { id: BIKE, status: 'PENDING' },
      { id: PAY, status: 'CLAIMED' },
    ],
  })
  expect(ways(troubleRoute(claimed, 'THEY_HAVENT')).slice(0, 2)).toEqual([
    ['DISPUTE', null, [`move:${PAY}:DISPUTE`]],
    ['WAIVE', null, [`move:${PAY}:WAIVE`]],
  ])
})

test('they owe nothing that is still open: said, with the ways that are left', () => {
  const settled = exchange({
    contributions: [
      { id: BIKE, status: 'PENDING' },
      { id: PAY, status: 'ACCEPTED' },
    ],
  })
  const route = troubleRoute(settled, 'THEY_HAVENT')
  expect(route.notes).toEqual(['nothingTheyOwe'])
  expect(route.offers.map((offer) => offer.way)).toEqual(['PROPOSE_END', 'REQUEST_CLOSE'])
})

test('I cannot do my part: propose a change, end together, or close alone', () => {
  expect(troubleRoute(exchange(), 'CANT_DO_MINE').offers.map((offer) => offer.way)).toEqual([
    'AMEND',
    'PROPOSE_END',
    'REQUEST_CLOSE',
  ])
  const pending = troubleRoute(
    exchange({ open_revision: exchange().in_force_revision, close_requested_by: 'B' }),
    'CANT_DO_MINE',
  )
  expect(pending.offers.map((offer) => offer.way)).toEqual(['PROPOSE_END'])
  expect(pending.notes).toEqual(['amendPending', 'closePending'])
})

test('we both want to stop: propose ending, or agree to theirs', () => {
  expect(ways(troubleRoute(exchange(), 'BOTH_STOP'))).toEqual([['PROPOSE_END', 'propose-end', []]])
  expect(ways(troubleRoute(exchange({ end_proposed_by: 'B' }), 'BOTH_STOP'))).toEqual([
    ['AGREE_END', 'agree-end', []],
  ])
  const mine = troubleRoute(exchange({ end_proposed_by: 'A' }), 'BOTH_STOP')
  expect(mine).toEqual({ offers: [], notes: ['endPending'] })
})

test('we disagree whether it was done: dispute, mark again, or let it go', () => {
  const doubted = exchange({
    contributions: [
      { id: BIKE, status: 'DISPUTED' },
      { id: PAY, status: 'CLAIMED' },
    ],
  })
  expect(ways(troubleRoute(doubted, 'DISAGREE'))).toEqual([
    ['DISPUTE', null, [`move:${PAY}:DISPUTE`]],
    ['RECLAIM', null, [`move:${BIKE}:RECLAIM`]],
    ['REQUEST_CLOSE', 'request-close', []],
  ])
  // Seen from the other side, the disputed bicycle can be let go.
  expect(ways(troubleRoute({ ...doubted, you: 'B' }, 'DISAGREE'))).toEqual([
    ['WAIVE', null, [`move:${BIKE}:WAIVE`]],
    ['REQUEST_CLOSE', 'request-close', []],
  ])
  expect(troubleRoute(exchange(), 'DISAGREE').notes).toEqual(['nothingInDoubt'])
})

test('the guide’s own panel names the situation it opened at', () => {
  expect(troubleSituationOf(troublePanel())).toBeNull()
  for (const situation of TROUBLE_SITUATIONS) {
    expect(troubleSituationOf(troublePanel(situation))).toBe(situation)
  }
  expect(troubleSituationOf('propose-end')).toBeUndefined()
  expect(troubleSituationOf(null)).toBeUndefined()
})

test('every sentence of the guide is filled in, in every language', () => {
  for (const language of ['en', 'es'] as const) {
    const { wording } = createI18n(language, wordingFor(language), () => {})
    const t = wording.trouble
    const all = [
      ...Object.values(t.situations),
      ...Object.values(t.explain),
      ...Object.values(t.means),
      t.nothingTheyOwe,
      t.endPending,
    ]
    for (const message of all) {
      expect(formatMessage(message, { name: 'Ben' }, language)).not.toMatch(/[{}]/)
    }
  }
})
