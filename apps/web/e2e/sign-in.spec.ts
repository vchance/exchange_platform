import { expect, test } from './support/fixtures'
import { signIn } from './support/flows'
import { en, fill } from './support/wording'

/*
 * Signing in asks for what the service can send codes to (README, "Signing
 * in"). The API here writes codes to its log, phone codes included, so it
 * offers both; a deployment without text messages is stood in for by
 * answering `GET /v1/meta` in the browser as such a service would.
 */

test('the service offers phone numbers here, and the form asks for them with the countries', async ({
  person,
}) => {
  const ana = await person('Ana')
  const { page } = ana
  const meta = (await (await page.request.get('/v1/meta')).json()) as {
    sign_in_channels: string[]
    sms_country_codes: string[]
  }
  expect(meta.sign_in_channels).toEqual(['email', 'phone'])
  expect(meta.sms_country_codes).toEqual(['+1'])

  await page.goto('/')
  await expect(page.getByLabel(en.signIn.identifierLabel)).toBeVisible()
  await expect(page.getByText(fill(en.signIn.identifierHintCountries, { codes: '+1' }))).toBeVisible()
  await signIn(ana)
  await expect(page.getByRole('heading', { name: en.profile.firstTitle })).toBeVisible()
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
  await expect(email).toHaveAttribute('type', 'email')
  await expect(page.getByLabel(en.signIn.identifierLabel)).toHaveCount(0)

  await email.fill('+1 202 555 0142')
  await page.getByRole('button', { name: en.signIn.sendCode, exact: true }).click()
  await expect(page.getByText(en.signIn.emailOnly)).toBeVisible()
  await expect(email).toHaveAttribute('aria-invalid', 'true')
  await expect(page.getByText(en.errors.SERVICE_UNAVAILABLE)).toHaveCount(0)
  expect(codesAsked).toEqual([])

  // An email address signs in as ever.
  await page.getByLabel(en.signIn.emailLabel, { exact: true }).fill(ben.email)
  await expect(page.getByText(en.signIn.emailOnly)).toHaveCount(0)
  await signIn(ben, en, en.signIn.emailLabel)
  await expect(page.getByRole('heading', { name: en.profile.firstTitle })).toBeVisible()
})
