import { useEffect, useId, useRef, type ReactNode } from 'react'

/**
 * An action that has been opened but not sent. It appears in place, under
 * the thing it acts on, and takes the focus so a keyboard or screen-reader
 * user lands on it instead of having to go looking.
 */
export function Panel({ title, children }: { title: string; children: ReactNode }) {
  const panel = useRef<HTMLDivElement>(null)
  const id = useId()

  useEffect(() => {
    panel.current?.focus()
  }, [])

  return (
    <div className="panel" role="group" aria-labelledby={id} tabIndex={-1} ref={panel}>
      <p className="label" id={id}>
        {title}
      </p>
      {children}
    </div>
  )
}
