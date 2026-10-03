import { randomInt } from 'node:crypto'

import type { BrowserContext } from '@playwright/test'

import { proxyURL } from './env'

/*
 * THE WORKAROUND FOR THE PER-ADDRESS SIGN-IN LIMITS, kept in this one file so
 * it is easy to replace once those limits can be set for a test run.
 *
 * Sign-in is limited per requester's network address (README, "Signing in"),
 * and every browser and every API call here comes from 127.0.0.1, so a few
 * tests in a row would use up one address's allowance. As in the web suite
 * and the load check, the API runs as if behind a proxy
 * (TRUSTED_PROXY_HEADER=X-Forwarded-For) and each person sends an address of
 * their own from 198.18.0.0/15, the range set aside for benchmarking.
 *
 * The web suite sets the header on the whole browser context. Here the page
 * calls the API on another origin, through the harness proxy, and a header
 * the page adds itself makes the browser ask the proxy first whether it may
 * send it, which the proxy does not allow. So the header is added to the
 * proxy's requests on their way out of the browser instead, past that check.
 * The proxy passes it through to the API untouched. Nothing in the app or the
 * proxy changes.
 */

/**
 * An address in 198.18.0.0/15, chosen at random so runs close together do
 * not share one either.
 */
export function networkAddress(): string {
  const n = randomInt(1, 2 ** 17 - 1)
  return `198.${18 + (n >> 16)}.${(n >> 8) & 255}.${n & 255}`
}

/** The headers a call made outside the browser sends, as `address`. */
export function addressHeaders(address: string): Record<string, string> {
  return { 'x-forwarded-for': address }
}

/** Makes every call the app in `context` makes to the API come from `address`. */
export async function connectFrom(context: BrowserContext, address: string): Promise<void> {
  await context.route(`${proxyURL}/**`, (route) =>
    route.continue({ headers: { ...route.request().headers(), ...addressHeaders(address) } }),
  )
}
