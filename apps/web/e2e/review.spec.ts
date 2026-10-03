import type { Page } from '@playwright/test'

import { expect, test, type Person } from './support/fixtures'
import { agree, signIn, signUp, stateTag } from './support/flows'
import { grantStaff, revokeStaff } from './support/staff'
import { en, fill } from './support/wording'

/*
 * Staff review of a report (DESIGN.md §9): a party reports the other from
 * their yup, the owner names a reviewer with the `staff` command, and the
 * reviewer finds the report in the queue, reads the yup and decides.
 */

const w = en.staff

/** The yup's reference, as its page shows it. */
async function referenceOf(page: Page): Promise<string> {
  const tag = page.locator('main .tags .tag').filter({ hasText: /^Reference / })
  return (await tag.textContent())!.replace('Reference ', '')
}

/** `reporter` reports the yup they are on, and with it the other party. */
async function report(reporter: Person, otherName: string, details: string): Promise<void> {
  const { page } = reporter
  const safety = page.getByRole('region', { name: en.safety.heading, exact: true })
  await safety.getByRole('button', { name: en.safety.report, exact: true }).click()
  const panel = safety.getByRole('group', { name: en.safety.report, exact: true })
  await expect(panel.getByText(fill(en.safety.reportIntro, { name: otherName }))).toBeVisible()
  await panel.getByLabel(en.safety.reasons.HARASSMENT).check()
  await panel.getByLabel(en.safety.detailsLabel).fill(details)
  await panel.getByRole('button', { name: en.safety.sendReport }).click()
  await expect(safety.getByText(en.safety.reportSent)).toBeVisible()
}

/** A new reviewer, signed up through the app and named with the command line, on the queue. */
async function reviewer(person: Person): Promise<void> {
  await signUp(person)
  grantStaff(person.email)
  await person.page.goto('/staff')
  await expect(person.page.getByRole('heading', { name: w.title, level: 1 })).toBeVisible()
}

/** Opens the report about the yup with this reference from the queue. */
async function openReport(page: Page, code: string): Promise<void> {
  const queue = page.getByRole('region', { name: w.queueHeading, exact: true })
  await queue.getByRole('link', { name: fill(w.reportLink, { code }), exact: true }).click()
  await expect(
    page.getByRole('heading', { name: fill(w.detailTitle, { code }), level: 1 }),
  ).toBeVisible()
}

test('a reviewer reads a report and dismisses it, and nobody else can open the queue', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  const rita = await person('Rita')
  await agree(ana, bruno, [{ from: 'me', kind: 'ITEM', description: 'A box of records' }])
  const code = await referenceOf(bruno.page)
  await report(bruno, ana.name, 'She keeps sending me messages after I said no.')

  // There is no way in for a party, and the address is not found.
  await expect(bruno.page.locator('a[href^="/staff"]')).toHaveCount(0)
  await bruno.page.goto('/staff')
  await expect(
    bruno.page.getByRole('heading', { name: en.common.notFoundTitle, level: 1 }),
  ).toBeVisible()

  await reviewer(rita)
  const { page } = rita
  await openReport(page, code)
  // What the reporter wrote, and the yup as recorded.
  await expect(page.getByText('She keeps sending me messages after I said no.')).toBeVisible()
  await expect(page.getByText('A box of records').first()).toBeVisible()
  const history = page.getByRole('region', { name: w.historyHeading, exact: true })
  await expect(history.getByText(w.actions.REPORT_VIEWED)).toBeVisible()

  const decision = page.getByRole('region', { name: w.decisionHeading, exact: true })
  await decision.getByRole('button', { name: w.outcomes.DISMISSED, exact: true }).click()
  const panel = decision.getByRole('group', { name: w.outcomes.DISMISSED, exact: true })
  await expect(panel.getByText(w.outcomeText.DISMISSED)).toBeVisible()
  await panel.getByLabel(w.noteOptionalLabel).fill('A disagreement, not abuse.')
  await panel.getByRole('button', { name: w.confirm, exact: true }).click()

  await expect(page.getByRole('heading', { name: w.title, level: 1 })).toBeVisible()
  await expect(page.getByText(w.outcomeDone.DISMISSED)).toBeVisible()
  await expect(page.getByRole('link', { name: fill(w.reportLink, { code }), exact: true })).toHaveCount(0)

  // Nothing changed for either party.
  await ana.page.reload()
  await expect(stateTag(ana.page)).toHaveText(en.states.ACTIVE)
  await expect(ana.page.getByText(en.exchange.contentHidden)).toHaveCount(0)
  revokeStaff(rita.email)
})

test('a reviewer suspends the person reported, who is signed out until it is lifted', async ({
  person,
}) => {
  // A name of its own, so that this person's suspension is told apart from others'.
  const ana = await person(`Ana ${Date.now() % 100_000}`)
  const bruno = await person('Bruno')
  const rita = await person('Rita')
  await agree(ana, bruno, [{ from: 'me', kind: 'ITEM', description: 'A box of records' }])
  const code = await referenceOf(bruno.page)
  await report(bruno, ana.name, 'Threats in the notes.')

  await reviewer(rita)
  const { page } = rita
  await openReport(page, code)
  const decision = page.getByRole('region', { name: w.decisionHeading, exact: true })
  await decision.getByRole('button', { name: w.outcomes.ACCOUNT_SUSPENDED, exact: true }).click()
  const panel = decision.getByRole('group', { name: w.outcomes.ACCOUNT_SUSPENDED, exact: true })
  // A suspension needs a note saying why.
  await panel.getByRole('button', { name: w.confirm, exact: true }).click()
  await expect(panel.getByText(w.noteRequired)).toBeVisible()
  await panel.getByLabel(w.noteLabel).fill('Threats, confirmed in the history.')
  await panel.getByRole('button', { name: w.confirm, exact: true }).click()
  await expect(page.getByText(w.outcomeDone.ACCOUNT_SUSPENDED)).toBeVisible()

  // Ana is signed out, and cannot sign in again.
  await ana.page.reload()
  await expect(ana.page.getByRole('heading', { name: en.signIn.title })).toBeVisible()
  await signIn(ana)
  await expect(ana.page.getByText(en.errors.ACCOUNT_SUSPENDED)).toBeVisible()

  // Bruno's yup with her is as it was.
  await bruno.page.reload()
  await expect(stateTag(bruno.page)).toHaveText(en.states.ACTIVE)

  // Listed under the suspensions, and lifted with a note.
  const suspended = page.getByRole('region', { name: w.suspensionsHeading, exact: true })
  const entry = suspended.locator('li.card').filter({ hasText: ana.name })
  await expect(entry.getByText('Threats, confirmed in the history.')).toBeVisible()
  await entry.getByRole('button', { name: w.lift, exact: true }).click()
  const lift = entry.getByRole('group', { name: w.lift, exact: true })
  await lift.getByLabel(w.noteLabel).fill('Appeal heard; warned instead.')
  await lift.getByRole('button', { name: w.lift, exact: true }).click()
  await expect(suspended.getByText(w.lifted)).toBeVisible()
  await expect(suspended.locator('li.card').filter({ hasText: ana.name })).toHaveCount(0)

  // She can sign in again.
  await ana.page.goto('/')
  await signIn(ana)
  await expect(ana.page.getByRole('heading', { name: en.home.title, level: 1 })).toBeVisible()
  revokeStaff(rita.email)
})
