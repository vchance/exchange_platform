import type { ExpoConfig } from 'expo/config';

import appConfig, {
  appLinkDomain,
  checkReleaseSettings,
  withAppLinks,
  withBuildCommit,
  withPushCredentials,
} from '../../../app.config';
import appJson from '../../../app.json';

const base = appJson.expo as ExpoConfig;

describe('the domain the app claims links on', () => {
  it('is the host of an HTTPS web origin', () => {
    expect(appLinkDomain('https://yuppers.example')).toBe('yuppers.example');
    expect(appLinkDomain('https://App.Yuppers.Example/')).toBe('app.yuppers.example');
  });

  it('is none for an origin neither system would verify', () => {
    for (const url of [
      undefined,
      '',
      'not a url',
      'http://yuppers.example',
      'http://localhost:5173',
      'https://localhost',
      'https://yuppers.example:8443',
      'https://192.168.1.20',
      'https://[::1]',
    ]) {
      expect(appLinkDomain(url)).toBeNull();
    }
  });
});

describe('the settings a release build needs', () => {
  const service = {
    EXPO_PUBLIC_API_URL: 'https://api.yuppers.example',
    EXPO_PUBLIC_WEB_URL: 'https://yuppers.example',
  };

  it('are not needed for a development build, or outside EAS', () => {
    for (const EAS_BUILD_PROFILE of [undefined, '', 'development', 'development-simulator']) {
      expect(() => checkReleaseSettings({ EAS_BUILD_PROFILE })).not.toThrow();
      expect(() =>
        checkReleaseSettings({
          EAS_BUILD_PROFILE,
          EXPO_PUBLIC_API_URL: 'http://localhost:8080',
          EXPO_PUBLIC_WEB_URL: 'http://localhost:5173',
        }),
      ).not.toThrow();
    }
  });

  it('pass with both set to HTTPS', () => {
    for (const EAS_BUILD_PROFILE of ['preview', 'production', 'staging']) {
      expect(() => checkReleaseSettings({ EAS_BUILD_PROFILE, ...service })).not.toThrow();
    }
  });

  it('stop any other EAS build when either is unset or not HTTPS', () => {
    // Any profile not known to be for development, including one added
    // later under a name nobody listed.
    for (const EAS_BUILD_PROFILE of ['preview', 'production', 'staging', 'Development', 'dev']) {
      for (const name of ['EXPO_PUBLIC_API_URL', 'EXPO_PUBLIC_WEB_URL'] as const) {
        for (const value of [undefined, '', '  ', 'http://api.yuppers.example', 'not a url', 'api.yuppers.example']) {
          const env = { EAS_BUILD_PROFILE, ...service, [name]: value };
          expect(() => checkReleaseSettings(env)).toThrow(new RegExp(`${EAS_BUILD_PROFILE} build needs ${name} `));
        }
      }
      expect(() => checkReleaseSettings({ EAS_BUILD_PROFILE })).toThrow(
        'needs EXPO_PUBLIC_API_URL and EXPO_PUBLIC_WEB_URL set to https:// URLs',
      );
    }
  });

  it('are checked when the config is read', () => {
    const before = { ...process.env };
    try {
      process.env.EAS_BUILD_PROFILE = 'production';
      delete process.env.EXPO_PUBLIC_API_URL;
      process.env.EXPO_PUBLIC_WEB_URL = 'https://yuppers.example';
      expect(() =>
        appConfig({ config: base, projectRoot: '.', staticConfigPath: null, packageJsonPath: null }),
      ).toThrow('EXPO_PUBLIC_API_URL');
    } finally {
      for (const name of ['EAS_BUILD_PROFILE', 'EXPO_PUBLIC_API_URL', 'EXPO_PUBLIC_WEB_URL']) {
        if (before[name] === undefined) delete process.env[name];
        else process.env[name] = before[name];
      }
    }
  });
});

describe('the app config', () => {
  it('names no domain without one, and keeps app.json as it is', () => {
    expect(withAppLinks(base, null)).toBe(base);
    expect(base.ios?.associatedDomains).toBeUndefined();
    expect(base.android?.intentFilters).toBeUndefined();
  });

  it('claims invitation links and exchange pages on the domain, on both platforms', () => {
    const config = withAppLinks(base, 'yuppers.example');
    expect(config.ios?.associatedDomains).toEqual(['applinks:yuppers.example']);
    expect(config.ios?.bundleIdentifier).toBe('app.yuppers');
    expect(config.android?.intentFilters).toEqual([
      {
        action: 'VIEW',
        autoVerify: true,
        data: [
          { scheme: 'https', host: 'yuppers.example', pathPattern: '/.*/i' },
          { scheme: 'https', host: 'yuppers.example', pathPattern: '/.*/i/' },
          { scheme: 'https', host: 'yuppers.example', pathPrefix: '/exchanges/' },
        ],
        category: ['BROWSABLE', 'DEFAULT'],
      },
    ]);
    expect(config.android?.package).toBe('app.yuppers');
    expect(config.android?.blockedPermissions).toEqual(base.android?.blockedPermissions);
  });

  it('builds in the Firebase file for Android push only when a build is given one', () => {
    expect(withPushCredentials(base, undefined)).toBe(base);
    expect(withPushCredentials(base, ' ')).toBe(base);
    expect(base.android?.googleServicesFile).toBeUndefined();
    const config = withPushCredentials(base, '/home/expo/workingdir/google-services.json');
    expect(config.android?.googleServicesFile).toBe('/home/expo/workingdir/google-services.json');
    expect(config.android?.package).toBe('app.yuppers');
    expect(config.ios).toEqual(base.ios);
  });

  it('names the commit it was built from when EAS or the build says', () => {
    const sha = '0123456789ABCDEF0123456789abcdef01234567';
    expect(withBuildCommit(base, {})).toBe(base);
    expect(withBuildCommit(base, { EAS_BUILD_GIT_COMMIT_HASH: 'unknown', GIT_SHA: ' ' })).toBe(base);
    expect(withBuildCommit(base, { EAS_BUILD_GIT_COMMIT_HASH: sha }).extra?.commit).toBe(sha.toLowerCase());
    expect(withBuildCommit(base, { GIT_SHA: 'abcdef1' }).extra?.commit).toBe('abcdef1');
    expect(withBuildCommit(base, { EAS_BUILD_GIT_COMMIT_HASH: sha, GIT_SHA: 'abcdef1' }).extra?.commit).toBe(
      sha.toLowerCase(),
    );
    expect(withBuildCommit(base, { GIT_SHA: 'abcdef1' }).name).toBe(base.name);
  });

  it('reads the domain from EXPO_PUBLIC_WEB_URL', () => {
    const before = process.env.EXPO_PUBLIC_WEB_URL;
    try {
      process.env.EXPO_PUBLIC_WEB_URL = 'https://yuppers.example';
      const config = appConfig({ config: base, projectRoot: '.', staticConfigPath: null, packageJsonPath: null });
      expect(config.ios?.associatedDomains).toEqual(['applinks:yuppers.example']);
      expect(config.version).toBe('0.1.0');
    } finally {
      if (before === undefined) delete process.env.EXPO_PUBLIC_WEB_URL;
      else process.env.EXPO_PUBLIC_WEB_URL = before;
    }
  });
});
