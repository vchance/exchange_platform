import { HELP_LINKS, helpPath, type HelpLinkPlace } from '@yuppers/shared'

import { useI18n } from '../app/context'

/**
 * A small "Learn more" link from one place in the product to the help topic
 * that explains it. It opens in a new tab, because the places it stands in,
 * a signing step, a panel half filled in, a composer, are ones the person
 * should not lose by reading about them; it says so to screen readers. The
 * address names the language on screen, so the new tab opens in it.
 */
export function HelpLink({ place }: { place: HelpLinkPlace }) {
  const { wording, language } = useI18n()
  return (
    <p className="learn-more">
      <a
        href={`${helpPath(HELP_LINKS[place])}?lang=${encodeURIComponent(language)}`}
        target="_blank"
        rel="noopener"
      >
        {wording.help.learnMore[place]}
        <span className="visually-hidden"> {wording.help.newTab}</span>
      </a>
    </p>
  )
}
