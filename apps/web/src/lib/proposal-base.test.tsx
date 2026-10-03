// @vitest-environment jsdom
import type { components, ExchangeView, RevisionTerms } from '@exchange/api-client'
import { useProposalBase, type ProposalBase, type RecordDocument } from '@exchange/shared'
import { act } from 'react'
import { createRoot } from 'react-dom/client'
import { describe, expect, test } from 'vitest'

/*
 * The shared hook that finds what a proposal waiting for the reader is
 * compared with, before they sign it: the agreement in force for an
 * amendment, the version it answers for a counteroffer.
 */

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

type Contribution = components['schemas']['ContributionDto']

const BIKE = '00000000-0000-4000-8000-000000000011'
const PAY = '00000000-0000-4000-8000-000000000012'
const bike: Contribution = {
  id: BIKE,
  from: 'A',
  type: 'ITEM',
  description: 'A blue bicycle',
  due: { kind: 'ON_AGREEMENT' },
  required: true,
}
const pay: Contribution = { ...bike, id: PAY, from: 'B', type: 'MONEY', amount_minor: 100 }

function terms(contributions: Contribution[]): RevisionTerms {
  return { party_a_name: 'Ana', party_b_name: 'Ben', terms: '', contributions }
}

describe('what a proposal waiting for the reader is compared with', () => {
  function exchange(fields: Partial<ExchangeView>): ExchangeView {
    return {
      id: 'e',
      version: 3,
      state: 'NEGOTIATING',
      you: 'A',
      counterparty: 'CONFIRMED',
      currency: 'USD',
      timezone: 'UTC',
      display_code: 'TEST-0001',
      contributions: [],
      ...fields,
    }
  }
  const view = (id: string, sequence: number, author: 'A' | 'B', revision: RevisionTerms) => ({
    id,
    sequence,
    author,
    accepted_by: [author],
    content_hash: '',
    expires_at: '',
    terms: revision,
  })

  async function baseOf(found: ExchangeView, record?: Partial<RecordDocument>) {
    const seen: (ProposalBase | null)[] = []
    // As on a screen, the client is the same one from render to render.
    const api = { recordPart: async () => ({ ...emptyRecord, ...record }) as RecordDocument }
    function Probe() {
      seen.push(useProposalBase(api, found))
      return null
    }
    const root = createRoot(document.createElement('div'))
    await act(async () => root.render(<Probe />))
    await act(async () => new Promise((resolve) => setTimeout(resolve, 0)))
    await act(async () => root.unmount())
    return seen.at(-1)
  }
  const emptyRecord = {
    exchange: { last_event: 1 },
    revisions: [],
    events: [],
    part: { from: { revisions_after: 0, events_after: 0 }, next: null, complete: true },
  } as unknown as RecordDocument
  const signedOf = (revision: RevisionTerms) => ({
    parties: { A: revision.party_a_name, B: revision.party_b_name },
    terms: revision.terms,
    contributions: revision.contributions,
  })

  test('an amendment: the agreement in force, with where each item stands', async () => {
    const inForce = view('r1', 1, 'A', terms([bike]))
    const found = await baseOf(
      exchange({
        state: 'ACTIVE',
        in_force_revision: inForce,
        open_revision: view('r2', 2, 'B', terms([bike, pay])),
        contributions: [{ id: BIKE, status: 'CLAIMED' }],
      }),
    )
    expect(found).toMatchObject({ against: 'IN_FORCE', sequence: 1, terms: inForce.terms })
    expect(found?.statuses?.get(BIKE)).toBe('CLAIMED')
  })

  test('a counteroffer: the version it answers, read from the record', async () => {
    const found = await baseOf(exchange({ open_revision: view('r2', 2, 'B', terms([pay])) }), {
      revisions: [
        { id: 'r1', sequence: 1, signed: signedOf(terms([bike])) },
        { id: 'r2', sequence: 2, answers: { id: 'r1', sequence: 1 }, signed: signedOf(terms([pay])) },
      ] as never,
    })
    expect(found).toMatchObject({ against: 'PREVIOUS', sequence: 1 })
    expect(found?.terms.contributions.map((item) => item.id)).toEqual([BIKE])
  })

  test('nothing to compare: a first proposal, or the reader’s own', async () => {
    expect(await baseOf(exchange({ open_revision: view('r1', 1, 'B', terms([bike])) }))).toBeNull()
    expect(await baseOf(exchange({ open_revision: view('r2', 2, 'A', terms([bike])) }))).toBeNull()
  })
})
