import { randomUUID } from 'node:crypto'

import { test as base, type BrowserContext, type Page } from '@playwright/test'

/*
 * The people in a test. Each has a browser context of their own, so two
 * people never share a cookie, and an `example.test` address nobody else
 * uses, so tests can run side by side against one database.
 */

export interface Person {
  /** The name they give their account. */
  name: string
  email: string
  context: BrowserContext
  page: Page
}

interface PersonOptions {
  /** The browser's language. English unless given. */
  locale?: string
}

interface Fixtures {
  /** A new person with a browser of their own, signed out. */
  person(name: string, options?: PersonOptions): Promise<Person>
}

export const test = base.extend<Fixtures>({
  // The second argument hands the fixture to the test; it is Playwright's
  // `use`, renamed so it is not taken for a React hook.
  person: async ({ browser, baseURL }, provide, testInfo) => {
    const contexts: BrowserContext[] = []
    // Unique per test, and readable in the log.
    const run = randomUUID().slice(0, 8)
    await provide(async (name, options = {}) => {
      const context = await browser.newContext({
        baseURL,
        locale: options.locale ?? 'en-US',
        timezoneId: 'UTC',
        permissions: ['clipboard-read', 'clipboard-write'],
      })
      contexts.push(context)
      const slug = name.toLowerCase().replace(/[^a-z0-9]+/g, '-')
      const email = `${slug}-w${testInfo.workerIndex}-${run}@example.test`
      return { name, email, context, page: await context.newPage() }
    })
    for (const context of contexts) await context.close()
  },
})

export { expect } from '@playwright/test'
