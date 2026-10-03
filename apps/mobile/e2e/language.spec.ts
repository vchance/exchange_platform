import { readFileSync } from 'node:fs'

import { ApiPerson } from './support/api'
import { apiLog } from './support/env'
import { expect, test } from './support/fixtures'
import { acceptOpen, button, setUpProfile, signIn, title } from './support/flows'
import { en, es, fill } from './support/wording'

test('a phone set to Spanish gets the app in Spanish, and the account keeps the language', async ({
  person,
}) => {
  const lucia = await person('Lucía', { locale: 'es-ES' })
  const { page } = lucia

  await page.goto('/')
  await expect(title(page, es.signIn.title)).toBeVisible()
  await expect(page.getByRole('radio', { name: 'Español' })).toBeChecked()
  await signIn(lucia, es)
  // The code was sent in Spanish too.
  expect(readFileSync(apiLog, 'utf8')).toMatch(
    new RegExp(`to="?${lucia.email}"? code="?\\d{6}"? purpose="?sign-in"? language="?es"?`),
  )
  await setUpProfile(lucia, es)
  await expect(title(page, es.home.title)).toBeVisible()
  await expect(page.getByText(es.home.empty)).toBeVisible()

  const elsewhere = await ApiPerson.signUp('Lucía', lucia.email)
  expect(((await elsewhere.call('GET', '/v1/me')) as { language: string }).language).toBe('es')
})

test('choosing Spanish on the first screen of an English phone changes the app and the account', async ({
  person,
}) => {
  const lucia = await person('Lucía')
  const { page } = lucia

  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
  await page.getByRole('radio', { name: 'Español' }).click()
  await expect(title(page, es.signIn.title)).toBeVisible()
  await signIn(lucia, es)
  await setUpProfile(lucia, es)
  await expect(title(page, es.home.title)).toBeVisible()

  // Back to English from the account screen, kept by the account.
  await page.getByRole('link', { name: es.nav.account }).click()
  // The language is part of the profile, saved with it.
  await page.getByRole('radio', { name: 'English' }).click()
  await expect(title(page, es.nav.account)).toBeVisible()
  await button(page, es.profile.save).click()
  await expect(title(page, en.nav.account)).toBeVisible()
  await expect(page.getByText(en.profile.saved)).toBeVisible()
  const elsewhere = await ApiPerson.signUp('Lucía', lucia.email)
  await expect
    .poll(async () => ((await elsewhere.call('GET', '/v1/me')) as { language: string }).language)
    .toBe('en')
})

test('a Spanish invitation is read, signed and accepted in Spanish', async ({ person, email }) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const lucia = await person('Lucía', { locale: 'es-ES' })
  const { page } = lucia
  const { id, link } = await ana.propose(
    'Lucía',
    [
      { from: 'A', kind: 'SERVICE', description: 'Clases de guitarra' },
      { from: 'B', kind: 'MONEY', description: 'Pago de las clases', amountMinor: 8000 },
    ],
    { language: 'es' },
  )

  await page.goto(link)
  await expect(title(page, es.invitation.signedOutTitle)).toBeVisible()
  await expect(page.getByText(es.invitation.signInToRead)).toBeVisible()
  await signIn(lucia, es)
  await expect(title(page, es.invitation.title)).toBeVisible()
  await expect(
    page.getByText(fill(es.claimant.invitationIntroSignedIn, { name: 'Ana' })),
  ).toBeVisible()
  await expect(page.getByText(es.terms.moneyOutside)).toBeVisible()
  // What the parties wrote is shown as they wrote it, never translated.
  await expect(page.getByText('Clases de guitarra')).toBeVisible()

  await button(page, es.invitation.respondNew).click()
  await setUpProfile(lucia, es)
  await page.waitForURL(`**/exchanges/${id}`)
  await acceptOpen(page, es)
  await ana.confirmCounterparty(id)
  await page.reload()
  await expect(page.getByText(es.states.ACTIVE, { exact: true })).toBeVisible()
})
