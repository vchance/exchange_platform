import { Platform } from 'react-native';

import type { RecordSharer } from './record-sharer.types';

/*
 * TEST HARNESS ONLY, like `token-store.web.ts`. A browser has no system share
 * sheet for a file and no app cache to write one to, so when the app's
 * screens are run in a browser the copy is handed to the browser as a
 * download instead. That shows what the copy holds; it says nothing about the
 * share sheet, which only a device has.
 *
 * It is named `.web.ts`, so the bundler resolves it for the web target only:
 * an iOS or Android bundle gets `record-sharer.ts` and never contains this.
 * The check below refuses to run anywhere else even so.
 */
if (Platform.OS !== 'web') {
  throw new Error('The harness record sharer was loaded outside the web target');
}

export const recordSharer: RecordSharer = {
  async share(file) {
    const address = URL.createObjectURL(new Blob([file.text], { type: file.type }));
    const link = document.createElement('a');
    link.href = address;
    link.download = file.name;
    document.body.append(link);
    link.click();
    link.remove();
    window.setTimeout(() => URL.revokeObjectURL(address), 1000);
    return 'handed';
  },
  // Nothing is kept: the browser has the download and the page has no copy.
  forget() {},
};
