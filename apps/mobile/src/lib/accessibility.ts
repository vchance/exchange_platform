import type { FocusKeeper } from '@yuppers/shared';
import { useEffect, useState } from 'react';
import { AccessibilityInfo, Platform, type View } from 'react-native';

/*
 * What the app does for a screen reader beyond labels and roles: saying
 * things that change without the focus moving, putting the screen reader's
 * focus back where it was, and following the setting for less motion.
 */

/** Says something to a screen reader without moving its focus. */
export function announce(text: string): void {
  if (!text) return;
  try {
    AccessibilityInfo.announceForAccessibility(text);
  } catch {
    // Not every platform can; what was to be announced is on screen regardless.
  }
}

/** Moves a screen reader's focus to `target`, once what is changing has been drawn. */
export function focusOn(target: View | null): void {
  if (!target || Platform.OS === 'web') return;
  setTimeout(() => {
    try {
      AccessibilityInfo.sendAccessibilityEvent(target, 'focus');
    } catch {
      // Gone in the meantime; the screen reader stays where it is.
    }
  }, 0);
}

// The control pressed last, kept by `Button`, so a panel it opens can give
// the screen reader's focus back to it when the panel is cancelled.
let pressed: View | null = null;
let opener: View | null = null;

export function notePressed(control: View | null): void {
  pressed = control;
}

/** For the shared action runner: where the focus goes around a panel. */
export const focusKeeper: FocusKeeper = {
  remember() {
    opener = pressed;
  },
  restore() {
    focusOn(opener);
  },
};

/** Whether the person has asked the system for less motion. */
export function useReduceMotion(): boolean {
  const [reduce, setReduce] = useState(false);
  useEffect(() => {
    let current = true;
    AccessibilityInfo.isReduceMotionEnabled().then(
      (enabled) => {
        if (current) setReduce(enabled);
      },
      () => {},
    );
    const subscription = AccessibilityInfo.addEventListener('reduceMotionChanged', setReduce);
    return () => {
      current = false;
      subscription.remove();
    };
  }, []);
  return reduce;
}
