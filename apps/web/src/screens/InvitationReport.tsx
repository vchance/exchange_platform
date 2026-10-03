import { useInvitationReport } from '@exchange/shared'
import { useEffect, useRef } from 'react'

import { useI18n } from '../app/context'
import { Panel } from '../components/Panel'
import { ReportForm } from '../components/ReportForm'
import { safetyApi } from '../lib/safety'

/**
 * Reporting a proposal from its invitation link, before signing in or
 * without ever doing so (DESIGN.md §9). The link's token is the proof of
 * having received the proposal, exactly as it is for reading it. There is no
 * block here: a block is between two accounts, and this reader may have none.
 */
export function InvitationReport({ token }: { token: string }) {
  const { wording } = useI18n()
  const w = wording.safety
  const { open, busy, failure, sent, begin, cancel, send } = useInvitationReport(safetyApi, token)
  const opener = useRef<HTMLButtonElement>(null)
  const announced = useRef<HTMLParagraphElement>(null)

  // The form is gone once the report is sent; say so where the focus can find it.
  useEffect(() => {
    if (sent) announced.current?.focus()
  }, [sent])

  return (
    <section aria-label={w.reportProposal}>
      {sent && (
        <p className="notice" tabIndex={-1} ref={announced}>
          {w.reportSent}
        </p>
      )}
      <div className="actions">
        <button type="button" className="link" aria-expanded={open} ref={opener} onClick={begin}>
          {w.reportProposal}
        </button>
      </div>
      {open && (
        <Panel title={w.reportProposal}>
          <ReportForm
            intro={w.reportProposalIntro}
            busy={busy}
            failure={failure}
            onSend={send}
            onCancel={() => {
              cancel()
              opener.current?.focus()
            }}
          />
        </Panel>
      )}
    </section>
  )
}
