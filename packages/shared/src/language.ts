import { en } from './wording/en'
import { es } from './wording/es'
import type { Wording } from './wording/types'

export const languages = ['en', 'es'] as const
export type Language = (typeof languages)[number]

const wording: Record<Language, Wording> = { en, es }

export function wordingFor(language: Language): Wording {
  return wording[language]
}

/**
 * Picks the first supported language from a device or browser preference
 * list such as `['es-MX', 'en-US']`, falling back to English.
 */
export function pickLanguage(preferred: readonly string[]): Language {
  for (const tag of preferred) {
    const base = tag.toLowerCase().split('-')[0]
    const match = languages.find((language) => language === base)
    if (match) return match
  }
  return 'en'
}
