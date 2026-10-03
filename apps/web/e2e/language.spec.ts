import { expect, test } from './support/fixtures'
import { propose, setUpProfile, signIn, signUp } from './support/flows'
import { es, fill } from './support/wording'

/*
 * Languages (DESIGN.md §4.2): an invitation link names the sender's language
 * so its preview is in that language; the page is in the reader's own, from
 * their browser or their choice, and an account keeps the language it was
 * made in.
 */

test('a Spanish invitation opens in Spanish for a Spanish reader, and stays Spanish once they sign in', async ({
  person,
}) => {
  const ana = await person('Ana')
  const carlos = await person('Carlos', { locale: 'es-ES' })
  await signUp(ana)
  const { link } = await propose(ana, carlos, [
    { from: 'me', kind: 'ITEM', description: 'Una mesa de roble' },
  ])
  const spanish = link.replace('/en/i#', '/es/i#')

  const { page } = carlos
  await page.goto(spanish)
  await expect(page.locator('html')).toHaveAttribute('lang', 'es')
  await expect(page.getByRole('heading', { name: es.invitation.title, level: 1 })).toBeVisible()
  await expect(page.getByText(es.invitation.notBinding)).toBeVisible()
  // What the parties wrote is never translated.
  await expect(page.getByText('Una mesa de roble', { exact: true })).toBeVisible()

  await page.getByRole('button', { name: es.invitation.respond }).click()
  await signIn(carlos, es)
  await setUpProfile(carlos, es)
  await page.waitForURL(/\/exchanges\/[0-9a-f-]{36}$/)
  await expect(
    page.getByRole('heading', { name: fill(es.exchange.title, { name: ana.name }), level: 1 }),
  ).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'es')

  // The account was made in Spanish, and says so.
  await page.getByRole('link', { name: es.nav.account, exact: true }).click()
  await expect(page.getByLabel(es.profile.languageLabel, { exact: true })).toHaveValue('es')
  await page.reload()
  await expect(page.getByRole('heading', { name: es.profile.title, level: 1 })).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'es')
})

test('Spanish chosen before signing in is the language after signing in', async ({ person }) => {
  const dora = await person('Dora')
  const { page } = dora
  await page.goto('/')
  await expect(page.locator('html')).toHaveAttribute('lang', 'en')

  await page.getByRole('combobox', { name: 'Language', exact: true }).selectOption('es')
  await expect(page.getByRole('heading', { name: es.signIn.title, level: 1 })).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'es')

  await signIn(dora, es)
  await setUpProfile(dora, es)
  await expect(page.getByRole('heading', { name: es.home.title, level: 1 })).toBeVisible()

  // A fresh page, with nothing remembered by this browser, still finds it on the account.
  await page.evaluate(() => window.localStorage.clear())
  await page.reload()
  await expect(page.getByRole('heading', { name: es.home.title, level: 1 })).toBeVisible()
  await expect(page.locator('html')).toHaveAttribute('lang', 'es')
})
