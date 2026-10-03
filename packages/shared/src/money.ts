import type { components, RevisionTerms } from '@yuppers/api-client'

import type { Move } from './fulfillment'
import type { Wording } from './wording/types'

type Status = components['schemas']['Status']

/*
 * Money is paid outside the product and only recorded here (DESIGN.md §7,
 * §11): one party says they paid, the other says they received it. So that
 * nobody takes a button for a payment, money is spoken of in words for
 * paying and receiving, never in the words for delivering goods or work.
 * These pick the money wording where a contribution is money.
 */

export function isMoney(contribution: { type: components['schemas']['ContributionType'] }): boolean {
  return contribution.type === 'MONEY'
}

/** The ids of every money contribution in any of several revisions' terms. */
export function moneyIds(terms: readonly (RevisionTerms | null | undefined)[]): Set<string> {
  const ids = new Set<string>()
  for (const revision of terms) {
    for (const contribution of revision?.contributions ?? []) {
      if (isMoney(contribution)) ids.add(contribution.id)
    }
  }
  return ids
}

/** Where a contribution stands, in words for money or for goods and work. */
export function statusWording(wording: Wording, status: Status, money: boolean): string {
  return money ? wording.moneyStatus[status] : wording.contributionStatus[status]
}

/** What a move is called on its button. */
export function moveWording(wording: Wording, move: Move, money: boolean): string {
  return money ? wording.exchange.moneyMoves[move] : wording.exchange.moves[move]
}

/** What the person is told before making a move. Uses `{name}`. */
export function moveTextWording(wording: Wording, move: Move, money: boolean): string {
  return money ? wording.exchange.moneyMoveText[move] : wording.exchange.moveText[move]
}
