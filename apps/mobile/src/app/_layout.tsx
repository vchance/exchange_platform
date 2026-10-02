import { directionOf } from '@exchange/shared';
import { DarkTheme, DefaultTheme, LocaleProvider, Stack, ThemeProvider } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { Actions, Button, Heading, Screen } from '../components/ui';
import { AppProviders, useI18n, useSession } from '../lib/context';
import { installPluralRules } from '../lib/plural-rules';
import { colorsFor, useScheme } from '../lib/theme';

// Before any wording is formatted: the engine lacks the plural rules it needs.
installPluralRules();

export default function RootLayout() {
  return (
    <SafeAreaProvider>
      <AppProviders>
        <Navigation />
      </AppProviders>
    </SafeAreaProvider>
  );
}

/**
 * One stack of screens. Titles come from the wording; colors follow the
 * system's light or dark setting; the layout runs in the direction of the
 * language being shown (DESIGN.md §4.2).
 */
function Navigation() {
  const { wording, language } = useI18n();
  const { outdated } = useSession();
  const scheme = useScheme();
  const colors = colorsFor(scheme);
  const direction = directionOf(language);
  const base = scheme === 'dark' ? DarkTheme : DefaultTheme;

  return (
    <ThemeProvider
      value={{
        ...base,
        colors: {
          ...base.colors,
          background: colors.background,
          card: colors.surface,
          text: colors.text,
          border: colors.border,
          primary: colors.primary,
        },
      }}>
      <LocaleProvider direction={direction}>
        <View style={[styles.fill, { direction, backgroundColor: colors.background }]}>
          {/* A build too old to act shows that it must be updated, and nothing else. */}
          {outdated && <Outdated />}
          <Stack
            screenOptions={{
              title: wording.productName,
              headerBackTitle: wording.mobile.back,
              headerTintColor: colors.primary,
              headerTitleStyle: { color: colors.text },
              headerStyle: { backgroundColor: colors.surface },
              contentStyle: { backgroundColor: colors.background },
            }}>
            <Stack.Screen name="index" />
            <Stack.Screen name="account" options={{ title: wording.nav.account }} />
            <Stack.Screen name="invitation" />
            <Stack.Screen
              name="exchanges/[id]/index"
              options={{ title: wording.exchange.titleNoName }}
            />
            <Stack.Screen
              name="exchanges/[id]/revise"
              options={{ title: wording.exchange.titleNoName }}
            />
            <Stack.Screen
              name="exchanges/[id]/record"
              options={{ title: wording.exchange.titleNoName }}
            />
            <Stack.Screen name="[language]/i" options={{ headerShown: false }} />
            <Stack.Screen name="+not-found" options={{ title: wording.common.notFoundTitle }} />
          </Stack>
        </View>
        <StatusBar style="auto" />
      </LocaleProvider>
    </ThemeProvider>
  );
}

/**
 * This build is older than the service accepts changes from. It covers the
 * screens, since nothing in them may offer a change it cannot make, and says
 * the one thing to do. Trying again asks the service once more, for a build
 * updated while the app was open.
 */
function Outdated() {
  const { wording } = useI18n();
  const { retry } = useSession();
  const colors = colorsFor(useScheme());
  return (
    <View style={[styles.cover, { backgroundColor: colors.background }]}>
      <Screen>
        <Heading>{wording.errors.CLIENT_TOO_OLD}</Heading>
        <Actions>
          <Button label={wording.common.tryAgain} onPress={retry} />
        </Actions>
      </Screen>
    </View>
  );
}

const styles = StyleSheet.create({
  fill: { flex: 1 },
  cover: { position: 'absolute', top: 0, bottom: 0, left: 0, right: 0, zIndex: 1 },
});
