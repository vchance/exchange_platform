//! What a revision says, and whether it may be sent (DESIGN.md §6, §7).

use std::collections::BTreeSet;

use time::Date;
use uuid::Uuid;

use super::Rules;

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct RevisionId(pub Uuid);

/// Stable for the life of the exchange: the same contribution keeps its ID
/// across revisions, so claims and confirmations survive an amendment.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct ContributionId(pub Uuid);

/// One side of the exchange. `A` is the initiator, `B` the invited counterparty.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Slot {
    A,
    B,
}

impl Slot {
    pub fn other(self) -> Self {
        match self {
            Slot::A => Slot::B,
            Slot::B => Slot::A,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Slot::A => "A",
            Slot::B => "B",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Settlement {
    /// Paid by any outside means and tracked by claim and confirm.
    OffPlatform,
    /// Paid through the payment processor (V2).
    Processor,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Item,
    Service,
    Task,
    Other,
    /// In the exchange's currency; only money carries an amount.
    Money {
        amount_minor: i64,
        settlement: Settlement,
    },
}

/// When a contribution falls due.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Due {
    /// A calendar date in the exchange's timezone.
    Date(Date),
    /// When the agreement is accepted.
    OnAgreement,
    /// When another contribution in the same revision is accepted.
    After(ContributionId),
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Quantity {
    /// A positive decimal number as written, such as `2` or `1.5`.
    pub amount: String,
    pub unit: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Contribution {
    pub id: ContributionId,
    /// The party who owes it; the other party receives it.
    pub from: Slot,
    pub kind: Kind,
    pub description: String,
    pub quantity: Option<Quantity>,
    pub due: Due,
    pub completion_criteria: Option<String>,
    /// Only required contributions gate completion of the exchange.
    pub required: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Revision {
    /// Each party's name as written in the agreement.
    pub party_a: String,
    pub party_b: String,
    pub terms: String,
    /// In the order the parties see them.
    pub contributions: Vec<Contribution>,
    /// Content hashes of attached files.
    pub attachments: Vec<[u8; 32]>,
    /// A message to the other party. Not part of the agreed terms.
    pub note: Option<String>,
}

impl Revision {
    pub fn contribution(&self, id: ContributionId) -> Option<&Contribution> {
        self.contributions.iter().find(|c| c.id == id)
    }
}

/// The largest whole number JSON can carry exactly, which bounds amounts so
/// the content hash is the same in every language that computes it.
pub const MAX_AMOUNT_MINOR: i64 = (1 << 53) - 1;

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Invalid {
    #[error("a party's name is empty")]
    EmptyPartyName,
    #[error("the note is longer than allowed")]
    NoteTooLong,
    #[error("there is no required contribution")]
    NoRequiredContribution,
    #[error("contribution {0:?} appears more than once")]
    DuplicateContribution(ContributionId),
    #[error("contribution {0:?} has no description")]
    EmptyDescription(ContributionId),
    #[error("contribution {0:?} has an invalid quantity")]
    InvalidQuantity(ContributionId),
    #[error("contribution {0:?} has an invalid amount")]
    InvalidAmount(ContributionId),
    #[error("contribution {0:?} waits on a contribution that is not in this revision")]
    UnknownDependency(ContributionId),
    #[error("contribution {0:?} waits, directly or through others, on itself")]
    DependencyCycle(ContributionId),
    #[error("contribution {0:?} was removed earlier and its ID cannot be used again")]
    ReusedContribution(ContributionId),
}

/// Checks a revision before it is sent, reporting every problem found.
///
/// A revision needs at least one required contribution: without one the
/// exchange would be complete the moment it was agreed.
pub fn validate(revision: &Revision, rules: &Rules) -> Result<(), Vec<Invalid>> {
    let mut problems = Vec::new();

    if revision.party_a.trim().is_empty() || revision.party_b.trim().is_empty() {
        problems.push(Invalid::EmptyPartyName);
    }
    if let Some(note) = &revision.note
        && note.chars().count() > rules.note_max_chars
    {
        problems.push(Invalid::NoteTooLong);
    }
    if !revision.contributions.iter().any(|c| c.required) {
        problems.push(Invalid::NoRequiredContribution);
    }

    let mut seen = BTreeSet::new();
    for contribution in &revision.contributions {
        let id = contribution.id;
        if !seen.insert(id) {
            problems.push(Invalid::DuplicateContribution(id));
        }
        if contribution.description.trim().is_empty() {
            problems.push(Invalid::EmptyDescription(id));
        }
        if let Some(quantity) = &contribution.quantity
            && !is_positive_decimal(&quantity.amount)
        {
            problems.push(Invalid::InvalidQuantity(id));
        }
        if let Kind::Money { amount_minor, .. } = contribution.kind
            && !(1..=MAX_AMOUNT_MINOR).contains(&amount_minor)
        {
            problems.push(Invalid::InvalidAmount(id));
        }
    }

    for contribution in &revision.contributions {
        if let Some(problem) = dependency_problem(revision, contribution) {
            problems.push(problem);
        }
    }

    if problems.is_empty() {
        Ok(())
    } else {
        Err(problems)
    }
}

/// Follows the chain of "due after" links from one contribution. Each
/// contribution waits on at most one other, so the chain either ends or loops.
fn dependency_problem(revision: &Revision, start: &Contribution) -> Option<Invalid> {
    let mut visited = BTreeSet::from([start.id]);
    let mut current = start;

    while let Due::After(next) = current.due {
        let Some(target) = revision.contribution(next) else {
            // Reported once, on the contribution that names the missing one.
            return (current.id == start.id).then_some(Invalid::UnknownDependency(start.id));
        };
        if !visited.insert(next) {
            return Some(Invalid::DependencyCycle(start.id));
        }
        current = target;
    }
    None
}

fn is_positive_decimal(text: &str) -> bool {
    let (whole, fraction) = match text.split_once('.') {
        Some((whole, fraction)) => (whole, Some(fraction)),
        None => (text, None),
    };
    let all_digits = |part: &str| !part.is_empty() && part.bytes().all(|b| b.is_ascii_digit());

    text.len() <= 20
        && all_digits(whole)
        && fraction.is_none_or(all_digits)
        && text.bytes().any(|b| (b'1'..=b'9').contains(&b))
}

#[cfg(test)]
pub(crate) mod tests {
    use time::macros::date;

    use super::*;

    pub(crate) fn id(n: u128) -> ContributionId {
        ContributionId(Uuid::from_u128(n))
    }

    pub(crate) fn contribution(n: u128, from: Slot, kind: Kind, due: Due) -> Contribution {
        Contribution {
            id: id(n),
            from,
            kind,
            description: format!("Contribution {n}"),
            quantity: None,
            due,
            completion_criteria: None,
            required: true,
        }
    }

    pub(crate) fn money(amount_minor: i64) -> Kind {
        Kind::Money {
            amount_minor,
            settlement: Settlement::OffPlatform,
        }
    }

    /// A fence repair by A due on a date, paid for by B once it is accepted.
    pub(crate) fn fence_job() -> Revision {
        Revision {
            party_a: "Ana".into(),
            party_b: "Ben".into(),
            terms: "Repair the back fence.".into(),
            contributions: vec![
                contribution(1, Slot::A, Kind::Service, Due::Date(date!(2026 - 11 - 01))),
                contribution(2, Slot::B, money(50_000), Due::After(id(1))),
            ],
            attachments: vec![],
            note: None,
        }
    }

    fn problems(revision: &Revision) -> Vec<Invalid> {
        validate(revision, &Rules::default())
            .err()
            .unwrap_or_default()
    }

    #[test]
    fn a_well_formed_revision_passes() {
        assert_eq!(problems(&fence_job()), vec![]);
    }

    #[test]
    fn party_names_are_required() {
        let mut revision = fence_job();
        revision.party_b = "  ".into();
        assert_eq!(problems(&revision), vec![Invalid::EmptyPartyName]);
    }

    #[test]
    fn the_note_has_a_length_limit_counted_in_characters() {
        let mut revision = fence_job();
        revision.note = Some("ñ".repeat(1000));
        assert_eq!(problems(&revision), vec![]);
        revision.note = Some("ñ".repeat(1001));
        assert_eq!(problems(&revision), vec![Invalid::NoteTooLong]);
    }

    #[test]
    fn at_least_one_contribution_must_be_required() {
        let mut revision = fence_job();
        for contribution in &mut revision.contributions {
            contribution.required = false;
        }
        assert_eq!(problems(&revision), vec![Invalid::NoRequiredContribution]);

        revision.contributions.clear();
        assert_eq!(problems(&revision), vec![Invalid::NoRequiredContribution]);
    }

    #[test]
    fn contribution_ids_are_unique_within_a_revision() {
        let mut revision = fence_job();
        revision.contributions[1].id = id(1);
        revision.contributions[1].due = Due::OnAgreement;
        assert_eq!(
            problems(&revision),
            vec![Invalid::DuplicateContribution(id(1))]
        );
    }

    #[test]
    fn descriptions_quantities_and_amounts_are_checked() {
        let mut revision = fence_job();
        revision.contributions[0].description = String::new();
        revision.contributions[0].quantity = Some(Quantity {
            amount: "0.0".into(),
            unit: None,
        });
        revision.contributions[1].kind = money(0);
        assert_eq!(
            problems(&revision),
            vec![
                Invalid::EmptyDescription(id(1)),
                Invalid::InvalidQuantity(id(1)),
                Invalid::InvalidAmount(id(2)),
            ]
        );

        revision.contributions[1].kind = money(MAX_AMOUNT_MINOR + 1);
        assert!(problems(&revision).contains(&Invalid::InvalidAmount(id(2))));
    }

    #[test]
    fn quantities_are_positive_decimals() {
        for good in ["1", "2.5", "0.25", "10", "007"] {
            assert!(is_positive_decimal(good), "{good}");
        }
        for bad in [
            "", "0", "0.00", "-1", "1.", ".5", "1.2.3", "1e3", "two", " 1",
        ] {
            assert!(!is_positive_decimal(bad), "{bad}");
        }
    }

    #[test]
    fn a_dependency_must_be_in_the_same_revision() {
        let mut revision = fence_job();
        revision.contributions[1].due = Due::After(id(99));
        assert_eq!(problems(&revision), vec![Invalid::UnknownDependency(id(2))]);
    }

    #[test]
    fn dependency_cycles_are_rejected_at_any_length() {
        let mut revision = fence_job();
        revision.contributions[0].due = Due::After(id(1));
        revision.contributions[1].due = Due::OnAgreement;
        assert_eq!(problems(&revision), vec![Invalid::DependencyCycle(id(1))]);

        let mut revision = fence_job();
        revision.contributions[0].due = Due::After(id(2));
        assert_eq!(
            problems(&revision),
            vec![
                Invalid::DependencyCycle(id(1)),
                Invalid::DependencyCycle(id(2))
            ]
        );

        // 3 waits on a two-step loop it is not part of.
        revision
            .contributions
            .push(contribution(3, Slot::A, Kind::Task, Due::After(id(1))));
        assert!(problems(&revision).contains(&Invalid::DependencyCycle(id(3))));
    }

    #[test]
    fn a_chain_of_dependencies_is_fine() {
        let mut revision = fence_job();
        revision
            .contributions
            .push(contribution(3, Slot::A, Kind::Task, Due::After(id(2))));
        assert_eq!(problems(&revision), vec![]);
    }
}
