import type { ErrorCode, ExchangeView as Exchange } from '@exchange/api-client';
import { failureCode, type RevisionSent } from '@exchange/shared';
import { useFocusEffect, useRouter } from 'expo-router';
import { useCallback, useState } from 'react';

import { Actions, Button, Failure, Heading, P, Screen } from '../components/ui';
import { useI18n } from '../lib/context';
import { api } from '../lib/session';
import { Composer } from './Composer';
import { ExchangeView } from './ExchangeView';

/** Loads one exchange, and again whenever its screen comes back to the front. */
function useExchange(id: string) {
  const [exchange, setExchange] = useState<Exchange | null>(null);
  const [failure, setFailure] = useState<ErrorCode | null>(null);

  const reload = useCallback(async () => {
    try {
      const latest = await api.getExchange(id);
      setExchange(latest);
      setFailure(null);
      return latest;
    } catch (error) {
      setFailure(failureCode(error));
      return null;
    }
  }, [id]);

  // Coming back from writing a counteroffer, the exchange shows it.
  useFocusEffect(
    useCallback(() => {
      void reload();
    }, [reload]),
  );

  return { exchange, setExchange, failure, reload };
}

function Unavailable({ failure }: { failure: ErrorCode | null }) {
  const { wording } = useI18n();
  const router = useRouter();
  if (!failure) {
    return (
      <Screen>
        <P>{wording.common.loading}</P>
      </Screen>
    );
  }
  return (
    <Screen>
      <Heading>{wording.exchange.titleNoName}</Heading>
      <Failure code={failure} />
      <Actions>
        <Button label={wording.common.goHome} onPress={() => router.dismissTo('/')} />
      </Actions>
    </Screen>
  );
}

/**
 * One exchange. A draft is its composer; anything further along is the
 * exchange view.
 */
export function ExchangeScreen({ id }: { id: string }) {
  const router = useRouter();
  const { exchange, setExchange, failure, reload } = useExchange(id);
  // The invitation token, held only while this screen stays open: it is shown
  // once and cannot be fetched again.
  const [issued, setIssued] = useState<string | null>(null);

  if (!exchange) return <Unavailable failure={failure} />;

  if (exchange.state === 'DRAFT') {
    return (
      <Composer
        exchange={exchange}
        reload={reload}
        onLeave={() => router.dismissTo('/')}
        onSent={(sent: RevisionSent) => {
          // The first thing on the exchange after a first proposal is the
          // invitation link.
          setIssued(sent.invitation_token ?? null);
          setExchange(sent.exchange);
        }}
      />
    );
  }

  return (
    <ExchangeView
      exchange={exchange}
      issued={issued}
      onIssued={setIssued}
      onChange={setExchange}
      reload={reload}
    />
  );
}

/** Writing a counteroffer or an amendment, over the exchange it belongs to. */
export function ReviseScreen({ id }: { id: string }) {
  const router = useRouter();
  const { exchange, failure, reload } = useExchange(id);

  // Back to the exchange, which reloads as it comes to the front.
  const back = () => {
    if (router.canGoBack()) router.back();
    else router.replace(`/exchanges/${id}`);
  };

  if (!exchange) return <Unavailable failure={failure} />;
  return <Composer exchange={exchange} reload={reload} onLeave={back} onSent={back} />;
}
