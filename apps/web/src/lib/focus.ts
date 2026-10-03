/**
 * Whether the focus has been lost: the element that had it was removed, as
 * a button is when what it did makes it no longer apply, or was disabled
 * while it worked, and the browser has fallen back to the top of the
 * document. Something that appears then may take the focus without taking
 * it from anything.
 */
export function focusLost(): boolean {
  const active = document.activeElement
  return active === null || active === document.body || !active.isConnected
}
