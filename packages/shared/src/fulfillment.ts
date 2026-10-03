import type { Command, components, ExchangeView } from '@exchange/api-client'

type Status = components['schemas']['Status']
type Due = components['schemas']['DueDto']
type Slot = components['schemas']['Slot']
type Parties = components['schemas']['Parties']

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

/**
 * The name of the panel that opens a move on one contribution, so that
 * anything on the screen can open it: its own button, or the guide for when
 * something isn't working.
 */
export function movePanel(contribution: string, move: Move): string {
  return `move:${contribution}:${move}`
}

/**
 * What a move has to say for itself. A dispute must say why, and a claim
 * after a dispute must say what was done about it. A first claim may carry a
 * note; nothing else takes one. `label` is which of the note labels in the
 * wording goes over the field.
 */
export interface MoveNote {
  takes: boolean
  needs: boolean
  label: 'reasonLabel' | 'remedyLabel' | 'noteLabel'
}

export function noteFor(move: Move): MoveNote {
  const needs = move === 'DISPUTE' || move === 'RECLAIM'
  return {
    takes: needs || move === 'CLAIM',
    needs,
    label: move === 'DISPUTE' ? 'reasonLabel' : move === 'RECLAIM' ? 'remedyLabel' : 'noteLabel',
  }
}

/**
 * The command a move sends, or `null` when it needs a note and `written` has
 * none. A note on a move that takes none is left out.
 */
export function moveCommand(move: Move, contribution: string, written: string): Command | null {
  const note = written.trim()
  const { takes, needs } = noteFor(move)
  if (needs && note === '') return null
  return {
    type: 'CONTRIBUTION',
    contribution,
    action: move === 'RECLAIM' ? 'CLAIM' : move,
    note: takes && note !== '' ? note : null,
  }
}

/** The other side of the exchange. */
export function otherSlot(you: Slot): Slot {
  return you === 'A' ? 'B' : 'A'
}

/**
 * The other party's name as it is written in the latest terms, or empty when
 * nobody has been named yet. An exchange with no revision to read, such as
 * one closed without agreement, has no name in its view; `parties`, which
 * its history carries, names both sides as the last terms did.
 */
export function otherPartyName(exchange: ExchangeView, parties?: Parties | null): string {
  const other = otherSlot(exchange.you)
  const latest = (exchange.open_revision ?? exchange.in_force_revision)?.terms
  if (latest) return other === 'A' ? latest.party_a_name : latest.party_b_name
  return parties?.[other] ?? ''
}

/** How long a delivery may go unconfirmed before the provider is pointed to the ways out. */
export const LONG_WAIT_DAYS = 7

/**
 * Whether something marked delivered has waited a long time for the other
 * party's confirmation (DESIGN.md §5.2): a claim nobody answers stays a
 * claim, and the provider's way out is to ask to close.
 */
export function waitingLong(
  status: Status,
  since: string | null | undefined,
  now: Date = new Date(),
): boolean {
  if (status !== 'CLAIMED' || !since) return false
  const from = new Date(since).getTime()
  if (Number.isNaN(from)) return false
  return now.getTime() - from >= LONG_WAIT_DAYS * 24 * 60 * 60 * 1000
}

/** Where each contribution of the agreement in force stands. */
export function statusesOf(exchange: ExchangeView): Map<string, Status> {
  return new Map(exchange.contributions.map((contribution) => [contribution.id, contribution.status]))
}

/**
 * How many required contributions are neither accepted nor waived: what
 * stands between the agreement in force and completion (DESIGN.md §5.1).
 */
export function remainingRequired(exchange: ExchangeView): number {
  const inForce = exchange.in_force_revision
  if (!inForce) return 0
  const statuses = statusesOf(exchange)
  return inForce.terms.contributions.filter((contribution) => {
    const status = statuses.get(contribution.id)
    return contribution.required && status !== 'ACCEPTED' && status !== 'WAIVED'
  }).length
}

/** Today's calendar date, `YYYY-MM-DD`, in an IANA timezone. */
export function todayIn(timezone: string, now: Date = new Date()): string {
  // One field at a time, each from plain `format`: the engine in the mobile
  // apps takes a formatted date apart less reliably than a browser does.
  const field = (options: Intl.DateTimeFormatOptions, width: number) =>
    new Intl.DateTimeFormat('en', { timeZone: timezone, ...options })
      .format(now)
      .replace(/\D/g, '')
      .padStart(width, '0')
  return `${field({ year: 'numeric' }, 4)}-${field({ month: '2-digit' }, 2)}-${field({ day: '2-digit' }, 2)}`
}

/**
 * Overdue is derived, not stored: nothing delivered yet, and the due date has
 * gone by in the exchange's timezone (DESIGN.md §5.2).
 */
export function isOverdue(status: Status, due: Due, today: string): boolean {
  return status === 'PENDING' && due.kind === 'DATE' && due.date < today
}
