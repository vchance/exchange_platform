import { HELP_LINKS, type HelpLinkPlace } from '@yuppers/shared';

import { useI18n } from '../lib/context';
import { openHelp } from '../lib/help';
import { Actions, Button } from './ui';

/**
 * A small "Learn more" link from one place in the app to the help topic that
 * explains it, opened in the browser. The hint says it leaves the app. It
 * sits at the start of its own row, the size of its words, so it reads as
 * secondary to the controls around it.
 */
export function HelpLink({ place }: { place: HelpLinkPlace }) {
  const { wording, language } = useI18n();
  return (
    <Actions>
      <Button
        testID={`help-${place}`}
        variant="link"
        label={wording.help.learnMore[place]}
        hint={wording.help.inBrowser}
        onPress={() => void openHelp(language, HELP_LINKS[place])}
      />
    </Actions>
  );
}
