import type { ExchangeView as Exchange } from '@yuppers/api-client';
import { useWalletButton, type WalletApi, type WalletPlatform } from '@yuppers/shared';
import { Linking, Platform } from 'react-native';

import { useI18n } from '../lib/context';
import { api } from '../lib/session';
import { Actions, Button, Failure, Hint } from './ui';

/** The device's own wallet: Apple's on iOS, Google's on Android. */
function thisDevice(): WalletPlatform | null {
  if (Platform.OS === 'ios') return 'APPLE';
  if (Platform.OS === 'android') return 'GOOGLE';
  return null;
}

/*
 * How a pass gets into the wallet. On Android the "Save to Google Wallet"
 * link is opened, which Google Wallet takes, or the browser where it is not
 * installed. On iOS the link downloads the pass in Safari, which shows
 * Wallet's own sheet to add it: Apple's in-app sheet (PassKit's
 * PKAddPassesViewController) needs a native module this app does not have
 * yet, and handing the file to the share sheet does not reliably offer
 * Wallet (docs/wallet.md). The link works for a few minutes and needs no
 * session; Safari is told nothing else.
 */
function follow(url: string): Promise<void> {
  return Linking.openURL(url);
}

interface Props {
  exchange: Exchange;
  /** For tests: the service's Wallet calls, the device's wallet, and how a link is followed. */
  client?: WalletApi;
  device?: WalletPlatform | null;
  open?: (url: string) => Promise<void>;
}

/**
 * "Add to Apple Wallet" on iOS, "Add to Google Wallet" on Android, for an
 * agreement in force (DESIGN.md §11, §4.1), when the service issues passes
 * for that wallet; nothing otherwise. A plain button in the product's words:
 * the platforms' badge artwork comes under their brand terms
 * (docs/wallet.md).
 */
export function WalletButton({ exchange, client = api, device, open = follow }: Props) {
  const { wording, fmt } = useI18n();
  const wallet = useWalletButton(
    client,
    exchange,
    device === undefined ? thisDevice() : device,
    open,
  );
  if (wallet.platform === null) return null;
  const w = wording.wallet;
  const apple = wallet.platform === 'APPLE';
  return (
    <>
      <Hint>{fmt(w.intro, { productName: wording.productName })}</Hint>
      <Actions>
        <Button
          label={wallet.busy ? w.adding : apple ? w.addToApple : w.addToGoogle}
          disabled={wallet.busy}
          hint={apple ? wording.help.inBrowser : undefined}
          onPress={() => void wallet.add()}
        />
      </Actions>
      <Failure code={wallet.failure} />
    </>
  );
}
