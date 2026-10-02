import { invitationToken } from '@exchange/shared';
import { useLocalSearchParams, useRouter } from 'expo-router';
import { useEffect } from 'react';

import { holdInvitation, INVITATION_ROUTE } from '../../lib/invitation';

/**
 * The address of an invitation link, `/{language}/i#{token}`, the same as on
 * the web (DESIGN.md §13.5). A link that opens the app has its token taken
 * out before it gets this far (`+native-intent.ts`); this is for one that
 * arrives as an address all the same. Either way the token is moved out of
 * the address at once and the invitation screen is shown in its place.
 */
export default function InvitationAddress() {
  const params = useLocalSearchParams<{ '#'?: string }>();
  const router = useRouter();
  const token = invitationToken(params['#'] ?? '');

  useEffect(() => {
    if (token) holdInvitation(token);
    router.replace(INVITATION_ROUTE);
  }, [token, router]);

  return null;
}
