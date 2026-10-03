// @vitest-environment jsdom
import {
  useHistory,
  useRecord,
  type ExchangeApi,
  type HistoryPage,
  type HistoryReading,
  type RecordDocument,
  type RecordReading,
} from '@exchange/shared'
import { act, useEffect } from 'react'
import { createRoot, type Root } from 'react-dom/client'
import { afterEach, expect, test } from 'vitest'

/*
 * The shared hooks that read an exchange's history and record, moved from
 * one exchange to another on the same screen: nothing read for the first may
 * be shown for, or joined onto, the second.
 */

;(globalThis as { IS_REACT_ACT_ENVIRONMENT?: boolean }).IS_REACT_ACT_ENVIRONMENT = true

let root: Root | null = null

afterEach(async () => {
  const mounted = root
  root = null
  if (mounted) await act(async () => mounted.unmount())
})

/** A promise and the means to settle it from the test. */
function later<T>() {
  let resolve!: (value: T) => void
  const promise = new Promise<T>((settle) => {
    resolve = settle
  })
  return { promise, resolve }
}

function page(id: string, sequences: number[], earlier: number | null): HistoryPage {
  return {
    you: 'A',
    parties: { A: `${id} A`, B: `${id} B` },
    earlier,
    events: sequences.map((sequence) => ({
      sequence,
      type: 'STATEMENT_ADDED',
      actor: 'A',
      at: '2026-10-02T15:00:00Z',
    })),
  } as HistoryPage
}

async function render(element: React.ReactElement) {
  document.body.innerHTML = '<div id="root"></div>'
  await act(async () => {
    root = createRoot(document.getElementById('root')!)
    root.render(element)
  })
}

async function rerender(element: React.ReactElement) {
  await act(async () => root!.render(element))
}

async function settle() {
  await act(async () => {
    await new Promise((resolve) => setTimeout(resolve, 0))
  })
}

test('history read for one exchange is never shown for, or joined onto, the next', async () => {
  const answers = new Map<string, ReturnType<typeof later<HistoryPage>>>()
  const asked = (id: string, before?: number) => `${id}:${before ?? 'latest'}`
  const api = {
    history: (id: string, before?: number) => {
      const answer = later<HistoryPage>()
      answers.set(asked(id, before), answer)
      return answer.promise
    },
  } as unknown as Pick<ExchangeApi, 'history'>

  let reading: HistoryReading | null = null
  const keep = (read: HistoryReading) => {
    reading = read
  }
  function Screen({ id }: { id: string }) {
    const read = useHistory(api, { id, version: 1 })
    // Handed out after each render, as a screen would show it.
    useEffect(() => keep(read))
    return null
  }

  await render(<Screen id="one" />)
  await act(async () => answers.get(asked('one'))!.resolve(page('one', [5, 6], 4)))
  expect(reading!.page?.events.map((event) => event.sequence)).toEqual([5, 6])

  // Reading further back in the first exchange, then moving to the second
  // before that answers.
  let earlier: Promise<void> = Promise.resolve()
  await act(async () => {
    earlier = reading!.readEarlier()
  })
  expect(reading!.readingEarlier).toBe(true)
  await rerender(<Screen id="two" />)

  // Nothing of the first is shown while the second is read.
  expect(reading!.page).toBeNull()
  expect(reading!.readingEarlier).toBe(false)

  // The second's latest page arrives with sequences the first had read
  // before it; they are not put in front of it.
  await act(async () => answers.get(asked('two'))!.resolve(page('two', [3, 4], null)))
  expect(reading!.page?.parties.A).toBe('two A')
  expect(reading!.page?.events.map((event) => event.sequence)).toEqual([3, 4])
  expect(reading!.page?.earlier).toBeNull()

  // The first's earlier page finally answers; the second is untouched.
  await act(async () => {
    answers.get(asked('one', 4))!.resolve(page('one', [1, 2], null))
    await earlier
  })
  await settle()
  expect(reading!.page?.parties.A).toBe('two A')
  expect(reading!.page?.events.map((event) => event.sequence)).toEqual([3, 4])
})

test('a failure reading one exchange’s history is not the next one’s', async () => {
  const api = {
    history: (id: string) =>
      id === 'one' ? Promise.reject(new Error('gone')) : new Promise<HistoryPage>(() => {}),
  } as unknown as Pick<ExchangeApi, 'history'>
  let reading: HistoryReading | null = null
  const keep = (read: HistoryReading) => {
    reading = read
  }
  function Screen({ id }: { id: string }) {
    const read = useHistory(api, { id, version: 1 })
    // Handed out after each render, as a screen would show it.
    useEffect(() => keep(read))
    return null
  }
  await render(<Screen id="one" />)
  await settle()
  expect(reading!.failure).not.toBeNull()
  await rerender(<Screen id="two" />)
  expect(reading!.failure).toBeNull()
  expect(reading!.page).toBeNull()
})

test('the record of one exchange is not shown while the next is read', async () => {
  const record = (id: string) =>
    ({ exchange: { id }, part: { complete: true, next: null } }) as unknown as RecordDocument
  const pending = later<RecordDocument>()
  const api = {
    recordPart: (id: string) => (id === 'one' ? Promise.resolve(record('one')) : pending.promise),
  } as unknown as Pick<ExchangeApi, 'recordPart'>
  let reading: RecordReading | null = null
  const keep = (read: RecordReading) => {
    reading = read
  }
  function Screen({ id }: { id: string }) {
    const read = useRecord(api, id)
    // Handed out after each render, as a screen would show it.
    useEffect(() => keep(read))
    return null
  }
  await render(<Screen id="one" />)
  await settle()
  expect((reading!.record?.exchange as { id: string } | undefined)?.id).toBe('one')

  await rerender(<Screen id="two" />)
  expect(reading!.record).toBeNull()
  await act(async () => pending.resolve(record('two')))
  await settle()
  expect((reading!.record?.exchange as { id: string } | undefined)?.id).toBe('two')
})
