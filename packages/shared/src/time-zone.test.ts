import { expect, test } from 'vitest'

import { createI18n } from './i18n'
import { wordingFor } from './language'
import { deviceTimeZone, dueDateZone, dueOnDateText, timeZoneCity } from './time-zone'

const en = createI18n('en', wordingFor('en'), () => {})
const es = createI18n('es', wordingFor('es'), () => {})

test('nothing is added where the device keeps the exchange’s own zone', () => {
  expect(dueDateZone('America/Chicago', 'America/Chicago')).toBeNull()
  // Two names for one zone are one zone.
  expect(dueDateZone('US/Central', 'America/Chicago')).toBeNull()
  expect(dueDateZone('America/Chicago', 'america/chicago')).toBeNull()
})

test('the exchange’s zone is named where the device keeps another', () => {
  expect(dueDateZone('America/Chicago', 'America/New_York')).toBe('America/Chicago')
  expect(dueDateZone('US/Central', 'Europe/Madrid')).toBe('America/Chicago')
  // Same offset today is still another zone: their clocks change on other days.
  expect(dueDateZone('America/Chicago', 'America/Mexico_City')).toBe('America/Chicago')
})

test('with either zone unknown, nothing is added; a zone the engine cannot read is named as given', () => {
  expect(dueDateZone(undefined, 'America/Chicago')).toBeNull()
  expect(dueDateZone(null, 'America/Chicago')).toBeNull()
  expect(dueDateZone('America/Chicago', null)).toBeNull()
  expect(dueDateZone('Mars/Olympus_Mons', 'America/Chicago')).toBe('Mars/Olympus_Mons')
})

test('the device’s own zone is read by default', () => {
  const own = deviceTimeZone()
  expect(own).toBeTruthy()
  expect(dueDateZone(own)).toBeNull()
})

test('a zone is named by its city, or as it is where it has none', () => {
  expect(timeZoneCity('America/Chicago')).toBe('Chicago')
  expect(timeZoneCity('America/Argentina/Buenos_Aires')).toBe('Buenos Aires')
  expect(timeZoneCity('UTC')).toBe('UTC')
  expect(timeZoneCity('Etc/GMT+5')).toBe('Etc/GMT+5')
})

test('a due date reads the same as before, with the zone beside it only when given', () => {
  expect(dueOnDateText(en, '2026-03-12', null)).toBe('Due March 12, 2026')
  expect(dueOnDateText(en, '2026-03-12', 'America/Chicago')).toBe('Due March 12, 2026 (Chicago time)')
  expect(dueOnDateText(es, '2026-03-12', 'America/Chicago')).toBe(
    'Vence el 12 de marzo de 2026 (hora de Chicago)',
  )
})
