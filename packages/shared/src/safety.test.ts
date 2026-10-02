import { expect, test } from 'vitest'

import { checkReport, hasOtherParty, REPORT_REASONS, reportNeedsDetails } from './safety'

test('a report is never sent with a reason nobody picked', () => {
  expect(checkReport(null, '')).toEqual({ ok: false, reasonMissing: true, detailsMissing: false })
  // Writing something does not stand in for choosing.
  expect(checkReport(null, 'They keep sending this.')).toEqual({
    ok: false,
    reasonMissing: true,
    detailsMissing: false,
  })
})

test('every reason but "something else" can be sent with nothing written', () => {
  for (const reason of REPORT_REASONS) {
    if (reason === 'OTHER') continue
    expect(reportNeedsDetails(reason)).toBe(false)
    expect(checkReport(reason, '')).toEqual({ ok: true, reason, details: null })
    // Only spaces is nothing written.
    expect(checkReport(reason, '  \n ')).toEqual({ ok: true, reason, details: null })
  }
})

test('"something else" has to say what', () => {
  expect(reportNeedsDetails('OTHER')).toBe(true)
  expect(checkReport('OTHER', '   ')).toEqual({
    ok: false,
    reasonMissing: false,
    detailsMissing: true,
  })
  expect(checkReport('OTHER', 'It is my old address.')).toEqual({
    ok: true,
    reason: 'OTHER',
    details: 'It is my old address.',
  })
})

test('what was written is sent as written, without the space around it', () => {
  expect(checkReport('SCAM', '  Asked for a deposit,\nthen vanished. ')).toEqual({
    ok: true,
    reason: 'SCAM',
    details: 'Asked for a deposit,\nthen vanished.',
  })
})

test('there is someone to report or block once someone has joined', () => {
  expect(hasOtherParty({ state: 'DRAFT', counterparty: 'UNCLAIMED' })).toBe(false)
  expect(hasOtherParty({ state: 'NEGOTIATING', counterparty: 'UNCLAIMED' })).toBe(false)
  expect(hasOtherParty({ state: 'NEGOTIATING', counterparty: 'CLAIMED' })).toBe(true)
  expect(hasOtherParty({ state: 'ACTIVE', counterparty: 'CONFIRMED' })).toBe(true)
  // A closed exchange can still be reported, and its other party blocked.
  expect(hasOtherParty({ state: 'CLOSED', counterparty: 'CONFIRMED' })).toBe(true)
  // One that closed before anyone joined has nobody on the other side.
  expect(hasOtherParty({ state: 'CLOSED', counterparty: 'UNCLAIMED' })).toBe(false)
})
