import type { ErrorCode } from '@exchange/api-client';
import {
  checkReport,
  REPORT_DETAILS_MAX_CHARS,
  REPORT_REASONS,
  reportNeedsDetails,
  type ReportReason,
} from '@exchange/shared';
import { useState } from 'react';

import { useI18n } from '../lib/context';
import { Actions, Button, Choice, ErrorNote, Failure, P, TextField } from './ui';

interface Props {
  /** Said before anything is asked: that reports are reviewed, and who is not told. */
  intro: string;
  busy: boolean;
  failure: ErrorCode | null;
  onSend(reason: ReportReason, details: string | null): void;
  onCancel(): void;
}

/**
 * What a report says (DESIGN.md §9): one reason from a short list, and
 * whatever the person wants to add. The same form reports an exchange from
 * inside it and a proposal from its invitation. Nothing is chosen to begin
 * with, so a report is never sent with a reason nobody picked.
 */
export function ReportForm({ intro, busy, failure, onSend, onCancel }: Props) {
  const { wording } = useI18n();
  const w = wording.safety;
  const [reason, setReason] = useState<ReportReason | null>(null);
  const [details, setDetails] = useState('');
  const [checked, setChecked] = useState(false);

  // What is missing is said once sending has been tried, not before.
  const check = checkReport(reason, details);
  const reasonMissing = checked && !check.ok && check.reasonMissing;
  const detailsMissing = checked && !check.ok && check.detailsMissing;

  function submit() {
    setChecked(true);
    if (check.ok) onSend(check.reason, check.details);
  }

  return (
    <>
      <P>{intro}</P>
      <Choice<ReportReason>
        label={w.reasonLegend}
        error={reasonMissing ? w.reasonRequired : null}
        value={reason}
        options={REPORT_REASONS.map((option) => ({ value: option, label: w.reasons[option] }))}
        onChange={setReason}
        disabled={busy}
      />
      <TextField
        label={reportNeedsDetails(reason) ? w.detailsRequiredLabel : w.detailsLabel}
        hint={w.detailsHint}
        required={reportNeedsDetails(reason)}
        error={detailsMissing ? w.detailsRequired : null}
        multiline
        maxLength={REPORT_DETAILS_MAX_CHARS}
        disabled={busy}
        value={details}
        onChangeText={setDetails}
      />
      {/* Here the limit is on reports in a day, not on requests in a minute. */}
      {failure === 'TOO_MANY_REQUESTS' ? (
        <ErrorNote>{w.tooManyReports}</ErrorNote>
      ) : (
        <Failure code={failure} />
      )}
      <Actions>
        <Button variant="primary" label={w.sendReport} disabled={busy} onPress={submit} />
        <Button label={wording.common.cancel} disabled={busy} onPress={onCancel} />
      </Actions>
    </>
  );
}
