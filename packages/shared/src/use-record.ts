import type { ErrorCode, ExchangeView } from '@yuppers/api-client'
import { useCallback, useEffect, useRef, useState } from 'react'

import { failureCode, type ExchangeApi } from './api'
import { readWholeRecord, type HistoryPage, type RecordDocument } from './record'

/*
 * Reading an exchange's history and its record from a screen. The web app
 * and the mobile app lay them out differently; when they are read, and what
 * stays on screen meanwhile, is the same for both and is decided here.
 */

export interface HistoryReading {
  /**
   * `null` until the first answer. Its events are every one read so far,
   * oldest first; `earlier` is set while there is history before them.
   */
  page: HistoryPage | null
  failure: ErrorCode | null
  /** Reads the page before the earliest shown and adds it in front. */
  readEarlier(): Promise<void>
  readingEarlier: boolean
}

/** Something read, with the exchange it was read for. */
interface ReadFor<T> {
  id: string
  value: T
}

/** `read` if it was read for exchange `id`, otherwise `null`. */
function readFor<T>(read: ReadFor<T> | null, id: string): T | null {
  return read !== null && read.id === id ? read.value : null
}

/**
 * The latest of an exchange's history, and as much before it as the person
 * asks for. Every change to an exchange adds to its history, so the latest
 * page is read again whenever the exchange on screen is a newer version;
 * what was read before it stays, and until the new page arrives, so does
 * what is shown.
 *
 * Everything read is kept with the exchange it was read for. When the
 * screen moves to another exchange, nothing of the first is shown, built
 * on, or still counted as being read.
 */
export function useHistory(
  api: Pick<ExchangeApi, 'history'>,
  exchange: Pick<ExchangeView, 'id' | 'version'>,
): HistoryReading {
  const [page, setPage] = useState<ReadFor<HistoryPage> | null>(null)
  const [failure, setFailure] = useState<ReadFor<ErrorCode> | null>(null)
  const [readingEarlier, setReadingEarlier] = useState<string | null>(null)
  const { id, version } = exchange
  // The page on screen, for joining a new reading onto without a render.
  const shown = useRef<ReadFor<HistoryPage> | null>(null)

  const show = useCallback((forId: string, value: HistoryPage) => {
    const read = { id: forId, value }
    shown.current = read
    setPage(read)
    setFailure(null)
  }, [])

  useEffect(() => {
    let cancelled = false
    api.history(id).then(
      (found) => {
        if (cancelled) return
        // The earlier pages already read stay in front of the latest.
        const before = readFor(shown.current, id)
        const first = found.events[0]?.sequence ?? Number.POSITIVE_INFINITY
        const earlier = (before?.events ?? []).filter((event) => event.sequence < first)
        show(id, {
          ...found,
          events: [...earlier, ...found.events],
          earlier: earlier.length > 0 ? (before?.earlier ?? null) : found.earlier,
        })
      },
      (error: unknown) => {
        if (!cancelled) setFailure({ id, value: failureCode(error) })
      },
    )
    return () => {
      cancelled = true
    }
  }, [api, id, version, show])

  const readEarlier = useCallback(async () => {
    const current = readFor(shown.current, id)
    if (!current || current.earlier == null || readingEarlier === id) return
    setReadingEarlier(id)
    try {
      const found = await api.history(id, current.earlier)
      // The page may have been replaced by a newer reading meanwhile; what
      // was read goes in front of whatever is shown now, if that is still
      // this exchange's.
      const latest = readFor(shown.current, id)
      if (!latest) return
      show(id, {
        ...latest,
        events: [...found.events, ...latest.events],
        earlier: found.earlier,
      })
    } catch (error) {
      setFailure({ id, value: failureCode(error) })
    } finally {
      setReadingEarlier((reading) => (reading === id ? null : reading))
    }
  }, [api, id, readingEarlier, show])

  return {
    page: readFor(page, id),
    failure: readFor(failure, id),
    readEarlier,
    readingEarlier: readingEarlier === id,
  }
}

export interface RecordReading {
  /** `null` until it has been read whole. */
  record: RecordDocument | null
  failure: ErrorCode | null
  /** Reads it again. What is shown stays until the new one has arrived. */
  reload(): Promise<void>
}

/** The whole record of an exchange. A long record comes in parts; this joins them. */
export function useRecord(api: Pick<ExchangeApi, 'recordPart'>, id: string): RecordReading {
  // Kept with the exchange they are about, so another's is never shown.
  const [record, setRecord] = useState<ReadFor<RecordDocument> | null>(null)
  const [failure, setFailure] = useState<ReadFor<ErrorCode> | null>(null)
  // Only the reading asked for last may answer.
  const asked = useRef(0)

  const reload = useCallback(async () => {
    const mine = (asked.current += 1)
    try {
      const found = await readWholeRecord((from) => api.recordPart(id, from))
      if (mine !== asked.current) return
      setRecord({ id, value: found })
      setFailure(null)
    } catch (error) {
      if (mine === asked.current) setFailure({ id, value: failureCode(error) })
    }
  }, [api, id])

  useEffect(() => {
    void reload()
    return () => {
      asked.current += 1
    }
  }, [reload])

  return { record: readFor(record, id), failure: readFor(failure, id), reload }
}
