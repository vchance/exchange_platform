import { useColorScheme } from 'react-native';

/*
 * Plain, accessible defaults: the system font at the size the person has
 * chosen for their device, and two palettes that follow the system's light
 * and dark setting. Every text and background pair here meets WCAG AA
 * contrast for body text.
 */

export interface Colors {
  background: string;
  surface: string;
  text: string;
  muted: string;
  border: string;
  primary: string;
  onPrimary: string;
  danger: string;
  dangerSurface: string;
  warningSurface: string;
  noticeSurface: string;
}

const light: Colors = {
  background: '#ffffff',
  surface: '#f3f5f8',
  text: '#14181d',
  muted: '#4f5966',
  border: '#7a8494',
  primary: '#0b57a4',
  onPrimary: '#ffffff',
  danger: '#a3231c',
  dangerSurface: '#fdeceb',
  warningSurface: '#fff3cf',
  noticeSurface: '#e6f0fb',
};

const dark: Colors = {
  background: '#0f1216',
  surface: '#1b2027',
  text: '#f1f3f5',
  muted: '#b3bcc8',
  border: '#7b8594',
  primary: '#8fc2ff',
  onPrimary: '#06213f',
  danger: '#ffb4ab',
  dangerSurface: '#40160f',
  warningSurface: '#3d3108',
  noticeSurface: '#14283d',
};

export type Scheme = 'light' | 'dark';

export function colorsFor(scheme: Scheme): Colors {
  return scheme === 'dark' ? dark : light;
}

export function useScheme(): Scheme {
  return useColorScheme() === 'dark' ? 'dark' : 'light';
}

export function useColors(): Colors {
  return colorsFor(useScheme());
}

/** Nothing interactive is smaller than this in either direction. */
export const TOUCH_TARGET = 48;

export const space = { xs: 4, s: 8, m: 12, l: 16, xl: 24 } as const;

export const type = {
  body: { fontSize: 17, lineHeight: 24 },
  hint: { fontSize: 15, lineHeight: 21 },
  title: { fontSize: 26, lineHeight: 32, fontWeight: '700' },
  heading: { fontSize: 21, lineHeight: 27, fontWeight: '700' },
  subheading: { fontSize: 18, lineHeight: 24, fontWeight: '600' },
} as const;
