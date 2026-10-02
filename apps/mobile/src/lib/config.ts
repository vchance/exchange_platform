/**
 * Where the service is. A device cannot reach the development machine as
 * "localhost": set EXPO_PUBLIC_API_URL to the machine's LAN address when
 * running on hardware.
 */
export const API_URL = process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8080';

/**
 * Where the web app is served from. An invitation link points there, because
 * the person invited may not have the app (DESIGN.md §8); until the web
 * domain can open the app by itself, the same link can be pasted into it.
 */
export const WEB_URL = process.env.EXPO_PUBLIC_WEB_URL ?? 'http://localhost:5173';
