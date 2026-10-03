// @vitest-environment jsdom
import { HELP_FIGURES, HELP_LINKS, HELP_TOPICS, languages, type Language } from '@yuppers/shared'
import { afterEach, describe, expect, test } from 'vitest'

import { loadHelp } from '../app/wording'
import { heading, press, start, stop, until } from '../test/harness'
import { ana } from '../test/fake-service'

/*
 * The help pages, run in the whole app: every topic, in every language, at
 * its own address, and the ways between them.
 */

afterEach(stop)

const codes = languages.map((info) => info.code)

/** A link whose text is exactly `text`. */
function link(text: string): HTMLAnchorElement {
  const found = [...document.querySelectorAll('a')].find(
    (candidate) => candidate.textContent?.trim() === text,
  )
  if (!found) throw new Error(`no link “${text}”`)
  return found
}

describe.each<Language>(codes)('in %s', (language) => {
  test('the first page lists every topic', async () => {
    const help = await loadHelp(language)
    await start('/help', null, language)
    await heading(help.title)

    expect(document.documentElement.lang).toBe(language)
    const topics = [...document.querySelectorAll('nav.help-topics a')].map((a) => a.getAttribute('href'))
    expect(topics).toEqual(HELP_TOPICS.map((topic) => `/help/${topic}`))
  })

  test.each(HELP_TOPICS)('the topic %s', async (topic) => {
    const help = await loadHelp(language)
    const page = help.topics[topic]
    const { wording } = await start(`/help/${topic}`, null, language)
    await heading(page.title)

    expect(document.title).toBe(`${page.title} · ${wording.productName}`)
    // Every paragraph and list entry is on the page, with its figures filled in.
    const shown = document.querySelector('main')!.textContent!
    for (const block of page.blocks) {
      const parts = 'h' in block ? [block.h] : 'p' in block ? [block.p] : 'ul' in block ? block.ul : block.ol
      for (const part of parts) {
        const filled = part.replace(/\{(\w+)\}/g, (_, name: keyof typeof HELP_FIGURES) =>
          String(HELP_FIGURES[name]),
        )
        expect(shown).toContain(filled)
      }
    }
    expect(shown).not.toMatch(/[{}]/)
    // Its sections are listed at the top and lead to their headings.
    const sections = [...document.querySelectorAll('nav.help-sections a')]
    for (const entry of sections) {
      const target = document.getElementById(entry.getAttribute('href')!.slice(1))
      expect(target?.tagName).toBe('H2')
      expect(target?.textContent).toBe(entry.textContent)
    }
    // And the topic is marked as the current one in the list of all of them.
    expect(document.querySelector('nav.help-topics a[aria-current="page"]')?.getAttribute('href')).toBe(
      `/help/${topic}`,
    )
  })
})

describe('getting around', () => {
  test('from the footer to a topic and back to the list, without reloading', async () => {
    const { wording } = await start('/', null)
    const help = await loadHelp('en')
    await heading(wording.signIn.title)

    await press(link(wording.help.link))
    await heading(help.title)
    expect(window.location.pathname).toBe('/help')
    expect(link(wording.help.link).getAttribute('aria-current')).toBe('page')

    await press(link(help.topics.blocking.title))
    await heading(help.topics.blocking.title)
    expect(window.location.pathname).toBe('/help/blocking')
    expect(document.activeElement?.tagName).toBe('H1')

    await press(link(help.allTopics))
    await heading(help.title)
  })

  test('help is open to someone signed in, too', async () => {
    await start('/help/record', ana)
    const help = await loadHelp('en')
    await heading(help.topics.record.title)
  })

  test('a section of a topic can be linked to', async () => {
    const help = await loadHelp('en')
    await start('/help/keeping-track#section-3', null)
    await heading(help.topics['keeping-track'].title)
    await until(() => document.activeElement?.id === 'section-3', 'the section to take the focus')
  })

  test('a link can ask for a language', async () => {
    window.history.replaceState(null, '', '/help/signing?lang=es')
    const { addressLanguage } = await import('../app/wording')
    expect(addressLanguage()).toBe('es')
    window.history.replaceState(null, '', '/help/signing?lang=xx')
    expect(addressLanguage()).toBeNull()
    window.history.replaceState(null, '', '/help/signing')
    expect(addressLanguage()).toBeNull()
  })

  test('a topic there is not', async () => {
    const { wording } = await start('/help/nothing-here', null)
    const help = await loadHelp('en')
    await heading(wording.common.notFoundTitle)
    expect(link(help.allTopics).getAttribute('href')).toBe('/help')
  })
})

describe('“Learn more” links', () => {
  test('each opens its topic in a new tab, in the language on screen', async () => {
    const { wording } = await start('/', null, 'es')
    await heading(wording.signIn.title)
    const { HelpLink } = await import('../components/HelpLink')
    const { I18nContext, createI18n } = await import('../app/context')
    const { createRoot } = await import('react-dom/client')
    const { act } = await import('react')
    const box = document.createElement('div')
    document.body.append(box)
    const root = createRoot(box)
    const i18n = createI18n('es', wording, () => {})
    await act(async () =>
      root.render(
        <I18nContext value={i18n}>
          {Object.keys(HELP_LINKS).map((place) => (
            <HelpLink key={place} place={place as keyof typeof HELP_LINKS} />
          ))}
        </I18nContext>,
      ),
    )
    const links = [...box.querySelectorAll('a')]
    expect(links.map((a) => a.getAttribute('href'))).toEqual(
      Object.values(HELP_LINKS).map((topic) => `/help/${topic}?lang=es`),
    )
    for (const a of links) {
      expect(a.target).toBe('_blank')
      expect(a.textContent).toContain(wording.help.newTab)
    }
    await act(async () => root.unmount())
    box.remove()
  })
})
