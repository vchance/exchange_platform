import { expect, test } from 'vitest'

import { invitationLink, invitationPath, invitationToken, invitationTokenIn } from './invitation'

const token = 'a3'.repeat(32)

test('an invitation link carries its token in the fragment, never in the path or query', () => {
  const link = new URL(invitationLink('https://app.example/', 'es', token))
  expect(link.pathname).toBe('/es/i')
  expect(link.search).toBe('')
  expect(link.hash).toBe(`#${token}`)
  expect(invitationPath('pt-BR', token)).toBe(`/pt-BR/i#${token}`)
  expect(invitationToken(link.hash)).toBe(token)
})

test('a fragment that is not a token is not treated as one', () => {
  for (const hash of ['', '#', '#content', '#short', '#has spaces in it, plainly']) {
    expect(invitationToken(hash), hash).toBeNull()
  }
})

test('the token is found in a link however it arrives, or on its own', () => {
  for (const text of [
    `https://app.example/es/i#${token}`,
    `http://localhost:5173/en/i/#${token}`,
    `exchange://pt-BR/i#${token}`,
    `/zh-Hant/i#${token}`,
    `  https://app.example/en/i#${token}\n`,
    token,
    ` ${token} `,
  ]) {
    expect(invitationTokenIn(text), text).toBe(token)
  }
})

test('anything that is not an invitation link gives no token', () => {
  for (const text of [
    '',
    'hello',
    'https://app.example/en/i',
    'https://app.example/en/i#',
    'https://app.example/en/i#short',
    // Some other page that happens to have a long fragment.
    `https://app.example/account#${token}`,
    `https://elsewhere.example/#${token}`,
    `https://app.example/en/i?t=${token}`,
    `${token} and more`,
  ]) {
    expect(invitationTokenIn(text), text).toBeNull()
  }
})
