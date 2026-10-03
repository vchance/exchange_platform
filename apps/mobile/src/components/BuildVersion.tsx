import { versionText, type BuildIdentity } from '@yuppers/shared';
import { StyleSheet, Text } from 'react-native';

import { useI18n } from '../lib/context';
import { type, useColors } from '../lib/theme';

/**
 * Which build of the app this is, small, at the foot of the account screen:
 * "Version 0.1.0 (build 12, abc1234)". What a person reads out when
 * something goes wrong. Nothing when the build cannot read its own version.
 */
export function BuildVersion({ build }: { build: BuildIdentity | null }) {
  const { wording, language } = useI18n();
  const colors = useColors();
  if (!build) return null;
  return (
    <Text testID="build-version" selectable style={[type.hint, styles.ltr, { color: colors.muted }]}>
      {versionText(wording, language, build)}
    </Text>
  );
}

const styles = StyleSheet.create({
  // Version numbers and a commit read left to right in every language.
  ltr: { writingDirection: 'ltr' },
});
