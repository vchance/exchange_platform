import type { components, ErrorCode } from '@exchange/api-client'

import type { ProblemCode } from '../draft'
import type { Move } from '../fulfillment'

type Schemas = components['schemas']

/** Why a closed exchange closed, as the service names the reasons. */
export type ClosedReason = 'WITHDRAWN' | 'DECLINED' | 'EXPIRED' | 'CLOSE_REQUEST' | 'INACTIVE'

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
 * build a sentence by joining pieces. `formatMessage` fills one in.
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
  common: {
    loading: string
    cancel: string
    tryAgain: string
    skipToContent: string
    notFoundTitle: string
    notFoundBody: string
    goHome: string
  }
  nav: {
    label: string
    exchanges: string
    account: string
    signOut: string
    language: string
  }
  signIn: {
    title: string
    intro: string
    identifierLabel: string
    identifierHint: string
    sendCode: string
    codeSent: string
    codeLabel: string
    codeHint: string
    submit: string
    resend: string
    resent: string
    changeIdentifier: string
  }
  profile: {
    firstTitle: string
    firstIntro: string
    title: string
    nameLabel: string
    nameHint: string
    nameRequired: string
    languageLabel: string
    adultLabel: string
    adultConfirmed: string
    adultRequired: string
    continue: string
    save: string
    saved: string
    emailLabel: string
    phoneLabel: string
  }
  home: {
    title: string
    start: string
    empty: string
    withParty: string
    noParty: string
    reference: string
    updated: string
    tooManyToday: string
  }
  states: Record<Schemas['StateDto'], string>
  outcomes: Record<Schemas['OutcomeDto'], string>
  closedReasons: Record<ClosedReason, string>
  party: {
    you: string
    other: string
    nameYou: string
  }
  contributionTypes: Record<Schemas['ContributionType'], string>
  contributionStatus: Record<Schemas['Status'], string>
  /** Labels around an agreement's terms. The terms themselves are the parties' own words. */
  terms: {
    ownWords: string
    partiesHeading: string
    termsHeading: string
    youProvide: string
    otherProvides: string
    nothing: string
    required: string
    optional: string
    quantity: string
    quantityWithUnit: string
    amount: string
    criteriaLabel: string
    dueOnAgreement: string
    dueOnDate: string
    dueAfter: string
    overdue: string
    timezone: string
    fingerprint: string
  }
  composer: {
    titleFirst: string
    titleCounter: string
    titleAmend: string
    introFirst: string
    introCounter: string
    introAmend: string
    partiesLegend: string
    yourName: string
    otherName: string
    termsLabel: string
    termsHint: string
    itemsHeading: string
    itemLegend: string
    addYours: string
    addTheirs: string
    remove: string
    fromLabel: string
    typeLabel: string
    descriptionLabel: string
    amountLabel: string
    amountPreview: string
    quantityLabel: string
    unitLabel: string
    dueLabel: string
    dueOnAgreement: string
    dueOnDate: string
    dueAfter: string
    dateLabel: string
    afterLabel: string
    afterChoose: string
    itemOption: string
    itemOptionBlank: string
    criteriaLabel: string
    requiredLabel: string
    locked: string
    noteLabel: string
    noteHint: string
    saving: string
    saved: string
    saveFailed: string
    review: string
    problemsSummary: string
    /** One entry per thing `buildTerms` can find wrong with a working copy. */
    problems: Record<ProblemCode, string>
    signTitle: string
    signIntro: string
    yourNote: string
    backToEdit: string
    signAndSend: string
    conflict: string
    staleDraft: string
    staleDraftKeep: string
    staleDraftDiscard: string
    notAvailable: string
  }
  /**
   * What a signer is shown before signing. Versioned: `CONSENT_VERSION` names
   * the version these messages are, and changes whenever they do
   * (DESIGN.md §14.1).
   */
  consent: {
    heading: string
    pendingReview: string
    binding: string
    electronic: string
    noJudge: string
    agree: string
  }
  invitationLink: {
    heading: string
    intro: string
    shownOnce: string
    linkLabel: string
    copy: string
    copied: string
    copyFailed: string
    share: string
    /** Sent along with a shared link. Like the preview, never a name, a term or an amount. */
    shareText: string
    unclaimed: string
    reissueIntro: string
    forLabel: string
    forHint: string
    reissue: string
  }
  invitation: {
    title: string
    intro: string
    introSignedIn: string
    notBinding: string
    expires: string
    bound: string
    noteHeading: string
    respond: string
    respondAs: string
    opening: string
    missingTitle: string
    missing: string
    ownInvitation: string
    alreadyResponded: string
  }
  exchange: {
    title: string
    titleNoName: string
    refresh: string
    updated: string
    newer: string
    showLatest: string
    claimedHeading: string
    claimedBody: string
    claimedSigned: string
    confirmCounterparty: string
    notThem: string
    waitingConfirmation: string
    waitingConfirmationSigned: string
    proposalHeading: string
    amendmentHeading: string
    version: string
    sentByYou: string
    sentByOther: string
    expires: string
    noteFromYou: string
    noteFromOther: string
    signedByYou: string
    signedByOther: string
    unsignedByYou: string
    unsignedByOther: string
    accept: string
    decline: string
    counter: string
    withdraw: string
    change: string
    acceptBlocked: string
    declineEnds: string
    declineKeeps: string
    withdrawEnds: string
    withdrawKeeps: string
    confirmDecline: string
    confirmWithdraw: string
    signHeading: string
    signIntro: string
    agreementHeading: string
    agreementSigned: string
    remaining: string
    amend: string
    /** What each fulfillment action is called, and what the person is told before doing it. */
    moves: Record<Move, string>
    moveText: Record<Move, string>
    noteLabel: string
    reasonLabel: string
    remedyLabel: string
    noteRecord: string
    noteRequired: string
    endingHeading: string
    proposeEnd: string
    proposeEndText: string
    sendEndProposal: string
    endProposedByYou: string
    endProposedByOther: string
    cancelEnd: string
    declineEnd: string
    agreeEnd: string
    agreeEndText: string
    confirmAgreeEnd: string
    requestClose: string
    requestCloseText: string
    statementLabel: string
    statementRequiredLabel: string
    sendCloseRequest: string
    closeRequestedByYou: string
    closeRequestedByOther: string
    retractClose: string
    addStatement: string
    sendStatement: string
    statementAdded: string
  }
  /** One entry per error code the API can return. */
  errors: Record<ErrorCode, string>
}
