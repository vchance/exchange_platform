import type { Locator, Page } from '@playwright/test'

import { codeFrom } from './codes'
import { expect, type Person } from './fixtures'
import { en } from './wording'

/*
 * The steps most tests go through on the way to what they are about, done
 * the way a person does them on the app's screens, in English unless said
 * otherwise.
 *
 * On the web, React Native's controls come out as ordinary elements with
 * their roles and labels, so the app is found by the words a person sees and
 * hears, as on a phone. A switch is a checkbox labelled with the switch's own
 * label, inside an element that repeats it; `turnOn` takes the checkbox.
 */

type Wording = typeof en

const UUID = /\/exchanges\/([0-9a-f-]{36})$/

/** The screen's own title, below the navigation bar's (which is also a heading). */
export function title(page: Page, name: string | RegExp): Locator {
  return page.getByRole('heading', { name, level: 1 }).last()
}

export function button(scope: Page | Locator, name: string | RegExp): Locator {
  return scope.getByRole('button', { name, exact: typeof name === 'string' })
}

/** Turns on a switch, by its label. */
export async function turnOn(scope: Page | Locator, label: string): Promise<void> {
  await scope.getByLabel(label, { exact: true }).check()
}

// ---- Signing in ----------------------------------------------------------------

/*
 * Signing in and the profile are looked for among what is on screen only. A
 * screen opened from the signed-out first screen, such as an invitation, has
 * its own sign-in under it in the navigation stack, which the browser keeps
 * in the page, hidden.
 */

/**
 * Signs in from a sign-in form already on the screen, reading the code from
 * the API's log. The field is labelled for an email address or phone number
 * unless `label` says otherwise.
 */
export async function signIn(
  person: Person,
  w: Wording = en,
  label: string = w.signIn.identifierLabel,
): Promise<void> {
  const { page } = person
  const shown = (locator: Locator) => locator.filter({ visible: true })
  await shown(page.getByLabel(label)).fill(person.email)
  const code = await codeFrom(person.email, 'sign-in', () =>
    shown(button(page, w.signIn.sendCode)).click(),
  )
  await shown(page.getByLabel(w.signIn.codeLabel)).fill(code)
  await shown(button(page, w.signIn.submit)).click()
}

/** The profile a new account is asked for: a name and being 18 or over. */
export async function setUpProfile(person: Person, w: Wording = en): Promise<void> {
  const { page } = person
  const shown = (locator: Locator) => locator.filter({ visible: true })
  await expect(shown(page.getByRole('heading', { name: w.profile.firstTitle }))).toBeVisible()
  await shown(page.getByLabel(w.profile.nameLabel, { exact: true })).fill(person.name)
  await shown(page.getByLabel(w.profile.adultLabel, { exact: true })).check()
  await shown(button(page, w.profile.continue)).click()
}

/** A new account, signed in and set up, on the list of exchanges. */
export async function signUp(person: Person): Promise<void> {
  const { page } = person
  await page.goto('/')
  await expect(title(page, en.signIn.title)).toBeVisible()
  await signIn(person)
  await setUpProfile(person)
  await expect(title(page, en.home.title)).toBeVisible()
}

// ---- Joining -------------------------------------------------------------------

/**
 * Opens an invitation link, as a phone opens one it was handed, signs up on
 * the screen it opens, reads the proposal, responds and lands on the exchange.
 */
export async function join(person: Person, link: string, w: Wording = en): Promise<string> {
  const { page } = person
  await page.goto(link)
  await expect(title(page, w.invitation.signedOutTitle)).toBeVisible()
  await signIn(person, w)
  await expect(title(page, w.invitation.title)).toBeVisible()
  await button(page, w.invitation.respondNew).click()
  await setUpProfile(person, w)
  await page.waitForURL(UUID)
  return exchangeIdOf(page)
}

/** Signs the proposal waiting on this person, on the exchange screen. */
export async function acceptOpen(page: Page, w: Wording = en): Promise<void> {
  await button(page, w.exchange.accept).first().click()
  // The signing panel, headed under the proposal's own heading.
  const panel = page.getByRole('heading', { name: w.exchange.signHeading, level: 3 })
  await expect(panel).toBeVisible()
  await turnOn(page, w.consent.agree)
  await page.getByTestId('consent-sign').click()
  // Signed: there is nothing left to sign.
  await expect(panel).toBeHidden()
  await expect(button(page, w.exchange.accept)).toBeHidden()
}

export function exchangeIdOf(page: Page): string {
  const match = UUID.exec(new URL(page.url()).pathname)
  if (!match) throw new Error(`not on an exchange screen: ${page.url()}`)
  return match[1]
}

// ---- The exchange screen -------------------------------------------------------

/**
 * Takes a step on an item of the agreement: marking it delivered or paid,
 * confirming it. `label` is the button's wording, which is also the panel's
 * title and the button that sends it.
 */
export async function move(page: Page, label: string, note?: string): Promise<void> {
  await button(page, label).first().click()
  // The item's panel, headed under the agreement's own heading.
  const panel = page.getByRole('heading', { name: label, level: 3 })
  await expect(panel).toBeVisible()
  if (note !== undefined) await page.getByLabel(/^Note/).fill(note)
  await button(page, label).last().click()
  await expect(panel).toBeHidden()
}

/** The entries of the history at the foot of the exchange screen. */
export function historyEntries(page: Page): Locator {
  return page.getByRole('list').last().getByRole('listitem')
}
