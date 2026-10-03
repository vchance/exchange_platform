/*
 * A stand-in for `expo-notifications`, put in place for every test by
 * `setup-notifications.ts`: what the system would allow, what asking would
 * get, the token it would give, and the notifications tapped; and the Expo
 * project the build names, which `expo-constants` reports. A test sets what
 * it needs and reads back what the app did.
 */

interface Permission {
  granted: boolean;
  canAskAgain: boolean;
}

interface Response {
  notification: { request: { identifier: string; content: { data: unknown } } };
}

export const notifications = {
  /** The Expo project the build names (`eas init`), or none, as in the repository today. */
  projectId: null as string | null,
  /** What the system allows now. */
  permission: { granted: false, canAskAgain: true } as Permission,
  /** What asking gets: the person's answer to the system's prompt. */
  answer: { granted: true, canAskAgain: false } as Permission,
  /** Times the system's prompt was shown. */
  asked: 0,
  /** Each project a token was asked for. */
  tokensFor: [] as string[],
  token: 'ExponentPushToken[device-token]',
  /** Android channels made, by ID. */
  channels: {} as Record<string, { name: string }>,
  handler: null as null | {
    handleNotification: (notification: unknown) => Promise<Record<string, boolean>>;
  },
  /** The notification that started the app, if one did. */
  lastResponse: null as Response | null,
  listeners: [] as ((response: Response) => void)[],
};

export function resetNotifications(): void {
  notifications.projectId = null;
  notifications.permission = { granted: false, canAskAgain: true };
  notifications.answer = { granted: true, canAskAgain: false };
  notifications.asked = 0;
  notifications.tokensFor = [];
  notifications.channels = {};
  // The handler is set once, when the app's layout is first loaded, and
  // stays: it is part of the app, not of one test.
  notifications.lastResponse = null;
  notifications.listeners = [];
}

/** A notification as the system hands it over once tapped. */
export function tapped(identifier: string, data: unknown): Response {
  return { notification: { request: { identifier, content: { data } } } };
}

/** Taps a notification while the app runs. */
export function tap(response: Response): void {
  for (const listener of [...notifications.listeners]) listener(response);
}

function status(permission: Permission) {
  return {
    ...permission,
    status: permission.granted ? 'granted' : permission.canAskAgain ? 'undetermined' : 'denied',
    expires: 'never',
  };
}

export const notificationsModule = {
  AndroidImportance: { DEFAULT: 5 },
  IosAuthorizationStatus: { NOT_DETERMINED: 0, DENIED: 1, AUTHORIZED: 2, PROVISIONAL: 3 },
  getPermissionsAsync: async () => status(notifications.permission),
  requestPermissionsAsync: async () => {
    notifications.asked += 1;
    notifications.permission = { ...notifications.answer };
    return status(notifications.permission);
  },
  getExpoPushTokenAsync: async ({ projectId }: { projectId: string }) => {
    notifications.tokensFor.push(projectId);
    return { type: 'expo', data: notifications.token };
  },
  setNotificationChannelAsync: async (id: string, channel: { name: string }) => {
    notifications.channels[id] = channel;
    return channel;
  },
  setNotificationHandler: (handler: typeof notifications.handler) => {
    notifications.handler = handler;
  },
  getLastNotificationResponse: () => notifications.lastResponse,
  clearLastNotificationResponse: () => {
    notifications.lastResponse = null;
  },
  addNotificationResponseReceivedListener: (listener: (response: Response) => void) => {
    notifications.listeners.push(listener);
    return {
      remove: () => {
        notifications.listeners = notifications.listeners.filter((held) => held !== listener);
      },
    };
  },
};
