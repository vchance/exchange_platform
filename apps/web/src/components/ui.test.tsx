// @vitest-environment jsdom
import { createI18n, wordingFor } from '@exchange/shared'
import { act } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, expect, test } from 'vitest'

import { I18nContext } from '../app/context'
import { PageHeading } from './ui'

/*
 * A heading with a name the other party wrote in it. The name may hold
 * characters that turn text around, or a line break: in the page it is
 * isolated in `<bdi>`, and in the tab's title, which cannot isolate
 * anything, those characters are taken out.
 */

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

const wording = wordingFor('en')
const i18n = createI18n('en', wording, () => {})
let root: Root | null = null

afterEach(async () => {
  const mounted = root
  root = null
  if (mounted) await act(async () => mounted.unmount())
})

async function show(element: React.ReactNode) {
  document.body.innerHTML = '<div id="root"></div>'
  document.title = ''
  await act(async () => {
    root = createRoot(document.getElementById('root')!)
    root.render(<I18nContext.Provider value={i18n}>{element}</I18nContext.Provider>)
  })
  return document.querySelector('h1')!
}

test('a name in the heading is isolated, and kept out of the title’s direction', async () => {
  const name = 'Sam\u202E\n gnp.exe'
  const h1 = await show(<PageHeading name={name}>{wording.exchange.title}</PageHeading>)

  const isolated = h1.querySelectorAll('bdi')
  expect(isolated).toHaveLength(1)
  expect(isolated[0].textContent).toBe('Sam gnp.exe')
  expect(h1.textContent).toBe(wording.exchange.title.replace('{name}', 'Sam gnp.exe'))
  expect(document.title).toBe(
    `${wording.exchange.title.replace('{name}', 'Sam gnp.exe')} · ${wording.productName}`,
  )
  expect(document.title).not.toMatch(/[\u202A-\u202E\u2066-\u2069\n]/)
})

test('a heading without a name is as it was', async () => {
  const h1 = await show(<PageHeading>{wording.home.title}</PageHeading>)
  expect(h1.textContent).toBe(wording.home.title)
  expect(h1.querySelector('bdi')).toBeNull()
  expect(document.title).toBe(`${wording.home.title} · ${wording.productName}`)
})
