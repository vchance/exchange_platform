import { codeFrom } from './support/codes'
import { expect, test } from './support/fixtures'
import { agree, agreedItem, stateTag } from './support/flows'
import { en, fill } from './support/wording'

const TABLE = 'An oak table'

test('a deleted account is signed out, and the other party keeps the exchange and its record', async ({
  person,
}) => {
  const ana = await person('Ana')
  const bruno = await person('Bruno')
  const id = await agree(ana, bruno, [{ from: 'me', kind: 'ITEM', description: TABLE }])

  // Bruno deletes his account, proving it with a code sent for that alone.
  const { page } = bruno
  await page.getByRole('link', { name: en.nav.account, exact: true }).click()
  const deletion = page.getByRole('region', { name: en.deletion.heading, exact: true })
  await deletion.getByRole('button', { name: en.deletion.open }).click()
  const explained = deletion.getByRole('group', { name: en.deletion.intro })
  await expect(explained.getByText(en.deletion.agreementsStand)).toBeVisible()
  const code = await codeFrom(bruno.email, 'delete-account', () =>
    explained.getByRole('button', { name: en.deletion.sendCode }).click(),
  )
  await deletion.getByLabel(en.signIn.codeLabel).fill(code)
  await deletion.getByRole('button', { name: en.deletion.continue, exact: true }).click()
  await deletion
    .getByRole('group', { name: en.deletion.confirmHeading })
    .getByRole('button', { name: en.deletion.confirm })
    .click()

  // He is signed out, and told so.
  await expect(page.getByText(en.deletion.deleted)).toBeVisible()
  await expect(page.getByRole('heading', { name: en.signIn.title, level: 1 })).toBeVisible()
  await page.goto(`/exchanges/${id}`)
  await expect(page.getByRole('heading', { name: en.signIn.title, level: 1 })).toBeVisible()

  // Ana still has the exchange, is told he has left, and has the record.
  await ana.page.reload()
  await expect(stateTag(ana.page)).toHaveText(en.states.ACTIVE)
  await expect(
    ana.page.getByText(fill(en.deletion.otherPartyLeftActive, { name: bruno.name })),
  ).toBeVisible()
  await expect(agreedItem(ana.page, TABLE)).toBeVisible()
  await ana.page.getByRole('link', { name: en.record.open }).click()
  await expect(ana.page.getByRole('heading', { level: 1 })).toHaveText(/^Record /)
  await expect(ana.page.getByText(TABLE, { exact: true }).first()).toBeVisible()
  await expect(
    ana.page.getByText(new RegExp(`^${fill(en.record.versionSignedBy, { name: bruno.name, date: '.+' })}$`)),
  ).toBeVisible()
})
