import type { ErrorCode } from '@yuppers/api-client'
import { useId, useState } from 'react'

import { useI18n } from '../app/context'
import { Failure } from './ui'

interface Props {
  /** What the signing button says: sending and accepting both sign. */
  signLabel: string
  busy: boolean
  failure: ErrorCode | null
  onSign(): void
  onCancel(): void
  cancelLabel?: string
  /** Its heading's level: 3 inside a panel under a section, 2 where it follows the page's terms. */
  level?: 2 | 3
}

/**
 * The consent step, the same for sending and for accepting, because both are
 * signatures (DESIGN.md §14.1). It goes directly under the complete terms.
 * The box starts unticked every time it is shown and the button does nothing
 * until it is ticked, so nothing is signed by default or by accident.
 *
 * The wording is a placeholder until counsel approves the real text, and says
 * so on the screen.
 */
export function Consent({
  signLabel,
  busy,
  failure,
  onSign,
  onCancel,
  cancelLabel,
  level = 3,
}: Props) {
  const H = level === 2 ? 'h2' : 'h3'
  const { wording } = useI18n()
  const w = wording.consent
  const [agreed, setAgreed] = useState(false)
  const id = useId()

  return (
    <section className="consent" aria-labelledby={`${id}-heading`}>
      <H id={`${id}-heading`}>{w.heading}</H>
      <p className="notice notice-warning">{w.pendingReview}</p>
      <p>{w.binding}</p>
      <p>{w.electronic}</p>
      <p>{w.noJudge}</p>
      <label className="check">
        <input
          type="checkbox"
          required
          checked={agreed}
          onChange={(event) => setAgreed(event.target.checked)}
        />
        <span>{w.agree}</span>
      </label>
      <Failure code={failure} />
      <span hidden id={`${id}-why`}>
        {wording.a11y.signNeedsAgreement}
      </span>
      <div className="actions">
        <button
          type="button"
          className="primary"
          disabled={!agreed || busy}
          // A disabled button is still read in a screen reader's reading
          // mode; this says why it cannot be pressed yet.
          aria-describedby={agreed ? undefined : `${id}-why`}
          onClick={onSign}
        >
          {signLabel}
        </button>
        <button type="button" disabled={busy} onClick={onCancel}>
          {cancelLabel ?? wording.common.cancel}
        </button>
      </div>
    </section>
  )
}
