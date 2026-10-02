import type { components } from '@exchange/api-client'

type Status = components['schemas']['Status']
type Due = components['schemas']['DueDto']

/*
 * Which fulfillment actions to offer, mirroring the table in DESIGN.md §5.2.
 * This only decides which buttons appear; the service decides whether an
 * action is allowed.
 */

/** A party's role for one contribution: whoever owes it, or whoever receives it. */
export type Role = 'PROVIDER' | 'RECIPIENT'

/**
 * `RECLAIM` is marking a disputed contribution delivered again. It is sent as
 * a claim, and unlike a first claim it must say what was done about the
 * dispute.
 */
export type Move = 'CLAIM' | 'RECLAIM' | 'RETRACT_CLAIM' | 'CONFIRM' | 'DISPUTE' | 'WAIVE'

export function movesFor(status: Status, role: Role): Move[] {
  if (role === 'PROVIDER') {
    switch (status) {
      case 'PENDING':
        return ['CLAIM']
      case 'CLAIMED':
        return ['RETRACT_CLAIM']
      case 'DISPUTED':
        return ['RECLAIM']
      default:
        return []
    }
  }
  switch (status) {
    case 'PENDING':
      return ['CONFIRM', 'WAIVE']
    case 'CLAIMED':
      return ['CONFIRM', 'DISPUTE', 'WAIVE']
    case 'DISPUTED':
      return ['CONFIRM', 'WAIVE']
    default:
      return []
  }
}

/** Today's calendar date, `YYYY-MM-DD`, in an IANA timezone. */
export function todayIn(timezone: string, now: Date = new Date()): string {
  const parts = new Intl.DateTimeFormat('en', {
    timeZone: timezone,
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
  }).formatToParts(now)
  const part = (type: string) => parts.find((found) => found.type === type)?.value ?? ''
  return `${part('year')}-${part('month')}-${part('day')}`
}

/**
 * Overdue is derived, not stored: nothing delivered yet, and the due date has
 * gone by in the exchange's timezone (DESIGN.md §5.2).
 */
export function isOverdue(status: Status, due: Due, today: string): boolean {
  return status === 'PENDING' && due.kind === 'DATE' && due.date < today
}
