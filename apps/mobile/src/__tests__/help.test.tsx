import { wordingFor } from '@yuppers/shared';
import { fireEvent, renderRouter, screen, waitFor } from 'expo-router/testing-library';
import { Linking } from 'react-native';

import { WEB_URL } from '../lib/config';
import { helpUrl } from '../lib/help';
import { fakeService, TOKEN, ana, type FakeService } from './fake-service';

/*
 * The way to help from the app, as iOS and as Android builds would run it:
 * the help pages are the web app's, opened in the system's browser at the
 * right topic and in the app's language.
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

jest.mock('@react-native-community/datetimepicker', () => ({
  __esModule: true,
  default: () => null,
  DateTimePickerAndroid: { open: jest.fn(), dismiss: jest.fn() },
}));

let service: FakeService = fakeService();
globalThis.fetch = ((...args: Parameters<typeof fetch>) => service.fetch(...args)) as typeof fetch;

const opened = jest.spyOn(Linking, 'openURL').mockResolvedValue(true);

async function open(initialUrl: string, language = 'en') {
  mockKeychain.clear();
  opened.mockClear();
  service = fakeService();
  mockKeychain.set('yuppers.session', TOKEN);
  service.account = { ...ana, language };
  await renderRouter('src/app', { initialUrl });
}

test('the address of a help page names the web app, the topic and the language', () => {
  expect(helpUrl('en')).toBe(`${WEB_URL.replace(/\/+$/, '')}/help?lang=en`);
  expect(helpUrl('es', 'blocking')).toBe(`${WEB_URL.replace(/\/+$/, '')}/help/blocking?lang=es`);
});

test('the account screen’s Help link opens the help pages in the browser', async () => {
  const w = wordingFor('en');
  await open('/account');
  const link = await screen.findByRole('link', { name: w.help.link });
  expect(link.props.accessibilityHint).toBe(w.help.inBrowser);

  await fireEvent.press(link);
  await waitFor(() => expect(opened).toHaveBeenCalledWith(`${WEB_URL}/help?lang=en`));
});

test('in Spanish, it opens them in Spanish', async () => {
  const w = wordingFor('es');
  await open('/account', 'es');
  await fireEvent.press(await screen.findByRole('link', { name: w.help.link }));
  await waitFor(() => expect(opened).toHaveBeenCalledWith(`${WEB_URL}/help?lang=es`));
});

test('“Learn more” on deleting the account opens that topic', async () => {
  const w = wordingFor('en');
  await open('/account');
  await fireEvent.press(await screen.findByRole('link', { name: w.help.learnMore.deletion }));
  await waitFor(() => expect(opened).toHaveBeenCalledWith(`${WEB_URL}/help/deleting?lang=en`));
});
