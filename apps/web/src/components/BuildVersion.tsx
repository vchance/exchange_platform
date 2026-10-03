import { versionText } from '@yuppers/shared'

import { useI18n } from '../app/context'
import { WEB_BUILD } from '../lib/api'

/**
 * Which build of the web app this is, small, at the foot of a page: "Version
 * 0.1.0 (abc1234)". What a person reads out when something goes wrong, and
 * what a reviewer notes beside a report. The service's own build is in
 * `GET /v1/meta` and every response's `X-Yuppers-Version`.
 */
export function BuildVersion() {
  const { wording, language } = useI18n()
  return (
    <p className="hint build-version" dir="ltr">
      {versionText(wording, language, WEB_BUILD)}
    </p>
  )
}
