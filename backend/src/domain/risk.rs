//! Risk tiers: how strongly a party must be verified before signing
//! (DESIGN.md §8). Noncash contributions carry no value, so the tier comes
//! from the money in the agreement and from either party asking for more.

use time::{Duration, OffsetDateTime};

use super::revision::{Kind, Revision};

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Tier {
    /// A one-time code to a verified email address or phone number.
    Zero,
    /// A pre-bound invitation, both email and phone verified, and a fresh
    /// one-time code at acceptance.
    One,
}

/// The tier a revision calls for. `opted_up` is set once either party has
/// switched on stronger verification; nobody can switch it back off.
pub fn required_tier(revision: &Revision, opted_up: bool, threshold_minor: i64) -> Tier {
    let money: i128 = revision
        .contributions
        .iter()
        .filter_map(|c| match c.kind {
            Kind::Money { amount_minor, .. } => Some(i128::from(amount_minor)),
            _ => None,
        })
        .sum();

    if opted_up || money > i128::from(threshold_minor) {
        Tier::One
    } else {
        Tier::Zero
    }
}

/// What is known about a party at the moment they sign.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Verification {
    pub email_verified: bool,
    pub phone_verified: bool,
    /// When they last entered a one-time code.
    pub authenticated_at: OffsetDateTime,
    /// The counterparty was named in the invitation.
    pub pre_bound_invitation: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Missing {
    Email,
    Phone,
    FreshCode,
    PreBoundInvitation,
}

/// What a party still has to do before they may sign at this tier. Empty
/// means they may sign.
pub fn missing(
    tier: Tier,
    verification: &Verification,
    now: OffsetDateTime,
    fresh_within: Duration,
) -> Vec<Missing> {
    let mut missing = Vec::new();
    if tier == Tier::Zero {
        return missing;
    }
    if !verification.email_verified {
        missing.push(Missing::Email);
    }
    if !verification.phone_verified {
        missing.push(Missing::Phone);
    }
    if now - verification.authenticated_at > fresh_within {
        missing.push(Missing::FreshCode);
    }
    if !verification.pre_bound_invitation {
        missing.push(Missing::PreBoundInvitation);
    }
    missing
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;
    use crate::domain::revision::tests::{contribution, fence_job, money};
    use crate::domain::revision::{Due, Slot};

    const NOW: OffsetDateTime = datetime!(2026-10-05 09:00 UTC);
    const THRESHOLD: i64 = 50_000;
    const FRESH: Duration = Duration::minutes(10);

    #[test]
    fn money_at_the_threshold_stays_at_the_default_tier() {
        // The fence job has one payment of exactly $500.
        assert_eq!(required_tier(&fence_job(), false, THRESHOLD), Tier::Zero);
    }

    #[test]
    fn money_over_the_threshold_raises_the_tier() {
        let mut revision = fence_job();
        revision
            .contributions
            .push(contribution(3, Slot::B, money(1), Due::OnAgreement));
        assert_eq!(required_tier(&revision, false, THRESHOLD), Tier::One);
    }

    #[test]
    fn either_party_can_opt_up_whatever_the_amount() {
        let mut revision = fence_job();
        revision.contributions.truncate(1);
        assert_eq!(required_tier(&revision, false, THRESHOLD), Tier::Zero);
        assert_eq!(required_tier(&revision, true, THRESHOLD), Tier::One);
    }

    fn fully_verified() -> Verification {
        Verification {
            email_verified: true,
            phone_verified: true,
            authenticated_at: NOW - Duration::minutes(2),
            pre_bound_invitation: true,
        }
    }

    #[test]
    fn the_default_tier_asks_for_nothing_more() {
        let bare = Verification {
            email_verified: true,
            phone_verified: false,
            authenticated_at: NOW - Duration::days(20),
            pre_bound_invitation: false,
        };
        assert_eq!(missing(Tier::Zero, &bare, NOW, FRESH), vec![]);
    }

    #[test]
    fn the_higher_tier_lists_everything_still_needed() {
        assert_eq!(missing(Tier::One, &fully_verified(), NOW, FRESH), vec![]);

        let bare = Verification {
            email_verified: true,
            phone_verified: false,
            authenticated_at: NOW - Duration::minutes(11),
            pre_bound_invitation: false,
        };
        assert_eq!(
            missing(Tier::One, &bare, NOW, FRESH),
            vec![
                Missing::Phone,
                Missing::FreshCode,
                Missing::PreBoundInvitation
            ]
        );
    }
}
