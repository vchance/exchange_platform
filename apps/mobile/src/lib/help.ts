import { helpAddress, type HelpTopic } from '@yuppers/shared';
import { Linking } from 'react-native';

import { WEB_URL } from './config';

/*
 * Help is the web app's: `/help` and `/help/{topic}` on the web origin. The
 * app does not carry the pages; it opens them in the system's browser, which
 * every device has, with the app's language in the address so the page
 * opens in it whoever the browser thinks is signed in.
 *
 * `Linking` rather than an in-app browser, because the app has no in-app
 * browser module and the help needs none: nothing on those pages acts on
 * the account.
 */

/** The address of the help pages, or of one topic, in `language`. */
export function helpUrl(language: string, topic?: HelpTopic): string {
  return helpAddress(WEB_URL, language, topic);
}

/** Opens the help pages, or one topic, in the browser. */
export function openHelp(language: string, topic?: HelpTopic): Promise<void> {
  return Linking.openURL(helpUrl(language, topic)).catch(() => {
    // Only a device without any browser refuses an https address, and on it
    // there is nothing better to offer.
  });
}
