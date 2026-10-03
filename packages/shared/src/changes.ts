import type { components, ExchangeView, RevisionTerms } from '@yuppers/api-client'
import { useEffect, useState } from 'react'

import { amendmentEffects, type AmendmentEffect } from './amendment'
import type { ExchangeApi } from './api'
import { statusesOf } from './fulfillment'
import type { I18n } from './i18n'
import { isMoney } from './money'
import { readWholeRecord, termsOfRevision } from './record'

type Schemas = components['schemas']
type Contribution = Schemas['ContributionDto']
type Slot = Schemas['Slot']
type Status = Schemas['Status']

/*
 * What a proposal changes, for the person asked to sign it: field by field,
 * against what is in force for an amendment, or against the version it
 * answers for a counteroffer. Its author saw this while writing it
 * (`amendment.ts`); the other party sees it before signing, from the same
 * rule.
 */

/** A part of the terms that can change. `name` is a party's name; the rest are an item's. */
export type ChangedField =
  | 'name'
  | 'from'
  | 'type'
  | 'description'
  | 'quantity'
  | 'amount'
  | 'due'
  | 'criteria'
  | 'required'

export type ItemChangeKind = 'ADDED' | 'REMOVED' | 'CHANGED' | 'UNCHANGED'

type ItemField = Exclude<ChangedField, 'name'>

/** One field of an item, before and after, as the terms hold it. */
export type FieldChange =
  | { field: 'from'; before: Slot; after: Slot }
  | { field: 'type'; before: Schemas['ContributionType']; after: Schemas['ContributionType'] }
  | { field: 'description'; before: string; after: string }
  | { field: 'quantity'; before: Schemas['QuantityDto'] | null; after: Schemas['QuantityDto'] | null }
  | { field: 'amount'; before: number | null; after: number | null }
  | { field: 'due'; before: Schemas['DueDto']; after: Schemas['DueDto'] }
  | { field: 'criteria'; before: string | null; after: string | null }
  | { field: 'required'; before: boolean; after: boolean }

const quantityOf = (contribution: Contribution) =>
  contribution.quantity
    ? { amount: contribution.quantity.amount, unit: contribution.quantity.unit ?? null }
    : null

/** Compared as the service compares them: what is signed, written the same way. */
const same = (a: unknown, b: unknown) => JSON.stringify(a ?? null) === JSON.stringify(b ?? null)

/** Every field of one item that differs between two versions, in reading order. */
export function contributionChanges(before: Contribution, after: Contribution): FieldChange[] {
  const changes: FieldChange[] = []
  if (before.from !== after.from) changes.push({ field: 'from', before: before.from, after: after.from })
  if (before.type !== after.type) changes.push({ field: 'type', before: before.type, after: after.type })
  if (before.description !== after.description) {
    changes.push({ field: 'description', before: before.description, after: after.description })
  }
  if (!same(quantityOf(before), quantityOf(after))) {
    changes.push({ field: 'quantity', before: quantityOf(before), after: quantityOf(after) })
  }
  if ((before.amount_minor ?? null) !== (after.amount_minor ?? null)) {
    changes.push({
      field: 'amount',
      before: before.amount_minor ?? null,
      after: after.amount_minor ?? null,
    })
  }
  if (!same(before.due, after.due)) changes.push({ field: 'due', before: before.due, after: after.due })
  if ((before.completion_criteria ?? null) !== (after.completion_criteria ?? null)) {
    changes.push({
      field: 'criteria',
      before: before.completion_criteria ?? null,
      after: after.completion_criteria ?? null,
    })
  }
  if (before.required !== after.required) {
    changes.push({ field: 'required', before: before.required, after: after.required })
  }
  return changes
}

export interface ItemChange {
  id: string
  kind: ItemChangeKind
  /** As the proposal writes it, or as the earlier version did for a removed item. */
  description: string
  money: boolean
  fields: FieldChange[]
  /** For an amendment: what it does to the item, and where the item would stand. */
  effect?: AmendmentEffect
  status?: Status | null
}

export interface ProposalChanges {
  names: { slot: Slot; before: string; after: string }[]
  termsChanged: boolean
  /** In the proposal's order, then the removed ones in the earlier version's. */
  items: ItemChange[]
}

/**
 * What `after` changes from `before`. With `statuses`, where each item of
 * the agreement in force stands, `after` is an amendment, and each item
 * also says what the amendment would do to it (`amendmentEffects`).
 */
export function proposalChanges(
  before: RevisionTerms,
  after: RevisionTerms,
  statuses?: ReadonlyMap<string, Status>,
): ProposalChanges {
  const names = (['A', 'B'] as const).flatMap((slot) => {
    const key = slot === 'A' ? 'party_a_name' : 'party_b_name'
    return before[key] === after[key] ? [] : [{ slot, before: before[key], after: after[key] }]
  })
  const earlier = new Map(before.contributions.map((contribution) => [contribution.id, contribution]))
  const effects = statuses
    ? new Map(amendmentEffects(before, statuses, after.contributions).map((item) => [item.id, item]))
    : null
  const withEffect = (change: ItemChange): ItemChange => {
    const found = effects?.get(change.id)
    return found ? { ...change, effect: found.effect, status: found.status } : change
  }

  const items: ItemChange[] = after.contributions.map((contribution) => {
    const was = earlier.get(contribution.id)
    const fields = was ? contributionChanges(was, contribution) : []
    return withEffect({
      id: contribution.id,
      kind: !was ? 'ADDED' : fields.length > 0 ? 'CHANGED' : 'UNCHANGED',
      description: contribution.description,
      money: isMoney(contribution),
      fields,
    })
  })
  const kept = new Set(after.contributions.map((contribution) => contribution.id))
  for (const contribution of before.contributions) {
    if (kept.has(contribution.id)) continue
    items.push(
      withEffect({
        id: contribution.id,
        kind: 'REMOVED',
        description: contribution.description,
        money: isMoney(contribution),
        fields: [],
      }),
    )
  }
  return { names, termsChanged: before.terms !== after.terms, items }
}

/** A changed field, ready to show. */
export interface FieldChangeText {
  label: string
  /**
   * Set when both values are in the product's words: the whole change as
   * one sentence. Otherwise `before` and `after` hold the parties' own
   * words, to be shown as such under `label`.
   */
  sentence: string | null
  before: string
  after: string
}

/**
 * A changed field in words. `before` and `after` are the two versions'
 * terms, for party names and for an item another is due after.
 */
export function fieldChangeText(
  change: FieldChange,
  i18n: Pick<I18n, 'wording' | 'fmt' | 'day' | 'money'>,
  currency: string,
  terms: { before: RevisionTerms; after: RevisionTerms },
): FieldChangeText {
  const { wording, fmt, day, money } = i18n
  const w = wording.proposalChanges
  const nothing = w.notSet
  const label = w.fields[change.field as ItemField]

  const nameIn = (revision: RevisionTerms, slot: Slot) =>
    slot === 'A' ? revision.party_a_name : revision.party_b_name
  const dueText = (due: Schemas['DueDto'], revision: RevisionTerms) => {
    if (due.kind === 'DATE') return fmt(wording.terms.dueOnDate, { date: day(due.date) })
    if (due.kind === 'ON_AGREEMENT') return wording.terms.dueOnAgreement
    const awaited = revision.contributions.find((other) => other.id === due.contribution)
    return fmt(wording.terms.dueAfter, { description: awaited?.description ?? '' })
  }
  const sentence = (before: string, after: string): FieldChangeText => ({
    label,
    sentence: fmt(w.fieldChange, { field: label, before, after }),
    before,
    after,
  })
  const written = (before: string, after: string): FieldChangeText => ({
    label,
    sentence: null,
    before,
    after,
  })

  switch (change.field) {
    case 'from':
      return written(nameIn(terms.before, change.before), nameIn(terms.after, change.after))
    case 'type':
      return sentence(
        wording.contributionTypes[change.before],
        wording.contributionTypes[change.after],
      )
    case 'description':
      return written(change.before, change.after)
    case 'quantity': {
      const text = (quantity: Schemas['QuantityDto'] | null) =>
        quantity ? [quantity.amount, quantity.unit ?? ''].join(' ').trim() : nothing
      return written(text(change.before), text(change.after))
    }
    case 'amount': {
      const text = (minor: number | null) => (minor == null ? nothing : money(minor, currency))
      return sentence(text(change.before), text(change.after))
    }
    case 'due':
      return sentence(dueText(change.before, terms.before), dueText(change.after, terms.after))
    case 'criteria':
      return written(change.before ?? nothing, change.after ?? nothing)
    case 'required': {
      const text = (required: boolean) => (required ? wording.terms.required : wording.terms.optional)
      return sentence(text(change.before), text(change.after))
    }
  }
}

/** What a proposal waiting for the reader is compared with. */
export interface ProposalBase {
  against: 'IN_FORCE' | 'PREVIOUS'
  /** The number of the version it is compared with. */
  sequence: number
  terms: RevisionTerms
  /** For an amendment: where each item of the agreement in force stands. */
  statuses?: ReadonlyMap<string, Status>
}

/**
 * What the proposal waiting for the reader's signature is compared with:
 * the agreement in force for an amendment, or for a counteroffer the
 * version it answers, read from the record. `null` for a first proposal,
 * the reader's own, while the record is being read, or if it cannot be.
 */
export function useProposalBase(
  api: Pick<ExchangeApi, 'recordPart'>,
  exchange: ExchangeView,
): ProposalBase | null {
  const open = exchange.open_revision ?? null
  const inForce = exchange.in_force_revision ?? null
  const theirs = open !== null && open.author !== exchange.you
  // A counteroffer: there is an earlier version, and nothing in force to compare with.
  const answered =
    theirs && exchange.state === 'NEGOTIATING' && open.sequence > 1 ? open.id : null
  const [previous, setPrevious] = useState<{ for: string; base: ProposalBase | null } | null>(null)

  const id = exchange.id
  useEffect(() => {
    if (!answered) return
    let cancelled = false
    readWholeRecord((from) => api.recordPart(id, from)).then(
      (record) => {
        if (cancelled) return
        const proposal = record.revisions.find((revision) => revision.id === answered)
        const earlier = proposal?.answers
          ? record.revisions.find((revision) => revision.id === proposal.answers!.id)
          : undefined
        setPrevious({
          for: answered,
          base: earlier
            ? { against: 'PREVIOUS', sequence: earlier.sequence, terms: termsOfRevision(earlier) }
            : null,
        })
      },
      () => {
        if (!cancelled) setPrevious({ for: answered, base: null })
      },
    )
    return () => {
      cancelled = true
    }
  }, [api, id, answered])

  if (theirs && exchange.state === 'ACTIVE' && inForce) {
    return {
      against: 'IN_FORCE',
      sequence: inForce.sequence,
      terms: inForce.terms,
      statuses: statusesOf(exchange),
    }
  }
  return answered && previous?.for === answered ? previous.base : null
}
