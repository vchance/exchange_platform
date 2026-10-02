import { useLocalSearchParams } from 'expo-router';

import { Gate } from '../../../screens/AccountSetup';
import { ExchangeScreen } from '../../../screens/ExchangeScreen';

// The same address as on the web, `/exchanges/{id}`, which is where a
// notification's link will lead once links open the app.
export default function Exchange() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return (
    <Gate>
      <ExchangeScreen key={id} id={id} />
    </Gate>
  );
}
