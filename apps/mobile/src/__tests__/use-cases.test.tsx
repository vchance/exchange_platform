import { wordingFor, type RecordDocument } from '@yuppers/shared';
import { fireEvent, renderRouter, screen, waitFor } from 'expo-router/testing-library';
import * as Print from 'expo-print';
import * as Sharing from 'expo-sharing';
import { File, Paths } from 'expo-file-system';

import { forgetInvitation } from '../lib/invitation';
import { recordAndSafety, recordOf } from './fake-record';
import {
  EXCHANGE,
  PAYMENT,
  REPAIR,
  signInOnScreen,
  TOKEN,
  ana,
  fakeService,
  type FakeService,
} from './fake-service';

/*
 * What the use cases asked for, in the whole app as iOS and as Android builds
 * would run it: the plain summary at the top of the record and its PDF, the
 * guide for when something isn't working, a dispute that says it is recorded
 * and not decided, what a proposal changes for the person asked to sign it,
 * and the way to an invitation before signing in.
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
jest.mock('expo-print', () => ({ printToFileAsync: jest.fn() }));
const sharing = jest.mocked(Sharing);
const print = jest.mocked(Print);

let service: FakeService = fakeService();
globalThis.fetch = ((...args: Parameters<typeof fetch>) => service.fetch(...args)) as typeof fetch;

const w = wordingFor('en');

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
  await renderRouter('src/app', { initialUrl });
}

afterEach(() => forgetInvitation());

describe('the record: a plain summary first, and a PDF of it all', () => {
  test('the summary says who, what each gives, who signed, and where each item stands', async () => {
    await open(`/exchanges/${EXCHANGE}/record`, {
      signedIn: true,
      before: (fake) => {
        fake.exchange = {
          ...fake.exchange,
          contributions: [
            { id: REPAIR, status: 'ACCEPTED' },
            { id: PAYMENT, status: 'PENDING' },
          ],
        };
      },
    });
    await screen.findByText(w.record.summary.heading);
    screen.getByText('Between Ana Ruiz and Ben Ortiz.');
    screen.getByText('What Ana Ruiz agreed to give');
    screen.getByText('Delivered, and Ben Ortiz confirmed receiving it.');
    screen.getByText('Not paid yet.');
    // In the summary, and again in the version's full terms.
    expect(screen.getAllByText('Amount: $450.00')).toHaveLength(2);
    screen.getByText(w.record.summary.standing.ACTIVE);
    // The full detail is still there, under it.
    screen.getByText(w.record.summaryHeading);
    screen.getByText(w.record.eventsHeading);
  });

  test('“Save as PDF” prints the record, summary first, and hands the PDF to the share sheet', async () => {
    print.printToFileAsync.mockImplementation(async ({ html } = {}) => {
      const printed = new File(Paths.cache, 'Print-1234.pdf');
      printed.create({ overwrite: true });
      printed.write(html ?? '');
      return { uri: printed.uri, numberOfPages: 1 };
    });
    let handed = '';
    sharing.shareAsync.mockImplementation(async (uri) => {
      handed = new File(uri).textSync();
    });
    await open(`/exchanges/${EXCHANGE}/record`, { signedIn: true });
    await screen.findByText(w.record.summary.heading);

    await fireEvent.press(screen.getByRole('button', { name: w.mobile.record.savePdf }));
    await waitFor(() => expect(sharing.shareAsync).toHaveBeenCalled());
    const [uri, options] = sharing.shareAsync.mock.calls.at(-1)!;
    expect(uri).toBe(`${Paths.cache.uri}record-copies/agreement-record-PVVS-5Q2K.pdf`);
    expect(options).toMatchObject({ mimeType: 'application/pdf', UTI: 'com.adobe.pdf' });
    // The summary comes before the full record.
    expect(handed.indexOf(w.record.summary.heading)).toBeGreaterThan(0);
    expect(handed.indexOf(w.record.summary.heading)).toBeLessThan(handed.indexOf(w.record.eventsHeading));
  });

  test('a PDF that cannot be made says so', async () => {
    print.printToFileAsync.mockRejectedValue(new Error('no'));
    await open(`/exchanges/${EXCHANGE}/record`, { signedIn: true });
    await screen.findByText(w.record.summary.heading);
    await fireEvent.press(screen.getByRole('button', { name: w.mobile.record.savePdf }));
    await screen.findByText(w.mobile.record.pdfFailed);
  });
});

describe('“Something isn’t working”', () => {
  test('asks what the situation is, then offers the ways that fit, each opening its own panel', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText('Yup with Ben Ortiz');
    const t = w.trouble;

    await fireEvent.press(screen.getByRole('button', { name: t.open }));
    expect(screen.getByRole('button', { name: t.open, expanded: true })).toBeTruthy();
    screen.getByText(t.question);
    await fireEvent.press(screen.getByRole('button', { name: 'Ben Ortiz hasn’t done their part' }));
    screen.getByText(t.explain.THEY_HAVENT);
    screen.getByText('Waiving releases Ben Ortiz from that one item, for good. Everything else stays as agreed.');

    // Ending together opens the ending's own panel, which still asks to confirm.
    // The guide's button comes first; the ending section below has its own.
    const [fromGuide] = screen.getAllByRole('button', { name: w.exchange.proposeEnd });
    await fireEvent.press(fromGuide);
    await screen.findByText(w.exchange.sendEndProposal);
    expect(screen.queryByText(t.question)).toBeNull();
    expect(service.sent.filter((request) => request.path.endsWith('/commands'))).toEqual([]);
  });

  test('an item’s action says which item it acts on, and opens that item’s panel', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText('Yup with Ben Ortiz');
    await fireEvent.press(screen.getByRole('button', { name: w.trouble.open }));
    await fireEvent.press(screen.getByRole('button', { name: 'Ben Ortiz hasn’t done their part' }));
    const waive = screen
      .getAllByRole('button', { name: w.exchange.moneyMoves.WAIVE })
      .find((button) => button.props.accessibilityHint === 'Payment for the repair')!;
    await fireEvent.press(waive);
    await screen.findByText(w.exchange.moneyMoveText.WAIVE.replace('{name}', 'Ben Ortiz'));
  });

  test('back to the question, and cancelled', async () => {
    await open(`/exchanges/${EXCHANGE}`, { signedIn: true });
    await screen.findByText('Yup with Ben Ortiz');
    await fireEvent.press(screen.getByRole('button', { name: w.trouble.open }));
    await fireEvent.press(screen.getByRole('button', { name: w.trouble.situations.BOTH_STOP }));
    screen.getByText(w.trouble.explain.BOTH_STOP);
    await fireEvent.press(screen.getByRole('link', { name: w.trouble.change }));
    screen.getByText(w.trouble.question);
    await fireEvent.press(screen.getByRole('button', { name: w.common.cancel }));
    expect(screen.queryByText(w.trouble.question)).toBeNull();
  });
});

describe('a dispute is recorded, not decided', () => {
  test('a disputed item says so, and leads to the guide at the disagreement', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (fake) => {
        fake.exchange = {
          ...fake.exchange,
          contributions: [
            { id: REPAIR, status: 'DISPUTED' },
            { id: PAYMENT, status: 'PENDING' },
          ],
        };
      },
    });
    await screen.findByText(w.dispute.weRecord);
    screen.getByText(w.dispute.pointer);
    await fireEvent.press(screen.getByRole('link', { name: w.trouble.open }));
    await screen.findByText(w.trouble.explain.DISAGREE);
  });

  test('opening a dispute says it too', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (fake) => {
        // Ben has said the payment is made; Ana disagrees.
        fake.exchange = {
          ...fake.exchange,
          contributions: [
            { id: REPAIR, status: 'PENDING' },
            { id: PAYMENT, status: 'CLAIMED' },
          ],
        };
      },
    });
    await screen.findByText('Yup with Ben Ortiz');
    await fireEvent.press(screen.getByRole('button', { name: w.exchange.moneyMoves.DISPUTE }));
    await screen.findByText(w.dispute.weRecord);
    screen.getByText(w.dispute.pointer);
  });
});

describe('what a proposal changes, for the person asked to sign it', () => {
  test('an amendment, against the agreement in force, with where each item would stand', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (fake) => {
        const inForce = fake.exchange.in_force_revision!;
        fake.exchange = {
          ...fake.exchange,
          contributions: [
            { id: REPAIR, status: 'CLAIMED' },
            { id: PAYMENT, status: 'PENDING' },
          ],
          open_revision: {
            ...inForce,
            id: 'c0000000-0000-4000-8000-000000000002',
            sequence: 2,
            author: 'B',
            accepted_by: ['B'],
            terms: {
              ...inForce.terms,
              contributions: [
                inForce.terms.contributions[0],
                { ...inForce.terms.contributions[1], amount_minor: 50000 },
              ],
            },
          },
        };
      },
    });
    await screen.findByText(w.proposalChanges.heading);
    screen.getByText('Compared with the agreement in force, version 1.');
    screen.getByText('Amount: was $450.00, now $500.00.');
    screen.getByText(w.composer.effects.CHANGED);
    screen.getByText(`Once you both sign: ${w.contributionStatus.CLAIMED}`);
    screen.getByText(`Once you both sign: ${w.moneyStatus.PENDING}`);
  });

  test('a counteroffer, against the version it answers', async () => {
    await open(`/exchanges/${EXCHANGE}`, {
      signedIn: true,
      before: (fake) => {
        const first = recordOf(fake);
        const inForce = fake.exchange.in_force_revision!;
        const answer = {
          ...inForce,
          id: 'c0000000-0000-4000-8000-000000000002',
          sequence: 2,
          author: 'B' as const,
          accepted_by: ['B' as const],
          terms: {
            ...inForce.terms,
            contributions: [
              { ...inForce.terms.contributions[0], description: 'Repair the back fence and gate' },
              inForce.terms.contributions[1],
            ],
          },
        };
        fake.exchange = {
          ...fake.exchange,
          state: 'NEGOTIATING',
          in_force_revision: null,
          open_revision: answer,
          contributions: [],
        };
        const [v1] = first.revisions;
        const record: RecordDocument = {
          ...first,
          revisions: [
            { ...v1, standing: { status: 'SUPERSEDED', since: v1.standing.since } },
            {
              ...v1,
              id: answer.id,
              sequence: 2,
              author: 'B',
              answers: { id: v1.id, sequence: 1 },
              standing: { status: 'OPEN', since: v1.standing.since },
              signed: {
                ...v1.signed,
                contributions: [
                  { ...v1.signed.contributions[0], description: 'Repair the back fence and gate' },
                  v1.signed.contributions[1],
                ],
              },
            },
          ],
        };
        recordAndSafety(fake).record = record;
      },
    });
    await screen.findByText(w.proposalChanges.heading);
    screen.getByText('Compared with version 1, the one this answers.');
    screen.getByText(w.proposalChanges.kinds.CHANGED);
    // In the terms; then in what changed, as the item and as its new description.
    expect(screen.getAllByText('Repair the back fence and gate')).toHaveLength(3);
    screen.getByText(w.proposalChanges.fields.description);
    screen.getByText('Repair the back fence');
    screen.getByText('1 other item is unchanged.');
  });
});

describe('an invitation, before signing in', () => {
  test('the first screen offers it, and it leads to signing in, then the proposal', async () => {
    await open('/', { signedIn: false });
    await screen.findByText(w.mobile.invited.heading);
    screen.getByText(w.mobile.invited.introSignIn);
    await fireEvent.press(screen.getByRole('button', { name: w.mobile.openInvitation.title }));
    await screen.findByText(w.mobile.openInvitation.intro);
    await fireEvent.changeText(
      screen.getByLabelText(w.mobile.openInvitation.label),
      `https://app.example/en/i#${'a3'.repeat(32)}`,
    );
    await fireEvent.press(screen.getByText(w.mobile.openInvitation.open));
    await screen.findByText(w.invitation.signInToRead);
    // Nothing about the link went anywhere before signing in.
    expect(service.sent.some((request) => request.path.startsWith('/v1/invitations/'))).toBe(false);
    await signInOnScreen(w);
    await screen.findByText(w.invitation.notBinding);
    expect(
      service.sent.filter((request) => request.path.startsWith('/v1/invitations/')),
    ).toEqual([expect.objectContaining({ authorization: `Bearer ${TOKEN}` })]);
  });

  test('not offered once signed in, where the list has its own', async () => {
    await open('/', { signedIn: true });
    await screen.findByText(w.home.title);
    expect(screen.queryByText(w.mobile.invited.heading)).toBeNull();
    expect(screen.getByRole('button', { name: w.mobile.openInvitation.title })).toBeTruthy();
  });
});
