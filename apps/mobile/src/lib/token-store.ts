import * as SecureStore from 'expo-secure-store';

import type { TokenStore } from './token-store.types';

/*
 * The session token on iOS and Android: the Keychain and the Keystore,
 * through Expo's secure-store module, and never plain app storage
 * (DESIGN.md §8, §13.1).
 *
 * This is the file both platforms bundle. `token-store.web.ts` stands in for
 * it only when the app is run in a browser as a test harness; the bundler
 * picks that file for the web target alone, so it cannot end up in an iOS or
 * Android build.
 */

const KEY = 'yuppers.session';

const options: SecureStore.SecureStoreOptions = {
  // Readable only while the device is unlocked, and never carried to another
  // device in a backup: a session belongs to the device that signed in.
  keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
};

export const tokenStore: TokenStore = {
  read: () => SecureStore.getItemAsync(KEY, options),
  write: (token) => SecureStore.setItemAsync(KEY, token, options),
  clear: () => SecureStore.deleteItemAsync(KEY, options),
};
