import { formatMessage } from './message'
import type { Wording } from './wording/types'

/*
 * Which build of a client is running, as shown to a person (the account
 * screen) and sent to the service (`X-Client-Version`, `client-version.ts`).
 * The web build takes the commit from the build's environment (`GIT_SHA`,
 * vite.config.ts), the apps from EAS's (app.config.ts). The service's own
 * build is in `GET /v1/meta` (`commit`, `built_at`).
 */

export interface BuildIdentity {
  /** Dotted whole numbers, such as `0.1.0`. */
  version: string
  /** The git commit the build was made from, if the build says. */
  commit?: string | null
  /** The store build number of an app (EAS's `buildNumber` or `versionCode`). */
  build?: string | null
}

/**
 * The first seven characters of a git commit, lowercase, or `null` for
 * anything that is not one (missing, empty, `unknown`).
 */
export function shortCommit(commit: string | null | undefined): string | null {
  const value = commit?.trim() ?? ''
  if (!/^[0-9a-fA-F]{7,64}$/.test(value)) return null
  return value.slice(0, 7).toLowerCase()
}

/** "Version 0.1.0 (abc1234)", "Version 0.1.0 (build 12, abc1234)", and so on. */
export function versionText(wording: Wording, language: string, identity: BuildIdentity): string {
  const w = wording.profile
  const commit = shortCommit(identity.commit)
  const build = identity.build?.trim() || null
  const message =
    build && commit
      ? w.versionBuildCommit
      : build
        ? w.versionBuild
        : commit
          ? w.version
          : w.versionOnly
  return formatMessage(message, { version: identity.version, commit: commit ?? '', build: build ?? '' }, language)
}
