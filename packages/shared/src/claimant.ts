import type { ErrorCode, ExchangeView } from '@exchange/api-client'

import { failureCode } from './api'

/*
 * Someone who opened an invitation that named nobody, before the initiator
 * has confirmed them (DESIGN.md §8). Whoever holds a forwarded link can open
 * it, so until the initiator says "yes, this is who I invited" the claimant
 * can read the proposal, sign it and leave, and nothing else. This decides
 * what the screens offer; the service decides what is allowed.
 */

/**
 * Whether the person looking at this exchange is such a claimant: they can
 * sign or leave, and must not be offered declining or proposing changes.
 */
export function isUnconfirmedClaimant(exchange: ExchangeView): boolean {
  return (
    exchange.you === 'B' && exchange.state === 'NEGOTIATING' && exchange.counterparty === 'CLAIMED'
  )
}

/**
 * Whether the initiator is being asked "is this who you invited?", to which
 * the answers are confirming the claimant and removing them.
 */
export function isAwaitingYourConfirmation(exchange: ExchangeView): boolean {
  return (
    exchange.you === 'A' && exchange.state === 'NEGOTIATING' && exchange.counterparty === 'CLAIMED'
  )
}

/**
 * Whether the link the initiator sent can no longer bring anyone in, with
 * nobody in the invited party's place: it was used by someone who has since
 * been removed or has left, or it ran out. A new one has to be made.
 */
export function isInvitationSpent(exchange: ExchangeView): boolean {
  return exchange.invitation_open === false
}

/** The one call leaving makes. */
export interface Leaver {
  leaveExchange(id: string): Promise<void>
}

/**
 * Leaves an exchange, and says why not if it could not be done; `null` means
 * the exchange is no longer the caller's. That includes finding it already
 * gone: a first try whose reply was lost, or the initiator having removed
 * them in the meantime, ends in the same place.
 */
export async function leaveExchange(api: Leaver, id: string): Promise<ErrorCode | null> {
  try {
    await api.leaveExchange(id)
    return null
  } catch (error) {
    const code = failureCode(error)
    return code === 'NOT_FOUND' ? null : code
  }
}
