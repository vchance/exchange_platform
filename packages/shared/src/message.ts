/**
 * Fills in a wording message. The wording files use a small part of ICU
 * MessageFormat, and this handles exactly that part:
 *
 *   `Waiting for {name}`
 *   `{count, plural, =0 {Nothing left} one {# item left} other {# items left}}`
 *
 * A number is written the way the language writes numbers, and the plural
 * form is the language's own. A placeholder with no value is left as it is,
 * so a mistake shows up on the screen instead of taking the screen down.
 */
export type MessageValues = Record<string, string | number>

export function formatMessage(message: string, values: MessageValues, language: string): string {
  return render(message, values, language, undefined)
}

function render(
  message: string,
  values: MessageValues,
  language: string,
  /** The number a `#` stands for, inside a plural branch. */
  count: number | undefined,
): string {
  let output = ''
  let index = 0
  while (index < message.length) {
    const char = message[index]
    if (char === '#' && count !== undefined) {
      output += formatNumber(count, language)
      index += 1
    } else if (char === '{') {
      const end = closing(message, index)
      if (end === -1) return output + message.slice(index)
      output += placeholder(message.slice(index + 1, end), values, language)
      index = end + 1
    } else {
      output += char
      index += 1
    }
  }
  return output
}

/** The index of the brace that closes the one at `open`, or -1. */
function closing(message: string, open: number): number {
  let depth = 0
  for (let index = open; index < message.length; index += 1) {
    if (message[index] === '{') depth += 1
    else if (message[index] === '}') {
      depth -= 1
      if (depth === 0) return index
    }
  }
  return -1
}

function placeholder(inner: string, values: MessageValues, language: string): string {
  const comma = inner.indexOf(',')
  const name = (comma === -1 ? inner : inner.slice(0, comma)).trim()
  const value = values[name]
  if (value === undefined) return `{${inner}}`

  if (comma === -1) {
    return typeof value === 'number' ? formatNumber(value, language) : value
  }

  const rest = inner.slice(comma + 1).trimStart()
  if (!rest.startsWith('plural') || typeof value !== 'number') return `{${inner}}`
  const branches = pluralBranches(rest.slice(rest.indexOf(',') + 1))
  const chosen =
    branches.get(`=${value}`) ??
    branches.get(pluralCategory(value, language)) ??
    branches.get('other')
  return chosen === undefined ? `{${inner}}` : render(chosen, values, language, value)
}

/**
 * Which of the language's plural forms a number takes. The JavaScript engine
 * in the mobile apps has no `Intl.PluralRules`; the mobile app supplies one
 * from the same Unicode data before anything is shown. Should it ever be
 * missing all the same, `other` is the form every message has.
 */
function pluralCategory(value: number, language: string): string {
  if (typeof Intl.PluralRules !== 'function') return 'other'
  return new Intl.PluralRules(language).select(value)
}

/** Reads `one {…} other {…}` into its branches. */
function pluralBranches(text: string): Map<string, string> {
  const branches = new Map<string, string>()
  let index = 0
  while (index < text.length) {
    const open = text.indexOf('{', index)
    if (open === -1) break
    const end = closing(text, open)
    if (end === -1) break
    branches.set(text.slice(index, open).trim(), text.slice(open + 1, end))
    index = end + 1
  }
  return branches
}

function formatNumber(value: number, language: string): string {
  return new Intl.NumberFormat(language).format(value)
}
