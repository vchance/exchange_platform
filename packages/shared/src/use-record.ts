import type { ErrorCode, ExchangeView } from '@exchange/api-client'
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

/**
 * The latest of an exchange's history, and as much before it as the person
 * asks for. Every change to an exchange adds to its history, so the latest
 * page is read again whenever the exchange on screen is a newer version;
 * what was read before it stays, and until the new page arrives, so does
 * what is shown.
 */
export function useHistory(
  api: Pick<ExchangeApi, 'history'>,
  exchange: Pick<ExchangeView, 'id' | 'version'>,
): HistoryReading {
  const [page, setPage] = useState<HistoryPage | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const [readingEarlier, setReadingEarlier] = useState(false)
  const { id, version } = exchange
  // The page on screen, for joining a new reading onto without a render.
  const shown = useRef<HistoryPage | null>(null)

  useEffect(() => {
    let cancelled = false
    api.history(id).then(
      (found) => {
        if (cancelled) return
        // The earlier pages already read stay in front of the latest.
        const first = found.events[0]?.sequence ?? Number.POSITIVE_INFINITY
        const earlier = (shown.current?.events ?? []).filter((event) => event.sequence < first)
        const joined: HistoryPage = {
          ...found,
          events: [...earlier, ...found.events],
          earlier: earlier.length > 0 ? (shown.current?.earlier ?? null) : found.earlier,
        }
        shown.current = joined
        setPage(joined)
        setFailure(null)
      },
      (error: unknown) => {
        if (!cancelled) setFailure(failureCode(error))
      },
    )
    return () => {
      cancelled = true
    }
  }, [api, id, version])

  const readEarlier = useCallback(async () => {
    const current = shown.current
    if (!current || current.earlier == null || readingEarlier) return
    setReadingEarlier(true)
    try {
      const found = await api.history(id, current.earlier)
      // The page may have been replaced by a newer reading meanwhile; what
      // was read goes in front of whatever is shown now.
      const latest = shown.current ?? current
      const joined: HistoryPage = {
        ...latest,
        events: [...found.events, ...latest.events],
        earlier: found.earlier,
      }
      shown.current = joined
      setPage(joined)
      setFailure(null)
    } catch (error) {
      setFailure(failureCode(error))
    } finally {
      setReadingEarlier(false)
    }
  }, [api, id, readingEarlier])

  return { page, failure, readEarlier, readingEarlier }
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
  const [record, setRecord] = useState<RecordDocument | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  // Only the reading asked for last may answer.
  const asked = useRef(0)

  const reload = useCallback(async () => {
    const mine = (asked.current += 1)
    try {
      const found = await readWholeRecord((from) => api.recordPart(id, from))
      if (mine !== asked.current) return
      setRecord(found)
      setFailure(null)
    } catch (error) {
      if (mine === asked.current) setFailure(failureCode(error))
    }
  }, [api, id])

  useEffect(() => {
    void reload()
    return () => {
      asked.current += 1
    }
  }, [reload])

  return { record, failure, reload }
}
