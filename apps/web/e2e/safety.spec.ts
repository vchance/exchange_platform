import { expect, test } from './support/fixtures'
import { negotiate, stateTag } from './support/flows'
import { en, fill } from './support/wording'

test('blocking the other party from an exchange lists them on the account, until they are unblocked', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  const id = await negotiate(ana, bruno, [
    { from: 'me', kind: 'ITEM', description: 'A box of records' },
  ])
  const { page } = ana
  const name = { name: bruno.name }

  const safety = page.getByRole('region', { name: en.safety.heading, exact: true })
  await safety.getByRole('button', { name: fill(en.safety.block, name) }).click()
  const panel = safety.getByRole('group', { name: fill(en.safety.block, name) })
  await expect(panel.getByText(fill(en.safety.blockQuiet, name))).toBeVisible()
  await panel.getByRole('button', { name: fill(en.safety.confirmBlock, name) }).click()
  await expect(safety.getByText(fill(en.safety.blocked, name))).toBeVisible()
  await expect(safety.getByRole('button', { name: fill(en.safety.unblock, name) })).toBeVisible()

  // The proposal that was waiting between them has ended.
  await page.reload()
  await expect(stateTag(page)).toHaveText(en.outcomes.NOT_AGREED)

  // The account lists him, with the exchange he was blocked from.
  await page.getByRole('link', { name: en.nav.account, exact: true }).click()
  const blocked = page.getByRole('region', { name: en.safety.blockedHeading, exact: true })
  const entry = blocked.locator('li.card')
  await expect(entry).toHaveCount(1)
  await expect(entry.getByRole('link', { name: bruno.name })).toHaveAttribute(
    'href',
    `/exchanges/${id}`,
  )

  await entry.getByRole('button', { name: fill(en.safety.unblock, name) }).click()
  await expect(blocked.getByText(fill(en.safety.unblocked, name))).toBeVisible()
  await expect(blocked.locator('li.card')).toHaveCount(0)
  await page.reload()
  await expect(
    page
      .getByRole('region', { name: en.safety.blockedHeading, exact: true })
      .getByText(en.safety.blockedEmpty),
  ).toBeVisible()
})
