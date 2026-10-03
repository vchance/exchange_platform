import type { ConfigContext, ExpoConfig } from 'expo/config';

/*
 * The app config. Everything that does not change between builds is in
 * `app.json`, which this starts from; what is added here depends on the
 * environment the build runs in.
 *
 * Universal links (iOS) and app links (Android): an invitation link is
 * `{web origin}/{language}/i#{token}` (`src/lib/config.ts`), and on a device
 * with the app installed the system can hand that link to the app instead of
 * the browser. It does so only for a domain the app names here and that
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
          data: INVITATION_PATH_PATTERNS.map((pathPattern) => ({
            scheme: 'https',
            host: domain,
            pathPattern,
          })),
          category: ['BROWSABLE', 'DEFAULT'],
        },
      ],
    },
  };
}

export default function appConfig({ config }: ConfigContext): ExpoConfig {
  // `config` is app.json's `expo` object; it always has a name and a slug.
  return withAppLinks(config as ExpoConfig, appLinkDomain(process.env.EXPO_PUBLIC_WEB_URL));
}
