import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { Platform } from 'react-native';

import type { RecordSharer } from './record-sharer.types';

/*
 * A party's copy of a record, on iOS and Android. The system's share sheet
 * takes a file, not text, so the copy is written to the app's own cache
 * directory and that file is handed over. The cache is private to the app;
 * the share sheet gives the app the person picks leave to read that one file.
 *
 * The record holds the agreement's terms and both names, so the copy is not
 * left lying about: there is never more than one, it goes when the next is
 * made and when the account signs out, and on iOS as soon as the share sheet
 * closes. On Android the sheet closes before the app that was picked has
 * necessarily read the file, so there it stays until one of the others.
 *
 * This is the file both platforms bundle. `record-sharer.web.ts` stands in
 * for it only in the browser test harness.
 */

/** A folder of its own, so that clearing it can never touch anything else. */
const FOLDER = 'record-copies';

const folder = () => new Directory(Paths.cache, FOLDER);

/** A file name is a single path segment, whatever the wording made of it. */
function fileName(name: string): string {
  const safe = Array.from(name, (char) =>
    char === '/' || char === '\\' || char === ':' || char < ' ' ? '-' : char,
  )
    .join('')
    .replace(/^\.+/, '');
  return safe === '' ? 'record.json' : safe;
}

function forget(): void {
  try {
    const copies = folder();
    if (copies.exists) copies.delete();
  } catch {
    // Nothing was there, or it cannot be reached. The system clears the cache itself in time.
  }
}

export const recordSharer: RecordSharer = {
  async share(file, title) {
    if (!(await Sharing.isAvailableAsync())) return 'unavailable';

    forget();
    const copies = folder();
    copies.create({ intermediates: true, idempotent: true });
    const copy = new File(copies, fileName(file.name));
    copy.create({ overwrite: true });
    copy.write(file.text);

    try {
      // Resolves when the sheet closes, whether or not anything was sent.
      await Sharing.shareAsync(copy.uri, {
        mimeType: file.type,
        UTI: 'public.json',
        dialogTitle: title,
      });
    } finally {
      if (Platform.OS === 'ios') forget();
    }
    return 'handed';
  },
  forget,
};
