import { createApiClient } from '@yuppers/api-client'
import { createExchangeApi, type ClientIdentity } from '@yuppers/shared'

export { ApiFailure, failureCode } from '@yuppers/shared'
export type {
  InvitationPreview,
  RevisionSent,
  RevisionView,
  SendRevision,
  Slot,
} from '@yuppers/shared'

/** Which client this is and which build, named to the service on every request. */
export const WEB_CLIENT: ClientIdentity = { name: 'web', version: __WEB_VERSION__ }

// Same origin: the dev server proxies API paths to the Rust service, and in
// production the service serves the app. The session is an HTTP-only cookie
// the browser attaches by itself; no token is ever held by the page
// (DESIGN.md §8). The calls themselves are the ones the mobile app makes.
export const api = createExchangeApi({
  client: createApiClient(''),
  session: { delivery: 'COOKIE' },
  identity: WEB_CLIENT,
})

/** Registers what to do when the service says the session is no longer valid. */
export function onSignedOut(handler: () => void): void {
  api.onSignedOut(handler)
}

/** Registers what to do when the service says this build is too old to act. */
export function onClientTooOld(handler: () => void): void {
  api.onClientTooOld(handler)
}
