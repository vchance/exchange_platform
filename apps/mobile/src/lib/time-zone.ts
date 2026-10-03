import { getCalendars } from 'expo-localization';

/**
 * The time zone the device keeps, as an IANA name. A new exchange's due dates
 * are read in that of whoever starts it; a due date is shown with its
 * exchange's zone beside it wherever this one differs (`dueDateZone`).
 */
export function deviceTimezone(): string {
  return getCalendars()[0]?.timeZone ?? Intl.DateTimeFormat().resolvedOptions().timeZone;
}
