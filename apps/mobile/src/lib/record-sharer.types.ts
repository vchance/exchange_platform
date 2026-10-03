import type { RecordFile } from '@exchange/shared';

/**
 * `handed` means the file was given to the system, which is all the app can
 * know: whether the person then sent it anywhere is theirs alone.
 * `unavailable` means this device has no way to share a file.
 */
export type ShareOutcome = 'handed' | 'unavailable';

/**
 * How a party's copy of a record leaves the app (DESIGN.md §14.1): as a file,
 * through the system's own share sheet, to wherever the person chooses.
 */
export interface RecordSharer {
  /**
   * Offers the file to the system's share sheet under `title`. Rejects when
   * the file could not be made ready or the sheet could not be shown.
   */
  share(file: RecordFile, title: string): Promise<ShareOutcome>;
  /**
   * Prints `html`, the record laid out as a page, to a PDF named `name`
   * (without its extension) and offers that to the share sheet, where the
   * person can save it or send it. Rejects when the PDF could not be made.
   */
  sharePdf(html: string, name: string, title: string): Promise<ShareOutcome>;
  /** Removes any copy made for sharing that is still on the device. */
  forget(): void;
}
