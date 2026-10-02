/*
 * Idempotency keys for changes to an exchange (DESIGN.md §13.4).
 *
 * Each attempt to make a change gets its own key. The one exception is an
 * attempt that went unanswered: the request left, and no reply came back, so
 * nobody here knows whether the service applied it. Trying the very same
 * request again reuses that attempt's key, which is what lets the service
 * recognize it as a retry and answer with the current state instead of
 * acting twice or refusing it as stale.
 */

export interface IdempotencyKeys {
  /** The key to send with a request, identified by where it goes and what it says. */
  keyFor(request: string): string
  /** The service replied, with success or a refusal: the attempt is over. */
  answered(request: string): void
  /** No reply came back. The same request sent again is a retry of this attempt. */
  unanswered(request: string, key: string): void
}

export function idempotencyKeys(newKey: () => string = () => crypto.randomUUID()): IdempotencyKeys {
  const waiting = new Map<string, string>()
  return {
    keyFor: (request) => waiting.get(request) ?? newKey(),
    answered: (request) => {
      waiting.delete(request)
    },
    unanswered: (request, key) => {
      waiting.set(request, key)
    },
  }
}
