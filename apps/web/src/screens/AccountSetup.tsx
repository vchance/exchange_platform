import { isComplete, useI18n, useSession } from '../app/context'
import { PageHeading, StepHeading } from '../components/ui'
import { ProfileForm } from './ProfileForm'
import { SignIn } from './SignIn'

/**
 * Everything between "not signed in" and "ready to act": signing in with a
 * one-time code, then, for a new account, the profile. It shows whichever
 * step is next and nothing once both are done, so it can stand in for any
 * page that needs an account.
 */
export default function AccountSetup({ headingLevel = 'h1' }: { headingLevel?: 'h1' | 'h2' }) {
  const { wording } = useI18n()
  const { account } = useSession()
  // Below the invitation, the step opens where the button that asked for it
  // was, and the keyboard is taken to it.
  const heading = (text: string) =>
    headingLevel === 'h1' ? (
      <PageHeading key={text}>{text}</PageHeading>
    ) : (
      <StepHeading key={text}>{text}</StepHeading>
    )

  if (!account) {
    return (
      <section>
        {heading(wording.signIn.title)}
        <SignIn />
      </section>
    )
  }
  if (!isComplete(account)) {
    return (
      <section>
        {heading(wording.profile.firstTitle)}
        <p>{wording.profile.firstIntro}</p>
        <ProfileForm account={account} first />
      </section>
    )
  }
  return null
}
