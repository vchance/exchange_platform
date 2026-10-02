import type { components } from '@exchange/api-client'

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
