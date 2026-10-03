// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import { ACTIVE, OFFER, ana } from '../test/fake-service'
import { button, heading, press, start, stop, until, violations } from '../test/harness'

afterEach(stop)

const fill = (message: string, values: Record<string, string>) =>
  message.replace(/\{(\w+)\}/g, (_, key: string) => values[key])
const other = { name: 'Ben Ortiz' }

/** The report-and-block section. */
function section(): HTMLElement {
  const found = document.getElementById('safety-heading')?.closest('section')
  if (!found) throw new Error('no report-and-block section')
  return found
}

function within(scope: HTMLElement, text: string): HTMLButtonElement {
  const found = [...scope.querySelectorAll('button')].find(
    (candidate) => candidate.textContent?.trim() === text,
  )
  if (!found) throw new Error(`no button “${text}” here`)
  return found
}

describe('blocking from an agreement in force', () => {
  test('says what the block leaves standing, before it is made', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const w = wording.safety
    await heading(fill(wording.exchange.title, other))
    await until(() => section().textContent!.includes(fill(w.block, other)), 'the block button')

    await press(button(fill(w.block, other)))
    const said = section().querySelector('.panel')!.textContent!
    const order = [
      fill(w.blockStops, other),
      w.blockEnds,
      w.blockKeeps,
      fill(w.blockInForce, other),
      fill(w.blockThenClose, other),
      fill(w.blockQuiet, other),
    ].map((sentence) => said.indexOf(sentence))
    expect(order.every((index) => index >= 0)).toBe(true)
    expect(order).toEqual([...order].sort((a, b) => a - b))
    // Nothing about closing is offered until the block is made.
    expect(section().textContent).not.toContain(fill(w.blockedInForce, other))
  })

  test('once made, offers to close without agreement right there, through the usual request', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const w = wording.safety
    await heading(fill(wording.exchange.title, other))
    await until(() => section().textContent!.includes(fill(w.block, other)), 'the block button')
    await press(button(fill(w.block, other)))
    await press(button(fill(w.confirmBlock, other)))
    await until(
      () => section().textContent!.includes(fill(w.blockedInForce, other)),
      'the offer to close',
    )

    const offer = within(section(), wording.exchange.requestClose)
    expect(offer.getAttribute('aria-expanded')).toBe('false')
    await press(offer)
    expect(offer.getAttribute('aria-expanded')).toBe('true')
    // The request to close opens here, under the button, and takes the focus.
    const panel = section().querySelector<HTMLElement>('.panel')!
    expect(panel.querySelector('.label')?.textContent).toBe(wording.exchange.requestClose)
    expect(panel.textContent).toContain(fill(wording.exchange.requestCloseText, other))
    expect(document.activeElement).toBe(panel)
    expect(within(panel, wording.exchange.sendCloseRequest)).toBeTruthy()
    // It is the one request to close on the page: the ending's own stays shut.
    expect(document.querySelectorAll('.panel')).toHaveLength(1)
    expect(await violations()).toEqual([])

    await press(within(panel, wording.common.cancel))
    expect(section().querySelector('.panel')).toBeNull()
    expect(document.activeElement).toBe(offer)
  })
})

test('blocking from a proposal says nothing of an agreement in force', async () => {
  const { wording } = await start(`/exchanges/${OFFER}`, ana)
  const w = wording.safety
  await until(() => document.getElementById('safety-heading') !== null, 'the safety section')
  await until(() => section().textContent!.includes(fill(w.block, other)), 'the block button')
  await press(button(fill(w.block, other)))
  const said = section().querySelector('.panel')!.textContent!
  expect(said).toContain(w.blockEnds)
  expect(said).not.toContain(fill(w.blockInForce, other))
  expect(said).not.toContain(fill(w.blockThenClose, other))
})
