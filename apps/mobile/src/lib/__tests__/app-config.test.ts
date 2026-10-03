import type { ExpoConfig } from 'expo/config';

import appConfig, { appLinkDomain, withAppLinks } from '../../../app.config';
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
