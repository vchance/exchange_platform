import type { ApiClient, ExchangeView } from '@exchange/api-client'
import { expect, test } from 'vitest'

import { ApiFailure, createExchangeApi } from './api'
import {
  isAwaitingYourConfirmation,
  isInvitationSpent,
  isUnconfirmedClaimant,
  leaveExchange,
} from './claimant'
import { canCompose } from './composer'
import { wordingFor } from './language'
import { formatMessage } from './message'
import { eventMessage, type RecordEvent } from './record'

const ID = '0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70'

function exchange(patch: Partial<ExchangeView>): ExchangeView {
  return {
    id: ID,
    version: 3,
    state: 'NEGOTIATING',
    you: 'B',
    counterparty: 'CLAIMED',
    currency: 'USD',
    timezone: 'America/Chicago',
    display_code: 'ABCD-1234',
    contributions: [],
    open_revision: {
      id: 'open',
      sequence: 1,
      author: 'A',
      expires_at: '2026-10-20T15:00:00Z',
      accepted_by: ['A'],
      content_hash: 'ab'.repeat(32),
      terms: { party_a_name: 'Ana Ruiz', party_b_name: 'Ben Ortiz', terms: '', contributions: [] },
    },
    ...patch,
  }
}

test('a claimant is limited from opening the link until the initiator confirms them', () => {
  expect(isUnconfirmedClaimant(exchange({}))).toBe(true)
  // Confirmed, or named in the invitation, they are a party like any other.
  expect(isUnconfirmedClaimant(exchange({ counterparty: 'CONFIRMED' }))).toBe(false)
  // The initiator is never the one limited.
  expect(isUnconfirmedClaimant(exchange({ you: 'A' }))).toBe(false)
  // Once it has closed there is nothing left to sign or to leave.
  expect(isUnconfirmedClaimant(exchange({ state: 'CLOSED' }))).toBe(false)
})

test('a claimant who is not yet confirmed is not offered the composer', () => {
  expect(canCompose(exchange({}))).toBe(false)
  expect(canCompose(exchange({ counterparty: 'CONFIRMED' }))).toBe(true)
  // The initiator may still change their own offer while they decide.
  expect(canCompose(exchange({ you: 'A' }))).toBe(true)
})

test('the initiator is asked about a claimant only while one is waiting', () => {
  expect(isAwaitingYourConfirmation(exchange({ you: 'A' }))).toBe(true)
  expect(isAwaitingYourConfirmation(exchange({}))).toBe(false)
  expect(isAwaitingYourConfirmation(exchange({ you: 'A', counterparty: 'UNCLAIMED' }))).toBe(false)
  expect(isAwaitingYourConfirmation(exchange({ you: 'A', counterparty: 'CONFIRMED' }))).toBe(false)
  expect(isAwaitingYourConfirmation(exchange({ you: 'A', state: 'CLOSED' }))).toBe(false)
})

test('a link is spent only when the service says none can be used', () => {
  expect(isInvitationSpent(exchange({ you: 'A', invitation_open: false }))).toBe(true)
  expect(isInvitationSpent(exchange({ you: 'A', invitation_open: true }))).toBe(false)
  // Not said at all: someone is in the place, or the reader is not the initiator.
  expect(isInvitationSpent(exchange({ you: 'A', invitation_open: null }))).toBe(false)
  expect(isInvitationSpent(exchange({}))).toBe(false)
})

test('leaving is one call with no body, and the exchange is then gone', async () => {
  const calls: { method: string; path: string; init: unknown }[] = []
  const client = {
    POST: async (path: string, init: unknown) => {
      calls.push({ method: 'POST', path, init })
      return { response: { ok: true, status: 204 } }
    },
  } as unknown as ApiClient
  const api = createExchangeApi({ client, session: { delivery: 'TOKEN', token: () => 's3cret' } })

  expect(await leaveExchange(api, ID)).toBeNull()
  expect(calls).toEqual([
    {
      method: 'POST',
      path: '/v1/exchanges/{id}/leave',
      init: { headers: { Authorization: 'Bearer s3cret' }, params: { path: { id: ID } } },
    },
  ])
})

test('finding the exchange already gone counts as having left; any other refusal is said', async () => {
  const refusing = (code: ConstructorParameters<typeof ApiFailure>[0]) => ({
    leaveExchange: () => Promise.reject(new ApiFailure(code)),
  })
  // A first try whose reply was lost, or the initiator got there first.
  expect(await leaveExchange(refusing('NOT_FOUND'), ID)).toBeNull()
  // Confirmed in the meantime: they are a party now, and are told so.
  expect(await leaveExchange(refusing('ACTION_NOT_ALLOWED'), ID)).toBe('ACTION_NOT_ALLOWED')
  expect(await leaveExchange(refusing('SERVICE_UNAVAILABLE'), ID)).toBe('SERVICE_UNAVAILABLE')
})

// ---- The history ----------------------------------------------------------

const parties = { A: 'Ana Ruiz', B: 'Ben Ortiz' }
const revision = { id: '00000000-0000-0000-0000-000000000001', sequence: 1 }

function event(fields: Partial<RecordEvent> & Pick<RecordEvent, 'type' | 'actor'>): RecordEvent {
  return { sequence: 1, at: '2026-10-02T15:00:00Z', ...fields }
}

function say(found: RecordEvent, reader: 'A' | 'B' | null, language: 'en' | 'es' = 'en'): string {
  const words = wordingFor(language).record.events
  const { message, values } = eventMessage(found, words, reader, parties)
  return formatMessage(message, values, language)
}

test('what a removed claimant did is never put in the name of the party the agreement names', () => {
  const theirs = [
    event({ type: 'COUNTERPARTY_CLAIMED', actor: 'B', by_removed_claimant: true }),
    event({ type: 'REVISION_ACCEPTED', actor: 'B', by_removed_claimant: true, revision }),
    event({ type: 'REVISION_SENT', actor: 'B', by_removed_claimant: true, revision }),
    event({ type: 'COUNTERPARTY_RELEASED', actor: 'B', by_removed_claimant: true }),
  ]
  for (const language of ['en', 'es'] as const) {
    for (const found of theirs) {
      // Ben, who holds that place now, is not told "you" did it either.
      const told = (['A', 'B', null] as const).map((reader) => say(found, reader, language))
      expect(new Set(told).size, `${language} ${found.type}`).toBe(1)
      expect(told[0]).not.toContain('Ben Ortiz')
      expect(told[0]).not.toMatch(/[{}]/)
      expect(told[0].trim()).not.toBe('')
    }
  }
  expect(say(theirs[1], 'A')).toContain('version 1')
  expect(say(theirs[1], 'A')).toContain('void')

  // The same things done by the person who is the party are theirs.
  const bens = event({ type: 'REVISION_ACCEPTED', actor: 'B', revision })
  expect(say(bens, 'B')).toBe('You signed version 1.')
  expect(say(bens, 'A')).toBe('Ben Ortiz signed version 1.')
})

test('removing a claimant is said as the initiator’s act', () => {
  const rejected = event({ type: 'COUNTERPARTY_REJECTED', actor: 'A', signature_void: true })
  expect(say(rejected, 'A')).toMatch(/^You said /)
  expect(say(rejected, 'B')).toMatch(/^Ana Ruiz said /)
  expect(say(rejected, null)).toMatch(/^Ana Ruiz said /)
})
