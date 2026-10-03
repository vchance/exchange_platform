import { wordingFor } from '@yuppers/shared';
import { act, fireEvent, renderRouter, screen, waitFor } from 'expo-router/testing-library';
import { Linking, Platform } from 'react-native';

import { notifications, resetNotifications, tap, tapped } from './fake-notifications';
import { DEVICE, EXCHANGE, INVITATION, TOKEN, ana, fakeService, type FakeService } from './fake-service';

/*
 * Push notifications in the app, run as iOS and as Android builds would:
 * when it offers them and asks the system, what it registers with the
 * service, the switch on the account screen, and where a tapped
 * notification leads. `expo-notifications` is the stand-in of
 * `fake-notifications.ts`.
 */

const mockKeychain = new Map<string, string>();
jest.mock('expo-secure-store', () => ({
  WHEN_UNLOCKED_THIS_DEVICE_ONLY: 'when-unlocked-this-device-only',
  getItemAsync: jest.fn(async (key: string) => mockKeychain.get(key) ?? null),
  setItemAsync: jest.fn(async (key: string, value: string) => {
    mockKeychain.set(key, value);
  }),
  deleteItemAsync: jest.fn(async (key: string) => {
    mockKeychain.delete(key);
  }),
}));

jest.mock('@react-native-community/datetimepicker', () => {
  const { Platform: platform } = jest.requireActual('react-native');
  if (platform.OS === 'ios') return jest.requireActual('@react-native-community/datetimepicker');
  return {
    __esModule: true,
    default: () => null,
    DateTimePickerAndroid: { open: jest.fn(), dismiss: jest.fn() },
  };
});
jest.mock('expo-application', () => ({ nativeApplicationVersion: '1.2.0' }));
jest.mock('../lib/time-zone', () => ({ deviceTimezone: () => 'America/Chicago' }));
jest.mock('expo-crypto', () => {
  let next = 0;
  return { randomUUID: () => `00000000-0000-4000-8000-${String((next += 1)).padStart(12, '0')}` };
});

let service: FakeService = fakeService();
globalThis.fetch = ((...args: Parameters<typeof fetch>) => service.fetch(...args)) as typeof fetch;

const w = wordingFor('en');
const n = w.mobile.notifications;

/** Starts the app at an address, signed in or not, against a service that sends push or not. */
async function open(
  initialUrl: string,
  {
    signedIn = true,
    push = true,
    prepare,
  }: { signedIn?: boolean; push?: boolean; prepare?: (service: FakeService) => void } = {},
) {
  mockKeychain.clear();
  service = fakeService();
  service.push = push;
  if (signedIn) {
    mockKeychain.set('yuppers.session', TOKEN);
    service.account = ana;
  }
  prepare?.(service);
  await renderRouter('src/app', { initialUrl });
}

const registrations = () => service.sent.filter((request) => request.path === '/v1/me/devices');

beforeEach(() => {
  resetNotifications();
  notifications.projectId = 'test-project-id';
});

test('nothing is asked at launch, and the offer waits for a yup past its draft', async () => {
  // A list of drafts only: nothing to be told about yet.
  await open('/', {
    prepare: (service) => {
      service.exchange = { ...service.exchange, state: 'DRAFT' };
    },
  });
  await screen.findByText(w.home.title);
  await screen.findByText(w.home.groupDrafts);
  expect(screen.queryByText(n.askHeading)).toBeNull();
  expect(notifications.asked).toBe(0);
});

test('turning notifications on from the list asks the system then, and registers the device', async () => {
  await open('/');
  await screen.findByText(n.askHeading);
  screen.getByText(n.askBody);
  // Shown, but nothing asked of the system until the person chooses.
  expect(notifications.asked).toBe(0);
  expect(registrations()).toEqual([]);

  await fireEvent.press(screen.getByText(n.turnOn));
  await waitFor(() => expect(screen.queryByText(n.askHeading)).toBeNull());
  expect(notifications.asked).toBe(1);
  expect(notifications.tokensFor).toEqual(['test-project-id']);
  // Android asks only once a channel exists, so it is made first, in the
  // person's language.
  if (Platform.OS === 'android') expect(notifications.channels.default).toMatchObject({ name: n.channel });
  else expect(notifications.channels).toEqual({});

  expect(registrations()).toHaveLength(1);
  expect(registrations()[0]).toMatchObject({
    method: 'PUT',
    authorization: `Bearer ${TOKEN}`,
    body: {
      token: 'ExponentPushToken[device-token]',
      platform: Platform.OS,
      app_version: '1.2.0',
      language: 'en',
    },
  });
  expect(mockKeychain.get('yuppers.push.device')).toBe(DEVICE);
  expect(mockKeychain.get('yuppers.push.preference')).toBe('on');
});

test('not now puts the offer away for good and asks nothing', async () => {
  await open('/');
  await screen.findByText(n.askHeading);
  await fireEvent.press(screen.getByText(n.notNow));
  await waitFor(() => expect(screen.queryByText(n.askHeading)).toBeNull());
  expect(notifications.asked).toBe(0);
  expect(registrations()).toEqual([]);
  expect(mockKeychain.get('yuppers.push.preference')).toBe('dismissed');
});

test('refused at the system prompt, nothing is registered and the offer goes', async () => {
  notifications.answer = { granted: false, canAskAgain: false };
  await open('/');
  await screen.findByText(n.askHeading);
  await fireEvent.press(screen.getByText(n.turnOn));
  await waitFor(() => expect(screen.queryByText(n.askHeading)).toBeNull());
  expect(notifications.asked).toBe(1);
  expect(registrations()).toEqual([]);
});

test('nothing about notifications without an Expo project', async () => {
  const info = jest.spyOn(console, 'info').mockImplementation(() => {});
  const environment = process.env.NODE_ENV;
  process.env.NODE_ENV = 'development';
  notifications.projectId = null;
  await open('/');
  await screen.findByText('With Ben Ortiz');
  expect(screen.queryByText(n.askHeading)).toBeNull();
  // Said once, for whoever runs a development build.
  await waitFor(() =>
    expect(info.mock.calls.filter(([said]) => String(said).includes('eas init'))).toHaveLength(1),
  );
  process.env.NODE_ENV = environment;
  info.mockRestore();
  expect(notifications.tokensFor).toEqual([]);
});

test('nothing on the list when the service sends no push notifications', async () => {
  await open('/', { push: false });
  await screen.findByText('With Ben Ortiz');
  expect(screen.queryByText(n.askHeading)).toBeNull();
});

test('no switch on the account screen when the service sends no push notifications', async () => {
  await open('/account', { push: false });
  await screen.findByText(w.profile.title);
  expect(screen.queryByText(n.switch)).toBeNull();
  expect(notifications.asked).toBe(0);
  expect(notifications.tokensFor).toEqual([]);
});

test('signed out, an invitation asks nothing about notifications', async () => {
  await open(`/en/i#${INVITATION}`, { signedIn: false });
  await screen.findByText(w.invitation.signInToRead);
  expect(screen.queryByText(n.askHeading)).toBeNull();
  expect(notifications.asked).toBe(0);
  expect(registrations()).toEqual([]);
});

test('the account screen switch registers and removes the device', async () => {
  await open('/account');
  const toggle = await screen.findByLabelText(n.switch);
  expect(toggle.props.accessibilityState).toMatchObject({ checked: false, disabled: false });

  await fireEvent(toggle, 'valueChange', true);
  await waitFor(() =>
    expect(screen.getByLabelText(n.switch).props.accessibilityState).toMatchObject({
      checked: true,
    }),
  );
  expect(notifications.asked).toBe(1);
  expect(service.devices.has(DEVICE)).toBe(true);

  await fireEvent(screen.getByLabelText(n.switch), 'valueChange', false);
  await waitFor(() =>
    expect(screen.getByLabelText(n.switch).props.accessibilityState).toMatchObject({
      checked: false,
    }),
  );
  expect(service.sent.at(-2)).toMatchObject({
    method: 'DELETE',
    path: `/v1/me/devices/${DEVICE}`,
  });
  expect(service.devices.size).toBe(0);
  expect(mockKeychain.has('yuppers.push.device')).toBe(false);
  expect(mockKeychain.get('yuppers.push.preference')).toBe('off');
});

test('turned off in the system settings, the switch says so and leads there', async () => {
  notifications.permission = { granted: false, canAskAgain: false };
  const settings = jest.spyOn(Linking, 'openSettings').mockResolvedValue();
  await open('/account');
  const toggle = await screen.findByLabelText(n.switch);
  expect(toggle.props.accessibilityState).toMatchObject({ checked: false, disabled: true });
  screen.getByText(n.blocked);
  await fireEvent.press(screen.getByText(n.openSettings));
  expect(settings).toHaveBeenCalled();
  settings.mockRestore();
});

const phoneOnly = (service: FakeService) => {
  service.account = { ...ana, email: null, phone: '+15555550123' };
};

test('a phone-only account is told push is how it hears about its yups', async () => {
  await open('/account', { prepare: phoneOnly });
  await screen.findByText(n.phoneOnly);
});

test('a phone-only account without push is told nothing will tell it', async () => {
  await open('/account', { push: false, prepare: phoneOnly });
  await screen.findByText(n.phoneOnlyNoPush);
});

test('at each launch signed in, a device turned on here is registered again', async () => {
  notifications.permission = { granted: true, canAskAgain: true };
  mockKeychain.clear();
  service = fakeService();
  service.push = true;
  service.account = ana;
  mockKeychain.set('yuppers.session', TOKEN);
  mockKeychain.set('yuppers.push.preference', 'on');
  await renderRouter('src/app', { initialUrl: '/' });
  await screen.findByText(w.home.title);
  await waitFor(() => expect(registrations()).toHaveLength(1));
  expect(notifications.asked).toBe(0);
});

test('signing out forgets this device’s choice', async () => {
  await open('/account');
  await fireEvent(await screen.findByLabelText(n.switch), 'valueChange', true);
  await waitFor(() => expect(mockKeychain.get('yuppers.push.preference')).toBe('on'));
  await fireEvent.press(screen.getByText(w.nav.signOut));
  await screen.findByText(w.signIn.intro);
  await waitFor(() => expect(mockKeychain.has('yuppers.push.preference')).toBe(false));
  expect(mockKeychain.has('yuppers.push.device')).toBe(false);
  // The service let the device go with the session.
  expect(service.devices.size).toBe(0);
});

test('a notification tapped while the app runs opens its exchange', async () => {
  await open('/');
  await screen.findByText(w.home.title);
  await act(async () => {
    tap(tapped('n-1', { url: `/exchanges/${EXCHANGE}` }));
  });
  await screen.findByText('Yup with Ben Ortiz');
});

test('a notification that started the app opens its exchange once', async () => {
  notifications.lastResponse = tapped('n-2', { url: `/exchanges/${EXCHANGE}` });
  await open('/');
  await screen.findByText('Yup with Ben Ortiz');
  expect(notifications.lastResponse).toBeNull();
});

test('a notification arriving while the app is open is not shown', async () => {
  await open('/');
  await screen.findByText(w.home.title);
  expect(await notifications.handler?.handleNotification({})).toEqual({
    shouldShowBanner: false,
    shouldShowList: false,
    shouldPlaySound: false,
    shouldSetBadge: false,
  });
});
