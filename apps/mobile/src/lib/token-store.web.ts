import { Platform } from 'react-native';

import type { TokenStore } from './token-store.types';

/*
 * TEST HARNESS ONLY. The web client people use is `apps/web`, which holds its
 * session in a cookie the page cannot read. This file exists so the app's
 * screens can be exercised in a browser on a development machine, where
 * Expo's secure-store module has no implementation.
 *
 * It is named `.web.ts`, so the bundler resolves it for the web target only:
 * an iOS or Android bundle gets `token-store.ts` and never contains this. The
 * check below refuses to run anywhere else even so.
 */
if (Platform.OS !== 'web') {
  throw new Error('The harness token store was loaded outside the web target');
}

const KEY = 'yuppers.harness.session';

// Kept per browser tab, so two tabs can be two people.
export const tokenStore: TokenStore = {
  read: async () => window.sessionStorage.getItem(KEY),
  write: async (token) => window.sessionStorage.setItem(KEY, token),
  clear: async () => window.sessionStorage.removeItem(KEY),
};
