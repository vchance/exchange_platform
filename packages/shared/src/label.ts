/*
 * Text someone else wrote, such as the other party's name, made fit for a
 * browser tab's title or a screen reader's label.
 *
 * Neither of those can isolate a piece of text the way `<bdi>` does on a
 * page. A right-to-left override (U+202E) in a name would turn the rest of
 * the title around, in the tab and in the browser's history; a line break or
 * other control character could make a label say something it does not.
 * What is left is the name as it reads, on one line.
 */

/*
 * Every C0 and C1 control character (line breaks and tabs among them), the
 * line and paragraph separators, and every character that only steers the
 * direction of text: the Arabic letter mark, the left-to-right and
 * right-to-left marks, the embeddings and overrides, and the isolates.
 */
// eslint-disable-next-line no-control-regex
const UNSAFE =
  /[\u0000-\u001f\u007f-\u009f\u2028\u2029\u061C\u200E\u200F\u202A-\u202E\u2066-\u2069]/g

/**
 * `text` without control or direction characters, each run of what was
 * removed and of white space made one space, and trimmed.
 */
export function labelText(text: string): string {
  return text.replace(UNSAFE, ' ').replace(/\s+/g, ' ').trim()
}
