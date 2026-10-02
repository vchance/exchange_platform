import type { components, RevisionTerms } from '@exchange/api-client'

import { toMinorUnits } from './decimal'
import { isUnchanged, type Draft, type DraftContribution } from './draft'

type ContributionDto = components['schemas']['ContributionDto']
type Status = components['schemas']['Status']

/*
 * What an amendment will do to each item of the agreement, said before it is
 * sent (DESIGN.md §7). This is the rule in `backend/src/domain/amendment.rs`,
 * worked out on the client from the same facts the service has: the agreement
 * in force, where each of its contributions stands, and the terms proposed.
 * The service applies the rule again when the amendment is sent and when it
 * is accepted, and has the last word.
 */

export type AmendmentEffect =
  /** Carried over exactly as it is: it keeps its status. */
  | 'UNTOUCHED'
  /** Its terms differ: it goes back to not delivered yet, its history kept. */
  | 'CHANGED'
  /** Not in the proposed terms: it is removed and its id retired. */
  | 'REMOVED'
  /** Not in the agreement: it starts as not delivered yet. */
  | 'NEW'
  /** Accepted, and changed or removed: the service refuses the amendment. */
  | 'LOCKED'
  /** Removed by an earlier amendment: the id cannot come back. Refused. */
  | 'REUSED'

export interface ItemEffect {
  id: string
  effect: AmendmentEffect
  /** Where it would stand once the amendment is in force; `null` where the amendment is refused. */
  status: Status | null
  /** Its description as the proposal writes it, or as the agreement did for a removed one. */
  description: string
}

/** Whether the amendment would be refused as it stands. */
export function amendmentRefused(effects: readonly ItemEffect[]): boolean {
  return effects.some((item) => item.effect === 'LOCKED' || item.effect === 'REUSED')
}

/**
 * What the service compares when it asks whether a contribution's terms
 * changed: everything that is signed about it, and nothing else.
 */
function signedTerms(contribution: ContributionDto): string {
  return JSON.stringify([
    contribution.from,
    contribution.type,
    contribution.description,
    contribution.quantity ? [contribution.quantity.amount, contribution.quantity.unit ?? null] : null,
    contribution.due,
    contribution.completion_criteria ?? null,
    contribution.required,
    contribution.amount_minor ?? null,
  ])
}

/**
 * The effect of replacing `inForce` with `proposed`, one entry per
 * contribution of either, in the proposal's order and then the removed ones in
 * the agreement's. `statuses` is where every contribution the exchange has
 * had stands, as the service keeps it: one removed earlier is in it as
 * `REMOVED`, though the agreement no longer lists it.
 */
export function amendmentEffects(
  inForce: RevisionTerms,
  statuses: ReadonlyMap<string, Status>,
  proposed: readonly ContributionDto[],
): ItemEffect[] {
  const effects: ItemEffect[] = []
  const current = new Map(inForce.contributions.map((contribution) => [contribution.id, contribution]))

  for (const contribution of proposed) {
    const before = current.get(contribution.id)
    const status = statuses.get(contribution.id)
    let effect: AmendmentEffect
    let next: Status | null
    if (before && status !== undefined && signedTerms(before) === signedTerms(contribution)) {
      ;[effect, next] = ['UNTOUCHED', status]
    } else if (before && status === 'ACCEPTED') {
      ;[effect, next] = ['LOCKED', null]
    } else if (before) {
      ;[effect, next] = ['CHANGED', 'PENDING']
    } else if (status !== undefined) {
      ;[effect, next] = ['REUSED', null]
    } else {
      ;[effect, next] = ['NEW', 'PENDING']
    }
    effects.push({ id: contribution.id, effect, status: next, description: contribution.description })
  }

  const kept = new Set(proposed.map((contribution) => contribution.id))
  for (const contribution of inForce.contributions) {
    if (kept.has(contribution.id)) continue
    const locked = statuses.get(contribution.id) === 'ACCEPTED'
    effects.push({
      id: contribution.id,
      effect: locked ? 'LOCKED' : 'REMOVED',
      status: locked ? null : 'REMOVED',
      description: contribution.description,
    })
  }
  return effects
}

/**
 * A contribution as the working copy would send it, without the checks
 * `buildTerms` makes: enough to ask whether its signed terms still match the
 * agreement's. A copy that is not yet valid is compared as far as it goes.
 */
function loosely(item: DraftContribution, fractionDigits: number): ContributionDto {
  const unit = item.unit.trim()
  const criteria = item.criteria.trim()
  const due: ContributionDto['due'] =
    item.due.kind === 'AFTER_CONTRIBUTION' && item.due.contribution === ''
      ? { kind: 'ON_AGREEMENT' }
      : item.due
  return {
    id: item.id,
    from: item.from,
    type: item.type,
    description: item.description.trim(),
    quantity:
      item.type !== 'MONEY' && item.quantity
        ? { amount: item.quantity, unit: unit === '' ? null : unit }
        : null,
    due,
    completion_criteria: criteria === '' ? null : criteria,
    required: item.required,
    amount_minor:
      item.type === 'MONEY' && item.amount ? toMinorUnits(item.amount, fractionDigits) : null,
  }
}

/**
 * The effects of the working copy as it is being written, against the
 * agreement in force. A contribution the person has not touched is the
 * agreement's own, exactly as `buildTerms` sends it; one they have touched is
 * compared on what would be signed, so putting a field back as it was makes
 * the item untouched again.
 */
export function draftEffects(
  draft: Draft,
  inForce: RevisionTerms,
  statuses: ReadonlyMap<string, Status>,
  fractionDigits: number,
): ItemEffect[] {
  const proposed = draft.contributions.map((item) => {
    const original = inForce.contributions.find((contribution) => contribution.id === item.id)
    return original && isUnchanged(item, original, fractionDigits)
      ? original
      : loosely(item, fractionDigits)
  })
  return amendmentEffects(inForce, statuses, proposed)
}
