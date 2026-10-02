import { invitationPath } from '@exchange/shared'

// Reading the token back out of a link is the same on every client.
export { invitationToken } from '@exchange/shared'

/*
 * The app's addresses. Two of them are fixed points other things depend on:
 * an exchange lives at `/exchanges/{id}`, which notification emails link to,
 * and an invitation link is `/{language}/i#{token}` (DESIGN.md §13.5).
 */

export type Route =
  | { name: 'home' }
  | { name: 'account' }
  /** `language` is the sender's: it chose which entry page the link previews with, nothing more. */
  | { name: 'invitation'; language: string }
  | { name: 'exchange'; id: string }
  /** Writing a counteroffer or an amendment. */
  | { name: 'revise'; id: string }
  /** The whole record of an exchange, laid out for reading and printing. */
  | { name: 'record'; id: string }
  | { name: 'notFound' }

const UUID = '[0-9a-fA-F]{8}(?:-[0-9a-fA-F]{4}){3}-[0-9a-fA-F]{12}'
const LANGUAGE_TAG = '[A-Za-z]{2,3}(?:-[A-Za-z0-9]{2,8})*'

const EXCHANGE = new RegExp(`^/exchanges/(${UUID})$`)
const REVISE = new RegExp(`^/exchanges/(${UUID})/revise$`)
const RECORD = new RegExp(`^/exchanges/(${UUID})/record$`)
const INVITATION = new RegExp(`^/(${LANGUAGE_TAG})/i$`)

export function matchRoute(pathname: string): Route {
  // A static host may answer `/en/i` at `/en/i/`.
  const path = pathname.length > 1 ? pathname.replace(/\/+$/, '') : pathname
  if (path === '/' || path === '') return { name: 'home' }
  if (path === '/account') return { name: 'account' }

  const invitation = INVITATION.exec(path)
  if (invitation) return { name: 'invitation', language: invitation[1] }
  const revise = REVISE.exec(path)
  if (revise) return { name: 'revise', id: revise[1].toLowerCase() }
  const record = RECORD.exec(path)
  if (record) return { name: 'record', id: record[1].toLowerCase() }
  const exchange = EXCHANGE.exec(path)
  if (exchange) return { name: 'exchange', id: exchange[1].toLowerCase() }
  return { name: 'notFound' }
}

export const paths = {
  home: '/',
  account: '/account',
  exchange: (id: string) => `/exchanges/${id}`,
  revise: (id: string) => `/exchanges/${id}/revise`,
  record: (id: string) => `/exchanges/${id}/record`,
  /**
   * The link an initiator shares. The token goes in the fragment, which a
   * browser never sends, so it cannot end up in a server log; the path names
   * the sender's language so the link previews in it.
   */
  invitation: invitationPath,
}
