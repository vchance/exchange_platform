import { createI18n, wordingFor } from '@exchange/shared';
import { DateTimePickerAndroid } from '@react-native-community/datetimepicker';
import { fireEvent, render, screen } from '@testing-library/react-native';
import { Platform } from 'react-native';

import { I18nContext } from '../../lib/context';
import { DateField } from '../DateField';

jest.mock('expo-secure-store', () => ({}));
jest.mock('expo-crypto', () => ({ randomUUID: () => '00000000-0000-4000-8000-000000000000' }));

// Each platform's picker is replaced by something that records what it was
// given: a view on iOS, the dialog's `open` call on Android.
jest.mock('@react-native-community/datetimepicker', () => {
  const { View } = jest.requireActual('react-native');
  const { createElement } = jest.requireActual('react');
  return {
    __esModule: true,
    default: (props: object) => createElement(View, { testID: 'ios-picker', ...props }),
    DateTimePickerAndroid: { open: jest.fn(), dismiss: jest.fn() },
  };
});

const wording = wordingFor('en');
const i18n = createI18n('en', wording, () => {});
const label = wording.composer.dateLabel;
const openDialog = jest.mocked(DateTimePickerAndroid.open);

function show(value: string, onChange: (value: string) => void) {
  return render(
    <I18nContext value={i18n}>
      <DateField label={label} value={value} onChange={onChange} today="2026-10-02" />
    </I18nContext>,
  );
}

beforeEach(() => openDialog.mockClear());

if (Platform.OS === 'android') {
  test('Android: nothing is chosen until the person opens the dialog and picks', async () => {
    const onChange = jest.fn();
    await show('', onChange);
    await fireEvent.press(screen.getByText(wording.mobile.date.choose));
    expect(onChange).not.toHaveBeenCalled();

    // The dialog opens on today, as a date in UTC, so no timezone can move the day.
    const options = openDialog.mock.calls[0][0];
    expect(options).toMatchObject({ mode: 'date', timeZoneName: 'UTC' });
    expect(options.value.toISOString()).toBe('2026-10-02T00:00:00.000Z');

    const event = { nativeEvent: { timestamp: 0, utcOffset: 0 } };
    options.onValueChange?.(event, new Date('2026-12-31T00:00:00Z'));
    expect(onChange).toHaveBeenCalledWith('2026-12-31');
  });

  test('Android: a chosen date is shown as the language writes it, and reopens on that date', async () => {
    await show('2026-10-30', () => {});
    await fireEvent.press(screen.getByText('October 30, 2026'));
    expect(screen.getByLabelText('Due October 30, 2026. Change the date.')).toBeTruthy();
    expect(openDialog.mock.calls[0][0].value.toISOString()).toBe('2026-10-30T00:00:00.000Z');
  });
} else {
  test('iOS: the picker appears only once there is a date, so none is chosen for the person', async () => {
    const onChange = jest.fn();
    await show('', onChange);
    expect(screen.queryByTestId('ios-picker')).toBeNull();
    await fireEvent.press(screen.getByText(wording.mobile.date.choose));
    expect(onChange).toHaveBeenCalledWith('2026-10-02');
  });

  test('iOS: the picker works in UTC and in the reader’s language, and gives back the day picked', async () => {
    const onChange = jest.fn();
    await show('2026-10-30', onChange);
    const picker = screen.getByTestId('ios-picker');
    expect(picker.props).toMatchObject({
      mode: 'date',
      display: 'compact',
      timeZoneName: 'UTC',
      locale: 'en',
      accessibilityLabel: label,
    });
    expect(picker.props.value.toISOString()).toBe('2026-10-30T00:00:00.000Z');

    await fireEvent(picker, 'valueChange', {}, new Date('2026-12-31T00:00:00Z'));
    expect(onChange).toHaveBeenCalledWith('2026-12-31');
  });
}

test('a date that is no date is treated as none chosen', async () => {
  await show('not-a-date', () => {});
  expect(screen.getByText(wording.mobile.date.choose)).toBeTruthy();
});
