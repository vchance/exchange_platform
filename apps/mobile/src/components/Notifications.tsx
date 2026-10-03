import type { Account, ExchangeSummary } from '@yuppers/api-client';
import { useState } from 'react';
import { Linking } from 'react-native';

import { announce } from '../lib/accessibility';
import { useI18n } from '../lib/context';
import {
  dismissOffer,
  isOn,
  pushState,
  shouldOffer,
  turnOff,
  turnOn,
  usePush,
} from '../lib/push';
import { Actions, Button, Card, Check, ErrorNote, Heading, Hint, P } from './ui';

/*
 * What the screens say and offer about push notifications (`lib/push.ts`).
 * Nothing at all where push cannot be had: in the browser harness, in a
 * build that names no Expo project, or against a service that sends none.
 */

/**
 * On the list of yups: an offer to turn notifications on, once the person
 * has a yup that is past a draft, sent or joined, which is when there is
 * something to be told about. Turning on is what asks the system; not now
 * puts the offer away for good, and the account screen keeps the switch.
 */
export function NotificationsOffer({
  exchanges,
}: {
  exchanges: readonly ExchangeSummary[] | null;
}) {
  const { wording, language } = useI18n();
  const w = wording.mobile.notifications;
  const { state, setState } = usePush();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);

  const engaged = exchanges?.some((exchange) => exchange.state !== 'DRAFT') ?? false;
  if (!state || !engaged || !shouldOffer(state)) return null;

  async function on() {
    setBusy(true);
    setFailed(false);
    const outcome = await turnOn(language, w.channel);
    setBusy(false);
    if (outcome === 'failed') {
      setFailed(true);
      return;
    }
    if (outcome === 'on') announce(`${w.switch}: ${wording.profile.saved}`);
    setState(await pushState());
  }

  async function later() {
    await dismissOffer();
    setState(await pushState());
  }

  return (
    <Card>
      <Heading level={2}>{w.askHeading}</Heading>
      <P>{w.askBody}</P>
      <Actions>
        <Button
          variant="primary"
          label={w.turnOn}
          disabled={busy}
          onPress={() => void on()}
          testID="notifications-on"
        />
        <Button label={w.notNow} disabled={busy} onPress={() => void later()} />
      </Actions>
      {failed && <ErrorNote>{w.failed}</ErrorNote>}
    </Card>
  );
}

/**
 * On the account screen: the switch for this phone, which follows the
 * system's permission, and what a phone-only account should know about how
 * it hears of its yups.
 */
export function NotificationsSetting({ account }: { account: Account }) {
  const { wording, language } = useI18n();
  const w = wording.mobile.notifications;
  const { state, setState } = usePush();
  const [busy, setBusy] = useState(false);
  const [failed, setFailed] = useState(false);
  const phoneOnly = !account.email;

  if (!state) return null;
  if (!state.available) {
    // Text messages carry codes only, so without an email address, and
    // without push here, nothing reaches the person (DESIGN.md §12).
    return phoneOnly ? <Hint>{w.phoneOnlyNoPush}</Hint> : null;
  }

  const on = isOn(state);
  const blocked = state.permission === 'denied';

  async function change(next: boolean) {
    setBusy(true);
    setFailed(false);
    const done = next ? (await turnOn(language, w.channel)) !== 'failed' : await turnOff();
    setBusy(false);
    if (!done) setFailed(true);
    setState(await pushState());
  }

  return (
    <>
      <Heading level={2}>{w.heading}</Heading>
      <Check
        testID="notifications-switch"
        label={w.switch}
        hint={w.switchHint}
        value={on}
        disabled={busy || (blocked && !on)}
        onChange={(next) => void change(next)}
      />
      {blocked && (
        <>
          <P>{w.blocked}</P>
          <Actions>
            <Button label={w.openSettings} onPress={() => void Linking.openSettings()} />
          </Actions>
        </>
      )}
      {failed && <ErrorNote>{w.failed}</ErrorNote>}
      {phoneOnly && <Hint>{w.phoneOnly}</Hint>}
    </>
  );
}
