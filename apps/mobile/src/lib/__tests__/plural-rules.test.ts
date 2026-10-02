import { formatMessage, languages, wordingFor } from '@exchange/shared';

import { installPluralRules, pluralCategory } from '../plural-rules';

describe('plural forms on an engine without Intl.PluralRules', () => {
  const native = Intl.PluralRules;

  beforeEach(() => {
    // Hermes has none.
    Reflect.deleteProperty(Intl, 'PluralRules');
  });

  afterEach(() => {
    Object.defineProperty(Intl, 'PluralRules', {
      value: native,
      configurable: true,
      writable: true,
    });
  });

  test('each language takes its own forms', () => {
    expect(pluralCategory(1, 'en')).toBe('one');
    expect(pluralCategory(2, 'en')).toBe('other');
    expect(pluralCategory(0, 'en')).toBe('other');
    expect(pluralCategory(1, 'es')).toBe('one');
    expect(pluralCategory(5, 'es')).toBe('other');
    // Languages the product does not have yet already work.
    expect(pluralCategory(2, 'ar')).toBe('two');
    expect(pluralCategory(3, 'ru')).toBe('few');
    expect(pluralCategory(7, 'ja')).toBe('other');
  });

  test('a regional tag uses its own rule or its base language’s', () => {
    expect(pluralCategory(1, 'es-MX')).toBe('one');
    expect(pluralCategory(1, 'en-GB')).toBe('one');
    expect(pluralCategory(3, 'zh-Hant')).toBe('other');
  });

  test('an unknown language falls back to the form every message has', () => {
    expect(pluralCategory(1, 'xx')).toBe('other');
  });

  test('the same answers as the engine’s own rules, for every language the product has', () => {
    for (const { code } of languages) {
      const reference = new native(code);
      for (const count of [0, 1, 2, 3, 5, 11, 21, 100, 101, 1000000]) {
        expect(`${code} ${count} ${pluralCategory(count, code)}`).toBe(
          `${code} ${count} ${reference.select(count)}`,
        );
      }
    }
  });

  test('once installed, the wording’s plural messages read correctly', () => {
    const message = wordingFor('en').exchange.remaining;
    // Without it the shared formatting falls back to the plural form.
    expect(formatMessage(message, { count: 1 }, 'en')).toBe(
      '1 required items are still to be confirmed.',
    );

    installPluralRules();
    expect(formatMessage(message, { count: 1 }, 'en')).toBe(
      '1 required item is still to be confirmed.',
    );
    expect(formatMessage(message, { count: 2 }, 'en')).toBe(
      '2 required items are still to be confirmed.',
    );
    for (const { code } of languages) {
      const summary = wordingFor(code).composer.problemsSummary;
      expect(formatMessage(summary, { count: 1 }, code)).not.toBe(
        formatMessage(summary, { count: 2 }, code).replace('2', '1'),
      );
    }
  });
});

test('an engine that has Intl.PluralRules keeps its own', () => {
  const before = Intl.PluralRules;
  installPluralRules();
  expect(Intl.PluralRules).toBe(before);
});
