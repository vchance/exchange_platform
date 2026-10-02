import { colorsFor, TOUCH_TARGET, type Colors, type Scheme } from '../theme';

/** Relative luminance and contrast ratio, as WCAG 2 defines them. */
function luminance(hex: string): number {
  const channel = (index: number) => {
    const value = parseInt(hex.slice(1 + index * 2, 3 + index * 2), 16) / 255;
    return value <= 0.03928 ? value / 12.92 : ((value + 0.055) / 1.055) ** 2.4;
  };
  return 0.2126 * channel(0) + 0.7152 * channel(1) + 0.0722 * channel(2);
}

function contrast(a: string, b: string): number {
  const [light, dark] = [luminance(a), luminance(b)].sort((x, y) => y - x);
  return (light + 0.05) / (dark + 0.05);
}

// Every pair of text and the surface it is drawn on.
const textOn: [keyof Colors, keyof Colors][] = [
  ['text', 'background'],
  ['text', 'surface'],
  ['text', 'noticeSurface'],
  ['text', 'warningSurface'],
  ['text', 'dangerSurface'],
  ['muted', 'background'],
  ['muted', 'surface'],
  ['primary', 'background'],
  ['primary', 'surface'],
  ['onPrimary', 'primary'],
  ['danger', 'background'],
  ['danger', 'surface'],
];

test.each<Scheme>(['light', 'dark'])('text is readable in the %s palette', (scheme) => {
  const colors = colorsFor(scheme);
  for (const [text, surface] of textOn) {
    const ratio = contrast(colors[text], colors[surface]);
    // WCAG AA for body text.
    expect(`${text} on ${surface}: ${ratio >= 4.5 ? 'ok' : ratio.toFixed(2)}`).toBe(
      `${text} on ${surface}: ok`,
    );
  }
});

test.each<Scheme>(['light', 'dark'])('control outlines can be seen in the %s palette', (scheme) => {
  const colors = colorsFor(scheme);
  // WCAG 1.4.11: the boundary of a control against what is around it.
  expect(contrast(colors.border, colors.background)).toBeGreaterThanOrEqual(3);
  expect(contrast(colors.border, colors.surface)).toBeGreaterThanOrEqual(3);
});

test('touch targets are at least as large as both platforms ask', () => {
  // 44 points on iOS, 48 density-independent pixels on Android.
  expect(TOUCH_TARGET).toBeGreaterThanOrEqual(48);
});
