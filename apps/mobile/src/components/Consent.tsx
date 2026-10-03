import type { ErrorCode } from '@yuppers/api-client';
import { useState } from 'react';
import { StyleSheet, View } from 'react-native';

import { useI18n } from '../lib/context';
import { space } from '../lib/theme';
import { Actions, Button, Check, Failure, Heading, Notice, P } from './ui';

interface Props {
  /** What the signing button says: sending and accepting both sign. */
  signLabel: string;
  busy: boolean;
  failure: ErrorCode | null;
  onSign(): void;
  onCancel(): void;
  cancelLabel?: string;
  /** The level of its heading: one below whatever it sits under. */
  headingLevel?: 3 | 4;
}

/**
 * The consent step, the same for sending and for accepting, because both are
 * signatures (DESIGN.md §14.1). It goes directly under the complete terms.
 * The switch starts off every time it is shown and the button does nothing
 * until it is on, so nothing is signed by default or by accident.
 *
 * The wording is a placeholder until counsel approves the real text, and says
 * so on the screen.
 */
export function Consent({
  signLabel,
  busy,
  failure,
  onSign,
  onCancel,
  cancelLabel,
  headingLevel = 3,
}: Props) {
  const { wording } = useI18n();
  const w = wording.consent;
  const [agreed, setAgreed] = useState(false);

  return (
    <View style={styles.consent}>
      <Heading level={headingLevel}>{w.heading}</Heading>
      <Notice tone="warning" quiet>
        {w.pendingReview}
      </Notice>
      <P>{w.binding}</P>
      <P>{w.electronic}</P>
      <P>{w.noJudge}</P>
      <Check testID="consent-agree" label={w.agree} value={agreed} onChange={setAgreed} />
      <Failure code={failure} />
      <Actions>
        <Button
          testID="consent-sign"
          variant="primary"
          label={signLabel}
          hint={agreed ? undefined : wording.a11y.signNeedsAgreement}
          disabled={!agreed || busy}
          onPress={() => {
            // The button is disabled until the switch is on; this is the same rule again.
            if (agreed && !busy) onSign();
          }}
        />
        <Button label={cancelLabel ?? wording.common.cancel} disabled={busy} onPress={onCancel} />
      </Actions>
    </View>
  );
}

const styles = StyleSheet.create({
  consent: { gap: space.m },
});
