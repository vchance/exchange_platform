// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import { ACTIVE, AMENDING, COUNTER, OFFER, ana } from '../test/fake-service'
import { button, heading, start, stop, until } from '../test/harness'

afterEach(stop)

const changes = () => document.querySelector<HTMLElement>('.proposal-changes')

/** Whether `first` comes before `second` in the page. */
const before = (first: Element, second: Element) =>
  (first.compareDocumentPosition(second) & Node.DOCUMENT_POSITION_FOLLOWING) !== 0

describe('what a proposal changes, for the person asked to sign it', () => {
  test('a counteroffer, against the version it answers, above the way to sign', async () => {
    const { wording } = await start(`/exchanges/${COUNTER}`, ana)
    const w = wording.proposalChanges
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await until(() => changes() !== null, 'what changes')

    const shown = changes()!
    expect(shown.querySelector('h3')?.textContent).toBe(w.heading)
    expect(shown.textContent).toContain('Compared with version 1, the one this answers.')
    const items = [...shown.querySelectorAll('li.contribution')]
    expect(items.map((item) => item.querySelector('.tag')?.textContent)).toEqual([
      w.kinds.CHANGED,
      w.kinds.ADDED,
    ])
    expect(items[0].textContent).toContain('Amount: was $450.00, now $500.00.')
    expect(items[1].querySelector('.written')?.textContent).toBe('Paint the gate')
    expect(shown.textContent).toContain('1 other item is unchanged.')
    expect(before(shown, button(wording.exchange.accept))).toBe(true)
  })

  test('an amendment, against the agreement in force, with where each item would stand', async () => {
    const { wording } = await start(`/exchanges/${AMENDING}`, ana)
    const w = wording.proposalChanges
    await heading(wording.exchange.title.replace('{name}', 'Ana Ruiz'))
    await until(() => changes() !== null, 'what changes')

    const shown = changes()!
    expect(shown.textContent).toContain('Compared with the agreement in force, version 1.')
    const items = [...shown.querySelectorAll('li.contribution')]
    expect(items).toHaveLength(2)
    // The repair was marked delivered; changing it starts it again.
    const repair = items[0]
    expect(repair.querySelector('.tag')?.textContent).toBe(w.kinds.CHANGED)
    expect(repair.querySelector('.change .label')?.textContent).toBe(w.fields.description)
    const written = [...repair.querySelectorAll('.change .written-inline')].map((n) => n.textContent)
    expect(written).toEqual(['Repair the back fence', 'Repair the back fence and the gate'])
    expect(repair.textContent).toContain(wording.composer.effects.CHANGED)
    expect(repair.textContent).toContain(`Once you both sign: ${wording.contributionStatus.PENDING}`)
    expect(items[1].textContent).toContain(`Once you both sign: ${wording.moneyStatus.PENDING}`)
    expect(before(shown, button(wording.exchange.accept))).toBe(true)
  })

  test('not shown to its author, nor on a first proposal, nor with nothing waiting', async () => {
    const { wording } = await start(`/exchanges/${OFFER}`, ana)
    await until(() => document.querySelector('.history') !== null, 'the history')
    expect(changes()).toBeNull()
    expect(button(wording.exchange.accept)).toBeTruthy()

    await start(`/exchanges/${ACTIVE}`, ana)
    await until(() => document.querySelector('.history') !== null, 'the history')
    expect(changes()).toBeNull()
  })
})
