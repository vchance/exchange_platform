import type { ErrorCode } from '@exchange/api-client'

/**
 * Every notification the service can send: the names in `Notice` in
 * `backend/src/domain/notification.rs`. The backend's tests fail if a
 * language is missing one or has one the service never sends.
 */
export type NotificationKind =
  | 'INVITATION_CLAIMED'
  | 'INVITATION_CLAIMED_UNCONFIRMED'
  | 'COUNTERPARTY_CONFIRMED'
  | 'REVISION_SENT'
  | 'AMENDMENT_PROPOSED'
  | 'ACCEPTANCE_WAITING'
  | 'AGREEMENT_IN_FORCE'
  | 'AMENDMENT_IN_FORCE'
  | 'AMENDMENT_DECLINED'
  | 'AMENDMENT_WITHDRAWN'
  | 'AMENDMENT_EXPIRED'
  | 'DELIVERY_CLAIMED'
  | 'CLAIM_RETRACTED'
  | 'DELIVERY_CONFIRMED'
  | 'DISPUTE_OPENED'
  | 'CONTRIBUTION_WAIVED'
  | 'END_PROPOSED'
  | 'END_PROPOSAL_CANCELLED'
  | 'CLOSE_REQUESTED'
  | 'CLOSE_REQUEST_RETRACTED'
  | 'STATEMENT_ADDED'
  | 'INACTIVITY_PROMPTED'
  | 'CLOSED_WITHDRAWN'
  | 'CLOSED_DECLINED'
  | 'CLOSED_EXPIRED'
  | 'CLOSED_COMPLETED'
  | 'CLOSED_ENDED_BY_AGREEMENT'
  | 'CLOSED_UNRESOLVED'
  | 'CLOSED_INACTIVE'

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
  /**
   * What the service sends when the other party does something. The clients
   * never show these; they live here so that everything the product says is
   * in one file per language. A message may use `{code}` (the exchange's
   * display code) and `{productName}`, and never carries anything from the
   * agreement itself (DESIGN.md §12).
   */
  notifications: {
    email: {
      /** Wraps every message. Must contain `{body}` and `{link}`; may use `{code}` and `{productName}`. */
      layout: string
      messages: Record<NotificationKind, { subject: string; body: string }>
    }
  }
  /** One entry per error code the API can return. */
  errors: Record<ErrorCode, string>
}
