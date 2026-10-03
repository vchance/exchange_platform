import type { RevisionTerms } from '@yuppers/api-client'
import { expect, test } from 'vitest'

import { movesFor, type Move } from './fulfillment'
import { languages, wordingFor } from './language'
import { moneyIds, moveTextWording, moveWording, statusWording } from './money'

const terms: RevisionTerms = {
  party_a_name: 'Ana',
  party_b_name: 'Ben',
  terms: '',
  contributions: [
    { id: 'repair', from: 'A', type: 'SERVICE', description: 'Repair', due: { kind: 'ON_AGREEMENT' }, required: true },
    { id: 'payment', from: 'B', type: 'MONEY', description: 'Payment', due: { kind: 'ON_AGREEMENT' }, required: true, amount_minor: 40000 },
  ],
}

test('the money contributions of several revisions are found by id', () => {
  const amended: RevisionTerms = {
    ...terms,
    contributions: [
      ...terms.contributions,
      { id: 'tip', from: 'B', type: 'MONEY', description: 'Tip', due: { kind: 'ON_AGREEMENT' }, required: false, amount_minor: 500 },
    ],
  }
  expect([...moneyIds([terms, null, amended])].sort()).toEqual(['payment', 'tip'])
  expect(moneyIds([undefined]).size).toBe(0)
})

test('money is spoken of in words for paying and receiving, in every language', () => {
  const moves: Move[] = [...movesFor('CLAIMED', 'PROVIDER'), ...movesFor('CLAIMED', 'RECIPIENT'), 'RECLAIM']
  for (const { code } of languages) {
    const wording = wordingFor(code)
    for (const move of moves) {
      expect(moveWording(wording, move, false), `${code} ${move}`).toBe(wording.exchange.moves[move])
      expect(moveWording(wording, move, true), `${code} ${move}`).toBe(wording.exchange.moneyMoves[move])
      expect(moveTextWording(wording, move, true), `${code} ${move}`).toBe(
        wording.exchange.moneyMoveText[move],
      )
    }
    // The words that could be taken for a payment button are not reused for money.
    for (const move of ['CLAIM', 'CONFIRM'] as const) {
      expect(moveWording(wording, move, true)).not.toBe(moveWording(wording, move, false))
    }
    expect(statusWording(wording, 'CLAIMED', true)).toBe(wording.moneyStatus.CLAIMED)
    expect(statusWording(wording, 'CLAIMED', false)).toBe(wording.contributionStatus.CLAIMED)
    expect(statusWording(wording, 'PENDING', true)).not.toBe(statusWording(wording, 'PENDING', false))
  }
})
