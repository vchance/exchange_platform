import { ApiPerson } from './support/api'
import { expect, test } from './support/fixtures'
import { acceptOpen, button, join, title } from './support/flows'
import { en, fill } from './support/wording'

const name = { name: 'Ana' }

test('from an exchange, a person reports it and blocks the other party, and can unblock them later', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { id, link } = await ana.propose('Ben', [
    { from: 'A', kind: 'ITEM', description: 'A bicycle' },
    { from: 'B', kind: 'MONEY', description: 'Payment for the bicycle', amountMinor: 12000 },
  ])
  await join(ben, link)
  await acceptOpen(page)
  await ana.confirmCounterparty(id)
  await page.reload()
  await expect(page.getByRole('heading', { name: en.safety.heading })).toBeVisible()

  // A report needs a reason; "something else" needs words too.
  await button(page, en.safety.report).click()
  await expect(page.getByText(fill(en.safety.reportIntro, name))).toBeVisible()
  await button(page, en.safety.sendReport).click()
  await expect(page.getByText(en.safety.reasonRequired)).toBeVisible()
  await page.getByRole('radio', { name: en.safety.reasons.OTHER }).click()
  await button(page, en.safety.sendReport).click()
  await expect(page.getByText(en.safety.detailsRequired)).toBeVisible()
  await page.getByRole('radio', { name: en.safety.reasons.SCAM }).click()
  await page
    .getByLabel(en.safety.detailsLabel, { exact: true })
    .fill('Asked me to pay a deposit by gift card.')
  await button(page, en.safety.sendReport).click()
  await expect(page.getByText(en.safety.reportSent)).toBeVisible()

  // Blocking says what it does first, and leaves the agreement in force.
  await button(page, fill(en.safety.block, name)).click()
  await expect(page.getByText(en.safety.blockKeeps)).toBeVisible()
  await expect(page.getByText(fill(en.safety.blockQuiet, name))).toBeVisible()
  await button(page, fill(en.safety.confirmBlock, name)).click()
  await expect(page.getByText(fill(en.safety.blocked, name)).first()).toBeVisible()
  await expect(button(page, fill(en.safety.unblock, name))).toBeVisible()
  expect((await ana.view(id)).state).toBe('ACTIVE')
  await expect(page.getByText(en.states.ACTIVE, { exact: true })).toBeVisible()

  // The account screen lists who is blocked, and unblocks from there too.
  await page.goto('/')
  await page.getByRole('link', { name: en.nav.account }).click()
  await expect(title(page, en.nav.account)).toBeVisible()
  await expect(page.getByText(en.safety.blockedIntro)).toBeVisible()
  await button(page, fill(en.safety.unblock, name)).click()
  await expect(page.getByText(fill(en.safety.unblocked, name))).toBeVisible()
  await expect(page.getByText(en.safety.blockedEmpty)).toBeVisible()
})

test('blocking the sender of a proposal waiting to be signed declines it', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { id, link } = await ana.propose('Ben', [
    { from: 'A', kind: 'ITEM', description: 'A sofa' },
  ])
  await join(ben, link)
  await ana.confirmCounterparty(id)
  await page.reload()

  await button(page, fill(en.safety.block, name)).click()
  await expect(page.getByText(en.safety.blockEnds)).toBeVisible()
  await button(page, fill(en.safety.confirmBlock, name)).click()
  await expect(page.getByText(fill(en.safety.blocked, name)).first()).toBeVisible()

  const seen = await ana.view(id)
  expect(seen.state).toBe('CLOSED')
  expect(seen.closed_outcome).toBe('NOT_AGREED')
  await expect(page.getByText(en.outcomes.NOT_AGREED, { exact: true })).toBeVisible()
})

test('a proposal can be reported from the invitation, before signing in', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const ben = await person('Ben')
  const { page } = ben
  const { link } = await ana.propose('Ben', [{ from: 'A', kind: 'ITEM', description: 'A watch' }])

  await page.goto(link)
  await expect(title(page, en.invitation.title)).toBeVisible()
  await button(page, en.safety.reportProposal).click()
  await expect(page.getByText(en.safety.reportProposalIntro)).toBeVisible()
  await page.getByRole('radio', { name: en.safety.reasons.UNWANTED }).click()
  await button(page, en.safety.sendReport).click()
  await expect(page.getByText(en.safety.reportSent)).toBeVisible()
  // Nobody was signed in, and nobody is now.
  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
})
