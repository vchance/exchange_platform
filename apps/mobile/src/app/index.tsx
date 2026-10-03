import { isComplete } from '@exchange/shared';
import { Stack, useRouter } from 'expo-router';

import { AccountDeleted } from '../components/AccountDeleted';
import { Button } from '../components/ui';
import { useI18n, useSession } from '../lib/context';
import { Gate } from '../screens/AccountSetup';
import { HomeScreen, InvitedEntry } from '../screens/HomeScreen';

/** The first screen: your exchanges, or the way to sign in and the way to an invitation. */
export default function Home() {
  const { wording } = useI18n();
  const { account } = useSession();
  const router = useRouter();
  const able = account !== null && isComplete(account);

  return (
    <>
      <Stack.Screen
        options={{
          headerRight: able
            ? () => (
                <Button
                  variant="link"
                  label={wording.nav.account}
                  onPress={() => router.push('/account')}
                />
              )
            : undefined,
        }}
      />
      <AccountDeleted />
      <Gate signedOut={<InvitedEntry />}>
        <HomeScreen />
      </Gate>
    </>
  );
}
