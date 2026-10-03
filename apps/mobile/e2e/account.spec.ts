import { ApiPerson } from './support/api'
import { codeFrom } from './support/codes'
import { expect, test } from './support/fixtures'
import { acceptOpen, button, join, setUpProfile, signIn, signUp, title } from './support/flows'
import { en, fill } from './support/wording'

test('a new person signs in with a code from their email and sets up a profile', async ({
  person,
}) => {
  const ana = await person('Ana')
  const { page } = ana

  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
  await signIn(ana)

  // A first sign-in makes the account; it is asked for a name and an age first.
  await expect(page.getByText(en.profile.firstIntro)).toBeVisible()
  // Nothing is saved without both.
  await button(page, en.profile.continue).click()
  await expect(page.getByText(en.profile.nameRequired)).toBeVisible()
  await expect(page.getByText(en.profile.adultRequired)).toBeVisible()
  await setUpProfile(ana)

  await expect(title(page, en.home.title)).toBeVisible()
  await expect(page.getByText(en.home.empty)).toBeVisible()

  // The account screen shows what was saved.
  await page.getByRole('link', { name: en.nav.account }).click()
  await expect(title(page, en.nav.account)).toBeVisible()
  await expect(page.getByLabel(en.profile.nameLabel, { exact: true })).toHaveValue('Ana')
  await expect(page.getByText(ana.email)).toBeVisible()
  await expect(page.getByText(en.profile.adultConfirmed)).toBeVisible()

  // A new name is kept by the service, not just by the screen.
  await page.getByLabel(en.profile.nameLabel, { exact: true }).fill('Ana María')
  await button(page, en.profile.save).click()
  await expect(page.getByText(en.profile.saved)).toBeVisible()
  await page.reload()
  await expect(page.getByLabel(en.profile.nameLabel, { exact: true })).toHaveValue('Ana María')

  // Signing out leaves the way to sign in, and the session is gone.
  await button(page, en.nav.signOut).click()
  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
})

test('a person deletes their account while an agreement is in force', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { id, link } = await ana.propose('Ben', [
    { from: 'A', kind: 'ITEM', description: 'A bicycle' },
    { from: 'B', kind: 'SERVICE', description: 'Fix the gate' },
  ])
  await join(ben, link)
  await acceptOpen(page)
  await ana.confirmCounterparty(id)

  // To the account screen the way a person goes, from the list, which stays
  // beneath it and is where they come back to.
  await page.goto('/')
  await page.getByRole('link', { name: en.nav.account }).click()
  await button(page, en.deletion.open).click()

  // Before anything is sent, what deleting would do, including to the agreement.
  await expect(page.getByText(en.deletion.agreementsStand)).toBeVisible()
  await expect(page.getByText(/^1 agreement in progress gets a request to close/)).toBeVisible()
  await expect(page.getByText(fill(en.deletion.codeIntro, { identifier: ben.email }))).toBeVisible()

  // Proof is a code sent for deleting, which cannot sign in.
  const code = await codeFrom(ben.email, 'delete-account', () =>
    button(page, en.deletion.sendCode).click(),
  )
  await page.getByLabel(en.signIn.codeLabel).fill(code)
  await button(page, en.deletion.continue).click()
  await expect(page.getByText(en.deletion.confirmBody)).toBeVisible()
  await button(page, en.deletion.confirm).click()

  // Back on the first screen, signed out, told once.
  await expect(page.getByText(en.deletion.deleted)).toBeVisible()
  await expect(title(page, en.signIn.title)).toBeVisible()
  await button(page, en.deletion.dismiss).click()
  await expect(page.getByText(en.deletion.deleted)).toBeHidden()

  // The other party keeps the agreement, with a request to close it and word
  // that Ben has left.
  const seen = await ana.view(id)
  expect(seen.other_party_left).toBe(true)
  expect(seen.state).toBe('ACTIVE')

  // The same address signs up again as a new account, which sees nothing of the old one.
  await signIn(ben)
  await setUpProfile(ben)
  await expect(page.getByText(en.home.empty)).toBeVisible()
})

test('the account screen opened by a direct link still says the account was deleted', async ({
  person,
}) => {
  const ana = await person('Ana')
  const { page } = ana
  await signUp(ana)

  // Straight to the account screen, with nothing beneath it.
  await page.goto('/account')
  await expect(title(page, en.nav.account)).toBeVisible()
  await button(page, en.deletion.open).click()
  await expect(page.getByText(en.deletion.nothingOpen)).toBeVisible()
  const code = await codeFrom(ana.email, 'delete-account', () =>
    button(page, en.deletion.sendCode).click(),
  )
  await page.getByLabel(en.signIn.codeLabel).fill(code)
  await button(page, en.deletion.continue).click()
  await button(page, en.deletion.confirm).click()

  // On the first screen, signed out, and told.
  await expect(title(page, en.signIn.title)).toBeVisible()
  await expect(page.getByText(en.deletion.deleted)).toBeVisible()
  await button(page, en.deletion.dismiss).click()
  await expect(page.getByText(en.deletion.deleted)).toBeHidden()
})

test('a person who has signed in is asked again after reopening the app with a dead session', async ({
  person,
}) => {
  const ana = await person('Ana')
  const { page } = ana
  await signUp(ana)

  // The harness keeps the session for the tab, as the app keeps it in secure
  // storage: reopening keeps you signed in.
  await page.reload()
  await expect(title(page, en.home.title)).toBeVisible()

  // A session the service no longer honors is dropped, and sign-in is asked for.
  await page.evaluate(() => {
    const key = Object.keys(window.sessionStorage).find((name) => name.includes('session'))
    if (key) window.sessionStorage.setItem(key, 'not-a-session')
  })
  await page.reload()
  await expect(title(page, en.signIn.title)).toBeVisible()
})
