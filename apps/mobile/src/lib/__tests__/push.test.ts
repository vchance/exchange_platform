import { isOn, notificationPath, shouldOffer, type PushState } from '../push';

const EXCHANGE = '0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70';

test('a notification opens only an exchange’s own address', () => {
  expect(notificationPath({ url: `/exchanges/${EXCHANGE}` })).toBe(`/exchanges/${EXCHANGE}`);
  for (const data of [
    null,
    undefined,
    {},
    { url: 42 },
    { url: `/exchanges/${EXCHANGE}/record` },
    { url: `https://evil.example/exchanges/${EXCHANGE}` },
    { url: `//evil.example/exchanges/${EXCHANGE}` },
    { url: '/account' },
    { url: `/exchanges/${EXCHANGE}?x=1` },
    { url: `/exchanges/${EXCHANGE.toUpperCase()}` },
  ]) {
    expect(notificationPath(data)).toBeNull();
  }
});

const state = (changes: Partial<PushState>): PushState => ({
  available: true,
  permission: 'undetermined',
  preference: null,
  registered: false,
  ...changes,
});

test('the switch is on only when allowed, chosen here and registered', () => {
  expect(isOn(state({ permission: 'granted', preference: 'on', registered: true }))).toBe(true);
  for (const changes of [
    { available: false },
    { permission: 'denied' as const },
    { preference: 'off' as const },
    { registered: false },
  ]) {
    expect(isOn(state({ permission: 'granted', preference: 'on', registered: true, ...changes }))).toBe(
      false,
    );
  }
});

test('turning on is offered once, and never where the system has refused', () => {
  expect(shouldOffer(state({}))).toBe(true);
  expect(shouldOffer(state({ permission: 'granted' }))).toBe(true);
  for (const changes of [
    { available: false },
    { permission: 'denied' as const },
    { preference: 'dismissed' as const },
    { preference: 'off' as const },
    { preference: 'on' as const },
  ]) {
    expect(shouldOffer(state(changes))).toBe(false);
  }
});
