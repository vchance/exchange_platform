import { expect, test } from './support/fixtures'
import { propose, setUpProfile, signIn, signUp, stateTag } from './support/flows'
import { en, fill } from './support/wording'

test('a replaced invitation link stops working, and the new one opens the proposal', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await signUp(ana)
  const { link: first } = await propose(ana, bruno, [
    { from: 'me', kind: 'ITEM', description: 'A set of garden chairs' },
  ])

  // Ana makes a new link, which replaces the one she had.
  const card = ana.page.getByRole('region', { name: en.invitationLink.heading, exact: true })
  await card.getByRole('button', { name: en.invitationLink.reissue }).click()
  await card
    .getByRole('group', { name: en.invitationLink.reissue })
    .getByRole('button', { name: en.invitationLink.reissue })
    .click()
  const field = card.getByLabel(en.invitationLink.linkLabel, { exact: true })
  await expect(field).toHaveValue(/\/en\/i#/)
  await expect(field).not.toHaveValue(first)
  const second = await field.inputValue()

  // The old one shows nothing of the proposal.
  const { page } = bruno
  await page.goto(first)
  await expect(page.getByRole('heading', { name: en.invitation.title, level: 1 })).toBeVisible()
  await expect(page.getByText(en.errors.INVITATION_UNAVAILABLE)).toBeVisible()
  await expect(page.getByText(en.invitation.alreadyResponded)).toBeVisible()
  await expect(page.getByText('A set of garden chairs')).toHaveCount(0)
  await expect(page.getByRole('button', { name: en.invitation.respond })).toHaveCount(0)

  // The new one works. (A fresh page: only the fragment differs, which a
  // browser would not reload for.)
  await page.goto('about:blank')
  await page.goto(second)
  await expect(page.getByText('A set of garden chairs', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: en.invitation.respond }).click()
  await signIn(bruno)
  await setUpProfile(bruno)
  await page.waitForURL(/\/exchanges\/[0-9a-f-]{36}$/)
  await expect(
    page.getByRole('heading', { name: fill(en.exchange.title, { name: ana.name }), level: 1 }),
  ).toBeVisible()
  await expect(stateTag(page)).toHaveText(en.states.NEGOTIATING)
})

test('a second invitation link pasted into a tab showing one opens its own proposal', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await signUp(ana)
  const { link: first } = await propose(ana, bruno, [
    { from: 'me', kind: 'ITEM', description: 'A set of garden chairs' },
  ])
  const { id: secondId, link: second } = await propose(ana, bruno, [
    { from: 'me', kind: 'ITEM', description: 'A folding ladder' },
  ])

  const { page } = bruno
  await page.goto(first)
  await expect(page.getByText('A set of garden chairs', { exact: true })).toBeVisible()
  await expect(page).toHaveURL(/\/en\/i$/)

  // The second link, pasted into the same tab: only the fragment differs, so
  // the browser does not load the page again, which the mark set here shows.
  await page.evaluate(() => {
    ;(window as { samePage?: boolean }).samePage = true
  })
  await page.evaluate((link) => {
    window.location.href = link
  }, second)
  await expect(page.getByText('A folding ladder', { exact: true })).toBeVisible()
  await expect(page.getByText('A set of garden chairs', { exact: true })).toHaveCount(0)
  await expect(page).toHaveURL(/\/en\/i$/)
  expect(await page.evaluate(() => (window as { samePage?: boolean }).samePage)).toBe(true)

  // Responding claims the second.
  await page.getByRole('button', { name: en.invitation.respond }).click()
  await signIn(bruno)
  await setUpProfile(bruno)
  await page.waitForURL(`**/exchanges/${secondId}`)
})
