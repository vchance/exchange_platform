import type { components, ExchangeView } from '@yuppers/api-client'

export type ReportReason = components['schemas']['ReportReason']

/** Longest details a report may carry. The service enforces the same limit. */
export const REPORT_DETAILS_MAX_CHARS = 2000

// Listing a reason here is what offers it. A reason added to the API fails
// the typecheck on this line until it is given a place.
const offered: Record<ReportReason, true> = {
  HARASSMENT: true,
  PROHIBITED_TRADE: true,
  SCAM: true,
  IMPERSONATION: true,
  UNDERAGE: true,
  UNWANTED: true,
  OTHER: true,
}

/** Every reason a report can give, in the order they are offered. */
export const REPORT_REASONS = Object.keys(offered) as ReportReason[]

/**
 * Whether an exchange has someone on the other side. Until someone has joined
 * there is nobody to report and nobody to block.
 */
export function hasOtherParty(exchange: Pick<ExchangeView, 'state' | 'counterparty'>): boolean {
  return exchange.state !== 'DRAFT' && exchange.counterparty !== 'UNCLAIMED'
}

/**
 * Whether a block is being made, or was made, from an agreement in force.
 * A block leaves such an agreement standing (DESIGN.md §9): either party can
 * still complete or close it, and the person blocked can still act in it, so
 * the person blocking is told so, and how to close it.
 */
export function blockLeavesAgreement(exchange: Pick<ExchangeView, 'state'>): boolean {
  return exchange.state === 'ACTIVE'
}

/**
 * Whether to offer closing without agreement beside a block: the person has
 * blocked the other party, the agreement between them is still in force, and
 * nobody has asked to close it yet. The offer opens the same request to close
 * that the exchange offers everywhere else; nothing is sent until that is
 * confirmed too.
 */
export function offersCloseAfterBlock(
  exchange: Pick<ExchangeView, 'state' | 'close_requested_by'>,
  blocked: boolean | null,
): boolean {
  return blocked === true && blockLeavesAgreement(exchange) && !exchange.close_requested_by
}

/** "Something else" with nothing said is a report nobody can act on. */
export function reportNeedsDetails(reason: ReportReason | null): boolean {
  return reason === 'OTHER'
}

export type ReportCheck =
  | { ok: true; reason: ReportReason; details: string | null }
  | { ok: false; reasonMissing: boolean; detailsMissing: boolean }

/**
 * Whether a report can be sent as it stands, and if so what is sent: the
 * reason, and the details without the space around them, or `null` when
 * nothing was written. A reason is never assumed.
 */
export function checkReport(reason: ReportReason | null, details: string): ReportCheck {
  const written = details.trim()
  const detailsMissing = reportNeedsDetails(reason) && written === ''
  if (reason === null || detailsMissing) {
    return { ok: false, reasonMissing: reason === null, detailsMissing }
  }
  return { ok: true, reason, details: written === '' ? null : written }
}
