import { execFileSync } from 'node:child_process'
import { resolve } from 'node:path'

import { repoRoot } from './env'

/*
 * Naming a reviewer the way the owner does: with the `staff` command, run
 * as the schema owner (MIGRATION_DATABASE_URL, from the environment or the
 * repository's `.env`), never through the API.
 */

export const staffBinary = resolve(
  process.env.E2E_STAFF_BIN ?? resolve(repoRoot, 'backend/target/debug/staff'),
)

/** Makes the account with this email address a reviewer. It must exist: sign up first. */
export function grantStaff(email: string): void {
  execFileSync(staffBinary, ['grant', email], { cwd: repoRoot, stdio: 'pipe' })
}

/** Stops the account being a reviewer. */
export function revokeStaff(email: string): void {
  execFileSync(staffBinary, ['revoke', email], { cwd: repoRoot, stdio: 'pipe' })
}
