/*
 * The help pages: which topics there are, in the order they are listed, and
 * where each is. The pages themselves are the web app's, at `/help` and
 * `/help/{topic}`, and their text is in `wording/help/{language}.json`, a
 * file of its own per language so that nothing that does not show help
 * carries it: the invitation page loads none of it, and the mobile app opens
 * the web's pages in the browser rather than bundling them.
 */

/** Every help topic, as it appears in its address, in the order the pages list them. */
export const HELP_TOPICS = [
  'yups',
  'signing',
  'what-we-dont-do',
  'keeping-track',
  'changing',
  'blocking',
  'deleting',
  'record',
  'languages',
] as const

export type HelpTopic = (typeof HELP_TOPICS)[number]

export function isHelpTopic(value: string): value is HelpTopic {
  return (HELP_TOPICS as readonly string[]).includes(value)
}

/**
 * The places in the apps that link to a topic, each with link text of its
 * own (`wording.help.learnMore`), and the topic each one opens.
 */
export const HELP_LINKS = {
  signing: 'signing',
  moneyOutside: 'what-we-dont-do',
  trouble: 'keeping-track',
  amendment: 'changing',
  blocking: 'blocking',
  deletion: 'deleting',
  record: 'record',
} as const satisfies Record<string, HelpTopic>

export type HelpLinkPlace = keyof typeof HELP_LINKS

/**
 * The numbers the help text states, which are the service's rules
 * (`Rules::default` in `backend/src/domain/mod.rs`). The help files name
 * them as placeholders, such as `{closeDays}`, and `help.test.ts` fails if
 * they drift from the service's.
 */
export const HELP_FIGURES = {
  /** How long a sent version stays open for signing. */
  proposalDays: 14,
  /** How long an invitation link can be used. */
  invitationDays: 14,
  /** How long the other party has to answer a request to close. */
  closeDays: 7,
  /** Idle time after which both parties are asked whether it is still under way. */
  inactivityPromptDays: 60,
  /** Time after that question at which an idle agreement closes. */
  inactivityCloseDays: 30,
  /** How long a signature's network address and device are kept. */
  networkDays: 90,
} as const

/** The path of the help pages, or of one topic. */
export function helpPath(topic?: HelpTopic): string {
  return topic ? `/help/${topic}` : '/help'
}

/**
 * The full address of a help page on the web app at `origin`, in `language`.
 * The language is named in the address because the browser that opens it
 * may not know who is signed in to the app, or which language they chose.
 */
export function helpAddress(origin: string, language: string, topic?: HelpTopic): string {
  return `${origin.replace(/\/+$/, '')}${helpPath(topic)}?lang=${encodeURIComponent(language)}`
}
