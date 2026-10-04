// @vitest-environment jsdom
import { afterEach, describe, expect, test } from 'vitest'

import { button, field, press, settle, start, stop, type, until } from '../test/harness'

/*
 * Signing in asks for what the service can send a code to (`GET /v1/meta`,
 * `sign_in_channels`): an email address only where it cannot send text
 * messages, and a phone number typed anyway is stopped before it is sent.
 */

afterEach(stop)

function hasLabel(text: string): boolean {
  return [...document.querySelectorAll('label')].some((label) => label.textContent?.trim() === text)
}

describe('signing in where the service has no text messages', () => {
  test('asks for an email address, and only that', async () => {
    const { wording, service } = await start('/', null, 'en', (fake) => {
      fake.phone = false
    })
    const w = wording.signIn
    await until(() => service.sent.some((request) => request.call === 'GET /v1/meta'), 'meta')
    await settle()

    expect(document.body.textContent).toContain(w.introEmail)
    expect(document.body.textContent).not.toContain(w.intro)
    expect(hasLabel(w.identifierLabel)).toBe(false)
    const email = field(w.emailLabel) as HTMLInputElement
    expect(email.type).toBe('email')
    expect(email.inputMode).toBe('email')
    // Nothing about phone numbers or their countries.
    expect(document.body.textContent).not.toMatch(/\+1/)
  })

  test('a phone number typed anyway is stopped here, with what to do instead', async () => {
    const { wording, service } = await start('/', null, 'en', (fake) => {
      fake.phone = false
    })
    const w = wording.signIn
    await until(() => service.sent.some((request) => request.call === 'GET /v1/meta'), 'meta')
    await settle()

    const email = field(w.emailLabel)
    await type(email, '+1 555 123 4567')
    await press(button(w.sendCode))
    await until(() => document.body.textContent!.includes(w.emailOnly), 'the email-only message')
    expect(document.body.textContent).not.toContain(wording.errors.SERVICE_UNAVAILABLE)
    expect(email.getAttribute('aria-invalid')).toBe('true')
    const note = [...document.querySelectorAll('.notice-error')].find(
      (found) => found.textContent === w.emailOnly,
    )
    expect(email.getAttribute('aria-describedby')).toContain(note!.id)
    expect(service.sent.some((request) => request.call === 'POST /v1/auth/codes')).toBe(false)

    // Typing again clears it; an email address goes through.
    await type(email, 'ben@example.test')
    expect(document.body.textContent).not.toContain(w.emailOnly)
    await press(button(w.sendCode))
    await until(() => hasLabel(w.codeLabel), 'the code field')
    expect(service.sent.at(-1)).toEqual({
      call: 'POST /v1/auth/codes',
      body: { identifier: 'ben@example.test' },
    })
    // Going back offers another email address, not a phone number.
    button(w.changeEmail)
    expect(document.body.textContent).not.toContain(w.changeIdentifier)
  })
})

describe('signing in where the service sends text messages', () => {
  test('a phone number is asked for too, with the countries served, and sent', async () => {
    const { wording, service } = await start('/', null)
    const w = wording.signIn
    await until(() => hasLabel(w.identifierLabel), 'the email or phone field')

    expect(document.body.textContent).toContain(w.intro)
    expect(document.body.textContent).toContain(
      w.identifierHintCountries.replace('{codes}', '+1'),
    )
    const identifier = field(w.identifierLabel) as HTMLInputElement
    // An email field would refuse the `+` and the spaces of a phone number.
    expect(identifier.type).toBe('text')

    await type(identifier, '+15551234567')
    await press(button(w.sendCode))
    await until(() => hasLabel(w.codeLabel), 'the code field')
    expect(service.sent.at(-1)).toEqual({
      call: 'POST /v1/auth/codes',
      body: { identifier: '+15551234567' },
    })
    button(w.changeIdentifier)
  })
})
