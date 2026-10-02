import { createApiClient } from '@exchange/api-client'
import { createExchangeApi } from '@exchange/shared'

export { ApiFailure, failureCode } from '@exchange/shared'
export type {
  InvitationPreview,
  RevisionSent,
  RevisionView,
  SendRevision,
  Slot,
} from '@exchange/shared'

// Same origin: the dev server proxies API paths to the Rust service, and in
// production the service serves the app. The session is an HTTP-only cookie
// the browser attaches by itself; no token is ever held by the page
// (DESIGN.md §8). The calls themselves are the ones the mobile app makes.
export const api = createExchangeApi({
  client: createApiClient(''),
  session: { delivery: 'COOKIE' },
})

/** Registers what to do when the service says the session is no longer valid. */
export function onSignedOut(handler: () => void): void {
  api.onSignedOut(handler)
}
