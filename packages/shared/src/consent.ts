/**
 * The version of the consent wording the clients show before a signature
 * (`consent` in the wording files). The service refuses a signature that
 * names any other version, so the two change together.
 *
 * `draft-1` is a stand-in: the wording is a placeholder until counsel has
 * approved the real text (DESIGN.md §14.1).
 */
export const CONSENT_VERSION = 'draft-1'
