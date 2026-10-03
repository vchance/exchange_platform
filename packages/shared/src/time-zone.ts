import type { I18n } from './i18n'

/*
 * A due date is a calendar date, read in the exchange's time zone: that of
 * whoever started it (DESIGN.md §13.2). For someone elsewhere, "due 12 March"
 * can end hours before or after their own 12 March does, so wherever a due
 * date is shown to a reader whose device keeps another time zone, the zone
 * is named beside it. Where the two are the same, nothing is added.
 *
 * Only what the mobile engine (Hermes) has is used: `resolvedOptions()` and
 * the `timeZone` option of `Intl.DateTimeFormat`, as `todayIn` already does.
 * There is no `formatToParts` here, so the zone's name is not the localized
 * one ("hora central") but its city, read from the IANA name, the same in
 * every language and on every platform.
 */

/** The time zone the device keeps, as an IANA name, or `null` when it cannot say. */
export function deviceTimeZone(): string | null {
  try {
    return new Intl.DateTimeFormat().resolvedOptions().timeZone || null
  } catch {
    return null
  }
}

/**
 * The IANA name the engine knows a zone by, so that two names for one zone
 * ("US/Central" and "America/Chicago") compare equal. `null` when the engine
 * does not know the zone at all.
 */
function canonical(zone: string): string | null {
  try {
    return new Intl.DateTimeFormat('en', { timeZone: zone }).resolvedOptions().timeZone || zone
  } catch {
    return null
  }
}

/**
 * The time zone to name beside a calendar due date, or `null` when there is
 * no need: the reader's device keeps the exchange's own zone, or either zone
 * is not known. `deviceZone` is the device's own unless one is given.
 */
export function dueDateZone(
  exchangeZone: string | null | undefined,
  deviceZone: string | null = deviceTimeZone(),
): string | null {
  if (!exchangeZone || !deviceZone) return null
  if (exchangeZone.toLowerCase() === deviceZone.toLowerCase()) return null
  const exchange = canonical(exchangeZone)
  // A zone the engine cannot read is still named as it was given: the date
  // is read in it all the same.
  if (exchange === null) return exchangeZone
  const device = canonical(deviceZone)
  if (device !== null && device.toLowerCase() === exchange.toLowerCase()) return null
  return exchange
}

/**
 * A short name for a time zone that people recognise: its city, as the IANA
 * name writes it ("America/Argentina/Buenos_Aires" is "Buenos Aires"). A zone
 * that names no city, such as "UTC" or "Etc/GMT+5", is given as it is.
 */
export function timeZoneCity(zone: string): string {
  const slash = zone.lastIndexOf('/')
  if (slash === -1 || zone.startsWith('Etc/')) return zone
  const city = zone.slice(slash + 1).replace(/_/g, ' ')
  return city === '' ? zone : city
}

/**
 * "Due 12 March", or "Due 12 March (Chicago time)" when `zone`, from
 * `dueDateZone`, says the reader's device keeps another time zone.
 */
export function dueOnDateText(
  i18n: Pick<I18n, 'wording' | 'fmt' | 'day'>,
  date: string,
  zone: string | null,
): string {
  const { wording, fmt, day } = i18n
  return zone
    ? fmt(wording.terms.dueOnDateInZone, { date: day(date), zone: timeZoneCity(zone) })
    : fmt(wording.terms.dueOnDate, { date: day(date) })
}
