import {
  createApiClient,
  type Account,
  type Command,
  type components,
  type ErrorCode,
  type ExchangeSummary,
  type ExchangeView,
} from '@exchange/api-client'

import { idempotencyKeys } from './idempotency'

type Schemas = components['schemas']
export type InvitationPreview = Schemas['InvitationPreview']
export type RevisionSent = Schemas['RevisionSent']
export type RevisionView = Schemas['RevisionView']
export type SendRevision = Schemas['SendRevision']
export type Slot = Schemas['Slot']

/**
 * A refusal from the service, or no answer from it. Screens show
 * `wording.errors[code]`; nothing reads message text (DESIGN.md §13.3).
 */
export class ApiFailure extends Error {
  readonly code: ErrorCode
  /** No reply from the service itself, so whether the request took effect is unknown. */
  readonly unanswered: boolean

  constructor(code: ErrorCode, unanswered = false) {
    super(code)
    this.name = 'ApiFailure'
    this.code = code
    this.unanswered = unanswered
  }
}

/** The error code to show for anything a request can throw. */
export function failureCode(error: unknown): ErrorCode {
  return error instanceof ApiFailure ? error.code : 'INTERNAL'
}

// Same origin: the dev server proxies API paths to the Rust service, and in
// production the service serves the app. The session is an HTTP-only cookie
// the browser attaches by itself; no token is ever held by the page.
const client = createApiClient('')
const keys = idempotencyKeys()

let signedOut: () => void = () => {}

/** Registers what to do when the service says the session is no longer valid. */
export function onSignedOut(handler: () => void): void {
  signedOut = handler
}

interface Reply<T> {
  data?: T
  error?: unknown
  response: Response
}

async function send<T>(request: () => Promise<Reply<T>>): Promise<T> {
  let reply: Reply<T>
  try {
    reply = await request()
  } catch {
    throw new ApiFailure('SERVICE_UNAVAILABLE', true)
  }
  if (reply.response.ok) return reply.data as T

  const body = reply.error
  const code =
    typeof body === 'object' && body !== null && typeof (body as { code?: unknown }).code === 'string'
      ? ((body as { code: string }).code as ErrorCode)
      : null
  // An error that is not the service's own came from something in between.
  if (code === null) throw new ApiFailure('SERVICE_UNAVAILABLE', true)
  if (code === 'UNAUTHENTICATED') signedOut()
  throw new ApiFailure(code)
}

/**
 * Sends a change to an exchange with an idempotency key: a fresh one for each
 * attempt, the same one again when retrying a request that went unanswered.
 */
async function change<T>(
  path: string,
  body: unknown,
  request: (key: string) => Promise<Reply<T>>,
): Promise<T> {
  const fingerprint = `${path} ${JSON.stringify(body)}`
  const key = keys.keyFor(fingerprint)
  try {
    const result = await send(() => request(key))
    keys.answered(fingerprint)
    return result
  } catch (error) {
    if (error instanceof ApiFailure && error.unanswered) keys.unanswered(fingerprint, key)
    else keys.answered(fingerprint)
    throw error
  }
}

export const api = {
  /** The signed-in account, or `null` when nobody is signed in. */
  async me(): Promise<Account | null> {
    let reply: Reply<Account>
    try {
      reply = await client.GET('/v1/me')
    } catch {
      throw new ApiFailure('SERVICE_UNAVAILABLE', true)
    }
    if (reply.response.status === 401) return null
    return send(async () => reply)
  },

  requestCode(identifier: string): Promise<void> {
    return send(() => client.POST('/v1/auth/codes', { body: { identifier } }))
  },

  async signIn(identifier: string, code: string, language: string): Promise<Account> {
    const created = await send(() =>
      client.POST('/v1/auth/sessions', {
        // A cookie the page cannot read, never a token (DESIGN.md §8).
        body: { identifier, code, delivery: 'COOKIE', language },
      }),
    )
    return created.account
  },

  signOut(): Promise<void> {
    return send(() => client.DELETE('/v1/auth/session'))
  },

  updateMe(update: Schemas['UpdateAccount']): Promise<Account> {
    return send(() => client.PATCH('/v1/me', { body: update }))
  },

  listExchanges(): Promise<ExchangeSummary[]> {
    return send(() => client.GET('/v1/exchanges'))
  },

  createExchange(timezone: string): Promise<ExchangeView> {
    return send(() => client.POST('/v1/exchanges', { body: { timezone } }))
  },

  getExchange(id: string): Promise<ExchangeView> {
    return send(() => client.GET('/v1/exchanges/{id}', { params: { path: { id } } }))
  },

  /** Saves the working copy. It is private to its author and binds nobody. */
  saveDraft(id: string, draft: object): Promise<void> {
    return send(() =>
      client.PUT('/v1/exchanges/{id}/draft', {
        params: { path: { id } },
        // The service stores it as given; the generated type cannot say so.
        body: { body: draft as Record<string, never> },
      }),
    )
  },

  sendRevision(id: string, body: SendRevision): Promise<RevisionSent> {
    return change(`revisions/${id}`, body, (key) =>
      client.POST('/v1/exchanges/{id}/revisions', {
        params: { path: { id }, header: { 'Idempotency-Key': key } },
        body,
      }),
    )
  },

  runCommand(id: string, expectedVersion: number, command: Command): Promise<ExchangeView> {
    const body = { expected_version: expectedVersion, command }
    return change(`commands/${id}`, body, (key) =>
      client.POST('/v1/exchanges/{id}/commands', {
        params: { path: { id }, header: { 'Idempotency-Key': key } },
        body,
      }),
    )
  },

  /** Replaces the invitation link and returns the new token, which is shown once. */
  async reissueInvitation(id: string, boundTo: string | null): Promise<string> {
    const issued = await send(() =>
      client.POST('/v1/exchanges/{id}/invitation', {
        params: { path: { id } },
        body: { bound_to: boundTo },
      }),
    )
    return issued.invitation_token
  },

  // The token travels in the body, so it never appears in a URL the service
  // might log.
  previewInvitation(token: string): Promise<InvitationPreview> {
    return send(() => client.POST('/v1/invitations/preview', { body: { token } }))
  },

  claimInvitation(token: string): Promise<ExchangeView> {
    return send(() => client.POST('/v1/invitations/claim', { body: { token } }))
  },
}
