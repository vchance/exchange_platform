import type { components } from '@exchange/api-client'
import { eventMessage, noteKind, type RecordEvent } from '@exchange/shared'

import { useI18n } from '../app/context'
import './history.css'
import { Written } from './ui'

type Schemas = components['schemas']

interface Props {
  /** Oldest first. */
  events: readonly RecordEvent[]
  /** Each party's name as the agreement writes it. */
  parties: Schemas['Parties']
  /** The party reading, who is spoken to as "you". `null` names everyone. */
  reader: Schemas['Slot'] | null
  /** How a moment in time is written here. */
  when(instant: string): string
}

/**
 * What happened in an exchange, in order. Each entry is one sentence of the
 * product's, saying who did what, followed by anything the parties wrote
 * with it: the contribution it is about, as they described it, and their
 * message, note, reason or statement. Those are shown exactly as written and
 * set apart as theirs (DESIGN.md §4.2).
 */
export function EventList({ events, parties, reader, when }: Props) {
  const { wording, fmt } = useI18n()
  const w = wording.record

  return (
    <ol className="history">
      {events.map((event) => {
        const { message, values } = eventMessage(event, w.events, reader, parties)
        return (
          <li key={event.sequence} className="history-entry">
            <p className="hint">
              <time dateTime={event.at}>{when(event.at)}</time>
            </p>
            <p>{fmt(message, values)}</p>
            {event.contribution?.description && (
              <Written>{event.contribution.description}</Written>
            )}
            {event.note && (
              <>
                <p className="label">{w.noteLabels[noteKind(event)]}</p>
                <Written>{event.note}</Written>
              </>
            )}
          </li>
        )
      })}
    </ol>
  )
}
