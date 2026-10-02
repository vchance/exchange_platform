// Checks the wording files against each other, independent of TypeScript:
// every language in the manifest has a file, and every file has exactly the
// keys of the reference language, each with text in it. The typecheck catches
// a missing key; this also catches a stray one, an empty one, and a language
// listed without a file.

import { readFileSync, readdirSync } from 'node:fs'
import { fileURLToPath } from 'node:url'

const directory = fileURLToPath(new URL('../wording/', import.meta.url))
const read = (name) => JSON.parse(readFileSync(directory + name, 'utf8'))

/** Every path to a string in a wording file, such as `errors.NOT_FOUND`. */
function paths(value, prefix = '') {
  if (typeof value === 'string') return [[prefix, value]]
  return Object.entries(value).flatMap(([key, child]) =>
    paths(child, prefix ? `${prefix}.${key}` : key),
  )
}

/** The variables an ICU message uses, such as `count` in `{count, plural, …}`. */
function variables(message) {
  return [...message.matchAll(/\{\s*(\w+)/g)].map((found) => found[1]).sort().join(',')
}

const problems = []
const manifest = read('languages.json')
const codes = manifest.map((language) => language.code)
const [reference] = codes
const expected = new Map(paths(read(`${reference}.json`)))

for (const language of manifest) {
  if (!/^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$/.test(language.code)) {
    problems.push(`${language.code}: not a language tag`)
  }
  if (!language.name) problems.push(`${language.code}: no name`)
  if (!['ltr', 'rtl'].includes(language.direction)) {
    problems.push(`${language.code}: direction must be ltr or rtl`)
  }
}
if (new Set(codes).size !== codes.length) problems.push('languages.json lists a language twice')

const files = readdirSync(directory).filter((name) => name !== 'languages.json')
for (const file of files) {
  if (!codes.includes(file.replace(/\.json$/, ''))) {
    problems.push(`${file}: not listed in languages.json`)
  }
}

for (const code of codes) {
  let found
  try {
    found = new Map(paths(read(`${code}.json`)))
  } catch {
    problems.push(`${code}: no readable wording/${code}.json`)
    continue
  }
  for (const [path, message] of expected) {
    const translated = found.get(path)
    if (translated === undefined) problems.push(`${code}: missing ${path}`)
    else if (!translated.trim()) problems.push(`${code}: empty ${path}`)
    else if (variables(translated) !== variables(message)) {
      problems.push(`${code}: ${path} does not use the same variables as ${reference}`)
    }
  }
  for (const path of found.keys()) {
    if (!expected.has(path)) problems.push(`${code}: unexpected ${path}`)
  }
}

if (problems.length > 0) {
  console.error(problems.join('\n'))
  process.exit(1)
}
console.log(`wording: ${codes.length} languages, ${expected.size} messages each`)
