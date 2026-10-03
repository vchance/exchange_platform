import type { ExchangeView } from '@yuppers/api-client';

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

/**
 * Tells a party that a reviewer has hidden what was written in this
 * exchange from them (DESIGN.md §9): the service shows them a placeholder
 * in its place, and refuses signing or changing the terms.
 */
export function ContentHidden({ exchange }: { exchange: ExchangeView }) {
  const { wording } = useI18n();
  if (!exchange.content_hidden) return null;
  return (
    <Notice tone="warning" quiet>
      {wording.exchange.contentHidden}
    </Notice>
  );
}
