import { expect, test } from 'vitest'

import en from '../wording/en.json'
import es from '../wording/es.json'
import { shortCommit, versionText } from './build'
import type { Wording } from './wording/types'

const SHA = '0123456789ABCDEF0123456789abcdef01234567'

test('a commit is shortened to seven characters, and anything else is no commit', () => {
  expect(shortCommit(SHA)).toBe('0123456')
  expect(shortCommit('abcdef1')).toBe('abcdef1')
  for (const bad of [undefined, null, '', 'unknown', 'abc', 'g123456', `${SHA}${SHA}`]) {
    expect(shortCommit(bad), String(bad)).toBeNull()
  }
})

test('the version line says what the build says, and leaves out what it does not', () => {
  const wording = en as unknown as Wording
  expect(versionText(wording, 'en', { version: '0.1.0', commit: SHA })).toBe('Version 0.1.0 (0123456)')
  expect(versionText(wording, 'en', { version: '0.1.0', commit: 'unknown' })).toBe('Version 0.1.0')
  expect(versionText(wording, 'en', { version: '0.1.0', build: '12' })).toBe('Version 0.1.0 (build 12)')
  expect(versionText(wording, 'en', { version: '0.1.0', build: '12', commit: SHA })).toBe(
    'Version 0.1.0 (build 12, 0123456)',
  )
  expect(versionText(es as unknown as Wording, 'es', { version: '0.1.0', build: '12' })).toBe(
    'Versión 0.1.0 (compilación 12)',
  )
})
