import { CONSENT_VERSION, createI18n, wordingFor } from '@yuppers/shared';
import { fireEvent, render, screen } from '@testing-library/react-native';

import { I18nContext } from '../../lib/context';
import { Consent } from '../Consent';

jest.mock('expo-secure-store', () => ({}));
jest.mock('expo-crypto', () => ({ randomUUID: () => '00000000-0000-4000-8000-000000000000' }));

const wording = wordingFor('en');
const i18n = createI18n('en', wording, () => {});

/** Whether a switch is on. The two platforms' switches name the property differently. */
const isOn = (element: { props: Record<string, unknown> }) =>
  element.props.value ?? element.props.on;

function show(onSign: () => void, busy = false) {
  return render(
    <I18nContext value={i18n}>
      <Consent
        signLabel={wording.exchange.accept}
        busy={busy}
        failure={null}
        onSign={onSign}
        onCancel={() => {}}
      />
    </I18nContext>,
  );
}

test('the consent wording is shown, marked as pending legal review', async () => {
  await show(() => {});
  for (const text of [
    wording.consent.pendingReview,
    wording.consent.binding,
    wording.consent.electronic,
    wording.consent.noJudge,
  ]) {
    expect(screen.getByText(text)).toBeTruthy();
  }
  // The version the signature will name is the one this wording is.
  expect(CONSENT_VERSION).toBe('draft-1');
});

test('nothing is signed by default: the switch starts off and the button does nothing', async () => {
  const onSign = jest.fn();
  await show(onSign);

  const agree = screen.getByTestId('consent-agree');
  const sign = screen.getByTestId('consent-sign');
  expect(isOn(agree)).toBe(false);
  expect(sign.props.accessibilityState).toMatchObject({ disabled: true });

  await fireEvent.press(sign);
  expect(onSign).not.toHaveBeenCalled();
});

test('signing takes two deliberate acts: agreeing, then pressing sign', async () => {
  const onSign = jest.fn();
  await show(onSign);

  await fireEvent(screen.getByTestId('consent-agree'), 'valueChange', true);
  const sign = screen.getByTestId('consent-sign');
  expect(sign.props.accessibilityState).toMatchObject({ disabled: false });
  expect(onSign).not.toHaveBeenCalled();

  await fireEvent.press(sign);
  expect(onSign).toHaveBeenCalledTimes(1);
});

test('taking the agreement back disables signing again', async () => {
  const onSign = jest.fn();
  await show(onSign);
  await fireEvent(screen.getByTestId('consent-agree'), 'valueChange', true);
  await fireEvent(screen.getByTestId('consent-agree'), 'valueChange', false);
  await fireEvent.press(screen.getByTestId('consent-sign'));
  expect(onSign).not.toHaveBeenCalled();
});

test('while a signature is being sent, a second press sends nothing more', async () => {
  const onSign = jest.fn();
  await show(onSign, true);
  await fireEvent(screen.getByTestId('consent-agree'), 'valueChange', true);
  await fireEvent.press(screen.getByTestId('consent-sign'));
  expect(onSign).not.toHaveBeenCalled();
});

test('each time it is shown, it starts unagreed', async () => {
  const first = await show(() => {});
  await fireEvent(screen.getByTestId('consent-agree'), 'valueChange', true);
  await first.unmount();
  await show(() => {});
  expect(isOn(screen.getByTestId('consent-agree'))).toBe(false);
});
