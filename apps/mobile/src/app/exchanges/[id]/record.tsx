import { useLocalSearchParams } from 'expo-router';

import { Gate } from '../../../screens/AccountSetup';
import { RecordScreen } from '../../../screens/RecordScreen';

// The same address as on the web, `/exchanges/{id}/record`. Like the exchange
// itself, its record is only for its two parties, so it is behind sign-in.
export default function ExchangeRecord() {
  const { id } = useLocalSearchParams<{ id: string }>();
  return (
    <Gate>
      <RecordScreen key={id} id={id} />
    </Gate>
  );
}
