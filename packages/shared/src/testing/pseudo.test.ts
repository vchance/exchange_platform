import { describe, expect, test } from 'vitest'

import en from '../../wording/en.json'
import { formatMessage } from '../message'
import { formattedWords, pseudoMessage, pseudoWording, untranslated } from './pseudo'

describe('the pseudo-language', () => {
  test('keeps no plain Latin letter, and marks where a message starts and ends', () => {
    expect(pseudoMessage('Your terms')).toBe('[Ýöûŕ ţéŕɱš···]')
  })

  test('keeps placeholders and plurals working', () => {
    const waiting = pseudoMessage('Waiting for {name}')
    expect(formatMessage(waiting, { name: 'Ana' }, 'en')).toMatch(/^\[Ŵáíţíñĝ ƒöŕ Ana·+\]$/)
    const plural = pseudoMessage('{count, plural, one {# item left} other {# items left}}')
    expect(formatMessage(plural, { count: 1 }, 'en')).toMatch(/^\[1 íţéɱ ĺéƒţ·+\]$/)
    expect(formatMessage(plural, { count: 3 }, 'en')).toMatch(/^\[3 íţéɱš ĺéƒţ·+\]$/)
  })

  test('covers every message, each with the same placeholders as English', () => {
    const paths = (value: unknown, prefix = ''): [string, string][] =>
      typeof value === 'string'
        ? [[prefix, value]]
        : Object.entries(value as object).flatMap(([key, child]) =>
            paths(child, prefix ? `${prefix}.${key}` : key),
          )
    const pseudo = new Map(paths(pseudoWording()))
    const variables = (message: string) => [...message.matchAll(/\{\s*(\w+)/g)].map((m) => m[1])
    for (const [path, message] of paths(en)) {
      const rewritten = pseudo.get(path)!
      expect(variables(rewritten), path).toEqual(variables(message))
      // Outside its placeholders, nothing of the English is left.
      const syntax = rewritten
        .replace(/\{\s*\w+/g, '{')
        .replace(/,\s*plural\s*,/g, ',')
        .replace(/\b(zero|one|two|few|many|other)\s*\{/g, '{')
      expect(syntax, path).not.toMatch(/[A-Za-z]/)
    }
  })
})

describe('finding text that did not come through the wording', () => {
  test('finds a word written into a component', () => {
    expect(untranslated(`${pseudoMessage('Sign')} Cancel`, [])).toEqual(['Cancel'])
  })

  test('lets through what is allowed, whole words only', () => {
    const allowed = ['Ana Ruiz', ...formattedWords('en', ['America/Chicago'])]
    expect(untranslated('[Ŵíţĥ] Ana Ruiz, October 30, 2026 at 9:00:00 PM CDT', allowed)).toEqual([])
    expect(untranslated('Mayday', allowed)).toEqual(['Mayday'])
    expect(untranslated('PVVS-5Q2K ana@example.test', [])).toEqual([])
  })
})
