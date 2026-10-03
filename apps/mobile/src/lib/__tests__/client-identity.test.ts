import { clientHeader, createExchangeApi, isClientTooOld } from '@exchange/shared';

import { buildVersion, clientIdentity } from '../client-identity';

test('the installed build’s version comes first, then the bundled config’s', () => {
  expect(buildVersion({ native: '1.4.2', config: '1.0.0' })).toBe('1.4.2');
  expect(buildVersion({ native: null, config: '1.0.0' })).toBe('1.0.0');
  expect(buildVersion({ native: undefined, config: ' 2.1 ' })).toBe('2.1');
  // A native version that is not dotted numbers is no version.
  expect(buildVersion({ native: '1.4 (beta)', config: '1.3.0' })).toBe('1.3.0');
});

test('a build that cannot read its version names none, rather than 0.0.0', () => {
  for (const sources of [
    { native: null, config: null },
    { native: undefined, config: undefined },
    { native: '', config: '  ' },
    { native: 'unknown', config: 'dev' },
  ]) {
    expect(buildVersion(sources)).toBeNull();
    expect(clientIdentity('ios', sources)).toBeUndefined();
    expect(clientIdentity('android', sources)).toBeUndefined();
  }
});

test('each app names itself, and anything else names nothing', () => {
  const sources = { native: '1.4.2', config: null };
  expect(clientIdentity('ios', sources)).toEqual({ name: 'ios', version: '1.4.2' });
  expect(clientIdentity('android', sources)).toEqual({ name: 'android', version: '1.4.2' });
  expect(clientIdentity('web', sources)).toBeUndefined();
  expect(clientHeader(clientIdentity('ios', sources)!)).toBe('ios/1.4.2');
});

test('with no identity, no version header is sent and no minimum locks the app out', async () => {
  const minimums = { web: '9.0', ios: '9.0', android: '9.0' };
  const unknown = clientIdentity('ios', { native: null, config: null });
  // The startup check is skipped for an unknown build; asked anyway, an
  // unreadable version is never too old.
  expect(isClientTooOld(minimums, { name: 'ios', version: 'unknown' })).toBe(false);

  const headers: Record<string, string>[] = [];
  const call = async (_path: string, init: { headers?: Record<string, string> } = {}) => {
    headers.push(init.headers ?? {});
    return { data: {}, response: { ok: true, status: 200 } };
  };
  const api = createExchangeApi({
    client: { GET: call, POST: call, PUT: call, PATCH: call, DELETE: call } as never,
    session: { delivery: 'TOKEN', token: () => 'token' },
    newKey: () => 'key',
    identity: unknown,
  });
  await api.me();
  expect(headers).toHaveLength(1);
  expect(Object.keys(headers[0]).map((name) => name.toLowerCase())).not.toContain(
    'x-client-version',
  );
});
