import { routeForIncomingLink } from '../lib/invitation';

/**
 * Every link the system hands to the app passes through here before the
 * router sees it. An invitation link gives up its token at this point, so
 * the token never becomes part of a route or of the navigation history
 * (DESIGN.md §8); the router is sent to the invitation screen instead.
 */
export function redirectSystemPath({ path }: { path: string; initial: boolean }): string {
  try {
    return routeForIncomingLink(path);
  } catch {
    // Never fail here: an unreadable link opens the app where it would have anyway.
    return path;
  }
}
