import { useRouter } from 'expo-router';

import { Actions, Button, Heading, P, Screen } from '../components/ui';
import { useI18n } from '../lib/context';

export default function NotFound() {
  const { wording } = useI18n();
  const router = useRouter();
  return (
    <Screen>
      <Heading>{wording.common.notFoundTitle}</Heading>
      <P>{wording.common.notFoundBody}</P>
      <Actions>
        <Button label={wording.common.goHome} onPress={() => router.replace('/')} />
      </Actions>
    </Screen>
  );
}
