// @vitest-environment jsdom
import { afterEach, describe, expect, test, vi } from 'vitest'

import { ACTIVE, ENDED, ana } from '../test/fake-service'
import { button, heading, press, start, stop, until } from '../test/harness'

afterEach(stop)

/** The plain summary, as a reader meets it. */
function summary(): HTMLElement {
  const found = document.querySelector<HTMLElement>('.record-plain')
  if (!found) throw new Error('no summary')
  return found
}

describe('the plain summary at the top of the record', () => {
  test('comes before the full detail, which is still all there', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}/record`, ana)
    await heading(wording.record.title.replace('{code}', 'PVVS-5Q2K'))
    await until(() => document.querySelector('.record-plain') !== null, 'the summary')

    const sections = [...document.querySelectorAll('main section h2')].map((h) => h.textContent)
    expect(sections[0]).toBe(wording.record.summary.heading)
    expect(sections).toContain(wording.record.summaryHeading)
    expect(sections).toContain(wording.record.eventsHeading)

    const text = summary().textContent!
    expect(text).toContain('Between Ana Ruiz and Ben Ortiz.')
    expect(text).toContain('What Ana Ruiz agreed to give')
    expect(text).toContain('Ana Ruiz marked it delivered. Ben Ortiz has not confirmed it.')
    expect(text).toContain('Not paid yet.')
    expect(text).toContain('Amount: $450.00')
    expect(text).toContain(wording.record.summary.standing.ACTIVE)
    // The items are the parties' own words, marked as such.
    expect(summary().querySelector('.written')?.textContent).toBe('Repair the back fence')
  })

  test('says how an exchange ended, and what that released', async () => {
    const { wording } = await start(`/exchanges/${ENDED}/record`, ana)
    await until(() => document.querySelector('.record-plain') !== null, 'the summary')
    const text = summary().textContent!
    expect(text).toContain('It ended by agreement on October 24, 2026.')
    expect(text).toContain('Ben Ortiz proposed ending it and Ana Ruiz agreed.')
    expect(text).toContain('Delivered, and Ben Ortiz confirmed receiving it.')
    expect(text).toContain(wording.record.summary.moneyOutcome.WAIVED_BY_ENDING)
  })

  test('one action saves it as a PDF, through the browser’s print window', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}/record`, ana)
    await until(() => document.querySelector('.record-plain') !== null, 'the summary')
    const print = vi.fn()
    window.print = print
    await press(button(wording.record.summary.savePdf))
    expect(print).toHaveBeenCalledOnce()
    // The old wording is not offered beside it.
    expect(() => button(wording.record.print)).toThrow()
    expect(document.body.textContent).toContain(wording.record.summary.savePdfHint)
    // What is not wanted on paper is marked so.
    expect(document.querySelector('.record-page .actions')?.classList.contains('no-print')).toBe(true)
  })
})
