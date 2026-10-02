import type { ExchangeView } from '@exchange/api-client';

import { useI18n } from '../lib/context';
import { Notice } from './ui';

/**
 * Tells a party that the other one has deleted their account, while the
 * exchange is still open: nothing more will come from them, and waiting for
 * it would be waiting for nothing. The service says so only then.
 */
export function OtherPartyLeft({
  exchange,
  otherName,
}: {
  exchange: ExchangeView;
  otherName: string;
}) {
  const { wording, fmt } = useI18n();
  if (!exchange.other_party_left) return null;
  const w = wording.deletion;
  const message =
    exchange.state === 'ACTIVE' ? w.otherPartyLeftActive : w.otherPartyLeftNegotiating;
  return (
    <Notice tone="warning" quiet>
      {fmt(message, { name: otherName })}
    </Notice>
  );
}
