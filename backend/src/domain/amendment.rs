//! What an amendment may change, and what it does to contribution statuses
//! once both parties accept it (DESIGN.md §7).

use std::collections::BTreeMap;

use super::contribution::Status;
use super::revision::{ContributionId, Invalid, Revision};

pub type Statuses = BTreeMap<ContributionId, Status>;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    /// An accepted contribution cannot be changed or removed. To adjust for
    /// it, the amendment adds a new contribution.
    #[error("contribution {0:?} has been accepted and is locked")]
    Locked(ContributionId),
    #[error(transparent)]
    Invalid(#[from] Invalid),
}

/// The statuses every contribution would have if `proposed` replaced
/// `in_force`.
///
/// - Untouched contributions keep their status.
/// - A contribution whose terms change goes back to `Pending`; its history
///   stays in the event log.
/// - A contribution left out becomes `Removed`, and its ID is retired.
/// - A new contribution starts `Pending`.
/// - An `Accepted` contribution must be carried over exactly as it is.
///
/// This is checked when the amendment is sent and again when it is accepted,
/// because a contribution may be accepted in between.
pub fn effects(
    in_force: &Revision,
    statuses: &Statuses,
    proposed: &Revision,
) -> Result<Statuses, Refusal> {
    let mut next = statuses.clone();

    for contribution in &proposed.contributions {
        let id = contribution.id;
        let status = match (in_force.contribution(id), statuses.get(&id)) {
            (Some(current), Some(&status)) if current == contribution => status,
            (Some(_), Some(Status::Accepted)) => return Err(Refusal::Locked(id)),
            (Some(_), _) => Status::Pending,
            (None, Some(_)) => return Err(Invalid::ReusedContribution(id).into()),
            (None, None) => Status::Pending,
        };
        next.insert(id, status);
    }

    for contribution in &in_force.contributions {
        let id = contribution.id;
        if proposed.contribution(id).is_none() {
            if statuses.get(&id) == Some(&Status::Accepted) {
                return Err(Refusal::Locked(id));
            }
            next.insert(id, Status::Removed);
        }
    }

    Ok(next)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::revision::tests::{contribution, fence_job, id, money};
    use crate::domain::revision::{Due, Kind, Slot};

    fn statuses(pairs: &[(u128, Status)]) -> Statuses {
        pairs.iter().map(|&(n, status)| (id(n), status)).collect()
    }

    #[test]
    fn untouched_contributions_keep_their_status() {
        let in_force = fence_job();
        let current = statuses(&[(1, Status::Claimed), (2, Status::Pending)]);

        let mut proposed = in_force.clone();
        proposed.terms = "Repair the back fence and the gate.".into();

        assert_eq!(effects(&in_force, &current, &proposed), Ok(current));
    }

    #[test]
    fn a_changed_contribution_goes_back_to_pending() {
        let in_force = fence_job();
        for status in [
            Status::Claimed,
            Status::Disputed,
            Status::Waived,
            Status::Pending,
        ] {
            let current = statuses(&[(1, status), (2, Status::Pending)]);
            let mut proposed = in_force.clone();
            proposed.contributions[0].description = "Repair and paint the fence".into();

            assert_eq!(
                effects(&in_force, &current, &proposed),
                Ok(statuses(&[(1, Status::Pending), (2, Status::Pending)])),
                "from {status:?}"
            );
        }
    }

    #[test]
    fn a_dropped_contribution_is_removed_and_a_new_one_starts_pending() {
        let in_force = fence_job();
        let current = statuses(&[(1, Status::Claimed), (2, Status::Pending)]);

        let mut proposed = in_force.clone();
        proposed.contributions.remove(1);
        proposed
            .contributions
            .push(contribution(3, Slot::B, Kind::Item, Due::OnAgreement));

        assert_eq!(
            effects(&in_force, &current, &proposed),
            Ok(statuses(&[
                (1, Status::Claimed),
                (2, Status::Removed),
                (3, Status::Pending),
            ]))
        );
    }

    #[test]
    fn an_accepted_contribution_cannot_be_changed_or_removed() {
        let in_force = fence_job();
        let current = statuses(&[(1, Status::Accepted), (2, Status::Pending)]);

        let mut changed = in_force.clone();
        changed.contributions[0].required = false;
        assert_eq!(
            effects(&in_force, &current, &changed),
            Err(Refusal::Locked(id(1)))
        );

        let mut removed = in_force.clone();
        removed.contributions.remove(0);
        removed.contributions[0].due = Due::OnAgreement;
        assert_eq!(
            effects(&in_force, &current, &removed),
            Err(Refusal::Locked(id(1)))
        );
    }

    #[test]
    fn an_accepted_contribution_can_be_carried_over_and_adjusted_for() {
        let in_force = fence_job();
        let current = statuses(&[(1, Status::Accepted), (2, Status::Pending)]);

        let mut proposed = in_force.clone();
        proposed
            .contributions
            .push(contribution(3, Slot::B, money(5_000), Due::OnAgreement));

        assert_eq!(
            effects(&in_force, &current, &proposed),
            Ok(statuses(&[
                (1, Status::Accepted),
                (2, Status::Pending),
                (3, Status::Pending),
            ]))
        );
    }

    #[test]
    fn a_removed_contributions_id_is_retired() {
        let mut in_force = fence_job();
        in_force.contributions.truncate(1);
        let current = statuses(&[(1, Status::Pending), (2, Status::Removed)]);

        assert_eq!(
            effects(&in_force, &current, &fence_job()),
            Err(Refusal::Invalid(Invalid::ReusedContribution(id(2))))
        );
    }

    #[test]
    fn reordering_alone_changes_no_status() {
        let in_force = fence_job();
        let current = statuses(&[(1, Status::Claimed), (2, Status::Disputed)]);

        let mut proposed = in_force.clone();
        proposed.contributions.swap(0, 1);

        assert_eq!(effects(&in_force, &current, &proposed), Ok(current));
    }
}
