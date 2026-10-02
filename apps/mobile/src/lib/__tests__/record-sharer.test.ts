import { recordFile, type RecordDocument } from '@exchange/shared';
import { Directory, File, Paths } from 'expo-file-system';
import * as Sharing from 'expo-sharing';
import { Platform } from 'react-native';

import { recordSharer } from '../record-sharer';

/*
 * What is handed to the system's share sheet. The sheet itself is the
 * system's and cannot be shown here: these say what file it is given, with
 * what in it, and what becomes of the file afterwards. The file system is the
 * test preset's in-memory one.
 */

jest.mock('expo-sharing', () => ({
  isAvailableAsync: jest.fn(async () => true),
  shareAsync: jest.fn(async () => {}),
}));

const sharing = jest.mocked(Sharing);
const copies = () => new Directory(Paths.cache, 'record-copies');

// Enough of a record to tell one copy from another.
const record = {
  format: 'exchange-record',
  format_version: 1,
  exchange: { display_code: 'PVVS-5Q2K' },
  parties: { A: 'Ana Ruiz', B: 'Ben Ortiz' },
} as unknown as RecordDocument;
const file = recordFile(record, 'exchange-record-PVVS-5Q2K');

/** What the file held at the moment the share sheet was given it. */
let handed: { uri: string; text: string }[] = [];

beforeEach(() => {
  recordSharer.forget();
  handed = [];
  sharing.isAvailableAsync.mockReset().mockResolvedValue(true);
  sharing.shareAsync.mockReset().mockImplementation(async (uri) => {
    handed.push({ uri, text: new File(uri).textSync() });
  });
});

test('the share sheet is given one JSON file, named for the exchange, holding the record', async () => {
  await expect(recordSharer.share(file, 'Record of exchange PVVS-5Q2K')).resolves.toBe('handed');

  expect(sharing.shareAsync).toHaveBeenCalledTimes(1);
  const [uri, options] = sharing.shareAsync.mock.calls[0];
  // A file in the app's own cache, which no other app can read until it is shared.
  expect(uri).toBe(`${Paths.cache.uri}record-copies/exchange-record-PVVS-5Q2K.json`);
  expect(uri.startsWith('file://')).toBe(true);
  expect(options).toEqual({
    mimeType: 'application/json',
    UTI: 'public.json',
    dialogTitle: 'Record of exchange PVVS-5Q2K',
  });
  // Exactly the document the service wrote, as the web's download is.
  expect(handed[0].text).toBe(file.text);
  expect(JSON.parse(handed[0].text)).toEqual(record);
});

test('the copy does not stay on the device longer than it has to', async () => {
  await recordSharer.share(file, 'title');
  if (Platform.OS === 'ios') {
    // The sheet has closed, so whatever was picked has its copy.
    expect(copies().exists).toBe(false);
  } else {
    // The app that was picked may not have read it yet.
    expect(new File(handed[0].uri).exists).toBe(true);
  }

  // Signing out, and making the next copy, both clear what is left.
  recordSharer.forget();
  expect(copies().exists).toBe(false);
});

test('there is never more than one copy', async () => {
  await recordSharer.share(file, 'title');
  const other = recordFile(record, 'exchange-record-AB12-CD34');
  await recordSharer.share(other, 'title');
  expect(handed.map((given) => given.uri.split('/').at(-1))).toEqual([
    'exchange-record-PVVS-5Q2K.json',
    'exchange-record-AB12-CD34.json',
  ]);
  if (Platform.OS !== 'ios') {
    expect(new File(handed[0].uri).exists).toBe(false);
    expect(copies().list()).toHaveLength(1);
  }
});

test('a device that cannot share is told apart from a failure, and nothing is written', async () => {
  sharing.isAvailableAsync.mockResolvedValue(false);
  await expect(recordSharer.share(file, 'title')).resolves.toBe('unavailable');
  expect(sharing.shareAsync).not.toHaveBeenCalled();
  expect(copies().exists).toBe(false);
});

test('when the sheet cannot be shown the failure is passed on', async () => {
  sharing.shareAsync.mockRejectedValue(new Error('no activity'));
  await expect(recordSharer.share(file, 'title')).rejects.toThrow('no activity');
  if (Platform.OS === 'ios') expect(copies().exists).toBe(false);
});

test('a file name is one path segment, whatever the wording made of it', async () => {
  await recordSharer.share({ ...file, name: '../registro: A/B.json' }, 'title');
  expect(decodeURIComponent(handed[0].uri)).toBe(
    `${Paths.cache.uri}record-copies/-registro- A-B.json`,
  );
});

test('forgetting when there is nothing to forget does nothing', () => {
  expect(() => recordSharer.forget()).not.toThrow();
  expect(() => recordSharer.forget()).not.toThrow();
});
