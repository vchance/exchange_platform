// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import {
  announced,
  button,
  field,
  heading,
  press,
  settle,
  start,
  stop,
  type,
  until,
  violations,
} from './test/harness'
import { ACTIVE, DRAFT, GOOD_CODE, INVITATION, OFFER, REPAIR, ana } from './test/fake-service'

/*
 * The main screens, each in the state a person meets it in, checked with
 * axe for WCAG 2.2 A and AA and best practice, and for what axe cannot see:
 * where the focus goes, what is announced, the page's title and language.
 * Any violation fails the test, with the rule and the elements it found.
 */

afterEach(stop)

describe('the invitation page, where people arrive from a link', () => {
  test('reading the proposal before signing in', async () => {
    const { wording } = await start(`/en/i#${INVITATION}`, null)
    await until(() => document.querySelector('.terms') !== null, 'the proposal')

    expect(await violations()).toEqual([])
    expect(document.title).toBe(`${wording.invitation.title} · ${wording.productName}`)
    expect(document.documentElement.lang).toBe('en')
    expect(document.querySelectorAll('main')).toHaveLength(1)
    // The terms are headed directly under the page's heading.
    const levels = [...document.querySelectorAll('main h1, main h2, main h3')].map((h) => h.tagName)
    expect(levels[0]).toBe('H1')
    for (let i = 1; i < levels.length; i += 1) {
      expect(Number(levels[i][1]) - Number(levels[i - 1][1])).toBeLessThanOrEqual(1)
    }
  })

  test('signing in to respond, below the proposal', async () => {
    const { wording } = await start(`/en/i#${INVITATION}`, null)
    await until(() => document.querySelector('.terms') !== null, 'the proposal')
    await press(button(wording.invitation.respond))
    await until(
      () =>
        [...document.querySelectorAll('h2')].some((h) => h.textContent === wording.signIn.title),
      'sign-in',
    )

    // The step that replaced the button has the focus, so it is not lost.
    expect(document.activeElement?.textContent).toBe(wording.signIn.title)
    const identifier = field(wording.signIn.identifierLabel)
    expect(identifier.getAttribute('aria-required')).toBe('true')
    expect(identifier.getAttribute('aria-describedby')).toBeTruthy()
    expect(await violations()).toEqual([])
  })
})

describe('signing in', () => {
  test('the first step', async () => {
    const { wording } = await start('/', null)
    await heading(wording.signIn.title)
    expect(await violations()).toEqual([])
    expect(document.title).toBe(`${wording.signIn.title} · ${wording.productName}`)
  })

  test('a wrong code is tied to the field and announced', async () => {
    const { wording } = await start('/', null)
    await heading(wording.signIn.title)
    await type(field(wording.signIn.identifierLabel), ana.email!)
    await press(button(wording.signIn.sendCode))
    await until(() => document.activeElement === field(wording.signIn.codeLabel), 'the code field')

    await type(field(wording.signIn.codeLabel), '000000')
    await press(button(wording.signIn.submit))
    const refusal = wording.errors.INVALID_CODE
    await until(() => document.body.textContent!.includes(refusal), 'the refusal')
    await settle()

    const code = field(wording.signIn.codeLabel)
    expect(code.getAttribute('aria-invalid')).toBe('true')
    const describedBy = code.getAttribute('aria-describedby')!.split(' ')
    const said = describedBy.map((id) => document.getElementById(id)?.textContent)
    expect(said).toContain(refusal)
    expect(announced().assertive).toBe(refusal)
    expect(await violations()).toEqual([])

    // Then signing in for real lands on the exchanges, with the focus on their heading.
    await type(field(wording.signIn.codeLabel), GOOD_CODE)
    await press(button(wording.signIn.submit))
    await heading(wording.home.title)
    expect(document.activeElement?.tagName).toBe('H1')
    expect(await violations()).toEqual([])
  })

  test('a new account goes on to its profile, with the focus on the new heading', async () => {
    const { wording } = await start('/', null)
    await heading(wording.signIn.title)
    await type(field(wording.signIn.identifierLabel), 'ben@example.test')
    await press(button(wording.signIn.sendCode))
    await until(() => document.activeElement === field(wording.signIn.codeLabel), 'the code field')
    await type(field(wording.signIn.codeLabel), GOOD_CODE)
    await press(button(wording.signIn.submit))
    await heading(wording.profile.firstTitle)
    await settle()

    expect(document.activeElement?.textContent).toBe(wording.profile.firstTitle)
    expect(document.title).toBe(`${wording.profile.firstTitle} · ${wording.productName}`)
    expect(await violations()).toEqual([])
  })

  test('the language of the page follows the language chosen', async () => {
    const { wording } = await start('/', null)
    await heading(wording.signIn.title)
    const picker = document.querySelector<HTMLSelectElement>('header select')!
    expect(picker.labels?.[0]?.textContent).toContain(wording.nav.language)
    await type(picker, 'es')
    await until(() => document.documentElement.lang === 'es', 'Spanish')
    expect(document.title).not.toContain(wording.signIn.title)
    expect(await violations()).toEqual([])
  })
})

describe('the composer', () => {
  test('writing a first proposal', async () => {
    const { wording } = await start(`/exchanges/${DRAFT}`, ana)
    await heading(wording.composer.titleFirst)
    expect(field(wording.composer.yourName).getAttribute('aria-required')).toBe('true')
    expect(field(wording.composer.termsLabel).getAttribute('aria-required')).toBeNull()
    expect(await violations()).toEqual([])
  })

  test('what needs fixing is marked, announced and focused', async () => {
    const { wording } = await start(`/exchanges/${DRAFT}`, ana)
    const w = wording.composer
    await heading(w.titleFirst)
    const description = document.getElementById(`${REPAIR}-description`) as HTMLTextAreaElement
    await type(description, '')
    await press(button(w.review))
    await settle()

    expect(description.getAttribute('aria-invalid')).toBe('true')
    expect(document.activeElement).toBe(description)
    expect(announced().assertive).toMatch(/needs fixing/)
    expect(await violations()).toEqual([])
  })

  test('the signing step', async () => {
    const { wording } = await start(`/exchanges/${DRAFT}`, ana)
    await heading(wording.composer.titleFirst)
    await press(button(wording.composer.review))
    await heading(wording.composer.signTitle)

    expect(document.activeElement?.tagName).toBe('H1')
    const sign = button(wording.composer.signAndSend)
    expect(sign.disabled).toBe(true)
    // Why it cannot be pressed yet is part of its description.
    const why = document.getElementById(sign.getAttribute('aria-describedby')!)
    expect(why?.textContent).toBe(wording.a11y.signNeedsAgreement)
    expect(await violations()).toEqual([])

    await press(document.querySelector<HTMLInputElement>('.consent input[type=checkbox]')!)
    expect(button(wording.composer.signAndSend).disabled).toBe(false)
    expect(await violations()).toEqual([])
  })
})

describe('the exchange view', () => {
  test('a proposal waiting to be signed, and its signing panel', async () => {
    const { wording } = await start(`/exchanges/${OFFER}`, ana)
    const w = wording.exchange
    await heading(w.title.replace('{name}', 'Ana Ruiz'))
    await until(() => document.querySelector('.history') !== null, 'the history')
    expect(await violations()).toEqual([])

    const accept = button(w.accept)
    accept.focus()
    await press(accept)
    expect(accept.getAttribute('aria-expanded')).toBe('true')
    // The panel takes the focus, and gives it back when it is cancelled.
    expect(document.activeElement?.classList.contains('panel')).toBe(true)
    expect(await violations()).toEqual([])
    await press(button(wording.common.cancel))
    expect(document.activeElement).toBe(button(w.accept))
  })

  test('an agreement in force, with what has been delivered', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}`, ana)
    await heading(wording.exchange.title.replace('{name}', 'Ben Ortiz'))
    await until(() => document.querySelector('.history') !== null, 'the history')
    expect(await violations()).toEqual([])
  })
})

describe('the record', () => {
  test('laid out for reading', async () => {
    const { wording } = await start(`/exchanges/${ACTIVE}/record`, ana)
    await heading(wording.record.title.replace('{code}', 'PVVS-5Q2K'))
    expect(await violations()).toEqual([])
  })
})

describe('the other screens', () => {
  test('the list of exchanges', async () => {
    const { wording } = await start('/', ana)
    await heading(wording.home.title)
    await until(() => document.querySelector('.card') !== null, 'the list')
    expect(await violations()).toEqual([])
  })

  test('the account', async () => {
    const { wording } = await start('/account', ana)
    await heading(wording.profile.title)
    expect(await violations()).toEqual([])

    // A name left empty is announced through the field it belongs to.
    await type(field(wording.profile.nameLabel), '')
    await press(button(wording.profile.save))
    await settle()
    const name = field(wording.profile.nameLabel)
    expect(document.activeElement).toBe(name)
    expect(name.getAttribute('aria-invalid')).toBe('true')
    expect(await violations()).toEqual([])
  })
})
