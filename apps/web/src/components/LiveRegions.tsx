import { useEffect, useState } from 'react'

import { onAnnouncement } from '../lib/announce'

/**
 * The page's two live regions, mounted once in the app's shell and never
 * removed (`lib/announce.ts`). A message is put in after clearing the
 * region, so the same words said twice in a row are read twice.
 */
export function LiveRegions() {
  const [polite, setPolite] = useState('')
  const [assertive, setAssertive] = useState('')

  useEffect(() => {
    const timers = new Set<number>()
    const stop = onAnnouncement((politeness, text) => {
      const set = politeness === 'assertive' ? setAssertive : setPolite
      set('')
      // Long enough for the cleared region to be noticed before the new words arrive.
      const timer = window.setTimeout(() => {
        timers.delete(timer)
        set(text)
      }, 100)
      timers.add(timer)
    })
    return () => {
      stop()
      for (const timer of timers) window.clearTimeout(timer)
    }
  }, [])

  return (
    <div className="visually-hidden">
      <div role="status" aria-live="polite" aria-atomic="true" id="announce-polite">
        {polite}
      </div>
      <div role="alert" aria-live="assertive" aria-atomic="true" id="announce-assertive">
        {assertive}
      </div>
    </div>
  )
}
