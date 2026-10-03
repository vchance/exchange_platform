import type { ErrorCode } from '@exchange/api-client'
import { useEffect, useId, useRef, type ReactNode } from 'react'

import { useI18n } from '../app/context'
import { hasNavigated } from '../app/router'
import { announce, useAnnouncement } from '../lib/announce'
import { focusLost } from '../lib/focus'

// How many page headings this page load has shown, to tell the first one,
// which leaves the focus where the browser put it, from those that follow.
let headingsShown = 0

/**
 * A page's main heading. It names the page in the browser tab, and takes the
 * focus when the page was reached from another one, so a screen reader
 * announces the change and the keyboard starts from the top. It also takes
 * the focus when it replaces a page whose control had the focus, such as
 * the sign-in form giving way to the page it stood in for: the focus would
 * otherwise be lost to the top of the document.
 */
export function PageHeading({ children, step }: { children: string; step?: boolean }) {
  const { wording } = useI18n()
  const heading = useRef<HTMLHeadingElement>(null)
  // Where this heading came in this page load, decided once, so that running
  // the effect again (React does in development) does not change the answer.
  const order = useRef<number | null>(null)

  useEffect(() => {
    document.title = `${children} · ${wording.productName}`
  }, [children, wording.productName])

  // `step` marks a heading that replaces another without the address
  // changing, such as moving from editing to signing.
  useEffect(() => {
    if (order.current === null) order.current = headingsShown++
    if (step || hasNavigated() || (order.current > 0 && focusLost())) heading.current?.focus()
    if (step) window.scrollTo(0, 0)
  }, [step])

  return (
    <h1 ref={heading} tabIndex={-1}>
      {children}
    </h1>
  )
}

/**
 * A section heading that takes the focus when it appears, for a step that
 * opens below what the person pressed and replaces it, such as signing in
 * on the invitation page.
 */
export function StepHeading({ children }: { children: string }) {
  const heading = useRef<HTMLHeadingElement>(null)
  useEffect(() => {
    heading.current?.focus()
  }, [])
  return (
    <h2 ref={heading} tabIndex={-1}>
      {children}
    </h2>
  )
}

/**
 * A refusal from the service, in the reader's language. `id` lets the
 * control it is about point to it.
 */
export function Failure({ code, id }: { code: ErrorCode | null; id?: string }) {
  const { errorText } = useI18n()
  const text = code ? errorText(code) : null
  return text ? <ErrorNote id={id}>{text}</ErrorNote> : null
}

/**
 * Something that stops the person, in the product's own words. It is said at
 * once. If the focus was lost on the way, as it is when the button that was
 * pressed is disabled while it works, the focus comes here too, which leaves
 * the keyboard next to what to do about it rather than at the top.
 */
export function ErrorNote({ children, id }: { children: string; id?: string }) {
  const note = useRef<HTMLParagraphElement>(null)
  useEffect(() => {
    announce(children, 'assertive')
    if (focusLost()) note.current?.focus()
  }, [children])
  return (
    <p className="notice notice-error" id={id} tabIndex={-1} ref={note}>
      {children}
    </p>
  )
}

/** Something that happened, announced without interrupting. */
export function Notice({ children }: { children: string }) {
  useAnnouncement(children)
  return <p className="notice">{children}</p>
}

export interface ControlProps {
  id: string
  'aria-describedby': string | undefined
  'aria-invalid': true | undefined
  'aria-required': true | undefined
}

interface FieldProps {
  label: string
  hint?: string
  error?: string | null
  /** A fixed id, when something else needs to find the control. */
  id?: string
  /** Has to be filled in; said to assistive technology, and checked when the form is sent. */
  required?: boolean
  /**
   * Something else on the page that is about this control, such as a refusal
   * from the service shown under the form: its id, to be read with the
   * control, which is then marked invalid.
   */
  problem?: string | null
  children: (control: ControlProps) => ReactNode
}

/** A labelled control with its hint and its error, tied together for assistive technology. */
export function Field({
  label,
  hint,
  error,
  id: fixedId,
  required,
  problem,
  children,
}: FieldProps) {
  const generated = useId()
  const id = fixedId ?? generated
  const described = [
    hint ? `${id}-hint` : null,
    error ? `${id}-error` : null,
    problem ?? null,
  ].filter(Boolean)
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
        'aria-invalid': error || problem ? true : undefined,
        'aria-required': required ? true : undefined,
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
