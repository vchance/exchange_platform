import { createApiClient } from '@yuppers/api-client';
import { createExchangeApi, type ClientIdentity } from '@yuppers/shared';
import * as Application from 'expo-application';
import Constants from 'expo-constants';
import * as Crypto from 'expo-crypto';
import { Platform } from 'react-native';

import { appBuild, clientIdentity } from './client-identity';
import { API_URL } from './config';
import { recordSharer } from './record-sharer';
import { tokenStore } from './token-store';

/*
 * The app's session (DESIGN.md §8). Signing in asks the service for a token;
 * it is kept in the device's secure storage and sent as a bearer token. It is
 * held in memory while the app runs, so no request waits on the storage.
 */

let token: string | null = null;

/**
 * Which client this is and which build, named to the service on every
 * request so a build too old to act can be told so (`CLIENT_TOO_OLD`). The
 * browser test harness is neither app and names itself as neither, and a
 * build that cannot read its own version names none (`client-identity.ts`).
 */
export const CLIENT: ClientIdentity | undefined = clientIdentity(Platform.OS, {
  native: Application.nativeApplicationVersion,
  config: Constants.expoConfig?.version,
});

/** This build, for the version line at the foot of the account screen. */
export const APP_BUILD = appBuild({
  native: Application.nativeApplicationVersion,
  config: Constants.expoConfig?.version,
  nativeBuild: Application.nativeBuildVersion,
  commit: Constants.expoConfig?.extra?.commit,
});

/** The calls the screens make: the same ones as the web app, with a token session. */
export const api = createExchangeApi({
  client: createApiClient(API_URL),
  session: { delivery: 'TOKEN', token: () => token },
  // Idempotency keys must be unguessable; the engine has no `crypto` of its own.
  newKey: () => Crypto.randomUUID(),
  identity: CLIENT,
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
  // A copy of a record made for sharing does not outlast the session it was made in.
  recordSharer.forget();
  try {
    await tokenStore.clear();
  } catch {
    // Nothing was stored, or it cannot be reached; there is nothing to send either way.
  }
}
