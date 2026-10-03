import { createApiClient, type components, type ErrorCode } from '@yuppers/api-client'
import { ApiFailure, clientHeader } from '@yuppers/shared'

import { WEB_CLIENT } from './api'

type Schemas = components['schemas']
export type ReviewQueue = Schemas['ReviewQueue']
export type QueuedReport = Schemas['QueuedReport']
export type ReportDetail = Schemas['ReportDetail']
export type ReviewOutcome = Schemas['ReviewOutcome']
export type ReviewEntry = Schemas['ReviewEntry']
export type Suspension = Schemas['Suspension']
export type HiddenContent = Schemas['HiddenContent']

/*
 * The staff review calls (DESIGN.md §9). Only the staff screen makes them,
 * so they live here and not with the calls both apps share. Same origin and
 * the same session cookie as everything else; a refusal is thrown as an
 * `ApiFailure`, so the screen shows it like any other. To anyone who is not
 * a reviewer every one of them answers NOT_FOUND.
 */

const client = createApiClient('')
const headers = { 'X-Client-Version': clientHeader(WEB_CLIENT) }

interface Reply<T> {
  data?: T
  error?: unknown
  response: Response
}

async function send<T>(request: () => Promise<Reply<T>>): Promise<T> {
  let reply: Reply<T>
  try {
    reply = await request()
  } catch {
    throw new ApiFailure('SERVICE_UNAVAILABLE', true)
  }
  if (reply.response.ok) return reply.data as T
  const body = reply.error as { code?: unknown } | undefined
  if (typeof body?.code !== 'string') throw new ApiFailure('SERVICE_UNAVAILABLE', true)
  throw new ApiFailure(body.code as ErrorCode)
}

export const staffApi = {
  queue(): Promise<ReviewQueue> {
    return send(() => client.GET('/v1/staff/reports', { headers }))
  },

  /** Opens a report; the service records that it was opened. */
  report(id: string): Promise<ReportDetail> {
    return send(() => client.GET('/v1/staff/reports/{id}', { headers, params: { path: { id } } }))
  },

  resolve(id: string, outcome: ReviewOutcome, note: string): Promise<void> {
    return send(() =>
      client.POST('/v1/staff/reports/{id}/resolution', {
        headers,
        params: { path: { id } },
        body: { outcome, note: note.trim() === '' ? null : note },
      }),
    )
  },

  suspensions(): Promise<Suspension[]> {
    return send(() => client.GET('/v1/staff/suspensions', { headers }))
  },

  lift(account: string, note: string): Promise<void> {
    return send(() =>
      client.POST('/v1/staff/suspensions/{account}/lift', {
        headers,
        params: { path: { account } },
        body: { note },
      }),
    )
  },

  hidden(): Promise<HiddenContent[]> {
    return send(() => client.GET('/v1/staff/hidden', { headers }))
  },

  restore(exchange: string, account: string, note: string): Promise<void> {
    return send(() =>
      client.POST('/v1/staff/hidden/restore', {
        headers,
        body: { exchange_id: exchange, account_id: account, note },
      }),
    )
  },
}
