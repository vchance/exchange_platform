import { readFile } from 'node:fs/promises'

import { ApiError, ApiPerson } from './support/api'
import { expect, test, type Fixtures } from './support/fixtures'
import { acceptOpen, button, historyEntries, join, move, title } from './support/flows'
import { en, fill } from './support/wording'

/** Two people with an agreement in force: Ana through the API, Ben in the app. */
async function agreement(person: Fixtures['person'], ana: ApiPerson) {
  const ben = await person('Ben')
  const proposal = await ana.propose(
    'Ben',
    [
      { from: 'A', kind: 'SERVICE', description: 'Paint the fence' },
      { from: 'B', kind: 'MONEY', description: 'Payment for painting', amountMinor: 5000 },
    ],
    { note: 'As we discussed.' },
  )
  await join(ben, proposal.link)
  await acceptOpen(ben.page)
  await ana.confirmCounterparty(proposal.id)
  await ben.page.reload()
  await expect(ben.page.getByText(en.states.ACTIVE, { exact: true })).toBeVisible()
  return { ben, ...proposal }
}

test('the history shows the latest entries first to hand, and earlier ones on request', async ({
  person,
  email,
}) => {
  // A party may change one exchange only so often a minute, and a page of
  // history is 50 entries, so getting past one page takes a little over a minute.
  test.setTimeout(240_000)
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const {
    ben,
    id,
    contributionIds: [fence, payment],
  } = await agreement(person, ana)
  const { page } = ben
  const benElsewhere = await ApiPerson.signUp('Ben', ben.email)

  // Each in turn marks their item delivered and takes it back, again and
  // again, until the history is longer than a page. When the limit says
  // wait, they wait.
  const step = async (who: ApiPerson, contribution: string, n: number) => {
    const action = n % 2 === 0 ? 'CLAIM' : 'RETRACT_CLAIM'
    for (;;) {
      try {
        await who.command(id, { type: 'CONTRIBUTION', contribution, action })
        return
      } catch (error) {
        if (!(error instanceof ApiError && error.code === 'TOO_MANY_REQUESTS')) throw error
        await new Promise((settle) => setTimeout(settle, 2_000))
      }
    }
  }
  // Five entries already; 52 more makes 57, seven before the latest 50.
  for (let n = 0; n < 26; n += 1) {
    await step(ana, fence, n)
    await step(benElsewhere, payment, n)
  }

  await page.reload()
  await expect(page.getByRole('heading', { name: en.record.historyHeading })).toBeVisible()
  await expect(historyEntries(page)).toHaveCount(50)
  // The first entry, Ana sending the proposal, is not among the latest.
  const first = historyEntries(page).filter({ hasText: /Ana sent version 1/ })
  await expect(first).toHaveCount(0)

  await button(page, en.record.historyEarlier).click()
  await expect(historyEntries(page)).toHaveCount(57)
  await expect(first).toHaveCount(1)
  await expect(first).toContainText('As we discussed.')
  // Nothing more to read.
  await expect(button(page, en.record.historyEarlier)).toBeHidden()
})

test('the record lays out the whole exchange, and its copy is a file to keep', async ({
  person,
  email,
}) => {
  const ana = await ApiPerson.signUp('Ana', email('ana'))
  const {
    ben,
    id,
    contributionIds: [fence],
  } = await agreement(person, ana)
  const { page } = ben
  await ana.contribution(id, fence, 'CLAIM')
  await page.reload()
  await move(page, en.exchange.moves.CONFIRM)

  await button(page, en.record.open).click()
  await page.waitForURL(`**/exchanges/${id}/record`)
  const reference = (await ana.view(id)).display_code as string
  await expect(title(page, fill(en.record.title, { code: reference }))).toBeVisible()

  // The parts a person reads it for, from top to bottom.
  for (const heading of [
    en.record.summaryHeading,
    en.record.aboutHeading,
    en.record.itemsHeading,
    fill(en.record.versionHeading, { number: 1 }),
    en.record.signaturesHeading,
    en.record.eventsHeading,
  ]) {
    await expect(page.getByRole('heading', { name: heading })).toBeVisible()
  }
  await expect(page.getByRole('listitem').filter({ hasText: 'Signed by Ana' })).toBeVisible()
  await expect(page.getByRole('listitem').filter({ hasText: 'Signed by Ben' })).toBeVisible()
  await expect(page.getByText(en.terms.moneyOutside).filter({ visible: true }).first()).toBeVisible()
  await expect(
    page.getByRole('listitem').filter({ hasText: /Ben confirmed receiving this:\s*Paint the fence/ }),
  ).toBeVisible()

  // On a phone the copy goes to the share sheet; in the harness it is a
  // download, which shows what the file holds and nothing about the sheet.
  const [download] = await Promise.all([
    page.waitForEvent('download'),
    button(page, en.mobile.record.share).click(),
  ])
  expect(download.suggestedFilename()).toMatch(/\.json$/)
  const copy = JSON.parse(await readFile(await download.path(), 'utf8'))
  expect(copy).toMatchObject({ format: 'exchange-record', format_version: 2 })
  expect(copy.part?.complete).toBe(true)
  // It names the parties as the agreement does, and nothing that identifies their accounts.
  const text = JSON.stringify(copy)
  expect(text).toContain('Paint the fence')
  expect(text).not.toContain(ana.email)
  expect(text).not.toContain(ben.email)

  await page.getByRole('link', { name: en.record.back }).click()
  await page.waitForURL(`**/exchanges/${id}`)
})
