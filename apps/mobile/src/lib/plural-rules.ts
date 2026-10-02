import * as cardinals from 'make-plural/cardinals';

/*
 * Hermes, the JavaScript engine the apps run on, has no `Intl.PluralRules`,
 * and the wording needs each language's own plural forms (DESIGN.md §4.2).
 * This supplies the one method the shared message formatting uses, from the
 * same Unicode plural rules a browser has, for every language at once: adding
 * a language to the product needs nothing here.
 *
 * Where the engine does have `Intl.PluralRules`, it is left alone.
 */

type Category = 'zero' | 'one' | 'two' | 'few' | 'many' | 'other';
type Rule = (n: number | string) => Category;

const rules = cardinals as unknown as Record<string, Rule | undefined>;

/** The plural rule for a language tag: its own, else its base language's. */
function ruleFor(language: string): Rule | undefined {
  const tag = language.replace(/-/g, '_');
  return rules[tag] ?? rules[tag.split('_')[0].toLowerCase()];
}

/** The plural form a number takes in a language; `other` for a language without a rule. */
export function pluralCategory(value: number, language: string): Category {
  return ruleFor(language)?.(value) ?? 'other';
}

class PluralRules {
  private readonly language: string;

  constructor(locales?: string | readonly string[]) {
    this.language = (typeof locales === 'string' ? locales : locales?.[0]) ?? 'en';
  }

  select(value: number): Category {
    return pluralCategory(value, this.language);
  }
}

export function installPluralRules(): void {
  if (typeof Intl.PluralRules === 'function') return;
  Object.defineProperty(Intl, 'PluralRules', {
    value: PluralRules,
    configurable: true,
    writable: true,
  });
}
