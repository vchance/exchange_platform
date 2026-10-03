import * as SplashScreen from 'expo-splash-screen';
import * as SystemUI from 'expo-system-ui';
import { useEffect } from 'react';

import type { Colors } from './theme';

/*
 * The launch screen, the Yuppers mark on the app's own background in light
 * and in dark (`app.json`, the `expo-splash-screen` plugin), stays up until
 * the app knows whether anyone is signed in, so the first thing shown after
 * it is a real screen rather than "Loading" or a blank one.
 *
 * Asking the service has no time limit of its own, and a slow network must
 * not leave the person looking at a logo, so the launch screen goes after
 * SPLASH_LIMIT_MS whatever has happened; the screens then say they are
 * loading, as they would have.
 */

export const SPLASH_LIMIT_MS = 4000;

/** Called once, as the root layout loads, before anything renders. */
export function holdSplash(): void {
  // Rejects only when there is no launch screen to hold, which is no matter.
  SplashScreen.preventAutoHideAsync().catch(() => {});
}

/** Hides the launch screen once `ready`, or after SPLASH_LIMIT_MS. Hiding twice does nothing. */
export function useSplashUntil(ready: boolean): void {
  useEffect(() => {
    if (ready) {
      SplashScreen.hide();
      return;
    }
    const timer = setTimeout(() => SplashScreen.hide(), SPLASH_LIMIT_MS);
    return () => clearTimeout(timer);
  }, [ready]);
}

/**
 * The window behind every screen takes the app's background, in light and in
 * dark, so that nothing white shows through between the launch screen and
 * the first screen, or behind a screen sliding in.
 */
export function useWindowBackground(colors: Colors): void {
  useEffect(() => {
    SystemUI.setBackgroundColorAsync(colors.background).catch(() => {});
  }, [colors.background]);
}
