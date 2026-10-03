// @vitest-environment jsdom
import { createI18n, wordingFor, type BuildIdentity } from '@yuppers/shared'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, expect, test, vi } from 'vitest'

import { I18nContext } from '../app/context'
import { BuildVersion } from './BuildVersion'

/*
 * The build's version line at the foot of the account and staff screens:
 * the version, and the commit when the build said which.
 */

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const current = vi.hoisted(() => ({ build: { version: '0.1.0', commit: null } as BuildIdentity }))

vi.mock('../lib/api', () => ({
  get WEB_BUILD() {
    return current.build
  },
}))

let root: Root | null = null

afterEach(async () => {
  const mounted = root
  root = null
  if (mounted) await act(async () => mounted.unmount())
})

async function show(build: BuildIdentity, language: 'en' | 'es' = 'en'): Promise<string> {
  current.build = build
  document.body.innerHTML = '<main id="root"></main>'
  const i18n = createI18n(language, wordingFor(language), () => {})
  await act(async () => {
    root = createRoot(document.getElementById('root')!)
    root.render(
      <I18nContext.Provider value={i18n}>
        <BuildVersion />
      </I18nContext.Provider>,
    )
  })
  return document.querySelector('.build-version')?.textContent ?? ''
}

test('names the version and the commit the build was made from', async () => {
  expect(await show({ version: '0.1.0', commit: '0123456789abcdef0123456789abcdef01234567' })).toBe(
    'Version 0.1.0 (0123456)',
  )
  expect(await show({ version: '0.1.0', commit: 'abcdef1' }, 'es')).toBe('Versión 0.1.0 (abcdef1)')
})

test('names the version alone when the build did not say its commit', async () => {
  expect(await show({ version: '0.1.0', commit: null })).toBe('Version 0.1.0')
  expect(await show({ version: '0.1.0', commit: 'unknown' })).toBe('Version 0.1.0')
})
