import { ApiPerson } from './support/api'
import { expect, test } from './support/fixtures'
import {
  acceptOpen,
  button,
  historyEntries,
  join,
  move,
  setUpProfile,
  signIn,
  signUp,
  title,
} from './support/flows'
import { en, fill } from './support/wording'

/*
 * The invited person uses the app; the person who started the exchange acts
 * through the API, as their own app or the web app would.
 */

test('the invited person signs in, reads the proposal, with the notice that money is paid outside, and accepts', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { id, link } = await ana.propose(
    'Ben',
    [
      { from: 'A', kind: 'SERVICE', description: 'Paint the fence' },
      { from: 'B', kind: 'MONEY', description: 'Payment for painting', amountMinor: 5000 },
    ],
    { note: 'As we discussed on Saturday.' },
  )

  // The link opens the invitation screen, which asks Ben to sign in before
  // showing anything of it, and its token never becomes part of the
  // screen's address.
  await page.goto(link)
  await expect(title(page, en.invitation.signedOutTitle)).toBeVisible()
  await expect(page.getByText(en.invitation.signInToRead)).toBeVisible()
  await expect(page.getByText('Paint the fence')).toHaveCount(0)
  expect(new URL(page.url()).pathname).toBe('/invitation')
  expect(page.url()).not.toContain('#')
  await expect(page.getByRole('heading', { name: en.signIn.title, level: 2 })).toBeVisible()
  await signIn(ben)

  // Everything there is to read before deciding: who sent it, that it binds
  // nobody yet, the message, both sides' items, and that the money is paid
  // outside the product.
  await expect(title(page, en.invitation.title)).toBeVisible()
  await expect(
    page.getByText(fill(en.claimant.invitationIntroSignedIn, { name: 'Ana' })),
  ).toBeVisible()
  await expect(page.getByText(en.invitation.notBinding)).toBeVisible()
  await expect(page.getByText('As we discussed on Saturday.')).toBeVisible()
  await expect(page.getByText('Paint the fence')).toBeVisible()
  await expect(page.getByText('Payment for painting')).toBeVisible()
  await expect(page.getByText(en.terms.moneyOutside)).toBeVisible()
  await expect(page.getByText(fill(en.terms.amount, { amount: '$50.00' }))).toBeVisible()

  // Responding asks only for what a new account still lacks: the profile.
  await button(page, en.invitation.respondNew).click()
  await setUpProfile(ben)
  await page.waitForURL(`**/exchanges/${id}`)

  // Until Ana confirms them, Ben may sign but do little else.
  await expect(page.getByText(fill(en.claimant.limits, { name: 'Ana' }))).toBeVisible()
  await acceptOpen(page)

  // Ana sees the signature and confirms Ben, which puts the agreement in force.
  const confirmed = await ana.confirmCounterparty(id)
  expect(confirmed.state).toBe('ACTIVE')
  await page.getByRole('link', { name: en.exchange.refresh }).click()
  await expect(page.getByText(en.states.ACTIVE, { exact: true })).toBeVisible()
  await expect(
    page.getByRole('heading', { name: en.exchange.agreementHeading, exact: true }),
  ).toBeVisible()
})

test('a signed-in person pastes an invitation link into the app', async ({ person, email }) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  await signUp(ben)
  const { id, link } = await ana.propose('Ben', [
    { from: 'A', kind: 'ITEM', description: 'A wooden chair' },
  ])

  await button(page, en.mobile.openInvitation.title).click()
  const field = page.getByLabel(en.mobile.openInvitation.label, { exact: true })
  // Something that is not an invitation is refused where it was typed.
  await field.fill('https://example.test/nothing-here')
  await button(page, en.mobile.openInvitation.open).click()
  await expect(page.getByText(en.mobile.openInvitation.invalid)).toBeVisible()

  await field.fill(link)
  await button(page, en.mobile.openInvitation.open).click()
  await expect(page.getByText('A wooden chair')).toBeVisible()
  await button(page, fill(en.invitation.respondAs, { name: 'Ben' })).click()
  await page.waitForURL(`**/exchanges/${id}`)
  await expect(page.getByText(fill(en.claimant.limits, { name: 'Ana' }))).toBeVisible()
})

test('a signed-out person pastes an invitation link from the first screen', async ({
  person,
  email,
}) => {
  // Someone invited is usually new and signed out: the first screen offers
  // the invitation beside signing in, and the invitation asks them to sign in
  // before showing the proposal.
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { link } = await ana.propose('Ben', [{ from: 'A', kind: 'ITEM', description: 'A lamp' }])

  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
  await expect(page.getByRole('heading', { name: en.mobile.invited.heading })).toBeVisible()
  await button(page, en.mobile.openInvitation.title).click()
  await page.getByLabel(en.mobile.openInvitation.label, { exact: true }).fill(link)
  await button(page, en.mobile.openInvitation.open).click()
  // The same screen the link itself opens: sign in first, then read.
  await expect(title(page, en.invitation.signedOutTitle)).toBeVisible()
  await expect(page.getByText('A lamp')).toHaveCount(0)
  await signIn(ben)
  await expect(title(page, en.invitation.title)).toBeVisible()
  await expect(page.getByText('A lamp')).toBeVisible()
  await button(page, en.invitation.respondNew).click()
  await setUpProfile(ben)
  await expect(page.getByText(fill(en.claimant.limits, { name: 'Ana' }))).toBeVisible()
})

test('once the agreement is in force, delivery is marked and confirmed from the app', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const {
    id,
    link,
    contributionIds: [fence, payment],
  } = await ana.propose('Ben', [
    { from: 'A', kind: 'SERVICE', description: 'Paint the fence' },
    { from: 'B', kind: 'MONEY', description: 'Payment for painting', amountMinor: 5000 },
  ])
  await join(ben, link)
  await acceptOpen(page)
  await ana.confirmCounterparty(id)
  await page.reload()
  await expect(page.getByText(en.states.ACTIVE, { exact: true })).toBeVisible()

  // Ben pays, outside the product, and records it with a note. The panel says
  // again that no money moves here.
  const paid = en.exchange.moneyMoves.CLAIM
  await button(page, paid).click()
  await expect(page.getByText(fill(en.exchange.moneyMoveText.CLAIM, { name: 'Ana' }))).toBeVisible()
  await button(page, en.common.cancel).click()
  await move(page, paid, 'Paid in cash at the door.')
  await expect(page.getByText(en.moneyStatus.CLAIMED, { exact: true })).toBeVisible()
  await expect(historyEntries(page).last()).toContainText('Payment for painting')
  expect((await ana.view(id)).contributions).toEqual(
    expect.arrayContaining([expect.objectContaining({ id: payment, status: 'CLAIMED' })]),
  )

  // Ana says the fence is painted; Ben confirms receiving it, which is final.
  await ana.contribution(id, fence, 'CLAIM')
  await page.getByRole('link', { name: en.exchange.refresh }).click()
  await expect(page.getByText(en.contributionStatus.CLAIMED, { exact: true })).toBeVisible()
  await button(page, en.exchange.moves.CONFIRM).click()
  await expect(page.getByText(en.exchange.moveText.CONFIRM)).toBeVisible()
  await button(page, en.exchange.moves.CONFIRM).last().click()
  await expect(page.getByText(en.contributionStatus.ACCEPTED, { exact: true })).toBeVisible()

  // Ana confirms the payment: everything is delivered and the exchange completes.
  await ana.contribution(id, payment, 'CONFIRM')
  await page.getByRole('link', { name: en.exchange.refresh }).click()
  await expect(page.getByText(en.outcomes.COMPLETED, { exact: true }).first()).toBeVisible()
})
