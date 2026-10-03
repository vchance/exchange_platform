import type { Account, ExchangeSummary, ExchangeView } from '@exchange/api-client'
import type { HistoryPage, RecordDocument, RevisionView } from '@exchange/shared'

/*
 * A stand-in for the service, for rendering the web app's screens in a test
 * with the state a person would see them in. It answers the calls those
 * screens make and nothing else; the session is the cookie the browser
 * would hold, so here it is simply whether `account` is set.
 */

export const ACTIVE = '0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70'
export const DRAFT = '5d0c3b1a-2f64-4e8b-9a7d-1c2e3f4a5b6c'
export const OFFER = '7e2f3a4b-5c6d-4e7f-8a9b-0c1d2e3f4a5b'
export const REPAIR = '11111111-1111-4111-8111-111111111111'
export const PAYMENT = '22222222-2222-4222-8222-222222222222'
export const INVITATION = 'a3'.repeat(32)
/** The one code the stand-in accepts. */
export const GOOD_CODE = '123456'

export const ana: Account = {
  id: 'a0000000-0000-4000-8000-000000000001',
  display_name: 'Ana Ruiz',
  adult_confirmed: true,
  language: 'en',
  email: 'ana@example.test',
}

const PARTIES = { A: 'Ana Ruiz', B: 'Ben Ortiz' }

export const revision: RevisionView = {
  id: 'c0000000-0000-4000-8000-000000000001',
  sequence: 1,
  author: 'A',
  accepted_by: ['A', 'B'],
  content_hash: 'ab'.repeat(32),
  expires_at: '2026-10-16T12:00:00Z',
  note: 'Here is what we talked about on Tuesday.',
  terms: {
    party_a_name: PARTIES.A,
    party_b_name: PARTIES.B,
    terms: 'Repair the back fence.',
    contributions: [
      {
        id: REPAIR,
        from: 'A',
        type: 'SERVICE',
        description: 'Repair the back fence',
        due: { kind: 'DATE', date: '2026-10-30' },
        completion_criteria: 'The gate closes and latches.',
        required: true,
      },
      {
        id: PAYMENT,
        from: 'B',
        type: 'MONEY',
        description: 'Payment for the repair',
        due: { kind: 'AFTER_CONTRIBUTION', contribution: REPAIR },
        required: true,
        amount_minor: 45000,
      },
    ],
  },
}

const common = {
  currency: 'USD',
  timezone: 'America/Chicago',
  counterparty: 'CONFIRMED' as const,
}

/** An agreement in force, read by the person who started it. */
export function activeExchange(): ExchangeView {
  return {
    ...common,
    id: ACTIVE,
    version: 7,
    state: 'ACTIVE',
    you: 'A',
    display_code: 'PVVS-5Q2K',
    in_force_revision: revision,
    contributions: [
      { id: REPAIR, status: 'CLAIMED', since: '2026-10-20T14:30:00Z' },
      { id: PAYMENT, status: 'PENDING' },
    ],
  }
}

/** A proposal waiting for the reader, the invited party, to sign it. */
export function offerExchange(): ExchangeView {
  return {
    ...common,
    id: OFFER,
    version: 3,
    state: 'NEGOTIATING',
    you: 'B',
    display_code: 'OFFR-7Y2M',
    open_revision: { ...revision, accepted_by: ['A'] },
    contributions: [],
  }
}

/** A first proposal being written. */
export function draftExchange(): ExchangeView {
  return {
    ...common,
    id: DRAFT,
    version: 1,
    state: 'DRAFT',
    you: 'A',
    counterparty: 'UNCLAIMED',
    display_code: 'DRFT-0001',
    contributions: [],
    draft: {
      format: 1,
      base: null,
      partyA: PARTIES.A,
      partyB: PARTIES.B,
      terms: '',
      note: '',
      contributions: [
        {
          id: REPAIR,
          from: 'A',
          type: 'SERVICE',
          description: 'Repair the back fence',
          quantity: '',
          unit: '',
          due: { kind: 'DATE', date: '2026-10-30' },
          criteria: '',
          required: true,
          amount: '',
        },
        {
          id: PAYMENT,
          from: 'B',
          type: 'MONEY',
          description: 'Payment for the repair',
          quantity: '',
          unit: '',
          due: { kind: 'AFTER_CONTRIBUTION', contribution: REPAIR },
          criteria: '',
          required: true,
          amount: '450',
        },
      ],
    } as never,
  }
}

const exchanges = (): ExchangeView[] => [activeExchange(), offerExchange(), draftExchange()]

function summary(exchange: ExchangeView): ExchangeSummary {
  return {
    id: exchange.id,
    display_code: exchange.display_code,
    other_party_name:
      exchange.state === 'DRAFT' ? '' : exchange.you === 'A' ? PARTIES.B : PARTIES.A,
    state: exchange.state,
    updated_at: '2026-10-02T06:30:00Z',
    you: exchange.you,
  }
}

function history(exchange: ExchangeView): HistoryPage {
  const at = '2026-10-02T15:00:05Z'
  const sent = { id: revision.id, sequence: 1 }
  return {
    you: exchange.you,
    parties: PARTIES,
    earlier: null,
    events: [
      { sequence: 1, type: 'REVISION_SENT', actor: 'A', at, revision: sent, note: revision.note },
      ...(exchange.state === 'ACTIVE'
        ? [
            {
              sequence: 2,
              type: 'REVISION_ACCEPTED' as const,
              actor: 'B' as const,
              at,
              revision: sent,
            },
            {
              sequence: 3,
              type: 'CONTRIBUTION_CLAIMED' as const,
              actor: 'A' as const,
              at: '2026-10-20T14:30:00Z',
              revision: sent,
              contribution: { id: REPAIR, description: 'Repair the back fence' },
              status: 'CLAIMED' as const,
              note: 'Finished on Monday.',
            },
          ]
        : []),
    ],
  }
}

function record(exchange: ExchangeView): RecordDocument {
  const signedAt = '2026-10-02T16:10:00Z'
  const verification = (method: 'EMAIL_OTP' | 'PHONE_OTP') => ({
    method,
    verified_at: '2026-10-02T15:58:00Z',
    description: 'described by the record',
  })
  return {
    format: 'exchange-record',
    format_version: 2,
    generated_at: '2026-10-22T18:00:00Z',
    language: 'en',
    notices: {
      about: 'about, from the service',
      signatures: 'signatures, from the service',
      statements: 'statements, from the service',
      content_hash: 'content hash, from the service',
    },
    prepared_for: 'A',
    exchange: {
      id: exchange.id,
      display_code: exchange.display_code,
      timezone: exchange.timezone,
      currency: exchange.currency,
      created_at: '2026-10-02T14:50:00Z',
      state: exchange.state,
      counterparty: exchange.counterparty,
      in_force_revision: { id: revision.id, sequence: 1 },
      last_event: 3,
    },
    parties: PARTIES,
    contributions: revision.terms.contributions.map((item) => ({
      id: item.id,
      from: item.from,
      description: item.description,
      required: item.required,
      status: exchange.contributions.find((stands) => stands.id === item.id)?.status ?? 'PENDING',
      since: signedAt,
    })),
    revisions: [
      {
        id: revision.id,
        sequence: 1,
        author: 'A',
        sent_at: '2026-10-02T15:00:05Z',
        expires_at: revision.expires_at,
        standing: { status: 'IN_FORCE', since: signedAt, in_force_at: signedAt },
        note: revision.note,
        content_hash: revision.content_hash,
        signed: {
          v: 1,
          exchange: exchange.id,
          currency: exchange.currency,
          timezone: exchange.timezone,
          parties: PARTIES,
          terms: revision.terms.terms,
          attachments: [],
          contributions: revision.terms.contributions.map((item) => ({
            id: item.id,
            from: item.from,
            type: item.type,
            description: item.description,
            quantity: item.quantity ?? null,
            due: item.due,
            completion_criteria: item.completion_criteria ?? null,
            required: item.required,
            amount_minor: item.amount_minor ?? null,
            settlement: item.type === 'MONEY' ? 'OFF_PLATFORM' : null,
          })),
        },
        signatures: (['A', 'B'] as const).map((party) => ({
          party,
          name: PARTIES[party],
          signed_at: signedAt,
          content_hash: revision.content_hash,
          verification: verification(party === 'A' ? 'EMAIL_OTP' : 'PHONE_OTP'),
          consent: { language: 'en', version: 'draft-1' },
        })),
      },
    ],
    events: history(exchange).events,
    part: { from: { revisions_after: 0, events_after: 0 }, next: null, complete: true },
  } as RecordDocument
}

export interface FakeService {
  /** Who the session cookie belongs to; `null` when nobody is signed in. */
  account: Account | null
  fetch: typeof fetch
}

export function fakeService(account: Account | null): FakeService {
  const service: FakeService = {
    account,
    fetch: (async (input: RequestInfo | URL, init?: RequestInit) => {
      const request = input instanceof Request ? input : new Request(input, init)
      const text = await request.text()
      const body: unknown = text ? JSON.parse(text) : null
      const path = new URL(request.url).pathname
      const [status, answer] = respond(service, `${request.method} ${path}`, body)
      return new Response(answer === null ? null : JSON.stringify(answer), {
        status,
        headers: answer === null ? {} : { 'Content-Type': 'application/json' },
      })
    }) as typeof fetch,
  }
  return service
}

function respond(service: FakeService, call: string, body: unknown): [number, unknown] {
  if (call === 'GET /v1/meta') {
    return [
      200,
      {
        service: 'exchange-backend',
        version: '0.0.0',
        minimum_client_versions: { web: null, ios: null, android: null },
      },
    ]
  }
  if (call === 'POST /v1/auth/codes') return [204, null]
  if (call === 'POST /v1/auth/sessions') {
    const { code, identifier } = body as { code?: string; identifier?: string }
    if (code !== GOOD_CODE) return [400, { code: 'INVALID_CODE' }]
    // Anyone but Ana signs in for the first time, to an account with no name yet.
    service.account =
      identifier === ana.email ? ana : { ...ana, display_name: '', adult_confirmed: false }
    return [200, { account: service.account }]
  }
  if (call === 'POST /v1/invitations/preview') {
    const offer = offerExchange()
    return [
      200,
      {
        bound: false,
        display_code: offer.display_code,
        expires_at: revision.expires_at,
        currency: offer.currency,
        timezone: offer.timezone,
        revision: offer.open_revision,
      },
    ]
  }
  if (call === 'POST /v1/invitations/report') return [204, null]

  if (!service.account) return [401, { code: 'UNAUTHENTICATED' }]
  if (call === 'GET /v1/me') return [200, service.account]
  if (call === 'PATCH /v1/me') {
    service.account = { ...service.account, ...(body as Partial<Account>) }
    return [200, service.account]
  }
  if (call === 'GET /v1/exchanges') return [200, exchanges().map(summary)]
  if (call === 'GET /v1/blocks') return [200, []]
  for (const exchange of exchanges()) {
    const at = `/v1/exchanges/${exchange.id}`
    if (call === `GET ${at}`) return [200, exchange]
    if (call === `PUT ${at}/draft`) return [204, null]
    if (call === `GET ${at}/history`) return [200, history(exchange)]
    if (call === `GET ${at}/record`) return [200, record(exchange)]
    if (call === `GET ${at}/block`) return [200, { blocked: false, name: PARTIES.B }]
  }
  return [404, { code: 'NOT_FOUND' }]
}
