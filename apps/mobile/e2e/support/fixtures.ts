import { randomUUID } from 'node:crypto'

import { test as base, devices, type BrowserContext, type Page } from '@playwright/test'

import { webURL } from './env'

/*
 * The people in a test. Each has a browser context of their own, the size
 * and touch of a phone, so two people never share a session (the harness
 * keeps the token in the tab's session storage), and an `example.test`
 * address nobody else uses, so tests can run side by side against one
 * database.
 */

/** The phone the screens are laid out for. Chromium plays it; WebKit is not installed in CI. */
const { defaultBrowserType: _browser, ...phone } = devices['iPhone 15']
export { phone }

export interface Person {
  /** The name they give their account. */
  name: string
  email: string
  context: BrowserContext
  page: Page
}

interface PersonOptions {
  /** The phone's language. English unless given. */
  locale?: string
}

export interface Fixtures {
  /** A new person with a phone of their own, signed out. */
  person(name: string, options?: PersonOptions): Promise<Person>
  /** A new email address for someone acting through the API rather than the app. */
  email(name: string): string
}

export const test = base.extend<Fixtures>({
  // Unique per test, and readable in the log.
  email: async ({}, provide, testInfo) => {
    const run = randomUUID().slice(0, 8)
    await provide((name) => {
      const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-')
      return `m-${slug}-w${testInfo.workerIndex}-${run}@example.test`
    })
  },
  // The second argument hands the fixture to the test; it is Playwright's
  // `use`, renamed so it is not taken for a React hook.
  person: async ({ browser, email }, provide) => {
    const contexts: BrowserContext[] = []
    await provide(async (name, options = {}) => {
      const context = await browser.newContext({
        ...phone,
        baseURL: webURL,
        locale: options.locale ?? 'en-US',
        timezoneId: 'UTC',
        acceptDownloads: true,
      })
      contexts.push(context)
      return { name, email: email(name), context, page: await context.newPage() }
    })
    for (const context of contexts) await context.close()
  },
})

export { expect } from '@playwright/test'
