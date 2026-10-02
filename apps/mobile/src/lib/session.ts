import { createApiClient } from '@exchange/api-client';
import { createExchangeApi } from '@exchange/shared';
import * as Crypto from 'expo-crypto';

import { API_URL } from './config';
import { tokenStore } from './token-store';

/*
 * The app's session (DESIGN.md §8). Signing in asks the service for a token;
 * it is kept in the device's secure storage and sent as a bearer token. It is
 * held in memory while the app runs, so no request waits on the storage.
 */

let token: string | null = null;

/** The calls the screens make: the same ones as the web app, with a token session. */
export const api = createExchangeApi({
  client: createApiClient(API_URL),
  session: { delivery: 'TOKEN', token: () => token },
  // Idempotency keys must be unguessable; the engine has no `crypto` of its own.
  newKey: () => Crypto.randomUUID(),
});

/** Reads back the session left by an earlier launch, if there is one. */
export async function restoreSession(): Promise<void> {
  try {
    token = await tokenStore.read();
  } catch {
    // Storage that cannot be read holds nothing usable: sign in again.
    token = null;
  }
}

/** Takes up a session the service has just issued. */
export async function keepSession(issued: string): Promise<void> {
  token = issued;
  try {
    await tokenStore.write(issued);
  } catch {
    // The session still works until the app closes; it is never written
    // anywhere less safe instead.
  }
}

/** Stops acting as the account on this device. */
export async function dropSession(): Promise<void> {
  token = null;
  try {
    await tokenStore.clear();
  } catch {
    // Nothing was stored, or it cannot be reached; there is nothing to send either way.
  }
}
