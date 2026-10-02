import { expect, test } from 'vitest'

import { wordingFor } from './language'
import { formatMessage } from './message'
import {
  eventMessage,
  joinRecord,
  noteKind,
  readWholeRecord,
  termsOfRevision,
  type RecordDocument,
  type RecordEvent,
  type RecordRevision,
} from './record'

const words = wordingFor('en').record.events
const parties = { A: 'Ana Ruiz', B: 'Ben Ortiz' }

function event(fields: Partial<RecordEvent> & Pick<RecordEvent, 'type' | 'actor'>): RecordEvent {
  return { sequence: 1, at: '2026-10-02T15:00:00Z', ...fields }
}

function say(found: RecordEvent, reader: 'A' | 'B' | null): string {
  const { message, values } = eventMessage(found, words, reader, parties)
  return formatMessage(message, values, 'en')
}

const revision = { id: '00000000-0000-0000-0000-000000000001', sequence: 2 }

test('what a party did is said to them as "you" and to anyone else by name', () => {
  const sent = event({ type: 'REVISION_SENT', actor: 'B', revision })
  expect(say(sent, 'B')).toBe('You sent version 2 and, by sending it, signed it.')
  expect(say(sent, 'A')).toBe('Ben Ortiz sent version 2 and, by sending it, signed it.')
  // A page that may be handed to someone else names everyone.
  expect(say(sent, null)).toBe('Ben Ortiz sent version 2 and, by sending it, signed it.')
})

test('what nobody in particular did is said the same way to everyone', () => {
  const inForce = event({ type: 'AGREEMENT_IN_FORCE', actor: 'A', revision })
  expect(say(inForce, 'A')).toBe(say(inForce, 'B'))
  expect(say(inForce, null)).toContain('Version 2')

  const expired = event({ type: 'REVISION_EXPIRED', actor: 'SYSTEM', revision })
  expect(say(expired, 'A')).toBe('Version 2 expired before both parties had signed it.')
})

test('a closing says how the exchange closed', () => {
  const closed = (outcome: RecordEvent['outcome']) =>
    say(event({ type: 'EXCHANGE_CLOSED', actor: 'SYSTEM', outcome }), 'A')
  expect(closed('COMPLETED')).toBe(words.closed.COMPLETED)
  expect(closed('NOT_AGREED')).toBe(words.closed.NOT_AGREED)
  expect(closed('ENDED_BY_AGREEMENT')).toBe(words.closed.ENDED_BY_AGREEMENT)
  expect(closed('UNRESOLVED')).toBe(words.closed.UNRESOLVED)
})

test('every sentence is filled in completely, in every language, for every reader', () => {
  const types = [
    ...Object.keys(words.you),
    ...Object.keys(words.neutral),
    'EXCHANGE_CLOSED',
  ] as RecordEvent['type'][]
  for (const language of ['en', 'es'] as const) {
    const translated = wordingFor(language).record.events
    for (const type of types) {
      for (const reader of ['A', 'B', null] as const) {
        const found = event({ type, actor: 'A', revision, outcome: 'COMPLETED' })
        const { message, values } = eventMessage(found, translated, reader, parties)
        const text = formatMessage(message, values, language)
        expect(text, `${language} ${type}`).not.toMatch(/[{}]/)
        expect(text.trim(), `${language} ${type}`).not.toBe('')
      }
    }
  }
})

test('a note is labelled for what it is', () => {
  expect(noteKind(event({ type: 'REVISION_SENT', actor: 'A' }))).toBe('message')
  expect(noteKind(event({ type: 'CONTRIBUTION_DISPUTED', actor: 'B' }))).toBe('reason')
  expect(noteKind(event({ type: 'CLOSE_REQUESTED', actor: 'A' }))).toBe('statement')
  expect(noteKind(event({ type: 'STATEMENT_ADDED', actor: 'B' }))).toBe('statement')
  expect(noteKind(event({ type: 'CONTRIBUTION_CLAIMED', actor: 'A' }))).toBe('note')
})

function sentRevision(sequence: number): RecordRevision {
  return {
    id: `00000000-0000-0000-0000-00000000000${sequence}`,
    sequence,
    author: 'A',
    sent_at: '2026-10-02T15:00:00Z',
    expires_at: '2026-10-16T15:00:00Z',
    standing: { status: 'OPEN', since: '2026-10-02T15:00:00Z' },
    content_hash: 'ab'.repeat(32),
    signatures: [],
    signed: {
      v: 1,
      exchange: '00000000-0000-0000-0000-0000000000ee',
      currency: 'USD',
      timezone: 'America/Chicago',
      parties,
      terms: 'Repair the back fence.',
      attachments: [],
      contributions: [
        {
          id: '00000000-0000-0000-0000-0000000000c1',
          from: 'B',
          type: 'MONEY',
          description: 'Payment on completion',
          quantity: null,
          due: { kind: 'ON_AGREEMENT' },
          completion_criteria: null,
          required: true,
          amount_minor: 40000,
          settlement: 'OFF_PLATFORM',
        },
      ],
    },
  }
}

test('the signed terms read as terms do everywhere else', () => {
  const terms = termsOfRevision(sentRevision(1))
  expect(terms.party_a_name).toBe('Ana Ruiz')
  expect(terms.party_b_name).toBe('Ben Ortiz')
  expect(terms.terms).toBe('Repair the back fence.')
  expect(terms.contributions).toEqual([
    {
      id: '00000000-0000-0000-0000-0000000000c1',
      from: 'B',
      type: 'MONEY',
      description: 'Payment on completion',
      quantity: null,
      due: { kind: 'ON_AGREEMENT' },
      completion_criteria: null,
      required: true,
      amount_minor: 40000,
    },
  ])
})

function part(
  from: [number, number],
  next: [number, number] | null,
  revisions: number[],
  events: number[],
  lastEvent = 4,
): RecordDocument {
  const at = (pair: [number, number]) => ({ revisions_after: pair[0], events_after: pair[1] })
  return {
    format: 'exchange-record',
    format_version: 1,
    generated_at: '2026-10-02T15:00:00Z',
    language: 'en',
    notices: { about: 'a', signatures: 's', statements: 't', content_hash: 'h' },
    prepared_for: 'A',
    exchange: {
      id: '00000000-0000-0000-0000-0000000000ee',
      display_code: 'AB12-CD34',
      timezone: 'America/Chicago',
      currency: 'USD',
      created_at: '2026-10-02T15:00:00Z',
      state: 'NEGOTIATING',
      counterparty: 'CONFIRMED',
      last_event: lastEvent,
    },
    parties,
    contributions: [],
    revisions: revisions.map(sentRevision),
    events: events.map((sequence) => event({ type: 'END_PROPOSED', actor: 'A', sequence })),
    part: {
      from: at(from),
      next: next ? at(next) : null,
      complete: from[0] === 0 && from[1] === 0 && next === null,
    },
  }
}

test('one part that is the whole record is left exactly as it came', () => {
  const only = part([0, 0], null, [1], [1, 2])
  expect(joinRecord([only])).toBe(only)
})

test('the parts of a long record join into one whole document', () => {
  const joined = joinRecord([
    part([0, 0], [2, 2], [1, 2], [1, 2]),
    part([2, 2], [3, 4], [3], [3, 4]),
    part([3, 4], null, [], [5]),
  ])
  expect(joined.revisions.map((found) => found.sequence)).toEqual([1, 2, 3])
  expect(joined.events.map((found) => found.sequence)).toEqual([1, 2, 3, 4, 5])
  expect(joined.part).toEqual({
    from: { revisions_after: 0, events_after: 0 },
    next: null,
    complete: true,
  })
})

test('parts that stop short of the end do not claim to be the whole record', () => {
  const joined = joinRecord([part([0, 0], [1, 2], [1], [1, 2]), part([1, 2], [2, 4], [2], [3, 4])])
  expect(joined.part.complete).toBe(false)
  expect(joined.part.next).toEqual({ revisions_after: 2, events_after: 4 })
})

test('reading a whole record follows each part to the next', async () => {
  const asked: (string | null)[] = []
  const whole = await readWholeRecord(async (from) => {
    asked.push(from ? `${from.revisions_after},${from.events_after}` : null)
    if (!from) return part([0, 0], [1, 2], [1], [1, 2])
    return part([1, 2], null, [2], [3, 4])
  })
  expect(asked).toEqual([null, '1,2'])
  expect(whole.part.complete).toBe(true)
  expect(whole.events).toHaveLength(4)
})

test('a record that changed between two parts is read again', async () => {
  let reads = 0
  const whole = await readWholeRecord(async (from) => {
    reads += 1
    // The first time through, something happens before the second part.
    if (reads === 1) return part([0, 0], [1, 2], [1], [1, 2], 4)
    if (reads === 2) return part([1, 2], null, [2], [3, 4, 5], 5)
    if (!from) return part([0, 0], [1, 2], [1], [1, 2], 5)
    return part([1, 2], null, [2], [3, 4, 5], 5)
  })
  expect(reads).toBe(4)
  expect(whole.events.map((found) => found.sequence)).toEqual([1, 2, 3, 4, 5])
  expect(whole.exchange.last_event).toBe(5)
})
