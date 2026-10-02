import { deletedNotice, useDeletedNotice } from '@exchange/shared'
import { useEffect } from 'react'

import { useI18n, useSession } from '../app/context'

/**
 * Says, once, that the account was deleted. The page that did it is gone by
 * then and nobody is signed in, so it is said on whatever page comes next,
 * until it is dismissed or someone signs in.
 */
export function AccountDeleted() {
  const { wording } = useI18n()
  const { account } = useSession()
  const shown = useDeletedNotice()
  const signedIn = account !== null

  useEffect(() => {
    if (signedIn) deletedNotice.dismiss()
  }, [signedIn])

  if (!shown || signedIn) return null
  return (
    <p className="notice" role="status">
      {wording.deletion.deleted}{' '}
      <button type="button" onClick={deletedNotice.dismiss}>
        {wording.deletion.dismiss}
      </button>
    </p>
  )
}
