import { api } from './api'

export type { BlockedPerson, BlockStatus } from '@exchange/shared'

/**
 * Reporting and blocking (DESIGN.md §9). A refusal is thrown as an
 * `ApiFailure`, like every other call to the service. The calls are the
 * shared ones; this names the part of them these screens use.
 */
export const safetyApi = {
  reportExchange: api.reportExchange,
  reportInvitation: api.reportInvitation,
  blockStatus: api.blockStatus,
  block: api.block,
  unblock: api.unblock,
  blockedPeople: api.blockedPeople,
}
