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
