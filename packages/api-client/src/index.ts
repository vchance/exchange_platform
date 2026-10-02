import createClient from 'openapi-fetch'

import type { components, paths } from './schema'

export type { components, paths }

export type ErrorCode = components['schemas']['ErrorCode']
export type ErrorBody = components['schemas']['ErrorBody']
export type Meta = components['schemas']['Meta']
export type Account = components['schemas']['Account']
export type Language = components['schemas']['Language']

export type ApiClient = ReturnType<typeof createApiClient>

/**
 * Typed client for the Exchange API. `schema.ts` is generated from the
 * service's own API description by `npm run gen:api`; do not edit it by hand.
 */
export function createApiClient(baseUrl: string) {
  return createClient<paths>({ baseUrl })
}
