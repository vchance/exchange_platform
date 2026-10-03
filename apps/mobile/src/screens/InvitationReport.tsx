import { useInvitationReport } from '@yuppers/shared';

import { ReportForm } from '../components/ReportForm';
import { Actions, Button, Notice, Panel } from '../components/ui';
import { useI18n } from '../lib/context';
import { api } from '../lib/session';

/**
 * Reporting a proposal from its invitation, before signing in or without
 * ever doing so (DESIGN.md §9). The invitation's token is the proof of having
 * received the proposal, exactly as it is for reading it: it is sent in the
 * request's body and written nowhere. There is no block here: a block is
 * between two accounts, and this reader may have none.
 */
export function InvitationReport({ token }: { token: string }) {
  const { wording } = useI18n();
  const w = wording.safety;
  const report = useInvitationReport(api, token);

  return (
    <>
      {/* The form is gone once the report is sent; this is read out in its place. */}
      {report.sent && <Notice>{w.reportSent}</Notice>}
      <Actions>
        <Button label={w.reportProposal} expanded={report.open} onPress={report.begin} />
      </Actions>
      {report.open && (
        <Panel level={2} title={w.reportProposal}>
          <ReportForm
            intro={w.reportProposalIntro}
            busy={report.busy}
            failure={report.failure}
            onSend={report.send}
            onCancel={report.cancel}
          />
        </Panel>
      )}
    </>
  );
}
