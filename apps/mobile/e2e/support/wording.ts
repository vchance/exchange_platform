import { readFileSync } from 'node:fs'
import { resolve } from 'node:path'

import type { Wording } from '@exchange/shared'

import { repoRoot } from './env'

/*
 * A copy of apps/web/e2e/support/wording.ts. The mobile app speaks from the
 * same wording files as the web app.
 *
 * What the product says, from the same wording files the app is built from,
 * so the tests find things by the words a person sees without restating them.
 */

function load(language: string): Wording {
  const file = resolve(repoRoot, `packages/shared/wording/${language}.json`)
  return JSON.parse(readFileSync(file, 'utf8')) as Wording
}

export const en = load('en')
export const es = load('es')

/** A message with its `{placeholders}` filled in. Plural messages are not supported. */
export function fill(message: string, values: Record<string, string | number>): string {
  return message.replace(/\{(\w+)\}/g, (whole, name: string) =>
    name in values ? String(values[name]) : whole,
  )
}
