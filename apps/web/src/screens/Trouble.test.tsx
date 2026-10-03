// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import { ACTIVE, AMENDING, DISPUTED, ana } from '../test/fake-service'
import { button, heading, press, start, stop, until } from '../test/harness'

afterEach(stop)

/** The guide's panel. */
function guide(label: string): HTMLElement {
  const found = [...document.querySelectorAll<HTMLElement>('.panel')].find(
    (panel) => panel.querySelector('.label')?.textContent === label,
  )
  if (!found) throw new Error('no guide')
  return found
}

describe('“Something isn’t working”', () => {
  test('asks what the situation is, then offers the ways that fit and what each releases', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const t = wording.trouble
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))

    const open = button(t.open)
    await press(open)
    expect(open.getAttribute('aria-expanded')).toBe('true')
    const panel = guide(t.open)
    expect(document.activeElement).toBe(panel)
    const choices = [...panel.querySelectorAll('.trouble-choices button')].map((b) => b.textContent)
    expect(choices).toEqual([
      'Ben Ortiz hasn’t done their part',
      t.situations.CANT_DO_MINE,
      t.situations.BOTH_STOP,
      t.situations.DISAGREE,
    ])

    await press(button('Ben Ortiz hasn’t done their part'))
    expect(document.activeElement?.textContent).toBe(t.explain.THEY_HAVENT)
    const text = guide(t.open).textContent!
    // The payment Ben owes can be waived; ending together or alone are offered too.
    expect(text).toContain('Waiving releases Ben Ortiz from that one item, for good.')
    expect(text).toContain('If Ben Ortiz agrees, the agreement ends')
    expect(text).toContain('Nobody is released. Ben Ortiz gets time to respond')
    expect(guide(t.open).querySelector('.written')?.textContent).toBe('Payment for the repair')
  })

  test('only opens the existing action: nothing is sent from the guide', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const t = wording.trouble
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await press(button(t.open))
    await press(button(t.situations.BOTH_STOP))
    expect(guide(t.open).textContent).toContain(t.explain.BOTH_STOP)

    await press(button(wording.exchange.proposeEnd))
    // The guide gives way to the ending's own panel, which asks to confirm.
    expect(document.querySelector('.panel .label')?.textContent).toBe(wording.exchange.proposeEnd)
    expect(document.activeElement?.classList.contains('panel')).toBe(true)
    expect(button(wording.exchange.sendEndProposal)).toBeTruthy()
  })

  test('an item’s action opens that item’s own panel', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const t = wording.trouble
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await press(button(t.open))
    await press(button('Ben Ortiz hasn’t done their part'))
    const waive = [...guide(t.open).querySelectorAll('button')].find(
      (candidate) => candidate.textContent === wording.exchange.moneyMoves.WAIVE,
    )!
    // Which item it is about is part of what a screen reader says.
    const about = document.getElementById(waive.getAttribute('aria-describedby')!)
    expect(about?.textContent).toBe('Payment for the repair')
    await press(waive)
    const item = document.querySelector('.panel')!.closest('li.contribution')
    expect(item?.textContent).toContain('Payment for the repair')
    expect(item?.querySelector('.panel .label')?.textContent).toBe(wording.exchange.moneyMoves.WAIVE)
  })

  test('back to the question, and cancelled', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    const t = wording.trouble
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await press(button(t.open))
    await press(button(t.situations.CANT_DO_MINE))
    expect(guide(t.open).textContent).toContain(t.means.AMEND.replace('{name}', 'Ben Ortiz'))
    await press(button(t.change))
    expect(guide(t.open).textContent).toContain(t.question)
    await press(button(wording.common.cancel))
    expect(document.querySelector('.panel')).toBeNull()
    expect(document.activeElement).toBe(button(t.open))
  })
})

describe('a dispute: recorded, not decided', () => {
  test('a disputed item says so, and points to the guide', async () => {
    const { wording } = await start(`/exchanges/${DISPUTED}`, ana)
    await heading(wording.exchange.title.replace('{name}', 'Ana Ruiz'))
    await until(() => document.querySelector('.dispute-note') !== null, 'the dispute note')
    const note = document.querySelector('.dispute-note')!
    expect(note.textContent).toContain(wording.dispute.weRecord)
    expect(note.textContent).toContain(wording.dispute.pointer)

    await press(note.querySelector('button')!)
    // The guide opens at the disagreement.
    expect(document.body.textContent).toContain(wording.trouble.explain.DISAGREE)
  })

  test('opening a dispute says it too', async () => {
    const { wording } = await start(`/exchanges/${AMENDING}`, ana)
    await heading(wording.exchange.title.replace('{name}', 'Ana Ruiz'))
    // Ben was told the repair is done; he disagrees.
    await press(button(wording.exchange.moves.DISPUTE))
    const panel = document.querySelector('.panel')!
    expect(panel.querySelector('.label')?.textContent).toBe(wording.exchange.moves.DISPUTE)
    expect(panel.textContent).toContain(wording.dispute.weRecord)
    expect(panel.textContent).toContain(wording.dispute.pointer)
  })

  test('with nothing in doubt, the guide says so', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await press(button(wording.trouble.open))
    await press(button(wording.trouble.situations.DISAGREE))
    expect(document.body.textContent).toContain(wording.trouble.nothingInDoubt)
  })
})
