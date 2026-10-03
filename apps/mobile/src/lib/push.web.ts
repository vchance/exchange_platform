import type { PushState, TurnedOn } from './push';

/*
 * Push notifications in the browser test harness: there are none. A browser
 * has no Expo push token, so the screens offer nothing about notifications,
 * exactly as an app does whose build names no Expo project. The bundler
 * picks this file for the web target alone (`push.ts` is the app's).
 */

export const CHANNEL = 'default';

const unavailable: PushState = {
  available: false,
  permission: 'undetermined',
  preference: null,
  registered: false,
};

export async function pushAvailable(): Promise<boolean> {
  return false;
}

export async function pushState(): Promise<PushState> {
  return unavailable;
}

export function isOn(): boolean {
  return false;
}

export function shouldOffer(): boolean {
  return false;
}

export async function turnOn(): Promise<TurnedOn> {
  return 'unavailable';
}

export async function turnOff(): Promise<boolean> {
  return true;
}

export async function dismissOffer(): Promise<void> {}

export async function syncDevice(): Promise<void> {}

export async function forgetPush(): Promise<void> {}

export function usePush(): {
  state: PushState | null;
  refresh: () => void;
  setState: (state: PushState) => void;
} {
  return { state: unavailable, refresh: () => {}, setState: () => {} };
}

export function notificationPath(): string | null {
  return null;
}

export function installNotificationHandling(): void {}

export function useNotificationTaps(): void {}
