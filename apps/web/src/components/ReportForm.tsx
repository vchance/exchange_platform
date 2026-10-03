import type { ErrorCode } from '@exchange/api-client'
import {
  checkReport,
  REPORT_DETAILS_MAX_CHARS,
  REPORT_REASONS,
  reportNeedsDetails,
  type ReportReason,
} from '@exchange/shared'
import { useId, useState, type FormEvent } from 'react'

import { useI18n } from '../app/context'
import { ErrorNote, Failure, Field } from './ui'

interface Props {
  /** Said before anything is asked: that reports are reviewed, and who is not told. */
  intro: string
  busy: boolean
  failure: ErrorCode | null
  onSend(reason: ReportReason, details: string | null): void
  onCancel(): void
}

/**
 * What a report says (DESIGN.md §9): one reason from a short list, and
 * whatever the person wants to add. The same form reports an exchange from
 * inside it and a proposal from its invitation link. Nothing is chosen to
 * begin with, so a report is never sent with a reason nobody picked.
 */
export function ReportForm({ intro, busy, failure, onSend, onCancel }: Props) {
  const { wording } = useI18n()
  const w = wording.safety
  const id = useId()
  const [reason, setReason] = useState<ReportReason | null>(null)
  const [details, setDetails] = useState('')
  const [checked, setChecked] = useState(false)

  // What is missing is said once sending has been tried, not before.
  const check = checkReport(reason, details)
  const reasonMissing = checked && !check.ok && check.reasonMissing
  const detailsMissing = checked && !check.ok && check.detailsMissing
  const explain = reportNeedsDetails(reason)

  function submit(event: FormEvent) {
    event.preventDefault()
    setChecked(true)
    if (check.ok) onSend(check.reason, check.details)
    // The keyboard goes to the first thing to fix, once it has been marked.
    else if (check.reasonMissing) document.getElementById(`${id}-reason-first`)?.focus()
    else document.getElementById(`${id}-details`)?.focus()
  }

  return (
    <form noValidate onSubmit={submit}>
      <p>{intro}</p>
      <fieldset
        role="radiogroup"
        aria-required
        aria-invalid={reasonMissing ? true : undefined}
        aria-describedby={reasonMissing ? `${id}-reason-error` : undefined}
      >
        <legend>{w.reasonLegend}</legend>
        {REPORT_REASONS.map((option, index) => (
          <label className="check" key={option}>
            <input
              type="radio"
              id={index === 0 ? `${id}-reason-first` : undefined}
              aria-describedby={reasonMissing ? `${id}-reason-error` : undefined}
              name={`${id}-reason`}
              value={option}
              checked={reason === option}
              onChange={() => setReason(option)}
            />
            <span>{w.reasons[option]}</span>
          </label>
        ))}
        {reasonMissing && (
          <p className="field-error" id={`${id}-reason-error`}>
            {w.reasonRequired}
          </p>
        )}
      </fieldset>
      <Field
        label={explain ? w.detailsRequiredLabel : w.detailsLabel}
        hint={w.detailsHint}
        id={`${id}-details`}
        required={explain}
        error={detailsMissing ? w.detailsRequired : null}
      >
        {(control) => (
          <textarea
            {...control}
            rows={4}
            maxLength={REPORT_DETAILS_MAX_CHARS}
            value={details}
            onChange={(event) => setDetails(event.target.value)}
          />
        )}
      </Field>
      {/* Here the limit is on reports in a day, not on requests in a minute. */}
      {failure === 'TOO_MANY_REQUESTS' ? (
        <ErrorNote>{w.tooManyReports}</ErrorNote>
      ) : (
        <Failure code={failure} />
      )}
      <div className="actions">
        <button type="submit" className="primary" disabled={busy}>
          {w.sendReport}
        </button>
        <button type="button" disabled={busy} onClick={onCancel}>
          {wording.common.cancel}
        </button>
      </div>
    </form>
  )
}
