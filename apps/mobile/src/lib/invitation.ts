import { invitationTokenIn } from '@exchange/shared';
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

/**
 * Decides where a link the system handed to the app should go. An invitation
 * link, whether the web address or the app's own `exchange://{language}/i#…`,
 * gives up its token here and goes to the invitation screen; anything else
 * is left as it came.
 */
export function routeForIncomingLink(link: string): string {
  // A link with no fragment is not an invitation, whatever else it holds.
  if (!link.includes('#')) return link;
  const token = invitationTokenIn(link);
  if (!token) return link;
  holdInvitation(token);
  return INVITATION_ROUTE;
}
