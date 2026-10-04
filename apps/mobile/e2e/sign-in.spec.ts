import { expect, test } from './support/fixtures'
import { button, signIn, title } from './support/flows'
import { en, fill } from './support/wording'

/*
 * Signing in asks for what the service can send codes to (README, "Signing
 * in"). The API here writes codes to its log, phone codes included, so it
 * offers both; a deployment without text messages is stood in for by
 * answering `GET /v1/meta` in the browser as such a service would.
 */

test('the service offers phone numbers here, and the screen asks for them with the countries', async ({
  person,
}) => {
  const ana = await person('Ana')
  const { page } = ana

  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
  await expect(page.getByLabel(en.signIn.identifierLabel)).toBeVisible()
  await expect(
    page.getByText(fill(en.signIn.identifierHintCountries, { codes: '+1' })),
  ).toBeVisible()
  await signIn(ana)
  await expect(page.getByText(en.profile.firstIntro)).toBeVisible()
})

test('where the service has no text messages, an email address is asked for, and a phone number stopped', async ({
  person,
}) => {
  const ben = await person('Ben')
  const { page } = ben
  await page.route('**/v1/meta', async (route) => {
    const response = await route.fetch()
    const meta = (await response.json()) as Record<string, unknown>
    await route.fulfill({
      response,
      json: { ...meta, sign_in_channels: ['email'], sms_country_codes: [] },
    })
  })
  const codesAsked: string[] = []
  page.on('request', (request) => {
    if (request.url().endsWith('/v1/auth/codes')) codesAsked.push(request.url())
  })

  await page.goto('/')
  await expect(page.getByText(en.signIn.introEmail)).toBeVisible()
  const email = page.getByLabel(en.signIn.emailLabel, { exact: true })
  await expect(email).toBeVisible()
  await expect(page.getByLabel(en.signIn.identifierLabel)).toHaveCount(0)

  await email.fill('+1 202 555 0142')
  await button(page, en.signIn.sendCode).click()
  await expect(page.getByText(en.signIn.emailOnly)).toBeVisible()
  await expect(page.getByText(en.errors.SERVICE_UNAVAILABLE)).toHaveCount(0)
  expect(codesAsked).toEqual([])

  // An email address signs in as ever.
  await signIn(ben, en, en.signIn.emailLabel)
  await expect(page.getByText(en.profile.firstIntro)).toBeVisible()
})
