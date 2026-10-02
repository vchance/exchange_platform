import { useLocalSearchParams } from 'expo-router';

import { Gate } from '../../../screens/AccountSetup';
import { ReviseScreen } from '../../../screens/ExchangeScreen';

export default function Revise() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return (
    <Gate>
      <ReviseScreen key={id} id={id} />
    </Gate>
  );
}
