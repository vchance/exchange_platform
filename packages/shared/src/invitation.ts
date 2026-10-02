/*
 * The invitation link an initiator shares (DESIGN.md §8, §13.5). Every client
 * writes it and reads it the same way.
 *
 * The link is `{web origin}/{language}/i#{token}`. The path names the
 * sender's language, so the link previews in it. The token goes in the
 * fragment, which is never sent to a server, so it cannot end up in a server
 * log; the clients send it to the API in request bodies only.
 */

/** The path and fragment of an invitation link, without the origin. */
export function invitationPath(language: string, token: string): string {
  return `/${language}/i#${token}`
}

/** The whole link, as it is shared. `origin` is where the web app is served from. */
export function invitationLink(origin: string, language: string, token: string): string {
  return origin.replace(/\/+$/, '') + invitationPath(language, token)
}

/** The token from an invitation link's fragment, if it has one. */
export function invitationToken(hash: string): string | null {
  const token = hash.replace(/^#/, '').trim()
  return /^[A-Za-z0-9_-]{16,}$/.test(token) ? token : null
}

// What comes before the fragment in an invitation link, whichever way it
// arrives: `https://host/es/i`, an app's own `scheme://es/i`, or the path.
const BEFORE_TOKEN = /(^|\/)[A-Za-z]{2,3}(-[A-Za-z0-9]{2,8})*\/i\/?$/

/**
 * The token in something a person pasted or the system handed over: a whole
 * invitation link, or the token alone. `null` for anything else, including a
 * link to some other page that happens to have a fragment.
 */
export function invitationTokenIn(text: string): string | null {
  const given = text.trim()
  const mark = given.indexOf('#')
  if (mark === -1) return invitationToken(given)
  return BEFORE_TOKEN.test(given.slice(0, mark)) ? invitationToken(given.slice(mark)) : null
}
