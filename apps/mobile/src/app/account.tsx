import { Gate } from '../screens/AccountSetup';
import { AccountScreen } from '../screens/AccountScreen';

export default function Account() {
  return (
    <Gate>
      <AccountScreen />
    </Gate>
  );
}
