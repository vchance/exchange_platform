/// <reference types="node" />
import { readFileSync } from 'node:fs'

import { describe, expect, test } from 'vitest'

import en from '../wording/help/en.json'
import es from '../wording/help/es.json'
import { HELP_FIGURES, HELP_LINKS, HELP_TOPICS, helpAddress, helpPath, isHelpTopic } from './help'
import { languages, type Language } from './language'
import { formatMessage } from './message'
import type { HelpWording } from './wording/types'

// Each language's help pages have the shape the web app renders. The wording
// check makes them match each other; this makes them match the code.
const help = { en, es } satisfies Record<Language, HelpWording>

/** Every message in a help file. */
function messages(wording: HelpWording): string[] {
  return [
    wording.title,
    wording.intro,
    wording.topicsHeading,
    wording.onThisPage,
    wording.allTopics,
    ...Object.values(wording.topics).flatMap((topic) => [
      topic.title,
      topic.summary,
      ...topic.blocks.flatMap((block) => {
        if ('ul' in block) return block.ul
        if ('ol' in block) return block.ol
        return ['h' in block ? block.h : block.p]
      }),
    ]),
  ]
}

describe('the help pages', () => {
  test('every supported language has them', () => {
    expect(Object.keys(help).sort()).toEqual(languages.map((info) => info.code).sort())
  })

  test.each(Object.entries(help))('%s has every topic, and no other', (_, wording) => {
    expect(Object.keys(wording.topics).sort()).toEqual([...HELP_TOPICS].sort())
  })

  test.each(Object.entries(help))('%s fills in every placeholder it uses', (language, wording) => {
    for (const message of messages(wording)) {
      const filled = formatMessage(message, HELP_FIGURES, language)
      expect(filled, message).not.toMatch(/[{}]/)
    }
  })

  test.each(Object.entries(help))('%s starts each topic with something to read, not a list', (_, wording) => {
    for (const topic of Object.values(wording.topics)) {
      const first = topic.blocks[0]
      expect('p' in first || 'h' in first, topic.title).toBe(true)
    }
  })
})

describe('the figures the help states', () => {
  // The service's rules, as `Rules::default` sets them. The help says these
  // numbers to people, so a change on the service has to be a change here.
  const rules = readFileSync(
    new URL('../../../backend/src/domain/mod.rs', import.meta.url),
    'utf8',
  )
  const days = (field: string) => {
    const found = new RegExp(`${field}: Duration::days\\((\\d+)\\)`).exec(rules)
    if (!found) throw new Error(`no ${field} in Rules::default`)
    return Number(found[1])
  }

  test('are the service’s own', () => {
    expect(HELP_FIGURES).toEqual({
      proposalDays: days('revision_ttl'),
      invitationDays: days('invitation_ttl'),
      closeDays: days('close_response_window'),
      inactivityPromptDays: days('inactivity_prompt_after'),
      inactivityCloseDays: days('inactivity_close_after'),
      networkDays: days('network_metadata_retention'),
    })
  })
})

describe('addresses', () => {
  test('a topic, and the first page', () => {
    expect(helpPath()).toBe('/help')
    expect(helpPath('signing')).toBe('/help/signing')
  })

  test('the full address names the language', () => {
    expect(helpAddress('https://yuppers.example/', 'es', 'blocking')).toBe(
      'https://yuppers.example/help/blocking?lang=es',
    )
    expect(helpAddress('http://localhost:5173', 'en')).toBe('http://localhost:5173/help?lang=en')
  })

  test('every place that links to help names a topic there is', () => {
    for (const topic of Object.values(HELP_LINKS)) expect(isHelpTopic(topic)).toBe(true)
    expect(isHelpTopic('nothing')).toBe(false)
  })
})
