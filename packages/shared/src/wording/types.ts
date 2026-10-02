import type { ErrorCode } from '@exchange/api-client'

/**
 * Every piece of text the product says. Each language implements this whole
 * interface, so a missing translation is a type error (DESIGN.md §4.2).
 * What users write themselves is never translated and does not live here.
 */
export interface Wording {
  productName: string
  tagline: string
  service: {
    checking: string
    connected: string
    unreachable: string
  }
  /** One entry per error code the API can return. */
  errors: Record<ErrorCode, string>
}
