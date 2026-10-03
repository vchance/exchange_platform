import { expect, test } from 'vitest'

import { labelText } from './label'

test('a name reads the same, on one line, without anything that turns text around', () => {
  expect(labelText('Ana')).toBe('Ana')
  expect(labelText('  Ana   María ')).toBe('Ana María')
  // A right-to-left override would turn the rest of a tab's title around.
  expect(labelText('Sam\u202Egnp.exe')).toBe('Sam gnp.exe')
  for (const mark of [
    '\u061C',
    '\u200E',
    '\u200F',
    '\u202A',
    '\u202B',
    '\u202C',
    '\u202D',
    '\u2066',
    '\u2067',
    '\u2068',
    '\u2069',
  ]) {
    expect(labelText(`a${mark}b`), JSON.stringify(mark)).toBe('a b')
  }
  // Line breaks, tabs and other control characters, C0 and C1.
  expect(labelText('Sam\nCompleted\r\n\tReference')).toBe('Sam Completed Reference')
  expect(labelText('a\u0000b\u0007c\u007fd\u0085e\u009fz')).toBe('a b c d e z')
  expect(labelText('a\u2028b\u2029c')).toBe('a b c')
  expect(labelText('\u202E\u2066')).toBe('')
})

test('names written in any script are left as they are', () => {
  for (const name of ['محمد', 'שרה', '李小龙', 'Zoë Ó Briain', 'José 👩🏽‍🔧']) {
    expect(labelText(name)).toBe(name)
  }
})
