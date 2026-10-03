// Every test runs with `expo-notifications` replaced by a stand-in it can
// control (`fake-notifications.ts`): the app imports it at startup, and the
// native module behind it does not exist here.
jest.mock(
  'expo-notifications',
  () => jest.requireActual('./fake-notifications').notificationsModule,
);

// And with the Expo project the build names, which only `eas init` gives a
// real one, as `fake-notifications.ts` says: none unless a test sets one, as
// in the repository today.
jest.mock('expo-constants', () => {
  const actual = jest.requireActual('expo-constants');
  const { notifications } = jest.requireActual('./fake-notifications');
  return {
    ...actual,
    __esModule: true,
    default: new Proxy(actual.default, {
      get(target, key) {
        if (key === 'expoConfig') {
          const config = target.expoConfig ?? {};
          const id: string | null = notifications.projectId;
          return { ...config, extra: id ? { ...config.extra, eas: { projectId: id } } : config.extra };
        }
        if (key === 'easConfig') return null;
        return Reflect.get(target, key);
      },
    }),
  };
});
