import { InvitationScreen } from '../screens/InvitationScreen';

// Open signed out too, so a link that arrives before signing in is not lost:
// the screen asks to sign in, the same for every link, and shows the
// proposal after (DESIGN.md §8, §9).
export default function Invitation() {
  return <InvitationScreen />;
}
