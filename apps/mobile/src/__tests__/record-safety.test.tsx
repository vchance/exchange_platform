import { wordingFor } from '@yuppers/shared';
import { File } from 'expo-file-system';
import { fireEvent, renderRouter, screen, waitFor } from 'expo-router/testing-library';
import * as Sharing from 'expo-sharing';

import { forgetInvitation } from '../lib/invitation';
import { PARTIES, recordAndSafety, recordOf } from './fake-record';
import { EXCHANGE, fakeService, INVITATION, TOKEN, ana, type FakeService } from './fake-service';

/*
 * History, the record, and report and block, in the whole app as iOS and as
 * Android builds would run it, against the stand-in service. Like
 * `app.test.tsx`, this is the nearest thing to opening the app short of a
 * device. The system's share sheet is replaced by a mock that keeps what it
 * was handed; the file system is the test preset's in-memory one.
 */

const mockKeychain = new Map<string, string>();
jest.mock('expo-secure-store', () => ({
  WHEN_UNLOCKED_THIS_DEVICE_ONLY: 'when-unlocked-this-device-only',
  getItemAsync: jest.fn(async (key: string) => mockKeychain.get(key) ?? null),
  setItemAsync: jest.fn(async (key: string, value: string) => {
    mockKeychain.set(key, value);
  }),
  deleteItemAsync: jest.fn(async (key: string) => {
    mockKeychain.delete(key);
  }),
}));

jest.mock('@react-native-community/datetimepicker', () => ({
  __esModule: true,
  default: () => null,
  DateTimePickerAndroid: { open: jest.fn(), dismiss: jest.fn() },
}));

jest.mock('expo-crypto', () => {
  let next = 0;
  return { randomUUID: () => `00000000-0000-4000-8000-${String((next += 1)).padStart(12, '0')}` };
});

jest.mock('expo-sharing', () => ({
  isAvailableAsync: jest.fn(async () => true),
  shareAsync: jest.fn(async () => {}),
}));
const sharing = jest.mocked(Sharing);

// The client takes hold of `fetch` when it is made, so the stand-in for the
// service is in place before any of the app is loaded.
let service: FakeService = fakeService();
globalThis.fetch = ((...args: Parameters<typeof fetch>) => service.fetch(...args)) as typeof fetch;

const w = wordingFor('en');
const record = w.record;
const safety = w.safety;
const other = { name: PARTIES.B };
const fill = (message: string, values: Record<string, string>) =>
  message.replace(/\{(\w+)\}/g, (_, key: string) => values[key]);

/** Starts the app at an address, signed in or not. */
async function open(
  initialUrl: string,
  { signedIn, before }: { signedIn: boolean; before?: (service: FakeService) => void },
) {
  mockKeychain.clear();
  service = fakeService();
  if (signedIn) {
    mockKeychain.set('yuppers.session', TOKEN);
    service.account = ana;
  }
  before?.(service);
  const app = renderRouter('src/app', { initialUrl });
  await app;
  return { app };
}

const sentTo = (suffix: string) => service.sent.filter((request) => request.path.endsWith(suffix));
const calls = () => service.sent.map((request) => `${request.method} ${request.path}`);

type Rendered = null | string | { children?: Rendered[] | null } | Rendered[];

/** Every piece of text on screen, in the order it is laid out and read. */
function textsOf(node: Rendered): string[] {
  if (node === null) return [];
  if (typeof node === 'string') return [node];
  if (Array.isArray(node)) return node.flatMap(textsOf);
  return (node.children ?? []).flatMap(textsOf);
}

afterEach(() => forgetInvitation());

describe('history', () => {
  test('the exchange screen ends with what happened, in order, with what the parties wrote', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText(record.historyHeading);

    // What the reader did is said to them; what the other party did names them.
    await screen.findByText('You sent version 1 and, by sending it, signed it.');
    screen.getByText('Ben Ortiz opened the invitation and joined as the invited party.');
    screen.getByText('Ben Ortiz signed version 1.');
    // The message sent with the terms is labelled, and shown as it was written.
    screen.getByText(record.noteLabels.message);
    screen.getByText('Here is what we talked about on Tuesday.');

    const read = textsOf(screen.toJSON() as Rendered);
    const order = [
      'You sent version 1 and, by sending it, signed it.',
      'Ben Ortiz opened the invitation and joined as the invited party.',
      'You confirmed that the person who joined is the one you invited.',
      'Ben Ortiz signed version 1.',
      'Version 1, now signed by both parties, became the agreement.',
    ].map((sentence) => read.indexOf(sentence));
    expect(order).toEqual([...order].sort((a, b) => a - b));
    expect(order.every((index) => index > read.indexOf(record.historyHeading))).toBe(true);

    expect(sentTo('/history')).toEqual([
      expect.objectContaining({
        method: 'GET',
        path: `/v1/exchanges/${EXCHANGE}/history`,
        authorization: `Bearer ${TOKEN}`,
      }),
    ]);
  });

  test('each entry is one stop for a screen reader: when, what happened, what was written', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText('You sent version 1 and, by sending it, signed it.');

    const entries = screen.getAllByRole('listitem');
    expect(entries).toHaveLength(5);
    for (const entry of entries) expect(entry.props.accessible).toBe(true);
    // The first holds its time, its sentence, the label and the message, in that order.
    expect(textsOf(entries[0] as Rendered)).toEqual([
      expect.stringContaining('October 2, 2026'),
      'You sent version 1 and, by sending it, signed it.',
      record.noteLabels.message,
      'Here is what we talked about on Tuesday.',
    ]);
  });

  test('a note written on a delivery appears in the history once it is recorded', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText('Ben Ortiz signed version 1.');

    await fireEvent.press(screen.getByText(w.exchange.moves.CLAIM));
    await fireEvent.changeText(screen.getByLabelText(w.exchange.noteLabel), 'Done this morning');
    await fireEvent.press(screen.getAllByText(w.exchange.moves.CLAIM).at(-1)!);

    // The exchange is a newer version, so its history is read again.
    await screen.findByText('You marked this as delivered:');
    const entry = screen.getAllByRole('listitem').at(-1)!;
    expect(textsOf(entry as Rendered).slice(1)).toEqual([
      'You marked this as delivered:',
      'Repair the back fence',
      record.noteLabels.note,
      'Done this morning',
    ]);
    expect(sentTo('/history')).toHaveLength(2);
  });

  test('when the history cannot be read, the exchange is still there and the record still reachable', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (made) => {
        recordAndSafety(made).historyDown = true;
      },
    });
    await screen.findByText(w.errors.SERVICE_UNAVAILABLE);
    screen.getByText('Yup with Ben Ortiz');
    screen.getByText(record.open);
    screen.getByText(w.mobile.record.openHint);
  });
});

describe('the record', () => {
  async function openRecord() {
    const { app } = await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await fireEvent.press(await screen.findByText(record.open));
    await screen.findByText('Record PVVS-5Q2K');
    return { app };
  }

  test('it opens from the exchange and lays the whole record out in order', async () => {
    const { app } = await openRecord();
    expect(app.getPathname()).toBe(`/exchanges/${EXCHANGE}/record`);
    expect(sentTo('/record')).toEqual([
      expect.objectContaining({ method: 'GET', authorization: `Bearer ${TOKEN}` }),
    ]);

    // Its sections come in the order they are read in.
    const read = textsOf(screen.toJSON() as Rendered);
    const from = read.indexOf('Record PVVS-5Q2K');
    const sections = [
      record.summaryHeading,
      record.aboutHeading,
      record.itemsHeading,
      'Version 1',
      record.signaturesHeading,
      record.eventsHeading,
    ].map((heading) => read.indexOf(heading, from));
    expect(sections.every((index) => index > from)).toBe(true);
    expect(sections).toEqual([...sections].sort((a, b) => a - b));
    for (const heading of [record.summaryHeading, record.aboutHeading, record.eventsHeading]) {
      expect(screen.getByRole('header', { name: heading })).toBeTruthy();
    }

    // What the record is and is not, in the product's own words.
    screen.getByText(record.export.about);
    screen.getByText(record.export.signatures);
    screen.getByText(record.export.statements);
    screen.getByText(record.export.contentHash);
    screen.getByText('The agreement is version 1.');
  });

  test('times are in the exchange’s own zone, and everyone is named', async () => {
    await openRecord();
    screen.getByText('Times are shown in the America/Chicago time zone.');
    screen.getByText('This copy was made for Ana Ruiz on October 22, 2026 at 1:00:00 PM CDT.');
    screen.getByText('Started October 2, 2026 at 9:50:00 AM CDT');

    // The copy may be handed to someone who is neither party: nobody is "you".
    screen.getByText('Ana Ruiz sent version 1 and, by sending it, signed it.');
    expect(screen.queryByText('You sent version 1 and, by sending it, signed it.')).toBeNull();
    expect(screen.queryByText('Ana Ruiz (you)')).toBeNull();
  });

  test('a version shows what became of it, its terms, its fingerprint and what each signature rests on', async () => {
    await openRecord();
    screen.getByText(
      'Sent by Ana Ruiz on October 2, 2026 at 10:00:05 AM CDT. Open for signing until October 16, 2026 at 7:00:00 AM CDT.',
    );
    screen.getByText(record.versionStatus.IN_FORCE);
    screen.getByText('Repair the back fence.');
    // In the version's terms, and in the plain summary above them.
    expect(screen.getAllByText('Amount: $450.00')).toHaveLength(2);
    screen.getByText(`Fingerprint of these terms: ${'ab'.repeat(32)}`);

    // Each signature is one stop: who signed and when, then what it rests on.
    const signatures = screen
      .getAllByRole('listitem')
      .map((item) => textsOf(item as Rendered))
      .filter((lines) => lines[0].startsWith('Signed by'));
    expect(signatures).toEqual([
      [
        'Signed by Ana Ruiz on October 2, 2026 at 10:00:05 AM CDT.',
        `How the signer was verified: ${record.export.verification.EMAIL_OTP}`,
        'Code entered on October 2, 2026 at 9:45:00 AM CDT.',
        'Consent wording shown before signing: version draft-1, in language en.',
      ],
      [
        'Signed by Ben Ortiz on October 2, 2026 at 11:10:00 AM CDT.',
        `How the signer was verified: ${record.export.verification.PHONE_OTP}`,
        'Code entered on October 2, 2026 at 10:58:00 AM CDT.',
        'Consent wording shown before signing: version draft-1, in language es.',
      ],
    ]);
  });

  test('where each item stands is said as the parties’ own account', async () => {
    await openRecord();
    const items = screen
      .getAllByRole('listitem')
      .map((item) => textsOf(item as Rendered))
      .filter((lines) => lines.some((line) => line.startsWith('Provided by')));
    expect(items).toEqual([
      [
        'Repair the back fence',
        'Provided by Ana Ruiz',
        w.contributionStatus.PENDING,
        'Since October 2, 2026 at 11:10:00 AM CDT',
      ],
      // Money is paid outside the product, and is spoken of as paid, not delivered.
      [
        'Payment for the repair',
        'Provided by Ben Ortiz',
        w.moneyStatus.PENDING,
        'Since October 2, 2026 at 11:10:00 AM CDT',
      ],
    ]);
  });

  test('sharing hands the system one JSON file holding exactly the record', async () => {
    let handed = '';
    sharing.shareAsync.mockImplementationOnce(async (uri) => {
      handed = new File(uri).textSync();
    });
    await openRecord();
    screen.getByText(w.mobile.record.shareHint);

    await fireEvent.press(screen.getByRole('button', { name: w.mobile.record.share }));
    await waitFor(() => expect(sharing.shareAsync).toHaveBeenCalledTimes(1));

    const [uri, options] = sharing.shareAsync.mock.calls[0];
    expect(uri.endsWith('/record-copies/agreement-record-PVVS-5Q2K.json')).toBe(true);
    expect(options).toEqual({
      mimeType: 'application/json',
      UTI: 'public.json',
      dialogTitle: 'Record PVVS-5Q2K',
    });
    // The document as the service wrote it, with nothing of the app's added.
    expect(JSON.parse(handed)).toEqual(recordOf(service));
    expect(handed.endsWith('}\n')).toBe(true);
    // The session token is the app's and is in no copy.
    expect(handed).not.toContain(TOKEN);
    // Nothing more is asked of the service to make it.
    expect(sentTo('/record')).toHaveLength(1);
  });

  test('a device that cannot share says so, and a failure says to try again', async () => {
    await openRecord();
    sharing.isAvailableAsync.mockResolvedValueOnce(false);
    await fireEvent.press(screen.getByRole('button', { name: w.mobile.record.share }));
    await screen.findByText(w.mobile.record.shareUnavailable);

    sharing.shareAsync.mockRejectedValueOnce(new Error('no activity'));
    await fireEvent.press(screen.getByRole('button', { name: w.mobile.record.share }));
    await screen.findByText(w.mobile.record.shareFailed);
    expect(screen.queryByText(w.mobile.record.shareUnavailable)).toBeNull();
  });

  test('a record that cannot be read leaves a way back', async () => {
    await open('/exchanges/00000000-0000-4000-8000-00000000dead/record', { signedIn: true });
    await screen.findByText(w.errors.NOT_FOUND);
    screen.getByRole('button', { name: w.common.goHome });
  });

  test('the record is behind sign-in like the exchange itself', async () => {
    await open(`/exchanges/${EXCHANGE}/record`, { signedIn: false });
    await screen.findByText(w.signIn.intro);
    expect(sentTo('/record')).toEqual([]);
  });
});

describe('reporting an exchange', () => {
  async function openReport() {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText(safety.heading);
    await fireEvent.press(await screen.findByRole('button', { name: safety.report }));
    // Before anything is asked: that it is reviewed, and that the other party is not told.
    await screen.findByText(fill(safety.reportIntro, other));
  }

  test('a report needs a reason somebody picked, and "something else" needs saying what', async () => {
    await openReport();
    for (const reason of Object.values(safety.reasons)) {
      expect(screen.getByRole('radio', { name: reason }).props.accessibilityState).toMatchObject({
        checked: false,
      });
    }

    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));
    await screen.findByText(safety.reasonRequired);
    expect(sentTo('/reports')).toEqual([]);

    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.OTHER }));
    expect(screen.queryByText(safety.reasonRequired)).toBeNull();
    await screen.findByText(safety.detailsRequired);
    screen.getByLabelText(safety.detailsRequiredLabel);
    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));
    expect(sentTo('/reports')).toEqual([]);
  });

  test('the report is sent with its reason and details, and the reporter learns only that it arrived', async () => {
    await openReport();
    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.SCAM }));
    await fireEvent.changeText(
      screen.getByLabelText(safety.detailsLabel),
      '  Asked for the deposit in gift cards. ',
    );
    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));

    await screen.findByText(safety.reportSent);
    expect(sentTo('/reports')).toEqual([
      expect.objectContaining({
        method: 'POST',
        path: `/v1/exchanges/${EXCHANGE}/reports`,
        authorization: `Bearer ${TOKEN}`,
        body: { reason: 'SCAM', details: 'Asked for the deposit in gift cards.' },
      }),
    ]);
    // The form is gone, and the exchange itself is untouched.
    expect(screen.queryByText(safety.reasonLegend)).toBeNull();
    expect(sentTo('/commands')).toEqual([]);
  });

  test('past the day’s limit the refusal says so in its own words, and the form stays', async () => {
    await openReport();
    recordAndSafety(service).reportLimitReached = true;
    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.UNWANTED }));
    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));
    await screen.findByText(safety.tooManyReports);
    screen.getByText(safety.reasonLegend);
    expect(screen.queryByText(safety.reportSent)).toBeNull();
  });

  test('cancelling sends nothing', async () => {
    await openReport();
    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.HARASSMENT }));
    await fireEvent.press(screen.getByRole('button', { name: w.common.cancel }));
    expect(screen.queryByText(safety.reasonLegend)).toBeNull();
    expect(sentTo('/reports')).toEqual([]);
  });
});

describe('blocking from an exchange', () => {
  const blockLabel = fill(safety.block, other);
  const unblockLabel = fill(safety.unblock, other);

  test('everything a block does is said before it is done, and nothing is sent until then', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await fireEvent.press(await screen.findByRole('button', { name: blockLabel }));

    // Including that it ends offers still waiting to be signed, and that they are not told.
    const read = textsOf(screen.toJSON() as Rendered);
    const said = [
      fill(safety.blockStops, other),
      safety.blockEnds,
      safety.blockKeeps,
      // This exchange is an agreement in force: what the person blocked can
      // still do in it, and that it can be closed.
      fill(safety.blockInForce, other),
      fill(safety.blockThenClose, other),
      fill(safety.blockQuiet, other),
      fill(safety.confirmBlock, other),
    ].map((sentence) => read.indexOf(sentence));
    expect(said.every((index) => index >= 0)).toBe(true);
    expect(said).toEqual([...said].sort((a, b) => a - b));
    expect(calls().filter((call) => call.startsWith('PUT'))).toEqual([]);

    await fireEvent.press(screen.getByRole('button', { name: w.common.cancel }));
    expect(screen.queryByText(safety.blockEnds)).toBeNull();
    expect(calls().filter((call) => call.startsWith('PUT'))).toEqual([]);
  });

  test('confirming blocks, reads the exchange again, and offers to unblock', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await fireEvent.press(await screen.findByRole('button', { name: blockLabel }));
    await fireEvent.press(screen.getByRole('button', { name: fill(safety.confirmBlock, other) }));

    await screen.findByText(fill(safety.blocked, other));
    await screen.findByRole('button', { name: unblockLabel });
    expect(screen.queryByRole('button', { name: blockLabel })).toBeNull();
    expect(screen.queryByText(safety.blockEnds)).toBeNull();

    // A block withdraws or declines what was waiting, so the exchange is read again.
    await waitFor(() => {
      const after = calls().slice(calls().indexOf(`PUT /v1/exchanges/${EXCHANGE}/block`));
      expect(after).toContain(`GET /v1/exchanges/${EXCHANGE}`);
    });
    const put = service.sent.find((request) => request.method === 'PUT');
    expect(put).toMatchObject({ authorization: `Bearer ${TOKEN}`, body: null });
  });

  test('once blocked, an agreement in force can be asked to close right there', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await fireEvent.press(await screen.findByRole('button', { name: blockLabel }));
    // Nothing is offered before the block is made.
    expect(screen.queryByText(fill(safety.blockedInForce, other))).toBeNull();
    await fireEvent.press(screen.getByRole('button', { name: fill(safety.confirmBlock, other) }));

    await screen.findByText(fill(safety.blockedInForce, other));
    // The same request to close as under "Ending the agreement", opened here.
    const offers = screen.getAllByRole('button', { name: w.exchange.requestClose });
    await fireEvent.press(offers.at(-1)!);
    expect(
      screen.getAllByRole('button', { name: w.exchange.requestClose, expanded: true }),
    ).toHaveLength(1);
    expect(screen.getAllByText(fill(w.exchange.requestCloseText, other))).toHaveLength(1);
    expect(calls().filter((call) => call.endsWith('/commands'))).toEqual([]);

    await fireEvent.press(screen.getByRole('button', { name: w.exchange.sendCloseRequest }));
    await waitFor(() =>
      expect(sentTo('/commands').at(-1)).toMatchObject({
        body: { command: { type: 'REQUEST_CLOSE', note: null } },
      }),
    );
    // Asked once, it is not offered again.
    await waitFor(() =>
      expect(screen.queryByText(fill(safety.blockedInForce, other))).toBeNull(),
    );
  });

  test('a block from a proposal still being negotiated says nothing about an agreement', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (made) => {
        made.exchange = { ...made.exchange, state: 'NEGOTIATING' };
      },
    });
    await fireEvent.press(await screen.findByRole('button', { name: blockLabel }));
    screen.getByText(safety.blockEnds);
    expect(screen.queryByText(fill(safety.blockInForce, other))).toBeNull();
    expect(screen.queryByText(fill(safety.blockThenClose, other))).toBeNull();
    await fireEvent.press(screen.getByRole('button', { name: fill(safety.confirmBlock, other) }));
    await screen.findByText(fill(safety.blocked, other));
    expect(screen.queryByText(fill(safety.blockedInForce, other))).toBeNull();
  });

  test('someone already blocked is shown as blocked, and can be unblocked', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (made) => {
        recordAndSafety(made).blocked = true;
      },
    });
    await screen.findByText(fill(safety.blocked, other));
    expect(screen.queryByRole('button', { name: blockLabel })).toBeNull();

    await fireEvent.press(screen.getByRole('button', { name: unblockLabel }));
    await screen.findByText(fill(safety.unblocked, other));
    await screen.findByRole('button', { name: blockLabel });
    expect(calls()).toContain(`DELETE /v1/exchanges/${EXCHANGE}/block`);
    expect(recordAndSafety(service).blocked).toBe(false);
  });
});

describe('reporting a proposal from its invitation', () => {
  test('it is not offered signed out', async () => {
    await open(`/en/i#${INVITATION}`, { signedIn: false });
    await screen.findByText(w.invitation.signInToRead);
    expect(screen.queryByRole('button', { name: safety.reportProposal })).toBeNull();
  });

  test('signed in, the token goes in the body and nowhere else', async () => {
    const { app } = await open(`/en/i#${INVITATION}`, { signedIn: true });
    await screen.findByText(w.invitation.notBinding);

    await fireEvent.press(screen.getByRole('button', { name: safety.reportProposal }));
    await screen.findByText(safety.reportProposalIntro);
    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.UNWANTED }));
    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));

    await screen.findByText(safety.reportSent);
    expect(sentTo('/invitations/report')).toEqual([
      expect.objectContaining({
        method: 'POST',
        path: '/v1/invitations/report',
        authorization: `Bearer ${TOKEN}`,
        body: { token: INVITATION, reason: 'UNWANTED', details: null },
      }),
    ]);
    for (const request of service.sent) expect(request.path).not.toContain(INVITATION);
    expect(app.getPathnameWithParams()).toBe('/invitation');
    // There is no block here: a block is made through an exchange the two share.
    expect(screen.queryByText(fill(safety.block, other))).toBeNull();
    // The proposal is still there to respond to.
    screen.getByRole('button', { name: fill(w.invitation.respondAs, { name: ana.display_name }) });
  });

  test('a refusal stays in the form', async () => {
    await open(`/en/i#${INVITATION}`, {
      signedIn: true,
      before: (made) => {
        recordAndSafety(made).reportLimitReached = true;
      },
    });
    await fireEvent.press(await screen.findByRole('button', { name: safety.reportProposal }));
    await fireEvent.press(screen.getByRole('radio', { name: safety.reasons.SCAM }));
    await fireEvent.press(screen.getByRole('button', { name: safety.sendReport }));
    await screen.findByText(safety.tooManyReports);
    expect(screen.queryByText(safety.reportSent)).toBeNull();
  });
});

describe('the people blocked, on the account screen', () => {
  test('with nobody blocked it says so', async () => {
    await open('/account', { signedIn: true });
    await screen.findByText(safety.blockedHeading);
    await screen.findByText(safety.blockedEmpty);
    expect(sentTo('/v1/blocks')).toEqual([
      expect.objectContaining({ method: 'GET', authorization: `Bearer ${TOKEN}` }),
    ]);
  });

  test('each is named as their exchange names them, and can be unblocked', async () => {
    const { app } = await open('/account', {
      signedIn: true,
      before: (made) => {
        recordAndSafety(made).blocked = true;
      },
    });
    await screen.findByText(safety.blockedIntro);
    screen.getByText('Reference PVVS-5Q2K');
    screen.getByText(/^Blocked October 22, 2026/);
    // Their name leads to the exchange that names them.
    const link = screen.getByRole('link', { name: PARTIES.B });
    expect(link.props.accessibilityHint).toBe('Reference PVVS-5Q2K');

    await fireEvent.press(screen.getByRole('button', { name: fill(safety.unblock, other) }));
    await screen.findByText(fill(safety.unblocked, other));
    await screen.findByText(safety.blockedEmpty);
    expect(calls()).toContain(`DELETE /v1/exchanges/${EXCHANGE}/block`);

    expect(app.getPathname()).toBe('/account');
  });

  test('a name opens the exchange it comes from', async () => {
    const { app } = await open('/account', {
      signedIn: true,
      before: (made) => {
        recordAndSafety(made).blocked = true;
      },
    });
    await fireEvent.press(await screen.findByRole('link', { name: PARTIES.B }));
    await screen.findByText('Yup with Ben Ortiz');
    expect(app.getPathname()).toBe(`/exchanges/${EXCHANGE}`);
  });
});
