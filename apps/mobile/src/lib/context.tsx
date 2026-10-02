import type { Account, ErrorCode } from '@exchange/api-client';
import {
  createI18n,
  failureCode,
  pickLanguage,
  wordingFor,
  type I18n,
  type Language,
  type SessionCreated,
} from '@exchange/shared';
import { useLocales } from 'expo-localization';
import {
  createContext,
  useCallback,
  useContext,
  useEffect,
  useMemo,
  useState,
  type ReactNode,
} from 'react';

import { api, dropSession, keepSession, restoreSession } from './session';

export const I18nContext = createContext<I18n | null>(null);

export function useI18n(): I18n {
  const value = useContext(I18nContext);
  if (!value) throw new Error('useI18n outside the app');
  return value;
}

export interface Session {
  /** `false` until the service has said whether anyone is signed in. */
  ready: boolean;
  /** Set when the service could not be asked. */
  failure: ErrorCode | null;
  account: Account | null;
  setAccount(account: Account): void;
  /** Takes up a session the service just created, keeping its token in secure storage. */
  signedIn(created: SessionCreated): Promise<void>;
  signOut(): Promise<void>;
  /**
   * Stops acting as the account on this device without asking the service:
   * for when the account has just been deleted, and its sessions with it.
   */
  forget(): Promise<void>;
  retry(): void;
}

export const SessionContext = createContext<Session | null>(null);

export function useSession(): Session {
  const value = useContext(SessionContext);
  if (!value) throw new Error('useSession outside the app');
  return value;
}

/**
 * Who is signed in and which language the app speaks.
 *
 * The language is the device's before sign-in and the account's after
 * (DESIGN.md §4.2). The device takes up the language of whoever signs in, so
 * signing out leaves the screen in the language it was in.
 */
export function AppProviders({ children }: { children: ReactNode }) {
  const [account, setAccountState] = useState<Account | null>(null);
  const [ready, setReady] = useState(false);
  const [failure, setFailure] = useState<ErrorCode | null>(null);
  const [attempt, setAttempt] = useState(0);

  // The device's own preference, followed if it changes while the app is open,
  // until someone picks a language here.
  const locales = useLocales();
  const deviceLanguage = pickLanguage(locales.map((locale) => locale.languageTag));
  const [chosen, setChosen] = useState<Language | null>(null);

  const setAccount = useCallback((next: Account) => {
    setAccountState(next);
    setChosen(pickLanguage([next.language]));
  }, []);

  useEffect(() => {
    let cancelled = false;
    restoreSession()
      .then(() => api.me())
      .then(
        (found) => {
          if (cancelled) return;
          if (found) setAccount(found);
          else {
            // A token the service no longer honors is not worth keeping.
            void dropSession();
            setAccountState(null);
          }
          setFailure(null);
          setReady(true);
        },
        (error: unknown) => {
          if (cancelled) return;
          setFailure(failureCode(error));
          setReady(true);
        },
      );
    return () => {
      cancelled = true;
    };
  }, [attempt, setAccount]);

  // A session can end at any time: it expires, or the account is suspended.
  useEffect(() => {
    api.onSignedOut(() => {
      void dropSession();
      setAccountState(null);
    });
    return () => api.onSignedOut(() => {});
  }, []);

  const language = account ? pickLanguage([account.language]) : (chosen ?? deviceLanguage);
  const signedInNow = account !== null;

  const setLanguage = useCallback(
    (next: Language) => {
      setChosen(next);
      if (signedInNow) {
        // The preference belongs to the account and follows it to other devices.
        api.updateMe({ language: next }).then(setAccount, () => {});
      }
    },
    [signedInNow, setAccount],
  );

  const i18n = useMemo(
    () => createI18n(language, wordingFor(language), setLanguage),
    [language, setLanguage],
  );

  const session = useMemo<Session>(
    () => ({
      ready,
      failure,
      account,
      setAccount,
      async signedIn(created) {
        // The service answers a token session with its token, once.
        if (!created.token) throw new Error('the service sent no session token');
        await keepSession(created.token);
        setAccount(created.account);
      },
      async signOut() {
        try {
          await api.signOut();
        } catch {
          // Whether or not the service heard, this device stops acting as the
          // account: a refusal means the session had already ended.
        }
        await dropSession();
        setAccountState(null);
      },
      async forget() {
        await dropSession();
        setAccountState(null);
      },
      retry() {
        setReady(false);
        setAttempt((count) => count + 1);
      },
    }),
    [ready, failure, account, setAccount],
  );

  return (
    <I18nContext value={i18n}>
      <SessionContext value={session}>{children}</SessionContext>
    </I18nContext>
  );
}
