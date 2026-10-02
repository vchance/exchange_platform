import { createApiClient } from '@exchange/api-client';
import { pickLanguage, wordingFor } from '@exchange/shared';
import { getLocales } from 'expo-localization';
import { StatusBar } from 'expo-status-bar';
import { useEffect, useState } from 'react';
import { StyleSheet, Text, View } from 'react-native';

// A device cannot reach the development machine as "localhost": set
// EXPO_PUBLIC_API_URL to the machine's LAN address when running on hardware.
const api = createApiClient(process.env.EXPO_PUBLIC_API_URL ?? 'http://localhost:8080');

type ServiceState = 'checking' | 'connected' | 'unreachable';

export default function App() {
  const language = pickLanguage(getLocales().map((locale) => locale.languageTag));
  const wording = wordingFor(language);
  const [service, setService] = useState<ServiceState>('checking');

  useEffect(() => {
    let cancelled = false;
    api
      .GET('/v1/meta')
      .then(({ data }) => {
        if (!cancelled) setService(data ? 'connected' : 'unreachable');
      })
      .catch(() => {
        if (!cancelled) setService('unreachable');
      });
    return () => {
      cancelled = true;
    };
  }, []);

  return (
    <View style={styles.container}>
      <Text style={styles.title}>{wording.productName}</Text>
      <Text style={styles.text}>{wording.tagline}</Text>
      <Text style={styles.text}>{wording.service[service]}</Text>
      <StatusBar style="auto" />
    </View>
  );
}

const styles = StyleSheet.create({
  container: {
    flex: 1,
    backgroundColor: '#fff',
    alignItems: 'center',
    justifyContent: 'center',
    padding: 24,
  },
  title: {
    fontSize: 28,
    fontWeight: '600',
    marginBottom: 8,
  },
  text: {
    fontSize: 16,
    textAlign: 'center',
    marginBottom: 8,
  },
});
