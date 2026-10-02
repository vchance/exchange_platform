import { CONSENT_VERSION, type Language } from '@exchange/shared'

/**
 * What a signature says about the consent wording its signer was shown: the
 * language it was shown in and its version. The service stores both with the
 * signature and refuses any version but the current one (DESIGN.md §8).
 */
export function consentShown(language: Language): { language: string; version: string } {
  return { language, version: CONSENT_VERSION }
}
