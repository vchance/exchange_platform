// @vitest-environment jsdom
import { act } from 'react'
import { afterEach, describe, expect, test } from 'vitest'

import {
  GOOD_CODE,
  INVITATION,
  OFFER,
  OTHER_INVITATION,
  OTHER_INVITATION_CODE,
  ana,
} from '../test/fake-service'
import { button, field, heading, press, start, stop, type, until } from '../test/harness'

afterEach(stop)

const shown = (code: string) => document.body.textContent?.includes(code) ?? false

/** Pastes an invitation link into the tab's address bar: only the fragment changes. */
async function paste(link: string): Promise<void> {
  await act(async () => {
    const before = window.location.href
    window.history.replaceState(window.history.state, '', link)
    window.dispatchEvent(
      new HashChangeEvent('hashchange', { oldURL: before, newURL: window.location.href }),
    )
  })
}

describe('the invitation page signed out', () => {
  test('is the same for every link, and asks the service nothing about any', async () => {
    const pages: string[] = []
    for (const token of [INVITATION, OTHER_INVITATION, 'c5'.repeat(32)]) {
      const { service, wording } = await start(`/en/i#${token}`, null)
      await heading(wording.invitation.signedOutTitle)
      expect(shown(wording.invitation.signInToRead)).toBe(true)
      expect(field(wording.signIn.identifierLabel)).toBeTruthy()
      // Nothing of the proposal, and no way to report it before signing in.
      expect(document.querySelector('.terms')).toBeNull()
      expect(shown(wording.safety.reportProposal)).toBe(false)
      expect(service.sent.some(({ call }) => call.includes('/invitations/'))).toBe(false)
      expect(JSON.stringify(service.sent)).not.toContain(token)
      // The token has left the address bar and waits in this tab.
      expect(window.location.hash).toBe('')
      expect(window.sessionStorage.getItem('yuppers.invitation')).toBe(token)
      // React numbers the ids it makes across the whole run; they say nothing of the link.
      pages.push(document.querySelector('main')!.innerHTML.replace(/_r_[0-9a-z]+_/g, '_r_'))
    }
    expect(new Set(pages).size).toBe(1)
  })

  test('signing in shows the proposal, and responding asks only for the profile', async () => {
    const { service, wording } = await start(`/en/i#${INVITATION}`, null)
    await heading(wording.invitation.signedOutTitle)

    await type(field(wording.signIn.identifierLabel), 'ben@example.test')
    await press(button(wording.signIn.sendCode))
    await until(() => document.activeElement === field(wording.signIn.codeLabel), 'the code field')
    await type(field(wording.signIn.codeLabel), GOOD_CODE)
    await press(button(wording.signIn.submit))

    await heading(wording.invitation.title)
    await until(() => shown('OFFR-7Y2M'), 'the proposal')
    expect(document.activeElement?.tagName).toBe('H1')
    expect(service.sent.filter(({ call }) => call === 'POST /v1/invitations/preview')).toEqual([
      { call: 'POST /v1/invitations/preview', body: { token: INVITATION } },
    ])
    await until(() => shown(wording.safety.reportProposal), 'the way to report it')
    // Signed in, the sign-in form is gone and is not asked for again.
    expect(document.querySelector('input[autocomplete="username"]')).toBeNull()

    await press(button(wording.invitation.respondNew))
    await until(
      () =>
        [...document.querySelectorAll('h2')].some(
          (h) => h.textContent === wording.profile.firstTitle,
        ),
      'the profile',
    )
    expect(document.querySelector('input[autocomplete="username"]')).toBeNull()
    await type(field(wording.profile.nameLabel), 'Ben')
    await press(field(wording.profile.adultLabel) as HTMLInputElement)
    await press(button(wording.profile.continue))

    await until(() => window.location.pathname === `/exchanges/${OFFER}`, 'the exchange')
    expect(service.sent.filter(({ call }) => call === 'POST /v1/invitations/claim')).toEqual([
      { call: 'POST /v1/invitations/claim', body: { token: INVITATION } },
    ])
    expect(service.sent.filter(({ call }) => call === 'POST /v1/auth/codes')).toHaveLength(1)
    expect(window.sessionStorage.getItem('yuppers.invitation')).toBeNull()
  })

  test('an account that is already set up responds in its own name', async () => {
    const { wording } = await start(`/en/i#${INVITATION}`, ana)
    await until(() => shown('OFFR-7Y2M'), 'the proposal')
    await press(button(wording.invitation.respondAs.replace('{name}', ana.display_name)))
    await until(() => window.location.pathname === `/exchanges/${OFFER}`, 'the exchange')
  })
})

describe('the invitation page in a tab that already shows one', () => {
  test('a second link pasted in shows its own proposal and leaves the address bar', async () => {
    await start(`/en/i#${INVITATION}`, ana)
    await until(() => shown('OFFR-7Y2M'), 'the first proposal')
    expect(window.location.hash).toBe('')

    await paste(`/en/i#${OTHER_INVITATION}`)
    await until(() => shown(OTHER_INVITATION_CODE), 'the second proposal')
    expect(shown('OFFR-7Y2M')).toBe(false)
    expect(window.location.hash).toBe('')
    expect(window.location.pathname).toBe('/en/i')
    // A reload finds the second, not the first.
    expect(window.sessionStorage.getItem('yuppers.invitation')).toBe(OTHER_INVITATION)
  })

  test('a fragment that is not an invitation leaves the page as it is', async () => {
    await start(`/en/i#${INVITATION}`, ana)
    await until(() => shown('OFFR-7Y2M'), 'the proposal')

    await paste('/en/i#top')
    expect(shown('OFFR-7Y2M')).toBe(true)
    expect(window.sessionStorage.getItem('yuppers.invitation')).toBe(INVITATION)
  })
})
