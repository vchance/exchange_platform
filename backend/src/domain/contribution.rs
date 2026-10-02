//! Contribution state machine (DESIGN.md §5.2).
//!
//! Amendments are not actions here: they change contribution terms through a
//! new revision accepted by both parties, and are handled at the revision level.

/// Where a contribution stands. Overdue is derived from `Pending` plus the due
/// condition; it is not a status.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Status {
    Pending,
    Claimed,
    Disputed,
    Accepted,
    Waived,
    Removed,
}

/// The acting party's role relative to one contribution.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Party {
    /// Owes the contribution.
    Provider,
    /// Receives the contribution.
    Recipient,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Action {
    /// Mark delivered. From `Disputed` this is a re-claim after remedy.
    Claim,
    RetractClaim,
    Confirm,
    Dispute,
    Waive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("this action belongs to the other party")]
    WrongParty,
    #[error("this action is not allowed in the contribution's current status")]
    NotAllowed,
}

/// Applies one action. `Accepted`, `Waived` and `Removed` are final: nothing
/// is accepted by silence, and nothing accepted is reopened.
///
/// Notes required by the design (a dispute reason, a re-claim note) are
/// checked where the request is validated, not here.
pub fn transition(status: Status, action: Action, by: Party) -> Result<Status, Refusal> {
    use Action::*;
    use Status::*;

    let (next, allowed) = match (status, action) {
        (Pending | Disputed, Claim) => (Claimed, Party::Provider),
        (Claimed, RetractClaim) => (Pending, Party::Provider),
        (Pending | Claimed | Disputed, Confirm) => (Accepted, Party::Recipient),
        (Claimed, Dispute) => (Disputed, Party::Recipient),
        (Pending | Claimed | Disputed, Waive) => (Waived, Party::Recipient),
        _ => return Err(Refusal::NotAllowed),
    };

    if by == allowed {
        Ok(next)
    } else {
        Err(Refusal::WrongParty)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const STATUSES: [Status; 6] = [
        Status::Pending,
        Status::Claimed,
        Status::Disputed,
        Status::Accepted,
        Status::Waived,
        Status::Removed,
    ];
    const ACTIONS: [Action; 5] = [
        Action::Claim,
        Action::RetractClaim,
        Action::Confirm,
        Action::Dispute,
        Action::Waive,
    ];
    const PARTIES: [Party; 2] = [Party::Provider, Party::Recipient];

    /// Every row of the table in DESIGN.md §5.2.
    const TABLE: [(Status, Action, Party, Status); 10] = [
        (
            Status::Pending,
            Action::Claim,
            Party::Provider,
            Status::Claimed,
        ),
        (
            Status::Pending,
            Action::Confirm,
            Party::Recipient,
            Status::Accepted,
        ),
        (
            Status::Claimed,
            Action::Confirm,
            Party::Recipient,
            Status::Accepted,
        ),
        (
            Status::Claimed,
            Action::Dispute,
            Party::Recipient,
            Status::Disputed,
        ),
        (
            Status::Claimed,
            Action::RetractClaim,
            Party::Provider,
            Status::Pending,
        ),
        (
            Status::Disputed,
            Action::Claim,
            Party::Provider,
            Status::Claimed,
        ),
        (
            Status::Disputed,
            Action::Confirm,
            Party::Recipient,
            Status::Accepted,
        ),
        (
            Status::Pending,
            Action::Waive,
            Party::Recipient,
            Status::Waived,
        ),
        (
            Status::Claimed,
            Action::Waive,
            Party::Recipient,
            Status::Waived,
        ),
        (
            Status::Disputed,
            Action::Waive,
            Party::Recipient,
            Status::Waived,
        ),
    ];

    #[test]
    fn every_table_row_is_allowed() {
        for (status, action, by, expected) in TABLE {
            assert_eq!(
                transition(status, action, by),
                Ok(expected),
                "{status:?} + {action:?} by {by:?}"
            );
        }
    }

    #[test]
    fn everything_outside_the_table_is_refused() {
        for status in STATUSES {
            for action in ACTIONS {
                for by in PARTIES {
                    let in_table = TABLE
                        .iter()
                        .any(|&(s, a, p, _)| (s, a, p) == (status, action, by));
                    if !in_table {
                        assert!(
                            transition(status, action, by).is_err(),
                            "{status:?} + {action:?} by {by:?} should be refused"
                        );
                    }
                }
            }
        }
    }

    #[test]
    fn the_other_party_is_told_it_is_not_theirs() {
        assert_eq!(
            transition(Status::Claimed, Action::Confirm, Party::Provider),
            Err(Refusal::WrongParty)
        );
        assert_eq!(
            transition(Status::Pending, Action::Claim, Party::Recipient),
            Err(Refusal::WrongParty)
        );
    }

    #[test]
    fn final_statuses_accept_no_action() {
        for status in [Status::Accepted, Status::Waived, Status::Removed] {
            for action in ACTIONS {
                for by in PARTIES {
                    assert_eq!(transition(status, action, by), Err(Refusal::NotAllowed));
                }
            }
        }
    }
}
