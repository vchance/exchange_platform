import { redirectSystemPath } from '../../app/+native-intent';
import {
  forgetInvitation,
  heldInvitation,
  holdInvitation,
  INVITATION_ROUTE,
  routeForIncomingLink,
} from '../invitation';

const token = 'a3'.repeat(32);

afterEach(() => forgetInvitation());

test('an invitation link gives up its token before the router sees it', () => {
  for (const link of [
    `exchange://es/i#${token}`,
    `exchange:///en/i#${token}`,
    `https://app.example/pt-BR/i#${token}`,
    `/en/i#${token}`,
  ]) {
    forgetInvitation();
    const route = routeForIncomingLink(link);
    expect(route).toBe(INVITATION_ROUTE);
    // The token is not in the route, so it is in no address, history or log of one.
    expect(route).not.toContain(token);
    expect(heldInvitation()).toBe(token);
  }
});

test('any other link is left as it came and holds nothing', () => {
  for (const link of [
    'exchange://',
    'exchange://account',
    'exchange://exchanges/0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70',
    'https://app.example/exchanges/0b9f1c2e-7a41-4c6e-9a55-3d2f8e1b6c70',
    // A fragment on some other address is not an invitation.
    `exchange://account#${token}`,
    'exchange://en/i#short',
    'exchange://en/i',
  ]) {
    expect(routeForIncomingLink(link)).toBe(link);
    expect(heldInvitation()).toBeNull();
  }
});

test('the router’s hook for incoming links does the same, and never throws', () => {
  expect(redirectSystemPath({ path: `exchange://en/i#${token}`, initial: true })).toBe(
    INVITATION_ROUTE,
  );
  expect(heldInvitation()).toBe(token);
  expect(redirectSystemPath({ path: 'exchange://account', initial: false })).toBe(
    'exchange://account',
  );
  expect(redirectSystemPath({ path: undefined as unknown as string, initial: true })).toBe(
    undefined,
  );
});

test('the token is held in memory until it is spent, and then forgotten', () => {
  holdInvitation(token);
  expect(heldInvitation()).toBe(token);
  forgetInvitation();
  expect(heldInvitation()).toBeNull();
});
