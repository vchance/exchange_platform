import { parseVersion } from '@yuppers/shared';
import Constants from 'expo-constants';
import * as Notifications from 'expo-notifications';
import * as SecureStore from 'expo-secure-store';
import { useRouter, type Href } from 'expo-router';
import { useCallback, useEffect, useState } from 'react';
import { AppState, Platform } from 'react-native';

import { api, CLIENT } from './session';

/*
 * Push notifications on this device (DESIGN.md §12, §13.1).
 *
 * The app asks for permission only when the person chooses to turn
 * notifications on: from a card on the list of yups, shown once they have
 * sent a yup or joined one, or from the switch on the account screen. Never
 * at the first launch and never before signing in. With permission, it
 * gets an Expo push token and registers it with the service, which then
 * sends each notice there as well as by email.
 *
 * Nothing here runs unless three things hold: the app is on iOS or Android,
 * the build names an Expo project (`eas init`; without one there is no push
 * token to get), and the service says it sends push notifications
 * (`GET /v1/meta`). Otherwise the app offers nothing about notifications.
 *
 * What a notification says is generic and opens its exchange when tapped
 * (`useNotificationTaps`). One that arrives while the app is open is not
 * shown: the person is already looking at the app, and the email still
 * comes. `push.web.ts` stands in for this file in the browser harness.
 */

/** The Android channel notifications arrive on; the service names it too. */
export const CHANNEL = 'default';

/** What the person chose here, kept on the device. */
export type PushPreference = 'on' | 'off' | 'dismissed' | null;

export type Permission = 'granted' | 'denied' | 'undetermined';

export interface PushState {
  /** Whether this build, on this device, against this service, can have push at all. */
  available: boolean;
  /** What the system allows. */
  permission: Permission;
  preference: PushPreference;
  /** Whether this device is registered with the service now. */
  registered: boolean;
}

/** The outcome of turning notifications on. */
export type TurnedOn = 'on' | 'blocked' | 'unavailable' | 'failed';

const PREFERENCE_KEY = 'yuppers.push.preference';
const DEVICE_KEY = 'yuppers.push.device';
const store: SecureStore.SecureStoreOptions = {
  keychainAccessible: SecureStore.WHEN_UNLOCKED_THIS_DEVICE_ONLY,
};

async function read(key: string): Promise<string | null> {
  try {
    return await SecureStore.getItemAsync(key, store);
  } catch {
    return null;
  }
}

async function write(key: string, value: string | null): Promise<void> {
  try {
    if (value === null) await SecureStore.deleteItemAsync(key, store);
    else await SecureStore.setItemAsync(key, value, store);
  } catch {
    // Not kept: the app asks again or registers again next time, no worse.
  }
}

/** The platform as the service names it, or `null` where there is no push. */
function platform(): 'ios' | 'android' | null {
  return Platform.OS === 'ios' || Platform.OS === 'android' ? Platform.OS : null;
}

/** The Expo project this build belongs to, which a push token is issued for. */
export function projectId(): string | null {
  const extra = Constants.expoConfig?.extra as { eas?: { projectId?: unknown } } | undefined;
  const id = extra?.eas?.projectId ?? Constants.easConfig?.projectId;
  return typeof id === 'string' && id.trim() !== '' ? id : null;
}

let toldNoProject = false;

/**
 * Whether the service sends push notifications. Asked each time a screen
 * that offers them is shown, so a deployment that turns push on or off is
 * followed without restarting the app.
 */
function serviceSendsPush(): Promise<boolean> {
  return api.meta().then(
    (meta) => meta.push_notifications === true,
    // Nothing is offered while the service cannot say.
    () => false,
  );
}

/** Whether push can be offered at all here. */
export async function pushAvailable(): Promise<boolean> {
  if (!platform()) return false;
  if (!projectId()) {
    // Said once, to whoever runs a development build; not in every test.
    if (__DEV__ && !toldNoProject && process.env.NODE_ENV !== 'test') {
      toldNoProject = true;
      console.info(
        'Push notifications are not offered: this build names no Expo project ' +
          '(run `eas init` and add its project ID to app.json; docs/mobile-release.md).',
      );
    }
    return false;
  }
  return serviceSendsPush();
}

/** What the system allows, as the app needs to know it. */
export async function permission(): Promise<Permission> {
  try {
    const settings = await Notifications.getPermissionsAsync();
    const provisional = settings.ios?.status === Notifications.IosAuthorizationStatus.PROVISIONAL;
    if (settings.granted || provisional) return 'granted';
    return settings.canAskAgain ? 'undetermined' : 'denied';
  } catch {
    return 'denied';
  }
}

/** Everything the screens show about notifications. */
export async function pushState(): Promise<PushState> {
  const available = await pushAvailable();
  if (!available) {
    return { available, permission: 'undetermined', preference: null, registered: false };
  }
  const [allowed, preference, device] = await Promise.all([
    permission(),
    read(PREFERENCE_KEY),
    read(DEVICE_KEY),
  ]);
  return {
    available,
    permission: allowed,
    preference: (preference as PushPreference) ?? null,
    registered: device !== null,
  };
}

/** Whether notifications are on for this device, as the switch shows it. */
export function isOn(state: PushState): boolean {
  return (
    state.available &&
    state.permission === 'granted' &&
    state.preference === 'on' &&
    state.registered
  );
}

/**
 * Whether to offer turning notifications on, on the list: available, not
 * on, not answered here before, and not refused in the system's settings,
 * where a button here could do nothing.
 */
export function shouldOffer(state: PushState): boolean {
  return state.available && state.preference === null && state.permission !== 'denied';
}

/** The Android channel, named in the person's language. Before asking: Android 13 asks only once one exists. */
async function ensureChannel(name: string): Promise<void> {
  if (Platform.OS !== 'android') return;
  await Notifications.setNotificationChannelAsync(CHANNEL, {
    name,
    importance: Notifications.AndroidImportance.DEFAULT,
  });
}

/** Gets this device's token and registers it with the service. */
async function register(language: string): Promise<void> {
  const id = projectId();
  const os = platform();
  if (!id || !os) throw new Error('push is not available here');
  const token = await Notifications.getExpoPushTokenAsync({ projectId: id });
  const version = CLIENT?.version;
  const registered = await api.registerDevice({
    token: token.data,
    platform: os,
    app_version: version && parseVersion(version) ? version : '0.0.0',
    language,
  });
  await write(DEVICE_KEY, registered.id);
}

/**
 * Turns notifications on: asks the system if it has not been asked, and
 * registers this device. `channelName` names the Android channel.
 */
export async function turnOn(language: string, channelName: string): Promise<TurnedOn> {
  if (!(await pushAvailable())) return 'unavailable';
  try {
    await ensureChannel(channelName);
    let allowed = await permission();
    if (allowed === 'undetermined') {
      const asked = await Notifications.requestPermissionsAsync();
      allowed = asked.granted ? 'granted' : asked.canAskAgain ? 'undetermined' : 'denied';
    }
    if (allowed !== 'granted') {
      // Asked and refused: not offered again on the list; the switch stays.
      await write(PREFERENCE_KEY, 'dismissed');
      return 'blocked';
    }
    await register(language);
    await write(PREFERENCE_KEY, 'on');
    return 'on';
  } catch {
    return 'failed';
  }
}

/** Turns notifications off for this device, and tells the service. */
export async function turnOff(): Promise<boolean> {
  await write(PREFERENCE_KEY, 'off');
  const device = await read(DEVICE_KEY);
  if (device === null) return true;
  try {
    await api.removeDevice(device);
    await write(DEVICE_KEY, null);
    return true;
  } catch {
    // Kept, to be removed next time; the preference already says off.
    return false;
  }
}

/** Not now: the list stops offering; the switch on the account screen stays. */
export async function dismissOffer(): Promise<void> {
  await write(PREFERENCE_KEY, 'dismissed');
}

/**
 * Keeps the service's record of this device current, at each launch signed
 * in: a token can change, and the service forgets a device whose session
 * ended. Turned on here and allowed: registered again. Turned off, or
 * refused in the system's settings since: removed.
 */
export async function syncDevice(language: string, channelName: string): Promise<void> {
  const state = await pushState();
  if (!state.available) return;
  try {
    if (state.preference === 'on' && state.permission === 'granted') {
      await ensureChannel(channelName);
      await register(language);
    } else if (state.registered) {
      await turnOff();
      if (state.preference === 'on') await write(PREFERENCE_KEY, 'dismissed');
    }
  } catch {
    // Tried again at the next launch.
  }
}

/**
 * Forgets this device's choice and registration, when the account signs out
 * of it: whoever signs in next decides for themselves. The service has
 * already removed the device with the session.
 */
export async function forgetPush(): Promise<void> {
  if (!platform()) return;
  await write(PREFERENCE_KEY, null);
  await write(DEVICE_KEY, null);
}

/** The state, read when the screen is shown and when the app comes back from the system's settings. */
export function usePush(): {
  state: PushState | null;
  refresh: () => void;
  setState: (state: PushState) => void;
} {
  const [state, setState] = useState<PushState | null>(null);
  const refresh = useCallback(() => {
    pushState().then(setState, () => {});
  }, []);
  useEffect(() => {
    refresh();
    const subscription = AppState.addEventListener('change', (next) => {
      if (next === 'active') refresh();
    });
    return () => subscription.remove();
  }, [refresh]);
  return { state, refresh, setState };
}

// ---- Arriving and tapped --------------------------------------------------

const EXCHANGE_PATH = /^\/exchanges\/[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/;

/**
 * The screen a notification opens: its exchange's own address, and nothing
 * else, whatever else a payload might carry.
 */
export function notificationPath(data: unknown): string | null {
  const url = (data as { url?: unknown } | null | undefined)?.url;
  return typeof url === 'string' && EXCHANGE_PATH.test(url) ? url : null;
}

/** A notification arriving while the app is open is not shown: the person is already here. */
export function installNotificationHandling(): void {
  if (!platform()) return;
  Notifications.setNotificationHandler({
    handleNotification: async () => ({
      shouldShowBanner: false,
      shouldShowList: false,
      shouldPlaySound: false,
      shouldSetBadge: false,
    }),
  });
}

/**
 * Opens the exchange a tapped notification is about: the one that started
 * the app, and any tapped while it runs. Someone signed out is asked to sign
 * in first by the exchange's screen itself.
 */
export function useNotificationTaps(): void {
  const router = useRouter();
  useEffect(() => {
    if (!platform()) return;
    let handled: string | null = null;
    const open = (response: Notifications.NotificationResponse | null) => {
      if (!response) return;
      const id = response.notification.request.identifier;
      if (id === handled) return;
      handled = id;
      const path = notificationPath(response.notification.request.content.data);
      if (path) router.push(path as Href);
      try {
        Notifications.clearLastNotificationResponse();
      } catch {
        // Already gone; it is not opened twice either way.
      }
    };
    try {
      open(Notifications.getLastNotificationResponse());
    } catch {
      // No notification started the app.
    }
    const subscription = Notifications.addNotificationResponseReceivedListener(open);
    return () => subscription.remove();
  }, [router]);
}
