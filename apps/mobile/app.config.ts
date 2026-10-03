import type { ConfigContext, ExpoConfig } from 'expo/config';

/*
 * The app config. Everything that does not change between builds is in
 * `app.json`, which this starts from; what is added here depends on the
 * environment the build runs in.
 *
 * Universal links (iOS) and app links (Android): an invitation link is
 * `{web origin}/{language}/i#{token}` (`src/lib/config.ts`), and a
 * notification email links to `{web origin}/exchanges/{id}` (or its
 * `/record`). On a device with the app installed the system can hand either
 * link to the app instead of the browser (DESIGN.md §4.1). It does so only for a domain the app names here and that
 * serves `/.well-known/apple-app-site-association` and
 * `/.well-known/assetlinks.json` naming the app back (the API serves both
 * once `APPLE_APP_ID` and `ANDROID_SHA256_CERT_FINGERPRINTS` are set;
 * docs/mobile-release.md).
 *
 * The domain is the host of `EXPO_PUBLIC_WEB_URL`, the same setting the
 * invitation links are made from, so the links the app writes and the links
 * it opens cannot point at different places. Only an HTTPS origin on the
 * default port counts: neither system verifies anything else, so a
 * development build against `http://localhost:5173` names no domain and
 * opens links through its own scheme, `yuppers://`, as before.
 */

/** The host the app claims links on, or `null` when the origin cannot have any. */
export function appLinkDomain(webUrl: string | undefined): string | null {
  if (!webUrl) return null;
  let url: URL;
  try {
    url = new URL(webUrl);
  } catch {
    return null;
  }
  if (url.protocol !== 'https:' || url.port !== '') return null;
  const host = url.hostname.toLowerCase();
  // A machine name or an address is never verified by either system.
  if (!host.includes('.') || /^[\d.]+$/.test(host) || host.startsWith('[')) return null;
  return host;
}

/** The two invitation paths the web serves without a redirect: `/{language}/i` and `/{language}/i/`. */
const INVITATION_PATH_PATTERNS = ['/.*/i', '/.*/i/'];

/** An exchange's pages, where notification emails link to: `/exchanges/{id}` and `/exchanges/{id}/record`. */
const EXCHANGE_PATH_PREFIX = '/exchanges/';

export function withAppLinks(config: ExpoConfig, domain: string | null): ExpoConfig {
  if (!domain) return config;
  return {
    ...config,
    ios: {
      ...config.ios,
      associatedDomains: [...(config.ios?.associatedDomains ?? []), `applinks:${domain}`],
    },
    android: {
      ...config.android,
      intentFilters: [
        ...(config.android?.intentFilters ?? []),
        {
          action: 'VIEW',
          autoVerify: true,
          data: [
            ...INVITATION_PATH_PATTERNS.map((pathPattern) => ({
              scheme: 'https',
              host: domain,
              pathPattern,
            })),
            { scheme: 'https', host: domain, pathPrefix: EXCHANGE_PATH_PREFIX },
          ],
          category: ['BROWSABLE', 'DEFAULT'],
        },
      ],
    },
  };
}

/** The EAS build profiles that make an app for someone other than its developer. */
const RELEASE_PROFILES = ['preview', 'production'];

/** The settings `src/lib/config.ts` reads, each of which falls back to localhost when unset. */
const SERVICE_SETTINGS = ['EXPO_PUBLIC_API_URL', 'EXPO_PUBLIC_WEB_URL'] as const;

/**
 * Refuses a preview or production build whose service settings are missing
 * or not HTTPS. The app falls back to `http://localhost:8080` and
 * `http://localhost:5173` when they are unset, which is right for a
 * simulator on the development machine and wrong for any device anyone
 * else holds: such a build would install and then reach nothing. Every
 * other build (development, the tests, the browser harness, a local
 * `expo export`) runs without `EAS_BUILD_PROFILE` or with `development`,
 * and keeps the defaults.
 */
export function checkReleaseSettings(env: Record<string, string | undefined>): void {
  const profile = env.EAS_BUILD_PROFILE;
  if (!profile || !RELEASE_PROFILES.includes(profile)) return;
  const wrong = SERVICE_SETTINGS.filter((name) => {
    const value = env[name]?.trim();
    if (!value) return true;
    try {
      return new URL(value).protocol !== 'https:';
    } catch {
      return true;
    }
  });
  if (wrong.length > 0) {
    throw new Error(
      `The ${profile} build needs ${wrong.join(' and ')} set to ${wrong.length > 1 ? 'https:// URLs' : 'an https:// URL'} ` +
        `in its EAS environment ` +
        `(docs/mobile-release.md); without them the app would talk to localhost.`,
    );
  }
}

/**
 * Push on Android: an app gets a push token only with its Firebase project's
 * `google-services.json` built in. That file belongs to the owner's Firebase
 * project and is not kept in the repository; an EAS build is handed it as a
 * file environment variable, `GOOGLE_SERVICES_JSON`, whose value is the path
 * EAS writes it to (docs/mobile-release.md). Without it the build is as
 * before, and on Android the app finds no token and offers no notifications.
 * iOS needs nothing here: the APNs key is uploaded to Expo, not built in.
 */
export function withPushCredentials(config: ExpoConfig, googleServicesFile: string | undefined): ExpoConfig {
  const file = googleServicesFile?.trim();
  if (!file) return config;
  return { ...config, android: { ...config.android, googleServicesFile: file } };
}

export default function appConfig({ config }: ConfigContext): ExpoConfig {
  checkReleaseSettings(process.env);
  // `config` is app.json's `expo` object; it always has a name and a slug.
  const linked = withAppLinks(config as ExpoConfig, appLinkDomain(process.env.EXPO_PUBLIC_WEB_URL));
  return withPushCredentials(linked, process.env.GOOGLE_SERVICES_JSON);
}
