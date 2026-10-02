import DateTimePicker, { DateTimePickerAndroid } from '@react-native-community/datetimepicker';
import { Platform, StyleSheet, View } from 'react-native';

import { useI18n } from '../lib/context';
import { useScheme } from '../lib/theme';
import { Button, Field } from './ui';

export interface DateFieldProps {
  label: string;
  /** A calendar date, `YYYY-MM-DD`, or empty when none has been chosen. */
  value: string;
  onChange(value: string): void;
  error?: string | null;
  /** Today's date where the exchange keeps its dates: what the picker opens on. */
  today: string;
  disabled?: boolean;
}

/*
 * A due date is a calendar date, not a moment (DESIGN.md §13.2). The picker
 * works in moments, so the date is handed to it as midnight UTC and the
 * picker is told to work in UTC: the day it shows and the day it gives back
 * are then the same day, wherever the device is.
 */
const toMoment = (date: string) => new Date(`${date}T00:00:00Z`);
const toDate = (moment: Date) => moment.toISOString().slice(0, 10);

/** A date chosen with the platform's own date picker. */
export function DateField({ label, value, onChange, error, today, disabled }: DateFieldProps) {
  const { wording, fmt, day, language } = useI18n();
  const scheme = useScheme();
  const w = wording.mobile.date;
  const chosen = value !== '' && !Number.isNaN(toMoment(value).getTime());

  if (Platform.OS === 'android') {
    // Android's picker is a dialog, opened from a button that shows the date.
    return (
      <Field label={label} error={error}>
        <View style={styles.start}>
          <Button
            label={chosen ? day(value) : w.choose}
            accessibilityLabel={chosen ? fmt(w.change, { date: day(value) }) : w.choose}
            disabled={disabled}
            onPress={() =>
              DateTimePickerAndroid.open({
                value: toMoment(chosen ? value : today),
                mode: 'date',
                timeZoneName: 'UTC',
                onValueChange: (_event, picked) => onChange(toDate(picked)),
              })
            }
          />
        </View>
      </Field>
    );
  }

  // iOS shows the picker in place. It always shows some date, so it appears
  // only once there is a date to show: nothing is chosen for the person.
  return (
    <Field label={label} error={error}>
      <View style={styles.start}>
        {chosen ? (
          <DateTimePicker
            accessibilityLabel={label}
            value={toMoment(value)}
            mode="date"
            display="compact"
            timeZoneName="UTC"
            locale={language}
            themeVariant={scheme}
            disabled={disabled}
            onValueChange={(_event, picked) => onChange(toDate(picked))}
          />
        ) : (
          <Button label={w.choose} disabled={disabled} onPress={() => onChange(today)} />
        )}
      </View>
    </Field>
  );
}

const styles = StyleSheet.create({
  // The control keeps its own width, at the start of the line in either direction.
  start: { alignItems: 'flex-start' },
});
