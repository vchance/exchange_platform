import { readFileSync } from 'node:fs'

import { apiLog } from './env'

/*
 * One-time codes, read back from the API's log. With CODE_DELIVERY=log the
 * service writes a line for each code it would have sent:
 *
 *   ... one-time code (development delivery) to="ana@example.test" code="123456" purpose="sign-in" language="en"
 *
 * Every test uses addresses of its own, so the codes for an address are that
 * test's alone, however many tests share the log.
 */

export type CodePurpose = 'sign-in' | 'delete-account'

function escape(text: string): string {
  return text.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')
}

/** Every code sent to `to` for `purpose` so far, oldest first. */
export function codesSent(to: string, purpose: CodePurpose, log = apiLog): string[] {
  let text: string
  try {
    text = readFileSync(log, 'utf8')
  } catch {
    return []
  }
  const pattern = new RegExp(
    `one-time code \\(development delivery\\) to="?${escape(to)}"? code="?(\\d{6})"? purpose="?${escape(purpose)}"?`,
    'g',
  )
  return [...text.matchAll(pattern)].map((match) => match[1])
}

/**
 * Does `send`, which asks the service for a code, and returns the code that
 * request produced: the first one for `to` that was not in the log before.
 */
export async function codeFrom(
  to: string,
  purpose: CodePurpose,
  send: () => Promise<unknown>,
  log = apiLog,
): Promise<string> {
  const before = codesSent(to, purpose, log).length
  await send()
  const deadline = Date.now() + 15_000
  while (Date.now() < deadline) {
    const codes = codesSent(to, purpose, log)
    if (codes.length > before) return codes[codes.length - 1]
    await new Promise((settle) => setTimeout(settle, 100))
  }
  throw new Error(`No ${purpose} code for ${to} appeared in ${log}`)
}
