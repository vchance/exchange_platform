import { InvitationScreen } from '../screens/InvitationScreen';

// Not behind sign-in: a proposal can be read by anyone holding its link
// (DESIGN.md §8). Responding to it is what needs an account.
export default function Invitation() {
  return <InvitationScreen />;
}
