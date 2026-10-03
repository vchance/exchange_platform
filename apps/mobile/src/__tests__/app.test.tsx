import { wordingFor } from '@exchange/shared';
import { router } from 'expo-router';
import { act, fireEvent, renderRouter, screen, waitFor } from 'expo-router/testing-library';
import { Platform } from 'react-native';

import { redirectSystemPath } from '../app/+native-intent';
import { forgetInvitation, heldInvitation } from '../lib/invitation';
import {
  DRAFT,
  EXCHANGE,
  fakeService,
  INVITATION,
  REPAIR,
  TOKEN,
  ana,
  type FakeService,
} from './fake-service';

/*
 * The whole app, routes and all, run as iOS and as Android builds would run
 * it: the platform's own files are the ones loaded, never the browser
 * stand-ins. There is no simulator on the machines that run this, so it is
 * the nearest thing to opening the app, short of a device. Native modules
 * are replaced by the test preset's mocks.
 */

// The device's secure storage, in memory.
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

// The Android date dialog is a native module with nothing behind it here; the
// iOS picker is an ordinary native view and is rendered as it is.
jest.mock('@react-native-community/datetimepicker', () => {
  const { Platform: platform } = jest.requireActual('react-native');
  if (platform.OS === 'ios') return jest.requireActual('@react-native-community/datetimepicker');
  return {
    __esModule: true,
    default: () => null,
    DateTimePickerAndroid: { open: jest.fn(), dismiss: jest.fn() },
  };
});

// The installed build's version, which the test preset's mock leaves out. A
// build that cannot read one names none (`lib/client-identity.ts`).
jest.mock('expo-application', () => ({ nativeApplicationVersion: '1.2.0' }));

jest.mock('expo-crypto', () => {
  let next = 0;
  return { randomUUID: () => `00000000-0000-4000-8000-${String((next += 1)).padStart(12, '0')}` };
});

// The client takes hold of `fetch` when it is made, so the stand-in for the
// service is in place before any of the app is loaded.
let service: FakeService = fakeService();
globalThis.fetch = ((...args: Parameters<typeof fetch>) => service.fetch(...args)) as typeof fetch;

const w = wordingFor('en');

/** Starts the app at an address, signed in or not. */
async function open(initialUrl: string, { signedIn }: { signedIn: boolean }) {
  mockKeychain.clear();
  service = fakeService();
  if (signedIn) {
    mockKeychain.set('exchange.session', TOKEN);
    service.account = ana;
  }
  // The router's own answers hang off what `renderRouter` returns, so that is
  // handed back as it is, not unwrapped.
  const app = renderRouter('src/app', { initialUrl });
  await app;
  return { app };
}

afterEach(() => forgetInvitation());

test('this is a device build, not the browser harness', () => {
  expect(['ios', 'android']).toContain(Platform.OS);
});

test('signing in keeps the token in secure storage and nowhere else, then asks for the profile', async () => {
  await open('/', { signedIn: false });

  // Nobody is signed in: the first screen is the way to sign in. The app
  // has asked the service nothing but how old a build may be.
  await screen.findByText(w.signIn.intro);
  expect(service.sent.map((request) => request.path)).toEqual(['/v1/meta']);
  expect(service.sent[0].body).toBeNull();

  await fireEvent.changeText(
    screen.getByLabelText(w.signIn.identifierLabel),
    ' ana@example.test ',
  );
  await fireEvent.press(screen.getByText(w.signIn.sendCode));
  await screen.findByLabelText(w.signIn.codeLabel);
  expect(service.sent.at(-1)).toMatchObject({
    path: '/v1/auth/codes',
    body: { identifier: 'ana@example.test' },
  });

  await fireEvent.changeText(screen.getByLabelText(w.signIn.codeLabel), '123456');
  await fireEvent.press(screen.getByRole('button', { name: w.signIn.submit }));

  // A new account is asked for a name and its age before anything else.
  await screen.findByText(w.profile.firstIntro);
  expect(service.sent.find((request) => request.path === '/v1/auth/sessions')?.body).toEqual({
    identifier: 'ana@example.test',
    code: '123456',
    delivery: 'TOKEN',
    language: 'en',
  });
  expect(mockKeychain.get('exchange.session')).toBe(TOKEN);

  // Neither is assumed: leaving them out is refused here.
  await fireEvent.press(screen.getByText(w.profile.continue));
  await screen.findByText(w.profile.nameRequired);
  await screen.findByText(w.profile.adultRequired);

  await fireEvent.changeText(screen.getByLabelText(w.profile.nameLabel), 'Ana Ruiz');
  await fireEvent(screen.getByLabelText(w.profile.adultLabel), 'valueChange', true);
  await fireEvent.press(screen.getByText(w.profile.continue));

  await screen.findByText(w.home.title);
  expect(service.sent.at(-2)).toMatchObject({
    method: 'PATCH',
    path: '/v1/me',
    authorization: `Bearer ${TOKEN}`,
    body: { display_name: 'Ana Ruiz', adult_confirmed: true },
  });
});

test('a session from an earlier launch opens straight onto the exchanges', async () => {
  await open('/', { signedIn: true });
  await screen.findByText(w.home.title);
  await screen.findByText('With Ben Ortiz');
  // Besides asking how old a build may be, which needs no session.
  const asked = service.sent.filter((request) => request.path !== '/v1/meta');
  expect(asked.map((request) => request.path)).toEqual(['/v1/me', '/v1/exchanges']);
  for (const request of asked) expect(request.authorization).toBe(`Bearer ${TOKEN}`);
  expect(service.sent.some((request) => request.path === '/v1/meta')).toBe(true);
  // Every request names the client and its build.
  for (const request of service.sent) {
    expect(request.clientVersion).toMatch(/^(ios|android)\/1\.2\.0$/);
  }
});

test('a token the service no longer honors is dropped, and the app asks to sign in', async () => {
  await open('/', { signedIn: true });
  await screen.findByText(w.home.title);

  mockKeychain.clear();
  service = fakeService();
  mockKeychain.set('exchange.session', 'an-old-token');
  await renderRouter('src/app', { initialUrl: '/' });
  await screen.findByText(w.signIn.intro);
  await waitFor(() => expect(mockKeychain.has('exchange.session')).toBe(false));
});

test('an action on an exchange names its version and carries its own key', async () => {
  await open(`/exchanges/${EXCHANGE}`, { signedIn: true });

  // The agreement in force, in full, with dates and money in the reader's language.
  await screen.findByText('Exchange with Ben Ortiz');
  screen.getByText('Due October 30, 2026');
  screen.getByText('Amount: $450.00');
  screen.getByText('2 required items are still to be confirmed.');

  // Marking delivered opens a panel first; nothing is sent until it is confirmed.
  await fireEvent.press(screen.getByText(w.exchange.moves.CLAIM));
  expect(service.sent.some((request) => request.path.endsWith('/commands'))).toBe(false);
  await fireEvent.changeText(screen.getByLabelText(w.exchange.noteLabel), 'Done this morning');
  await fireEvent.press(screen.getAllByText(w.exchange.moves.CLAIM).at(-1)!);

  await screen.findByText(w.contributionStatus.CLAIMED);
  const command = service.sent.find((request) => request.path.endsWith('/commands'));
  expect(command).toMatchObject({
    method: 'POST',
    path: `/v1/exchanges/${EXCHANGE}/commands`,
    authorization: `Bearer ${TOKEN}`,
    body: {
      expected_version: 7,
      command: {
        type: 'CONTRIBUTION',
        contribution: REPAIR,
        action: 'CLAIM',
        note: 'Done this morning',
      },
    },
  });
  expect(command?.idempotencyKey).toMatch(/^[0-9a-f-]{36}$/);
  await screen.findByText(w.exchange.updated);
});

test('when the exchange changed underneath, it is reloaded and the person is told', async () => {
  await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
  await screen.findByText('Exchange with Ben Ortiz');

  service.conflictNext = true;
  await fireEvent.press(screen.getByText(w.exchange.moves.CLAIM));
  await fireEvent.press(screen.getAllByText(w.exchange.moves.CLAIM).at(-1)!);

  // The refusal says what happened, and the screen shows what the exchange is now.
  await screen.findByText(w.errors.VERSION_CONFLICT);
  await screen.findByText('Ben Ortiz proposed ending this exchange.');
  // The repair is spoken of as a delivery and the payment as money.
  expect(screen.getAllByText(w.contributionStatus.PENDING)).toHaveLength(1);
  expect(screen.getAllByText(w.moneyStatus.PENDING)).toHaveLength(1);
  // The history is read again with it, since the exchange is a newer version.
  await waitFor(() => {
    const after = service.sent.slice(service.sent.findIndex((r) => r.path.endsWith('/commands')));
    expect(after.map((request) => `${request.method} ${request.path}`)).toEqual([
      `POST /v1/exchanges/${EXCHANGE}/commands`,
      `GET /v1/exchanges/${EXCHANGE}`,
      `GET /v1/exchanges/${EXCHANGE}/history`,
    ]);
  });
});

test('a draft opens in the composer, with the platform’s own date control', async () => {
  await open(`/exchanges/${DRAFT}`, { signedIn: true });
  await screen.findByText(w.composer.titleFirst);
  expect(screen.getByLabelText(w.composer.otherName).props.defaultValue).toBe('Ben Ortiz');
  screen.getByText(w.composer.dateLabel);

  // Reviewing shows the complete terms and the consent step; nothing is sent yet.
  await fireEvent.press(screen.getByText(w.composer.review));
  await screen.findByText(w.composer.signIntro);
  screen.getByText('Due October 30, 2026');
  screen.getByText(w.consent.pendingReview);
  expect(screen.getByTestId('consent-sign').props.accessibilityState).toMatchObject({
    disabled: true,
  });
  expect(service.sent.some((request) => request.path.endsWith('/revisions'))).toBe(false);
});

test('an invitation address gives up its token: it is sent in a body and kept out of the route', async () => {
  const { app } = await open(`/en/i#${INVITATION}`, { signedIn: false });

  // The proposal can be read without signing in.
  await screen.findByText(w.invitation.title);
  await screen.findByText(w.invitation.notBinding);
  expect(app.getPathnameWithParams()).toBe('/invitation');
  expect(heldInvitation()).toBe(INVITATION);

  const preview = service.sent.find((request) => request.path === '/v1/invitations/preview');
  expect(preview).toMatchObject({ method: 'POST', body: { token: INVITATION } });
  for (const request of service.sent) expect(request.path).not.toContain(INVITATION);
});

test('a second invitation link arriving while one is open shows the new proposal', async () => {
  await open(`/en/i#${INVITATION}`, { signedIn: false });
  await screen.findByText(w.invitation.notBinding);

  // The system hands the app another link, as it does when one is tapped
  // with the app already open on an invitation.
  const other = 'b4'.repeat(32);
  await act(async () => {
    router.navigate(redirectSystemPath({ path: `exchange://es/i#${other}`, initial: false }));
  });
  await waitFor(() =>
    expect(service.sent.at(-1)).toMatchObject({
      path: '/v1/invitations/preview',
      body: { token: other },
    }),
  );
  await screen.findByText(w.invitation.notBinding);
  expect(heldInvitation()).toBe(other);
  for (const request of service.sent) expect(request.path).not.toContain(other);
});

test('with no link to open, an invitation can be pasted', async () => {
  await open('/invitation', { signedIn: false });
  const pasting = w.mobile.openInvitation;
  await screen.findByText(pasting.intro);

  await fireEvent.changeText(screen.getByLabelText(pasting.label), 'https://example.test/');
  await fireEvent.press(screen.getByText(pasting.open));
  await screen.findByText(pasting.invalid);
  expect(service.sent.filter((request) => request.path !== '/v1/meta')).toEqual([]);

  await fireEvent.changeText(
    screen.getByLabelText(pasting.label),
    `https://app.example/es/i#${INVITATION}`,
  );
  await fireEvent.press(screen.getByText(pasting.open));
  await screen.findByText(w.invitation.notBinding);
  expect(service.sent.at(-1)).toMatchObject({
    path: '/v1/invitations/preview',
    body: { token: INVITATION },
  });
});
