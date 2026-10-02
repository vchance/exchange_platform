import type { ErrorCode } from '@exchange/api-client'
import { useEffect, useId, useRef, type ReactNode } from 'react'

import { useI18n } from '../app/context'
import { hasNavigated } from '../app/router'

/**
 * A page's main heading. It names the page in the browser tab, and takes the
 * focus when the page was reached from another one, so a screen reader
 * announces the change and the keyboard starts from the top.
 */
export function PageHeading({ children, step }: { children: string; step?: boolean }) {
  const { wording } = useI18n()
  const heading = useRef<HTMLHeadingElement>(null)

  useEffect(() => {
    document.title = `${children} · ${wording.productName}`
  }, [children, wording.productName])

  // `step` marks a heading that replaces another without the address
  // changing, such as moving from editing to signing.
  useEffect(() => {
    if (step || hasNavigated()) heading.current?.focus()
    if (step) window.scrollTo(0, 0)
  }, [step])

  return (
    <h1 ref={heading} tabIndex={-1}>
      {children}
    </h1>
  )
}

/** A refusal from the service, in the reader's language. Announced when it appears. */
export function Failure({ code }: { code: ErrorCode | null }) {
  const { errorText } = useI18n()
  if (!code) return null
  return (
    <p className="notice notice-error" role="alert">
      {errorText(code)}
    </p>
  )
}

/** Something that happened, announced without interrupting. */
export function Notice({ children }: { children: ReactNode }) {
  return (
    <p className="notice" role="status">
      {children}
    </p>
  )
}

export interface ControlProps {
  id: string
  'aria-describedby': string | undefined
  'aria-invalid': true | undefined
}

interface FieldProps {
  label: string
  hint?: string
  error?: string | null
  /** A fixed id, when something else needs to find the control. */
  id?: string
  children: (control: ControlProps) => ReactNode
}

/** A labelled control with its hint and its error, tied together for assistive technology. */
export function Field({ label, hint, error, id: fixedId, children }: FieldProps) {
  const generated = useId()
  const id = fixedId ?? generated
  const described = [hint ? `${id}-hint` : null, error ? `${id}-error` : null].filter(Boolean)
  return (
    <div className="field">
      <label htmlFor={id}>{label}</label>
      {hint && (
        <p className="hint" id={`${id}-hint`}>
          {hint}
        </p>
      )}
      {children({
        id,
        'aria-describedby': described.length > 0 ? described.join(' ') : undefined,
        'aria-invalid': error ? true : undefined,
      })}
      {error && (
        <p className="field-error" id={`${id}-error`}>
          {error}
        </p>
      )}
    </div>
  )
}

/**
 * Text a person wrote: a name, terms, a description, a note. It is shown
 * exactly as written and never translated (DESIGN.md §4.2), set apart so it
 * cannot be mistaken for the product speaking, and laid out in whichever
 * direction its own script runs.
 */
export function Written({ children, inline }: { children: string; inline?: boolean }) {
  return inline ? (
    <bdi className="written-inline">{children}</bdi>
  ) : (
    <p className="written" dir="auto">
      {children}
    </p>
  )
}
