import { useRouter } from 'expo-router';
import { useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

import { Actions, Button, Heading, Label, Screen } from '../components/ui';
import { useI18n, useSession } from '../lib/context';
import { space, type, useColors } from '../lib/theme';
import { ProfileForm } from './AccountSetup';
import { BlockedPeople } from './BlockedPeople';

/** The account: what it is verified with, its name and language, and signing out. */
export function AccountScreen() {
  const { wording } = useI18n();
  const { account, signOut } = useSession();
  const colors = useColors();
  const router = useRouter();
  const w = wording.profile;
  const [leaving, setLeaving] = useState(false);

  if (!account) return null;

  async function leave() {
    setLeaving(true);
    // Back to the first screen before the account goes, so what is left on
    // screen is the way to sign in again.
    router.dismissTo('/');
    await signOut();
  }

  return (
    <Screen>
      <Heading>{w.title}</Heading>
      {account.email ? (
        <View style={styles.detail}>
          <Label>{w.emailLabel}</Label>
          <Text selectable style={[type.body, { color: colors.text }]}>
            {account.email}
          </Text>
        </View>
      ) : null}
      {account.phone ? (
        <View style={styles.detail}>
          <Label>{w.phoneLabel}</Label>
          {/* A phone number reads left to right in every language. */}
          <Text selectable style={[type.body, styles.ltr, { color: colors.text }]}>
            {account.phone}
          </Text>
        </View>
      ) : null}
      <ProfileForm account={account} first={false} />
      <BlockedPeople />
      <View style={[styles.rule, { backgroundColor: colors.border }]} />
      <Actions>
        <Button label={wording.nav.signOut} disabled={leaving} onPress={() => void leave()} />
      </Actions>
    </Screen>
  );
}

const styles = StyleSheet.create({
  detail: { gap: space.xs },
  ltr: { writingDirection: 'ltr' },
  rule: { height: StyleSheet.hairlineWidth },
});
