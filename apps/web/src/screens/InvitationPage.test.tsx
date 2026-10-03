// @vitest-environment jsdom
import { act } from 'react'
import { afterEach, describe, expect, test } from 'vitest'

import { INVITATION, OTHER_INVITATION, OTHER_INVITATION_CODE } from '../test/fake-service'
import { start, stop, until } from '../test/harness'

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

describe('the invitation page in a tab that already shows one', () => {
  test('a second link pasted in shows its own proposal and leaves the address bar', async () => {
    await start(`/en/i#${INVITATION}`, null)
    await until(() => shown('OFFR-7Y2M'), 'the first proposal')
    expect(window.location.hash).toBe('')

    await paste(`/en/i#${OTHER_INVITATION}`)
    await until(() => shown(OTHER_INVITATION_CODE), 'the second proposal')
    expect(shown('OFFR-7Y2M')).toBe(false)
    expect(window.location.hash).toBe('')
    expect(window.location.pathname).toBe('/en/i')
    // A reload while signing in finds the second, not the first.
    expect(window.sessionStorage.getItem('exchange.invitation')).toBe(OTHER_INVITATION)
  })

  test('a fragment that is not an invitation leaves the page as it is', async () => {
    await start(`/en/i#${INVITATION}`, null)
    await until(() => shown('OFFR-7Y2M'), 'the proposal')

    await paste('/en/i#top')
    expect(shown('OFFR-7Y2M')).toBe(true)
    expect(window.sessionStorage.getItem('exchange.invitation')).toBe(INVITATION)
  })
})
