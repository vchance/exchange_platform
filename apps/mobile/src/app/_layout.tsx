import { directionOf } from '@exchange/shared';
import { DarkTheme, DefaultTheme, LocaleProvider, Stack, ThemeProvider } from 'expo-router';
import { StatusBar } from 'expo-status-bar';
import { StyleSheet, View } from 'react-native';
import { SafeAreaProvider } from 'react-native-safe-area-context';

import { AppProviders, useI18n } from '../lib/context';
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

const styles = StyleSheet.create({
  fill: { flex: 1 },
});
