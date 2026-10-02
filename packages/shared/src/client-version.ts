import type { Meta } from '@exchange/api-client'

/*
 * Whether this build of a client is too old to act (`CLIENT_TOO_OLD`). The
 * service says in `GET /v1/meta` how old each client may be, and each client
 * compares itself at startup; it also names itself on every request, in an
 * `X-Client-Version` header, and the service refuses a change from one below
 * the minimum. The rule here is the service's (`backend/src/client_version.rs`):
 * dotted whole numbers, compared part by part, a missing part counting as
 * zero, and anything unreadable counting as not too old.
 */

export type ClientName = 'web' | 'ios' | 'android'

export interface ClientIdentity {
  name: ClientName
  /** Dotted whole numbers, such as `1.4.0`. */
  version: string
}

/** The header's value: `ios/1.4.0`. */
export function clientHeader(client: ClientIdentity): string {
  return `${client.name}/${client.version}`
}

export function parseVersion(text: string): number[] | null {
  const parts = text.trim().split('.')
  const numbers: number[] = []
  for (const part of parts) {
    if (!/^\d+$/.test(part)) return null
    numbers.push(Number(part))
  }
  return numbers
}

/** Negative when `a` is older than `b`, zero when they are the same version. */
export function compareVersions(a: readonly number[], b: readonly number[]): number {
  const width = Math.max(a.length, b.length)
  for (let index = 0; index < width; index += 1) {
    const difference = (a[index] ?? 0) - (b[index] ?? 0)
    if (difference !== 0) return difference
  }
  return 0
}

/** Whether `client` is below the minimum the service names for its kind. */
export function isClientTooOld(
  minimums: Meta['minimum_client_versions'],
  client: ClientIdentity,
): boolean {
  const minimum = minimums[client.name]
  if (minimum == null) return false
  const required = parseVersion(minimum)
  const own = parseVersion(client.version)
  if (!required || !own) return false
  return compareVersions(own, required) < 0
}
