import en from '../wording/en.json'
import es from '../wording/es.json'
import manifest from '../wording/languages.json'
import type { Wording } from './wording/types'

/*
 * Adding a language (DESIGN.md §4.2):
 *   1. wording/<code>.json, with every key that wording/en.json has;
 *   2. an entry in wording/languages.json;
 *   3. an import and an entry in `wording` below.
 * Nothing else in the clients, the API or the database names a language.
 */
const wording = { en, es } satisfies Record<string, Wording>

/** A supported language, as a BCP 47 tag such as `en` or `pt-BR`. */
export type Language = keyof typeof wording

export interface LanguageInfo {
  code: Language
  /** The language's name in that language, for a language picker. */
  name: string
  /** Which way text runs. Layouts follow it; nothing assumes left to right. */
  direction: 'ltr' | 'rtl'
}

/** Every supported language. The first is the default. */
export const languages = manifest as readonly LanguageInfo[]

export const defaultLanguage: Language = languages[0].code

export function wordingFor(language: Language): Wording {
  return wording[language]
}

export function directionOf(language: Language): 'ltr' | 'rtl' {
  return languages.find((info) => info.code === language)?.direction ?? 'ltr'
}

function match(tag: string): Language | undefined {
  const wanted = tag.toLowerCase()
  return languages.find((info) => info.code.toLowerCase() === wanted)?.code
}

/**
 * Picks a supported language from a device or browser preference list such
 * as `['es-MX', 'en-US']`. Each preference is tried as written, then by its
 * base language, before moving to the next; the default is the last resort.
 */
export function pickLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const found = match(tag) ?? match(tag.split('-')[0])
    if (found) return found
  }
  return defaultLanguage
}
