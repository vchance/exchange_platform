import { useEffect } from 'react'

/*
 * What a screen reader is told when something changes on the page without
 * the focus moving there. `LiveRegions` puts two live regions on the page
 * from the start, one polite and one assertive, and they stay there: a
 * region added together with its text is not reliably announced, and one
 * that is already there is.
 *
 * Polite is for outcomes and status ("Saved.", "This exchange was updated.").
 * Assertive is for a refusal that stops what the person was doing.
 */

export type Politeness = 'polite' | 'assertive'

type Listener = (politeness: Politeness, text: string) => void
const listeners = new Set<Listener>()

/** Tells a screen reader `text`, without moving the focus. */
export function announce(text: string, politeness: Politeness = 'polite'): void {
  if (!text) return
  for (const listener of listeners) listener(politeness, text)
}

/** Where `LiveRegions` hears what to say. */
export function onAnnouncement(listener: Listener): () => void {
  listeners.add(listener)
  return () => {
    listeners.delete(listener)
  }
}

/** Announces `text` when it appears and each time it changes. */
export function useAnnouncement(
  text: string | null | undefined | false,
  politeness: Politeness = 'polite',
): void {
  useEffect(() => {
    if (text) announce(text, politeness)
  }, [text, politeness])
}
