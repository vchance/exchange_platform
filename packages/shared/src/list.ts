import type { ExchangeSummary } from '@yuppers/api-client'

/*
 * The list of a person's exchanges, in groups: what is in progress first,
 * since that is what may be waiting on them, then drafts they have not sent,
 * then what is closed, which is kept but no longer needs anything. The
 * service lists most recently changed first, and each group keeps that order.
 */

export interface GroupedExchanges {
  /** Waiting to be signed, or agreed and in progress. */
  open: ExchangeSummary[]
  /** Never sent; only their initiator sees them. */
  drafts: ExchangeSummary[]
  closed: ExchangeSummary[]
}

export function groupExchanges(exchanges: readonly ExchangeSummary[]): GroupedExchanges {
  const groups: GroupedExchanges = { open: [], drafts: [], closed: [] }
  for (const exchange of exchanges) {
    if (exchange.state === 'CLOSED') groups.closed.push(exchange)
    else if (exchange.state === 'DRAFT') groups.drafts.push(exchange)
    else groups.open.push(exchange)
  }
  return groups
}
