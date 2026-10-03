import { invitationTokenIn } from '@yuppers/shared';
import { useSyncExternalStore } from 'react';

/*
 * The token from an invitation link, between the link arriving and the
 * invitation being claimed.
 *
 * It is treated as on the web (DESIGN.md §8, §13.5). It is taken out of the
 * link before the link reaches the router, so it never becomes part of a
 * route, the navigation history or anything that might log either. It is
 * held in memory only: not in any storage, and gone when the app closes. It
 * is sent to the service in request bodies, never in an address. It is not a
 * session and proves nothing about who opened the link.
 */

let held: string | null = null;
const listeners = new Set<() => void>();

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  return () => {
    listeners.delete(listener);
  };
}

export function holdInvitation(token: string): void {
  held = token;
  for (const listener of listeners) listener();
}

/** Dropped as soon as the invitation is claimed or turns out to be dead. */
export function forgetInvitation(): void {
  held = null;
  for (const listener of listeners) listener();
}

export function heldInvitation(): string | null {
  return held;
}

/** The held token, following a new link that arrives while the screen is open. */
export function useHeldInvitation(): string | null {
  return useSyncExternalStore(subscribe, heldInvitation, heldInvitation);
}

/** Where the app shows an invitation. The address carries no token. */
export const INVITATION_ROUTE = '/invitation';

const UUID = '[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}';

/**
 * A web address of an exchange's own pages, as notification emails write it:
 * `https://{web origin}/exchanges/{id}`, or its `/record`. The system hands
 * these to the app as universal and app links (`app.config.ts`).
 */
const EXCHANGE_LINK = new RegExp(
  `^https://[^/?#]+/exchanges/(${UUID})(/record)?/?(?:[?#].*)?$`,
  'i',
);

/**
 * The screen an exchange's web address stands for, as a route of the app's
 * own: the exchange, or its record. `null` for any other link.
 */
export function exchangeRouteIn(link: string): string | null {
  const found = EXCHANGE_LINK.exec(link.trim());
  if (!found) return null;
  const route = `/exchanges/${found[1].toLowerCase()}`;
  return found[2] ? `${route}/record` : route;
}

/**
 * Decides where a link the system handed to the app should go. An invitation
 * link, whether the web address or the app's own `yuppers://{language}/i#…`,
 * gives up its token here and goes to the invitation screen. A web address
 * of an exchange, from a notification email, opens that exchange's screen
 * (or its record), signed in or after signing in. Anything else is left as
 * it came.
 */
export function routeForIncomingLink(link: string): string {
  const exchange = exchangeRouteIn(link);
  if (exchange) return exchange;
  // A link with no fragment is not an invitation, whatever else it holds.
  if (!link.includes('#')) return link;
  const token = invitationTokenIn(link);
  if (!token) return link;
  holdInvitation(token);
  return INVITATION_ROUTE;
}
