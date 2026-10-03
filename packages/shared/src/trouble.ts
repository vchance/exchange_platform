import type { ExchangeView } from '@exchange/api-client'

import { movePanel, movesFor, otherSlot, statusesOf, type Move } from './fulfillment'
import { isMoney } from './money'

/*
 * "Something isn't working": one way in to the ways out of an agreement in
 * force (DESIGN.md §5.3). Waiving an item, ending by agreement and closing
 * without agreement each release different people from different things,
 * and a dispute is recorded, never decided. This asks what the situation is
 * and leads to the existing actions that fit it. It sends nothing itself and
 * adds no command: each way it offers opens the panel that action already
 * has, where the person confirms it as they always would.
 */

export type TroubleSituation = 'THEY_HAVENT' | 'CANT_DO_MINE' | 'BOTH_STOP' | 'DISAGREE'

/** The situations, in the order they are offered. */
export const TROUBLE_SITUATIONS: readonly TroubleSituation[] = [
  'THEY_HAVENT',
  'CANT_DO_MINE',
  'BOTH_STOP',
  'DISAGREE',
]

/** An existing action the guide can lead to. */
export type TroubleWay =
  | 'DISPUTE'
  | 'WAIVE'
  | 'RECLAIM'
  | 'PROPOSE_END'
  | 'AGREE_END'
  | 'REQUEST_CLOSE'
  | 'AMEND'

/** Something the guide says when a way is not open right now. */
export type TroubleNote =
  | 'nothingTheyOwe'
  | 'nothingInDoubt'
  | 'endPending'
  | 'closePending'
  | 'amendPending'

export interface TroubleItem {
  id: string
  description: string
  money: boolean
  /** The panel that opens this action on this item. */
  panel: string
}

export interface TroubleOffer {
  way: TroubleWay
  /**
   * For an action on one item, the items it can be taken on, each with the
   * panel that takes it. Empty for an action on the whole exchange.
   */
  items: TroubleItem[]
  /**
   * The panel that opens it, for an action on the whole exchange. `null`
   * for a change to the agreement, which is written in the composer.
   */
  panel: string | null
}

export interface TroubleRoute {
  offers: TroubleOffer[]
  notes: TroubleNote[]
}

/** The panel the guide itself is, opened at a situation or at the question. */
export function troublePanel(situation?: TroubleSituation): string {
  return situation ? `trouble:${situation}` : 'trouble'
}

/** Whether a panel name is the guide's, and if so at which situation. */
export function troubleSituationOf(panel: string | null): TroubleSituation | null | undefined {
  if (panel === 'trouble') return null
  const found = panel?.startsWith('trouble:') ? panel.slice('trouble:'.length) : undefined
  return TROUBLE_SITUATIONS.find((situation) => situation === found)
}

/** Whether the guide is offered: only on an agreement in force. */
export function troubleOffered(exchange: ExchangeView): boolean {
  return exchange.state === 'ACTIVE' && exchange.in_force_revision != null
}

/**
 * The ways forward for a situation, from where the exchange stands now.
 * Only actions the person could take at this moment are offered; what the
 * service then allows is still for the service to say.
 */
export function troubleRoute(exchange: ExchangeView, situation: TroubleSituation): TroubleRoute {
  const offers: TroubleOffer[] = []
  const notes: TroubleNote[] = []
  const terms = exchange.in_force_revision?.terms
  if (!troubleOffered(exchange) || !terms) return { offers, notes }

  const you = exchange.you
  const statuses = statusesOf(exchange)

  /** The items where `move` is open to the reader, among those `from` gives. */
  function itemsFor(move: Move, from: 'yours' | 'theirs', only?: (status: string) => boolean) {
    return terms!.contributions.flatMap((contribution) => {
      const theirs = contribution.from === otherSlot(you)
      if ((from === 'theirs') !== theirs) return []
      const status = statuses.get(contribution.id) ?? 'PENDING'
      if (only && !only(status)) return []
      if (!movesFor(status, theirs ? 'RECIPIENT' : 'PROVIDER').includes(move)) return []
      return [
        {
          id: contribution.id,
          description: contribution.description,
          money: isMoney(contribution),
          panel: movePanel(contribution.id, move),
        },
      ]
    })
  }

  function offerItems(way: TroubleWay, items: TroubleItem[]) {
    if (items.length > 0) offers.push({ way, items, panel: null })
  }

  /** Ending together: agree to the other's proposal, or propose it. */
  function endTogether() {
    const by = exchange.end_proposed_by ?? null
    if (by === null) offers.push({ way: 'PROPOSE_END', items: [], panel: 'propose-end' })
    else if (by !== you) offers.push({ way: 'AGREE_END', items: [], panel: 'agree-end' })
    else notes.push('endPending')
  }

  function closeAlone() {
    if (exchange.close_requested_by) notes.push('closePending')
    else offers.push({ way: 'REQUEST_CLOSE', items: [], panel: 'request-close' })
  }

  switch (situation) {
    case 'THEY_HAVENT': {
      const dispute = itemsFor('DISPUTE', 'theirs')
      const waive = itemsFor('WAIVE', 'theirs')
      if (dispute.length === 0 && waive.length === 0) notes.push('nothingTheyOwe')
      offerItems('DISPUTE', dispute)
      offerItems('WAIVE', waive)
      endTogether()
      closeAlone()
      break
    }
    case 'CANT_DO_MINE':
      if (exchange.open_revision) notes.push('amendPending')
      else offers.push({ way: 'AMEND', items: [], panel: null })
      endTogether()
      closeAlone()
      break
    case 'BOTH_STOP':
      endTogether()
      break
    case 'DISAGREE': {
      const dispute = itemsFor('DISPUTE', 'theirs')
      const reclaim = itemsFor('RECLAIM', 'yours')
      // Letting go of something already disputed is a way out too.
      const waive = itemsFor('WAIVE', 'theirs', (status) => status === 'DISPUTED')
      if (dispute.length + reclaim.length + waive.length === 0) notes.push('nothingInDoubt')
      offerItems('DISPUTE', dispute)
      offerItems('RECLAIM', reclaim)
      offerItems('WAIVE', waive)
      closeAlone()
      break
    }
  }
  return { offers, notes }
}
