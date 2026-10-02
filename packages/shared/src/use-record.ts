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
  /** `null` until the first answer. */
  page: HistoryPage | null
  failure: ErrorCode | null
}

/**
 * The latest of an exchange's history. Every change to an exchange adds to
 * its history, so it is read again whenever the exchange on screen is a newer
 * version. Until the new one arrives, the one already shown stays.
 */
export function useHistory(
  api: Pick<ExchangeApi, 'history'>,
  exchange: Pick<ExchangeView, 'id' | 'version'>,
): HistoryReading {
  const [page, setPage] = useState<HistoryPage | null>(null)
  const [failure, setFailure] = useState<ErrorCode | null>(null)
  const { id, version } = exchange

  useEffect(() => {
    let cancelled = false
    api.history(id).then(
      (found) => {
        if (cancelled) return
        setPage(found)
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

  return { page, failure }
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
