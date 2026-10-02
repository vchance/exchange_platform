import type { AnchorHTMLAttributes, MouseEvent } from 'react'

import { navigate } from './router'

interface LinkProps extends Omit<AnchorHTMLAttributes<HTMLAnchorElement>, 'href'> {
  to: string
}

/** An ordinary link that changes page without reloading the app. */
export function Link({ to, onClick, ...rest }: LinkProps) {
  function follow(event: MouseEvent<HTMLAnchorElement>) {
    onClick?.(event)
    // Leave new-tab, new-window and download clicks to the browser.
    const plain =
      event.button === 0 && !event.metaKey && !event.ctrlKey && !event.shiftKey && !event.altKey
    if (event.defaultPrevented || !plain) return
    event.preventDefault()
    navigate(to)
  }
  return <a {...rest} href={to} onClick={follow} />
}
