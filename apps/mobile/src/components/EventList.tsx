import type { components } from '@yuppers/api-client';
import { eventMessage, noteKind, type RecordEvent } from '@yuppers/shared';
import { StyleSheet, View } from 'react-native';

import { useI18n } from '../lib/context';
import { space, useColors } from '../lib/theme';
import { Hint, Label, P, Written } from './ui';

type Schemas = components['schemas'];

interface Props {
  /** Oldest first. */
  events: readonly RecordEvent[];
  /** Each party's name as the agreement writes it. */
  parties: Schemas['Parties'];
  /** The party reading, who is spoken to as "you". `null` names everyone. */
  reader: Schemas['Slot'] | null;
  /** How a moment in time is written here. */
  when(instant: string): string;
  /** The ids of the money contributions, which are spoken of in words for paying and receiving. */
  money?: ReadonlySet<string>;
}

/**
 * What happened in an exchange, in order. Each entry is one sentence of the
 * product's, saying who did what, followed by anything the parties wrote
 * with it: the contribution it is about, as they described it, and their
 * message, note, reason or statement. Those are shown exactly as written and
 * set apart as theirs (DESIGN.md §4.2).
 *
 * To a screen reader each entry is one stop, read whole and in the order it
 * is shown: when, what happened, then what was written with it.
 */
export function EventList({ events, parties, reader, when, money }: Props) {
  const { wording, fmt } = useI18n();
  const colors = useColors();
  const w = wording.record;

  return (
    <View role="list" style={styles.list}>
      {events.map((event) => {
        const { message, values } = eventMessage(event, w.events, reader, parties, money);
        return (
          <View
            key={event.sequence}
            role="listitem"
            accessible
            style={[styles.entry, { borderTopColor: colors.border }]}>
            <Hint>{when(event.at)}</Hint>
            <P>{fmt(message, values)}</P>
            {event.contribution?.description ? (
              <Written>{event.contribution.description}</Written>
            ) : null}
            {event.note ? (
              <>
                <Label>{w.noteLabels[noteKind(event)]}</Label>
                <Written>{event.note}</Written>
              </>
            ) : null}
          </View>
        );
      })}
    </View>
  );
}

const styles = StyleSheet.create({
  list: { gap: space.m },
  entry: { borderTopWidth: StyleSheet.hairlineWidth, paddingTop: space.m, gap: space.s },
});
