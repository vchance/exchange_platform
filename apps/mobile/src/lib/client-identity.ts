import { parseVersion, type ClientIdentity } from '@exchange/shared';

/*
 * Which client this is and which build, for the `X-Client-Version` header
 * and the startup check against `GET /v1/meta` (`CLIENT_TOO_OLD`).
 *
 * The build's own version comes from the installed app (`expo-application`),
 * which is what a store build reports, and otherwise from the app config
 * bundled with the JavaScript. A version that cannot be read is not guessed:
 * a made-up one such as 0.0.0 would be below any minimum a deployment set,
 * and lock the app out of everything. With no version the app names no
 * build, and the service, which ignores what it cannot read, treats it as
 * it treats any client that does not say.
 */

export interface VersionSources {
  /** `Application.nativeApplicationVersion`: the installed build's version. */
  native: string | null | undefined;
  /** `Constants.expoConfig?.version`: the version in the bundled app config. */
  config: string | null | undefined;
}

/** The first of the sources that reads as a version, or `null`. */
export function buildVersion({ native, config }: VersionSources): string | null {
  for (const candidate of [native, config]) {
    const version = candidate?.trim();
    if (version && parseVersion(version)) return version;
  }
  return null;
}

/**
 * This client as the service should know it, or `undefined` when it is
 * neither app (the browser test harness) or its version cannot be read.
 */
export function clientIdentity(os: string, sources: VersionSources): ClientIdentity | undefined {
  if (os !== 'ios' && os !== 'android') return undefined;
  const version = buildVersion(sources);
  return version === null ? undefined : { name: os, version };
}
