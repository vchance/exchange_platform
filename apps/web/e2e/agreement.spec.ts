import { expect, test } from './support/fixtures'
import {
  acceptOpen,
  addItems,
  agree,
  agreedItem,
  agreement,
  composerItem,
  history,
  move,
  stateTag,
  type ItemSpec,
} from './support/flows'
import { en, fill } from './support/wording'

/*
 * What happens to an agreement once it is in force: changing it, disputing
 * a delivery, and asking to close it without agreement.
 */

const BICYCLE = 'A blue bicycle, 54 cm frame'
const PAYMENT = 'Payment for the bicycle'
const ITEMS: ItemSpec[] = [
  { from: 'me', kind: 'ITEM', description: BICYCLE },
  { from: 'them', kind: 'MONEY', description: PAYMENT, amount: '120' },
]

test('an amendment shows what it does to each item, and once both sign, the changed item starts again', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)

  // The bicycle has been marked delivered, so changing it will undo that.
  await move(ana.page, BICYCLE, en.exchange.moves.CLAIM)

  const REPAIRED = 'A blue bicycle, 54 cm frame, with a new chain'
  const DELIVERY = 'Delivery to Bruno’s door'
  await agreement(ana.page).getByRole('link', { name: en.exchange.amend }).click()
  await expect(
    ana.page.getByRole('heading', { name: en.composer.titleAmend, level: 1 }),
  ).toBeVisible()

  // As it is written, each item says what the change will do to it.
  const bicycle = composerItem(ana.page, 1)
  const payment = composerItem(ana.page, 2)
  await expect(bicycle.getByText(en.composer.effects.UNTOUCHED)).toBeVisible()
  await bicycle.getByLabel(en.composer.descriptionLabel).fill(REPAIRED)
  await expect(bicycle.getByText(en.composer.effects.CHANGED)).toBeVisible()
  await expect(payment.getByText(en.composer.effects.UNTOUCHED)).toBeVisible()
  await addItems(ana.page, [{ from: 'me', kind: 'TASK', description: DELIVERY }])
  await expect(composerItem(ana.page, 3).getByText(en.composer.effects.NEW)).toBeVisible()

  // The signing step sums it up before anything is signed.
  await ana.page.getByRole('button', { name: en.composer.review, exact: true }).click()
  const effects = ana.page.getByRole('region', { name: en.composer.effectsHeading })
  await expect(effects).toBeVisible()
  const effectOf = (description: string) =>
    effects.locator('li').filter({ has: ana.page.getByText(description, { exact: true }) })
  await expect(effectOf(REPAIRED)).toContainText(en.composer.effects.CHANGED)
  await expect(effectOf(PAYMENT)).toContainText(en.composer.effects.UNTOUCHED)
  await expect(effectOf(DELIVERY)).toContainText(en.composer.effects.NEW)
  await ana.page.getByLabel(en.consent.agree).check()
  await ana.page.getByRole('button', { name: en.composer.signAndSend, exact: true }).click()

  // Until Bruno signs, the agreement stands as it was.
  await expect(
    ana.page.getByRole('region', { name: en.exchange.amendmentHeading, exact: true }),
  ).toBeVisible()
  await expect(agreedItem(ana.page, BICYCLE).locator('.status')).toHaveText(
    en.contributionStatus.CLAIMED,
  )

  await bruno.page.reload()
  await acceptOpen(bruno)

  // The changed item is back to the start, the new one has started, the
  // untouched one is as it was.
  for (const { page } of [bruno, ana]) {
    await page.reload()
    await expect(stateTag(page)).toHaveText(en.states.ACTIVE)
    await expect(agreedItem(page, BICYCLE)).toHaveCount(0)
    await expect(agreedItem(page, REPAIRED).locator('.status')).toHaveText(
      en.contributionStatus.PENDING,
    )
    await expect(agreedItem(page, DELIVERY).locator('.status')).toHaveText(
      en.contributionStatus.PENDING,
    )
    await expect(agreedItem(page, PAYMENT).locator('.status')).toHaveText(en.moneyStatus.PENDING)
    await expect(
      history(page).getByText(fill(en.record.events.neutral.AGREEMENT_IN_FORCE, { number: 2 })),
    ).toBeVisible()
  }
})

test('a disputed delivery is marked delivered again and then confirmed', async ({ person }) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)

  await move(ana.page, BICYCLE, en.exchange.moves.CLAIM)
  await bruno.page.reload()
  await move(bruno.page, BICYCLE, en.exchange.moves.DISPUTE, 'The front brake does not work.')
  await expect(agreedItem(bruno.page, BICYCLE).locator('.status')).toHaveText(
    en.contributionStatus.DISPUTED,
  )

  await ana.page.reload()
  await expect(agreedItem(ana.page, BICYCLE).locator('.status')).toHaveText(
    en.contributionStatus.DISPUTED,
  )
  await expect(history(ana.page).getByText('The front brake does not work.')).toBeVisible()
  await move(ana.page, BICYCLE, en.exchange.moves.RECLAIM, 'Brake cable replaced.')
  await expect(agreedItem(ana.page, BICYCLE).locator('.status')).toHaveText(
    en.contributionStatus.CLAIMED,
  )

  await bruno.page.reload()
  await expect(history(bruno.page).getByText('Brake cable replaced.')).toBeVisible()
  await move(bruno.page, BICYCLE, en.exchange.moves.CONFIRM)
  await expect(agreedItem(bruno.page, BICYCLE).locator('.status')).toHaveText(
    en.contributionStatus.ACCEPTED,
  )
  // The payment is still outstanding, so the exchange goes on.
  await expect(stateTag(bruno.page)).toHaveText(en.states.ACTIVE)
})

/*
 * Closing without agreement. The other party cannot accept a close request:
 * it is not an offer to release anything (DESIGN.md §5.3). They can answer it
 * with a statement, settle what is outstanding or propose ending by
 * agreement, and if nothing changes the worker closes the exchange as
 * unresolved once the window has run out, which is days. That last step is
 * not tested here.
 */
test('a request to close without agreement shows both parties when it lapses, and the other party can answer it', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  await agree(ana, bruno, ITEMS)

  const ending = (page: typeof ana.page) =>
    page.getByRole('region', { name: en.exchange.endingHeading, exact: true })

  await ending(ana.page).getByRole('button', { name: en.exchange.requestClose }).click()
  const panel = ending(ana.page).getByRole('group', { name: en.exchange.requestClose })
  await expect(panel.getByText(fill(en.exchange.requestCloseText, { name: bruno.name }))).toBeVisible()
  await panel.getByLabel(en.exchange.statementLabel).fill('The bicycle was never collected.')
  await panel.getByRole('button', { name: en.exchange.sendCloseRequest }).click()

  const lapses = new RegExp(`^${fill(en.exchange.closeRequestLapses, { date: '(.+)' })}$`)
  await expect(ending(ana.page).getByText(lapses)).toBeVisible()
  const anaSees = await ending(ana.page).getByText(lapses).textContent()

  // Bruno is told who asked and when it will close, the same date Ana sees.
  await bruno.page.reload()
  await expect(
    ending(bruno.page).getByText(
      new RegExp(`^${fill(en.exchange.closeRequestedByOther, { name: ana.name, date: '.+' })}$`),
    ),
  ).toBeVisible()
  await expect(ending(bruno.page).getByText(lapses)).toHaveText(anaSees!)
  await expect(history(bruno.page).getByText('The bicycle was never collected.')).toBeVisible()

  // He answers with a statement of his own, for the record.
  await ending(bruno.page).getByRole('button', { name: en.exchange.addStatement }).click()
  const statement = ending(bruno.page).getByRole('group', { name: en.exchange.addStatement })
  await statement.getByLabel(en.exchange.statementRequiredLabel).fill('I was away; I can collect it on Monday.')
  await statement.getByRole('button', { name: en.exchange.sendStatement }).click()
  await expect(ending(bruno.page).getByText(en.exchange.statementAdded)).toBeVisible()
  await expect(
    history(bruno.page).getByText('I was away; I can collect it on Monday.'),
  ).toBeVisible()

  // Nothing closes until the window runs out.
  await ana.page.reload()
  await expect(stateTag(ana.page)).toHaveText(en.states.ACTIVE)
  await expect(
    history(ana.page).getByText(fill(en.record.events.named.STATEMENT_ADDED, { name: bruno.name })),
  ).toBeVisible()
})
