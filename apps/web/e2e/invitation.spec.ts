import { expect, test } from './support/fixtures'
import { join, propose, setUpProfile, signIn, signUp, stateTag } from './support/flows'
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

  // Signed out, the old link and the new one open the same page: neither
  // says anything about itself until someone signs in. (A fresh page each
  // time: only the fragment differs, which a browser would not reload for.)
  const { page } = bruno
  const signedOut = async (link: string) => {
    await page.goto('about:blank')
    await page.goto(link)
    await expect(
      page.getByRole('heading', { name: en.invitation.signedOutTitle, level: 1 }),
    ).toBeVisible()
    return page.locator('main').innerHTML()
  }
  expect(await signedOut(first)).toBe(await signedOut(second))

  // Signed in, the old one shows nothing of the proposal.
  await signedOut(first)
  await signIn(bruno)
  await expect(page.getByText(en.errors.INVITATION_UNAVAILABLE)).toBeVisible()
  await expect(page.getByText('A set of garden chairs')).toHaveCount(0)
  await expect(page.getByRole('button', { name: en.invitation.respondNew })).toHaveCount(0)

  // The new one works.
  await page.goto('about:blank')
  await page.goto(second)
  await expect(page.getByText('A set of garden chairs', { exact: true })).toBeVisible()
  await page.getByRole('button', { name: en.invitation.respondNew, exact: true }).click()
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
  await signIn(bruno)
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
  await page.getByRole('button', { name: en.invitation.respondNew, exact: true }).click()
  await setUpProfile(bruno)
  await page.waitForURL(`**/exchanges/${secondId}`)
})

test('a used link takes the person who used it back to the exchange, and claims nothing for anyone else', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  const carla = await person('Carla')
  await signUp(ana)
  const { id, link } = await propose(ana, bruno, [
    { from: 'me', kind: 'ITEM', description: 'A set of garden chairs' },
  ])
  await join(bruno, link)

  // Bruno comes back to the message and opens the link again: it takes him
  // to the exchange he joined.
  await bruno.page.goto('about:blank')
  await bruno.page.goto(link)
  await bruno.page.waitForURL(`**/exchanges/${id}`)

  // Carla, signed in, opens the same link: it is spent for her, nothing on
  // the page takes it, and Bruno is still the one Ana is dealing with.
  await signUp(carla)
  await carla.page.goto(link)
  await expect(carla.page.getByText(en.errors.INVITATION_UNAVAILABLE)).toBeVisible()
  await expect(carla.page.getByText('A set of garden chairs')).toHaveCount(0)
  await expect(carla.page.getByRole('button', { name: en.invitation.respondNew })).toHaveCount(0)
  await expect(
    carla.page.getByRole('button', { name: fill(en.invitation.respondAs, { name: carla.name }) }),
  ).toHaveCount(0)
  await expect(carla.page).toHaveURL(/\/en\/i$/)
  await carla.page.goto('/')
  await expect(carla.page.getByRole('heading', { name: en.home.title, level: 1 })).toBeVisible()
  await expect(carla.page.getByText(en.home.empty)).toBeVisible()
})
