import type { components } from '@exchange/api-client'
import type { ReportReason } from '@exchange/shared'

import { client, send } from './api'

export type BlockedPerson = components['schemas']['BlockedPerson']
export type BlockStatus = components['schemas']['BlockStatus']

/**
 * Reporting and blocking (DESIGN.md §9). A refusal is thrown as an
 * `ApiFailure`, like every other call to the service.
 */
export const safetyApi = {
  /** Reports an exchange, and with it the other party. Repeating it is harmless. */
  reportExchange(id: string, reason: ReportReason, details: string | null): Promise<void> {
    return send(() =>
      client.POST('/v1/exchanges/{id}/reports', {
        params: { path: { id } },
        body: { reason, details },
      }),
    )
  },

  // Like the preview, the token travels in the body, never in a URL.
  reportInvitation(token: string, reason: ReportReason, details: string | null): Promise<void> {
    return send(() => client.POST('/v1/invitations/report', { body: { token, reason, details } }))
  },

  /** Whether the person signed in has blocked the other party of this exchange, and that party's name. */
  blockStatus(id: string): Promise<BlockStatus> {
    return send(() => client.GET('/v1/exchanges/{id}/block', { params: { path: { id } } }))
  },

  block(id: string): Promise<void> {
    return send(() => client.PUT('/v1/exchanges/{id}/block', { params: { path: { id } } }))
  },

  unblock(id: string): Promise<void> {
    return send(() => client.DELETE('/v1/exchanges/{id}/block', { params: { path: { id } } }))
  },

  blockedPeople(): Promise<BlockedPerson[]> {
    return send(() => client.GET('/v1/blocks'))
  },
}
