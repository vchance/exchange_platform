import { useState } from 'react'

import { useI18n } from '../app/context'
import { paths } from '../app/routes'
import { Field, Notice } from './ui'

/**
 * Naming who an invitation is for, so that only an account verified with
 * that email address or phone number can use the link (DESIGN.md §8).
 */
export function InvitationFor({ value, onChange }: { value: string; onChange(value: string): void }) {
  const { wording } = useI18n()
  return (
    <Field label={wording.invitationLink.forLabel} hint={wording.invitationLink.forHint}>
      {(control) => (
        <input
          {...control}
          type="text"
          inputMode="email"
          autoComplete="off"
          autoCapitalize="none"
          spellCheck={false}
          value={value}
          onChange={(event) => onChange(event.target.value)}
        />
      )}
    </Field>
  )
}

/**
 * The invitation link, shown once: only its hash is kept by the service, so
 * it cannot be shown again. The initiator sends it through a channel of
 * their own; the platform never does (DESIGN.md §8).
 *
 * The link is in the sender's language, so the preview a messaging app builds
 * for it is too. What is shared alongside it is fixed wording with no name,
 * term or amount in it.
 */
export function InvitationLink({ token }: { token: string }) {
  const { wording, language } = useI18n()
  const w = wording.invitationLink
  const [copied, setCopied] = useState<'yes' | 'failed' | null>(null)
  const link = window.location.origin + paths.invitation(language, token)

  async function copy() {
    try {
      await navigator.clipboard.writeText(link)
      setCopied('yes')
    } catch {
      setCopied('failed')
    }
  }

  function share() {
    // Dismissing the share sheet rejects; there is nothing to do about it.
    navigator.share({ title: wording.linkPreview.title, text: w.shareText, url: link }).catch(() => {})
  }

  return (
    <div className="invitation-link">
      <p>{w.intro}</p>
      <Field label={w.linkLabel} hint={w.shownOnce}>
        {(control) => (
          <input
            {...control}
            type="text"
            readOnly
            dir="ltr"
            value={link}
            onFocus={(event) => event.target.select()}
          />
        )}
      </Field>
      <div className="actions">
        <button type="button" className="primary" onClick={copy}>
          {w.copy}
        </button>
        {typeof navigator.share === 'function' && (
          <button type="button" onClick={share}>
            {w.share}
          </button>
        )}
      </div>
      {copied && <Notice>{copied === 'yes' ? w.copied : w.copyFailed}</Notice>}
    </div>
  )
}
