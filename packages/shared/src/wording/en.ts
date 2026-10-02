import type { Wording } from './types'

export const en: Wording = {
  productName: 'Exchange',
  tagline: 'Agree on a deal, then track what each of you has delivered.',
  service: {
    checking: 'Checking the service…',
    connected: 'Connected to the service.',
    unreachable: 'The service can’t be reached right now.',
  },
  errors: {
    STALE_REVISION: 'The terms changed while you were looking. Review the latest version.',
    WRONG_ACTOR: 'Only the other party can do this.',
    ACTION_NOT_ALLOWED: 'This can’t be done right now.',
    CONTRIBUTION_LOCKED: 'This item has been accepted and can’t be changed.',
    CLIENT_TOO_OLD: 'Update the app to continue.',
    NOT_FOUND: 'We couldn’t find that.',
    SERVICE_UNAVAILABLE: 'The service is temporarily unavailable. Try again shortly.',
    INTERNAL: 'Something went wrong on our side. Try again.',
  },
}
