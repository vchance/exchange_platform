import type { ErrorCode } from '@exchange/api-client'

/**
 * Every piece of text the product says. Each language's file in `wording/`
 * must supply all of it, so a missing translation fails the build
 * (DESIGN.md §4.2). What users write themselves is never translated and does
 * not live here.
 *
 * Text that contains a number or a name is written as an ICU message, so
 * each language can order the words and choose its own plural forms. Never
 * build a sentence by joining pieces.
 */
export interface Wording {
  productName: string
  tagline: string
  /** What a messaging app shows for a shared link. Never names a person, a term or an amount. */
  linkPreview: {
    title: string
    description: string
  }
  service: {
    checking: string
    connected: string
    unreachable: string
  }
  /** One entry per error code the API can return. */
  errors: Record<ErrorCode, string>
}
