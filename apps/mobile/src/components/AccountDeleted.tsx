import { deletedNotice, useDeletedNotice } from '@yuppers/shared';
import { useEffect } from 'react';
import { StyleSheet, View } from 'react-native';
import { useSafeAreaInsets } from 'react-native-safe-area-context';

import { useI18n, useSession } from '../lib/context';
import { space, useColors } from '../lib/theme';
import { Actions, Button, Notice } from './ui';

/**
 * Says, once, that the account was deleted. The screen that did it is gone
 * by then and nobody is signed in, so it is said on the first screen, above
 * the way to sign in, until it is dismissed or someone signs in.
 */
export function AccountDeleted() {
  const { wording } = useI18n();
  const { account } = useSession();
  const colors = useColors();
  const insets = useSafeAreaInsets();
  const shown = useDeletedNotice();
  const signedIn = account !== null;

  useEffect(() => {
    if (signedIn) deletedNotice.dismiss();
  }, [signedIn]);

  if (!shown || signedIn) return null;
  return (
    <View
      style={[
        styles.banner,
        {
          backgroundColor: colors.background,
          paddingStart: space.l + insets.left,
          paddingEnd: space.l + insets.right,
        },
      ]}>
      <Notice>{wording.deletion.deleted}</Notice>
      <Actions>
        <Button label={wording.deletion.dismiss} onPress={deletedNotice.dismiss} />
      </Actions>
    </View>
  );
}

const styles = StyleSheet.create({
  banner: { paddingTop: space.l, gap: space.m },
});
