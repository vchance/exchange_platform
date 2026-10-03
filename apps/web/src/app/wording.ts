import { pickLanguage, type Language, type Wording } from '@yuppers/shared'

/*
 * Each language's wording is its own file, fetched when that language is
 * shown. A visitor loads one language, however many the product supports,
 * which keeps the invitation page inside its budget (DESIGN.md §13.5). The
 * glob picks up every file in the shared wording folder, so a new language
 * needs nothing here.
 */
const files = import.meta.glob<Wording>(
  ['../../../../packages/shared/wording/*.json', '!**/languages.json'],
  { import: 'default' },
)

export function loadWording(language: Language): Promise<Wording> {
  const path = Object.keys(files).find((file) => file.endsWith(`/${language}.json`))
  if (!path) return Promise.reject(new Error(`no wording file for ${language}`))
  return files[path]()
}

const CHOICE = 'yuppers.language'

/**
 * The language to show before anyone has signed in: the one chosen in the
 * picker on this device, otherwise the browser's.
 */
export function deviceLanguage(): Language {
  let chosen: string | null = null
  try {
    chosen = window.localStorage.getItem(CHOICE)
  } catch {
    // Storage can be switched off; the browser's language still works.
  }
  return pickLanguage(chosen ? [chosen, ...navigator.languages] : navigator.languages)
}

export function rememberLanguage(language: Language): void {
  try {
    window.localStorage.setItem(CHOICE, language)
  } catch {
    // A convenience only.
  }
}
