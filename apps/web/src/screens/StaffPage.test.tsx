// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import { REPORT, RESOLVED_REPORT, RTL_NAME, rita, staleRita } from '../test/fake-service'
import { button, field, heading, press, settle, start, stop, type, until } from '../test/harness'

/*
 * The staff review screen against the stand-in service: what it says when the
 * reviewer must sign in again, when a report was resolved meanwhile, and
 * which decisions need a note.
 */

afterEach(stop)

/** The requests the screen made, as `METHOD /path`, with their bodies. */
function sent(service: { fetch: typeof fetch }) {
  const calls: { call: string; body: unknown }[] = []
  const original = service.fetch
  service.fetch = (async (input: RequestInfo | URL, init?: RequestInit) => {
    const request = input instanceof Request ? input : new Request(input, init)
    const text = await request.clone().text()
    calls.push({
      call: `${request.method} ${new URL(request.url).pathname}`,
      body: text ? JSON.parse(text) : null,
    })
    return original(request)
  }) as typeof fetch
  return calls
}

/**
 * The pieces of text on the page holding `text` that sit in no element that
 * isolates their direction (a `<bdi>`, or one with `dir="auto"`), each as
 * the HTML of its parent; none, when `text` is shown and always isolated.
 * Then nothing in what someone wrote can turn the screen's own words around.
 */
function unisolated(text: string): string[] {
  const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT)
  const found: string[] = []
  let shown = false
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.textContent!.includes(text)) continue
    shown = true
    if (!node.parentElement?.closest('bdi, [dir="auto"]')) found.push(node.parentElement!.outerHTML)
  }
  return shown ? found : [`${text} is not shown`]
}

describe('the staff review screen', () => {
  test('lists the queue oldest first, the overdue one marked', async () => {
    const { wording } = await start('/staff', rita)
    const w = wording.staff
    await heading(w.title)
    await until(() => document.querySelectorAll('li.card').length >= 2, 'the queue')
    const entries = [...document.querySelectorAll('section[aria-labelledby="staff-queue"] li')]
    expect(entries).toHaveLength(2)
    expect(entries[0].classList.contains('card-overdue')).toBe(true)
    expect(entries[0].textContent).toContain(w.overdue)
    expect(entries[0].textContent).toContain('30')
    expect(entries[1].classList.contains('card-overdue')).toBe(false)
    expect(entries[1].textContent).not.toContain(w.overdue)
    expect(entries[1].querySelector('a')?.getAttribute('href')).toBe(`/staff/reports/${REPORT}`)
  })

  test('dismissing needs no note, and goes back to the queue saying so', async () => {
    const { wording, service } = await start(`/staff/reports/${REPORT}`, rita)
    const calls = sent(service)
    const w = wording.staff
    await heading(w.detailTitle.replace('{code}', 'PVVS-5Q2K'))
    expect(document.querySelector('textarea')).toBeNull()
    await press(button(w.outcomes.DISMISSED))
    expect(field(w.noteOptionalLabel).getAttribute('aria-required')).toBeNull()
    await press(button(w.confirm))
    await heading(w.title)
    expect(calls.find((call) => call.call === `POST /v1/staff/reports/${REPORT}/resolution`)?.body).toEqual({
      outcome: 'DISMISSED',
      note: null,
    })
    await until(
      () => document.body.textContent!.includes(w.outcomeDone.DISMISSED),
      'what was done',
    )
  })

  test('hiding content needs a note, which is sent with it', async () => {
    const { wording, service } = await start(`/staff/reports/${REPORT}`, rita)
    const calls = sent(service)
    const w = wording.staff
    await heading(w.detailTitle.replace('{code}', 'PVVS-5Q2K'))
    await press(button(w.outcomes.CONTENT_HIDDEN))
    expect(document.body.textContent).toContain(w.outcomeText.CONTENT_HIDDEN)
    await press(button(w.confirm))
    await settle()
    expect(document.body.textContent).toContain(w.noteRequired)
    expect(calls.some((call) => call.call.startsWith('POST'))).toBe(false)

    await type(field(w.noteLabel), '  His address is in the terms.  ')
    await press(button(w.confirm))
    await heading(w.title)
    expect(calls.find((call) => call.call.startsWith('POST'))?.body).toEqual({
      outcome: 'CONTENT_HIDDEN',
      note: 'His address is in the terms.',
    })
  })

  test('a reviewer whose sign-in is too old is asked to sign in again', async () => {
    const { wording } = await start('/staff', staleRita)
    await heading(wording.staff.title)
    await until(
      () => document.body.textContent!.includes(wording.errors.SESSION_TOO_OLD),
      'the refusal',
    )
    await press(button(wording.nav.signOut))
    await heading(wording.signIn.title)
  })

  test('a report resolved meanwhile says so, with the way back', async () => {
    const { wording } = await start(`/staff/reports/${RESOLVED_REPORT}`, rita)
    await until(
      () => document.body.textContent!.includes(wording.errors.REPORT_RESOLVED),
      'the refusal',
    )
    const back = [...document.querySelectorAll('a')].find(
      (link) => link.textContent === wording.staff.back,
    )
    expect(back?.getAttribute('href')).toBe('/staff')
  })

  test('names and what people wrote are isolated, so they cannot turn the words around them', async () => {
    const { wording } = await start('/staff', rita)
    await heading(wording.staff.title)
    const rtl = 'مريم'
    await until(() => document.body.textContent!.includes(rtl), 'the hidden content')
    expect(unisolated(rtl)).toEqual([])
    // The character that turns text around is taken out of the name too.
    expect(document.body.textContent).not.toContain('\u202E')
    expect(RTL_NAME).toContain('\u202E')
    // The suspended account's name and the note about it.
    expect(unisolated('Ben Ortiz')).toEqual([])
    expect(unisolated('Threats in the notes.')).toEqual([])
  })

  test('on a report, every name and everything written is isolated', async () => {
    const { wording } = await start(`/staff/reports/${REPORT}`, rita)
    await heading(wording.staff.detailTitle.replace('{code}', 'PVVS-5Q2K'))
    await until(() => document.body.textContent!.includes('Ana Ruiz'), 'the report')
    // The people, as the report and the record name them, and what the
    // reporter and the parties wrote.
    for (const written of ['Ana Ruiz', 'Ben Ortiz', 'She threatened me in a note.']) {
      expect(unisolated(written)).toEqual([])
    }
  })
})
