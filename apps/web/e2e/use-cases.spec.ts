import type { Page } from '@playwright/test'

import { expect, test } from './support/fixtures'
import {
  acceptOpen,
  agree,
  agreedItem,
  agreement,
  composerItem,
  move,
  negotiate,
  reviewAndSend,
  stateTag,
  type ItemSpec,
} from './support/flows'
import { en, fill } from './support/wording'

/*
 * What people meet when they need the record, when an agreement isn't
 * working, and when they are asked to sign a change: the plain summary at the
 * top of the record and its PDF, the guide that leads to the right way out,
 * and what a proposal changes, shown to the person who has to sign it.
 */

const LAMP = 'A brass desk lamp'
const PAYMENT = 'Payment for the lamp'
const ITEMS: ItemSpec[] = [
  { from: 'me', kind: 'ITEM', description: LAMP },
  { from: 'them', kind: 'MONEY', description: PAYMENT, amount: '40' },
]

/** The plain summary at the top of the record. */
function summary(page: Page) {
  return page.getByRole('region', { name: en.record.summary.heading, exact: true })
}

/** The guide for when something isn't working. */
function guide(page: Page) {
  return page.getByRole('group', { name: en.trouble.open, exact: true })
}

test('the record opens with a plain summary, and saves as a PDF of the summary and the record without the page around it', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)
  await move(ana.page, LAMP, en.exchange.moves.CLAIM)
  await bruno.page.reload()
  await move(bruno.page, LAMP, en.exchange.moves.CONFIRM)

  await bruno.page.getByRole('link', { name: en.record.open }).click()
  const plain = summary(bruno.page)
  await expect(plain).toBeVisible()
  await expect(plain).toContainText(`Between ${ana.name} and ${bruno.name}.`)
  await expect(plain).toContainText(fill(en.record.summary.basisAgreement, { number: 1 }))
  await expect(plain.getByRole('heading', { name: `What ${ana.name} agreed to give` })).toBeVisible()
  await expect(plain).toContainText(`Delivered, and ${bruno.name} confirmed receiving it.`)
  await expect(plain).toContainText(en.record.summary.moneyOutcome.OUTSTANDING)
  await expect(plain).toContainText(fill(en.terms.amount, { amount: '$40.00' }))
  await expect(plain).toContainText(new RegExp(`${ana.name} signed it on .+\\.`))
  await expect(plain).toContainText(new RegExp(`${bruno.name} signed it on .+\\.`))
  await expect(plain).toContainText(en.record.summary.standing.ACTIVE)

  // It comes first; the full detail is still all there under it.
  const headings = await bruno.page.locator('main h2').allTextContents()
  expect(headings[0]).toBe(en.record.summary.heading)
  expect(headings).toContain(en.record.summaryHeading)
  expect(headings).toContain(fill(en.record.versionHeading, { number: 1 }))
  expect(headings).toContain(en.record.eventsHeading)

  // One action: the browser's print window, which saves a PDF.
  await bruno.page.evaluate(() => {
    const flag = window as unknown as { printed: number }
    flag.printed = 0
    window.print = () => {
      flag.printed += 1
    }
  })
  await bruno.page.getByRole('button', { name: en.record.summary.savePdf, exact: true }).click()
  expect(await bruno.page.evaluate(() => (window as unknown as { printed: number }).printed)).toBe(1)

  // On paper: the summary and the record, and nothing of the app around them.
  await bruno.page.emulateMedia({ media: 'print' })
  await expect(plain).toBeVisible()
  await expect(bruno.page.getByRole('heading', { name: en.record.eventsHeading })).toBeVisible()
  await expect(bruno.page.locator('header.site')).toBeHidden()
  await expect(bruno.page.getByRole('button')).toHaveCount(0)
  await expect(bruno.page.getByRole('link', { name: en.record.back })).toBeHidden()
  await bruno.page.emulateMedia({ media: 'screen' })
})

test('the summary of an exchange ended by agreement says who proposed it and what was released', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)

  // Both want to stop: the guide leads Ana to proposing it, and Bruno agrees.
  await agreement(ana.page).getByRole('button', { name: en.trouble.open }).click()
  await guide(ana.page).getByRole('button', { name: en.trouble.situations.BOTH_STOP }).click()
  await expect(guide(ana.page)).toContainText(en.trouble.explain.BOTH_STOP)
  await expect(guide(ana.page)).toContainText(fill(en.trouble.means.PROPOSE_END, { name: bruno.name }))
  await guide(ana.page).getByRole('button', { name: en.exchange.proposeEnd }).click()
  const proposing = ana.page.getByRole('group', { name: en.exchange.proposeEnd, exact: true })
  await proposing.getByRole('button', { name: en.exchange.sendEndProposal }).click()
  await expect(proposing).toBeHidden()

  await bruno.page.reload()
  const ending = bruno.page.getByRole('region', { name: en.exchange.endingHeading, exact: true })
  await ending.getByRole('button', { name: en.exchange.agreeEnd }).click()
  await bruno.page
    .getByRole('group', { name: en.exchange.agreeEnd, exact: true })
    .getByRole('button', { name: en.exchange.confirmAgreeEnd })
    .click()
  await expect(stateTag(bruno.page)).toHaveText(en.outcomes.ENDED_BY_AGREEMENT)

  await bruno.page.getByRole('link', { name: en.record.open }).click()
  const plain = summary(bruno.page)
  await expect(plain).toContainText(/It ended by agreement on .+\./)
  await expect(plain).toContainText(`${ana.name} proposed ending it and ${bruno.name} agreed.`)
  await expect(plain).toContainText(en.record.summary.outcome.WAIVED_BY_ENDING)
  await expect(plain).toContainText(en.record.summary.moneyOutcome.WAIVED_BY_ENDING)
})

test('“Something isn’t working” leads to the existing action, which still asks to confirm', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)

  // Bruno has not paid; Ana decides to let him off it.
  await agreement(ana.page).getByRole('button', { name: en.trouble.open }).click()
  await expect(guide(ana.page)).toBeFocused()
  await expect(guide(ana.page)).toContainText(en.trouble.question)
  await guide(ana.page)
    .getByRole('button', { name: fill(en.trouble.situations.THEY_HAVENT, { name: bruno.name }) })
    .click()
  await expect(guide(ana.page)).toContainText(fill(en.trouble.means.WAIVE, { name: bruno.name }))
  await expect(guide(ana.page)).toContainText(fill(en.trouble.means.REQUEST_CLOSE, { name: bruno.name }))
  await guide(ana.page).getByRole('button', { name: en.exchange.moneyMoves.WAIVE }).click()

  // The guide has given way to the item's own panel; nothing was sent yet.
  await expect(guide(ana.page)).toBeHidden()
  const payment = agreedItem(ana.page, PAYMENT)
  const waiving = payment.getByRole('group', { name: en.exchange.moneyMoves.WAIVE, exact: true })
  await expect(waiving).toBeFocused()
  await expect(payment.locator('.status')).toHaveText(en.moneyStatus.PENDING)
  await waiving.getByRole('button', { name: en.exchange.moneyMoves.WAIVE, exact: true }).click()
  await expect(payment.locator('.status')).toHaveText(en.moneyStatus.WAIVED)
})

test('a dispute says it is recorded, not decided, when it is opened and when it is seen', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)
  await move(ana.page, LAMP, en.exchange.moves.CLAIM)

  await bruno.page.reload()
  const lamp = agreedItem(bruno.page, LAMP)
  await lamp.getByRole('button', { name: en.exchange.moves.DISPUTE, exact: true }).click()
  const disputing = lamp.getByRole('group', { name: en.exchange.moves.DISPUTE, exact: true })
  await expect(disputing).toContainText(en.dispute.weRecord)
  await disputing.getByRole('textbox').fill('The switch does not work.')
  await disputing.getByRole('button', { name: en.exchange.moves.DISPUTE, exact: true }).click()
  await expect(lamp.locator('.status')).toHaveText(en.contributionStatus.DISPUTED)

  await ana.page.reload()
  const seen = agreedItem(ana.page, LAMP)
  await expect(seen).toContainText(en.dispute.weRecord)
  await expect(seen).toContainText(en.dispute.pointer)
  await seen.getByRole('button', { name: en.trouble.open }).click()
  await expect(guide(ana.page)).toContainText(en.trouble.explain.DISAGREE)
  await expect(guide(ana.page)).toContainText(fill(en.trouble.means.RECLAIM, { name: bruno.name }))
})

test('the person asked to sign an amendment sees what it changes and where each item will stand', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)
  await move(ana.page, LAMP, en.exchange.moves.CLAIM)

  // Ana raises the payment and adds a shade.
  await agreement(ana.page).getByRole('link', { name: en.exchange.amend }).click()
  await composerItem(ana.page, 2).getByLabel(/^Amount in /).fill('55')
  await ana.page.getByRole('button', { name: en.composer.addYours }).click()
  const shade = composerItem(ana.page, 3)
  await shade.getByLabel(en.composer.typeLabel).selectOption('ITEM')
  await shade.getByLabel(en.composer.descriptionLabel).fill('A green glass shade')
  await reviewAndSend(ana.page)

  await bruno.page.reload()
  const proposal = bruno.page.getByRole('region', { name: en.exchange.amendmentHeading, exact: true })
  const changes = proposal.getByRole('region', { name: en.proposalChanges.heading, exact: true })
  await expect(changes).toContainText(fill(en.proposalChanges.againstInForce, { number: 1 }))
  const item = (description: string) =>
    changes.locator('li').filter({ has: bruno.page.getByText(description, { exact: true }) })
  await expect(item(PAYMENT)).toContainText(en.proposalChanges.kinds.CHANGED)
  await expect(item(PAYMENT)).toContainText('Amount: was $40.00, now $55.00.')
  await expect(item(PAYMENT)).toContainText(
    fill(en.proposalChanges.statusAfter, { status: en.moneyStatus.PENDING }),
  )
  // The lamp is untouched, so it stays marked delivered.
  await expect(item(LAMP)).toContainText(en.proposalChanges.kinds.UNCHANGED)
  await expect(item(LAMP)).toContainText(
    fill(en.proposalChanges.statusAfter, { status: en.contributionStatus.CLAIMED }),
  )
  await expect(item('A green glass shade')).toContainText(en.proposalChanges.kinds.ADDED)

  // It comes before the way to sign.
  const order = await proposal.evaluate((section, heading) => {
    const shown = [...section.querySelectorAll('h3, button')].map((node) => node.textContent)
    return [shown.indexOf(heading), shown.findIndex((text) => text === 'Sign and accept')]
  }, en.proposalChanges.heading)
  expect(order[0]).toBeGreaterThanOrEqual(0)
  expect(order[0]).toBeLessThan(order[1])

  // The author does not see it: they wrote it.
  await ana.page.reload()
  await expect(ana.page.getByRole('region', { name: en.proposalChanges.heading })).toHaveCount(0)
  await acceptOpen(bruno)
})

test('the person asked to sign a counteroffer sees what changed since the version it answers', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await negotiate(ana, bruno, ITEMS)

  await bruno.page
    .getByRole('region', { name: en.exchange.proposalHeading, exact: true })
    .getByRole('link', { name: en.exchange.counter })
    .click()
  await composerItem(bruno.page, 2).getByLabel(/^Amount in /).fill('30')
  await composerItem(bruno.page, 1)
    .getByLabel(en.composer.descriptionLabel)
    .fill('A brass desk lamp, with its bulb')
  await reviewAndSend(bruno.page)

  await ana.page.reload()
  const changes = ana.page.getByRole('region', { name: en.proposalChanges.heading, exact: true })
  await expect(changes).toContainText(fill(en.proposalChanges.againstPrevious, { number: 1 }))
  await expect(changes).toContainText('Amount: was $40.00, now $30.00.')
  // A description is the parties' own words: shown before and after, as theirs.
  await expect(changes.getByText(en.proposalChanges.fields.description)).toBeVisible()
  await expect(changes.locator('bdi', { hasText: LAMP }).first()).toBeVisible()
  await expect(changes.locator('bdi', { hasText: 'A brass desk lamp, with its bulb' })).toBeVisible()
  await acceptOpen(ana)
  await expect(stateTag(ana.page)).toHaveText(en.states.ACTIVE)
})
