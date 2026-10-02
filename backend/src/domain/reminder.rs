//! Reminders that something is about to fall due, or is overdue
//! (DESIGN.md §5.2, §7, §12).
//!
//! A reminder is not caused by an event. It becomes true because time passed,
//! so the rule here takes the day it is, and what a contribution has already
//! been reminded about, and says what it should be reminded of now.
//!
//! What the two reminders mean:
//!
//! * A contribution due on a **calendar date** is *due soon* from
//!   [`Rules::due_soon_lead`] before that date until the end of it, and
//!   *overdue* from the day after. Both hold only while its status is
//!   `Pending`: once it is claimed, accepted, waived or removed there is
//!   nothing to remind anyone of.
//! * A contribution due **when the agreement is accepted**, or **when another
//!   contribution is accepted**, gets neither. It has no date to be near, and
//!   none to be late against: it falls due the moment something else happens,
//!   and the parties are told of that event when it does. This is the same
//!   reading of "overdue" the apps show.
//!
//! The party who owes a contribution is reminded that it is due soon. Both
//! parties are told that it is overdue. Each happens once per contribution
//! per due date, so an amendment that moves the date starts again and one
//! that leaves it alone does not.
//!
//! "Today" is a date in the exchange's timezone. Working that out needs a
//! timezone database, which the service does not have and PostgreSQL does;
//! the caller asks it and passes the date in.

use std::collections::{BTreeMap, BTreeSet};

use time::Date;

use super::Rules;
use super::contribution::Status;
use super::exchange::{Exchange, State};
use super::notification::Notice;
use super::revision::{Contribution, ContributionId, Due, Revision, Slot};

/// The name is what is stored with a recorded reminder.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Kind {
    DueSoon,
    Overdue,
}

impl Kind {
    pub const ALL: [Kind; 2] = [Kind::DueSoon, Kind::Overdue];

    pub fn as_str(self) -> &'static str {
        match self {
            Kind::DueSoon => "DUE_SOON",
            Kind::Overdue => "OVERDUE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.as_str() == text)
    }

    /// The kind of reminder a message is, if it is one.
    pub fn of(notice: Notice) -> Option<Self> {
        match notice {
            Notice::DueSoon => Some(Kind::DueSoon),
            Notice::OverdueToDeliver | Notice::OverdueToReceive => Some(Kind::Overdue),
            _ => None,
        }
    }
}

/// One reminder: which contribution, of what, and about which due date.
/// Recorded when it is sent, this is also what a contribution "has been
/// reminded about".
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub struct Reminder {
    pub contribution: ContributionId,
    pub kind: Kind,
    pub due: Date,
}

/// Every reminder an exchange's contributions have had.
pub type Reminded = BTreeSet<Reminder>;

/// What is true of a contribution today, whether or not anyone has been told.
fn standing(terms: &Contribution, status: Status, today: Date, rules: &Rules) -> Option<Reminder> {
    let Due::Date(due) = terms.due else {
        return None;
    };
    if status != Status::Pending {
        return None;
    }
    let kind = if today > due {
        Kind::Overdue
    } else {
        // A day too far ahead to compute is further off than any due date.
        let near = today
            .checked_add(rules.due_soon_lead)
            .is_none_or(|until| due <= until);
        if !near {
            return None;
        }
        Kind::DueSoon
    };
    Some(Reminder {
        contribution: terms.id,
        kind,
        due,
    })
}

/// The reminder a contribution calls for now, given its terms in the
/// agreement in force, its status, what it has already been reminded about,
/// and the date in the exchange's timezone.
///
/// A contribution that is already overdue when it is first looked at is told
/// only that: the chance to say "due soon" has gone, and saying it late would
/// be saying something untrue.
pub fn reminder(
    terms: &Contribution,
    status: Status,
    reminded: &Reminded,
    today: Date,
    rules: &Rules,
) -> Option<Reminder> {
    standing(terms, status, today, rules).filter(|reminder| !reminded.contains(reminder))
}

/// A contribution's terms in the agreement in force, and its status, on an
/// exchange that is under way. An exchange still being negotiated, or closed,
/// has nothing to be reminded of, and neither does a contribution that is
/// only in a proposed amendment.
fn in_force(exchange: &Exchange) -> impl Iterator<Item = (&Contribution, Status)> {
    let agreement = match exchange.state {
        State::Active => exchange.in_force.as_ref(),
        _ => None,
    };
    agreement
        .into_iter()
        .flat_map(|agreement| &agreement.revision.contributions)
        .filter_map(|terms| Some((terms, *exchange.statuses.get(&terms.id)?)))
}

/// Every reminder an exchange calls for now and has not had, in the order
/// the agreement lists its contributions.
pub fn due(exchange: &Exchange, reminded: &Reminded, today: Date, rules: &Rules) -> Vec<Reminder> {
    in_force(exchange)
        .filter_map(|(terms, status)| reminder(terms, status, reminded, today, rules))
        .collect()
}

/// Whether a reminder recorded earlier is still true: the contribution is
/// still in the agreement in force, still pending, still due on that date,
/// and the date is still ahead (for "due soon") or behind (for "overdue").
///
/// Asked just before a queued reminder is sent, which can be some time after
/// it was queued: by then the contribution may have been delivered, or the
/// date moved.
pub fn stands(exchange: &Exchange, reminder: &Reminder, today: Date) -> bool {
    in_force(exchange).any(|(terms, status)| {
        terms.id == reminder.contribution
            && status == Status::Pending
            && terms.due == Due::Date(reminder.due)
            && match reminder.kind {
                Kind::DueSoon => today <= reminder.due,
                Kind::Overdue => today > reminder.due,
            }
    })
}

/// One message a pass of reminders calls for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Message {
    pub notice: Notice,
    pub to: Slot,
    /// The contributions it is about. They are not named in the message,
    /// which carries nothing from the agreement (DESIGN.md §12); they are
    /// kept so that it can be checked again before it is sent.
    pub contributions: Vec<ContributionId>,
}

/// Who is told about the reminders that came due together.
///
/// The party who owes a contribution is told it is due soon, and both parties
/// that it is overdue, in words that fit which side of it they are on. A
/// message names no contribution, so several that come due at once for the
/// same person would read as the same email several times; they are told
/// once.
pub fn messages(agreement: &Revision, reminders: &[Reminder]) -> Vec<Message> {
    let mut grouped: BTreeMap<(Slot, Notice), Vec<ContributionId>> = BTreeMap::new();
    for reminder in reminders {
        let Some(terms) = agreement.contribution(reminder.contribution) else {
            continue;
        };
        let provider = terms.from;
        let told: &[(Slot, Notice)] = match reminder.kind {
            Kind::DueSoon => &[(provider, Notice::DueSoon)],
            Kind::Overdue => &[
                (provider, Notice::OverdueToDeliver),
                (provider.other(), Notice::OverdueToReceive),
            ],
        };
        for key in told {
            grouped.entry(*key).or_default().push(terms.id);
        }
    }
    grouped
        .into_iter()
        .map(|((to, notice), contributions)| Message {
            notice,
            to,
            contributions,
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use time::Duration;
    use time::macros::{date, datetime};

    use super::*;
    use crate::domain::amendment::Statuses;
    use crate::domain::exchange::{Counterparty, InForce, NotAgreed, Open, Outcome};
    use crate::domain::revision::tests::{contribution, fence_job, id, money};
    use crate::domain::revision::{Kind as Type, RevisionId};
    use uuid::Uuid;

    const DUE: Date = date!(2026 - 11 - 01);

    fn rules() -> Rules {
        Rules {
            due_soon_lead: Duration::days(2),
            ..Rules::default()
        }
    }

    /// The fence repair, owed by A on 1 November.
    fn repair() -> Contribution {
        fence_job().contributions.remove(0)
    }

    fn told(kind: Kind, due: Date) -> Reminder {
        Reminder {
            contribution: id(1),
            kind,
            due,
        }
    }

    fn pending(today: Date) -> Option<Kind> {
        reminder(
            &repair(),
            Status::Pending,
            &Reminded::new(),
            today,
            &rules(),
        )
        .map(|reminder| reminder.kind)
    }

    #[test]
    fn a_dated_contribution_is_due_soon_inside_the_lead_time_and_overdue_after_the_date() {
        assert_eq!(pending(date!(2026 - 10 - 01)), None);
        assert_eq!(pending(date!(2026 - 10 - 29)), None, "three days ahead");
        assert_eq!(pending(date!(2026 - 10 - 30)), Some(Kind::DueSoon));
        assert_eq!(pending(date!(2026 - 10 - 31)), Some(Kind::DueSoon));
        assert_eq!(
            pending(DUE),
            Some(Kind::DueSoon),
            "the due date itself is not late"
        );
        assert_eq!(pending(date!(2026 - 11 - 02)), Some(Kind::Overdue));
        assert_eq!(pending(date!(2027 - 11 - 02)), Some(Kind::Overdue));
    }

    #[test]
    fn the_reminder_names_the_contribution_and_the_date_it_is_about() {
        assert_eq!(
            reminder(&repair(), Status::Pending, &Reminded::new(), DUE, &rules()),
            Some(told(Kind::DueSoon, DUE))
        );
    }

    #[test]
    fn only_a_pending_contribution_is_reminded_of() {
        for status in [
            Status::Claimed,
            Status::Disputed,
            Status::Accepted,
            Status::Waived,
            Status::Removed,
        ] {
            for today in [DUE, date!(2026 - 11 - 02)] {
                assert_eq!(
                    reminder(&repair(), status, &Reminded::new(), today, &rules()),
                    None,
                    "{status:?} on {today}"
                );
            }
        }
    }

    #[test]
    fn a_contribution_due_on_an_event_has_no_reminders() {
        for due in [Due::OnAgreement, Due::After(id(2))] {
            let terms = contribution(1, Slot::A, Type::Service, due);
            for today in [date!(2026 - 10 - 01), date!(2030 - 01 - 01)] {
                assert_eq!(
                    reminder(&terms, Status::Pending, &Reminded::new(), today, &rules()),
                    None,
                    "{due:?} on {today}"
                );
            }
        }
    }

    #[test]
    fn each_reminder_is_given_once_per_due_date() {
        let soon = Reminded::from([told(Kind::DueSoon, DUE)]);
        let late = Reminded::from([told(Kind::DueSoon, DUE), told(Kind::Overdue, DUE)]);
        let on = |reminded: &Reminded, today| {
            reminder(&repair(), Status::Pending, reminded, today, &rules()).map(|r| r.kind)
        };

        assert_eq!(on(&soon, date!(2026 - 10 - 31)), None, "already told");
        assert_eq!(on(&soon, DUE), None);
        assert_eq!(on(&soon, date!(2026 - 11 - 02)), Some(Kind::Overdue));
        assert_eq!(on(&late, date!(2026 - 11 - 02)), None);
        assert_eq!(on(&late, date!(2026 - 12 - 25)), None);
    }

    #[test]
    fn a_contribution_first_seen_when_overdue_is_not_told_it_is_due_soon() {
        // The worker was not running while it was due soon.
        assert_eq!(pending(date!(2026 - 11 - 05)), Some(Kind::Overdue));
        // And having been told it is overdue, it is told nothing more.
        let late = Reminded::from([told(Kind::Overdue, DUE)]);
        assert_eq!(
            reminder(
                &repair(),
                Status::Pending,
                &late,
                date!(2026 - 11 - 05),
                &rules()
            ),
            None
        );
    }

    #[test]
    fn a_moved_due_date_is_something_new_to_be_reminded_about() {
        let reminded = Reminded::from([told(Kind::DueSoon, DUE), told(Kind::Overdue, DUE)]);
        let moved = date!(2026 - 11 - 20);
        let mut terms = repair();
        terms.due = Due::Date(moved);
        let on = |today| reminder(&terms, Status::Pending, &reminded, today, &rules());

        assert_eq!(on(date!(2026 - 11 - 05)), None, "no longer overdue");
        assert_eq!(on(date!(2026 - 11 - 18)), Some(told(Kind::DueSoon, moved)));
        assert_eq!(on(date!(2026 - 11 - 21)), Some(told(Kind::Overdue, moved)));

        // Moved back to a date it was already reminded about: nothing again.
        assert_eq!(
            reminder(
                &repair(),
                Status::Pending,
                &reminded,
                date!(2026 - 11 - 05),
                &rules()
            ),
            None
        );
    }

    #[test]
    fn the_lead_time_is_a_setting() {
        let on = |lead: i64, today| {
            let rules = Rules {
                due_soon_lead: Duration::days(lead),
                ..Rules::default()
            };
            reminder(&repair(), Status::Pending, &Reminded::new(), today, &rules).map(|r| r.kind)
        };
        assert_eq!(on(0, date!(2026 - 10 - 31)), None);
        assert_eq!(on(0, DUE), Some(Kind::DueSoon), "only on the day");
        assert_eq!(on(7, date!(2026 - 10 - 24)), None);
        assert_eq!(on(7, date!(2026 - 10 - 25)), Some(Kind::DueSoon));
        // A lead longer than the calendar is long enough for anything.
        assert_eq!(on(4_000_000, date!(2026 - 10 - 01)), Some(Kind::DueSoon));
    }

    /// The fence job in force: the repair (1) owed by A on 1 November, the
    /// payment (2) owed by B after it, and a deposit (3) owed by B on
    /// 1 November too.
    fn active(statuses: &[(u128, Status)]) -> Exchange {
        let mut revision = fence_job();
        revision
            .contributions
            .push(contribution(3, Slot::B, money(10_000), Due::Date(DUE)));
        let mut exchange = Exchange::draft(datetime!(2026-10-01 12:00 UTC));
        exchange.state = State::Active;
        exchange.counterparty = Counterparty::Confirmed;
        exchange.statuses = statuses
            .iter()
            .map(|&(n, status)| (id(n), status))
            .collect::<Statuses>();
        exchange.in_force = Some(InForce {
            id: RevisionId(Uuid::from_u128(1)),
            revision,
        });
        exchange
    }

    const ALL_PENDING: [(u128, Status); 3] = [
        (1, Status::Pending),
        (2, Status::Pending),
        (3, Status::Pending),
    ];

    fn kinds(reminders: &[Reminder]) -> Vec<(ContributionId, Kind)> {
        reminders.iter().map(|r| (r.contribution, r.kind)).collect()
    }

    #[test]
    fn an_exchange_is_reminded_of_what_is_in_force_and_pending() {
        let late = date!(2026 - 11 - 02);
        let exchange = active(&ALL_PENDING);
        assert_eq!(
            kinds(&due(&exchange, &Reminded::new(), DUE, &rules())),
            [(id(1), Kind::DueSoon), (id(3), Kind::DueSoon)],
            "the payment waits on the repair and has no date"
        );
        assert_eq!(
            kinds(&due(&exchange, &Reminded::new(), late, &rules())),
            [(id(1), Kind::Overdue), (id(3), Kind::Overdue)]
        );

        let exchange = active(&[
            (1, Status::Claimed),
            (2, Status::Pending),
            (3, Status::Pending),
        ]);
        assert_eq!(
            kinds(&due(&exchange, &Reminded::new(), late, &rules())),
            [(id(3), Kind::Overdue)]
        );

        let reminded = Reminded::from([Reminder {
            contribution: id(3),
            kind: Kind::Overdue,
            due: DUE,
        }]);
        assert_eq!(due(&exchange, &reminded, late, &rules()), []);
    }

    #[test]
    fn nothing_is_due_on_an_exchange_that_is_not_under_way() {
        let late = date!(2026 - 11 - 02);
        let agreed = active(&ALL_PENDING);

        let mut closed = agreed.clone();
        closed.state = State::Closed(Outcome::Completed);
        assert_eq!(due(&closed, &Reminded::new(), late, &rules()), []);

        // Still being negotiated: the terms are an offer, not an agreement.
        let mut negotiating = Exchange::draft(datetime!(2026-10-01 12:00 UTC));
        negotiating.state = State::Negotiating;
        negotiating.open = Some(Open {
            id: RevisionId(Uuid::from_u128(1)),
            author: Slot::A,
            expires_at: datetime!(2026-12-01 12:00 UTC),
            accepted: false,
            revision: fence_job(),
        });
        assert_eq!(due(&negotiating, &Reminded::new(), late, &rules()), []);
        let mut never_agreed = negotiating.clone();
        never_agreed.state = State::Closed(Outcome::NotAgreed(NotAgreed::Expired));
        assert_eq!(due(&never_agreed, &Reminded::new(), late, &rules()), []);
    }

    #[test]
    fn a_proposed_amendment_changes_no_reminder_until_it_is_in_force() {
        // The proposal moves the repair to 1 December and adds a contribution
        // due today. Neither counts: the agreement in force is what is owed.
        let mut proposed = fence_job();
        proposed.contributions[0].due = Due::Date(date!(2026 - 12 - 01));
        proposed
            .contributions
            .push(contribution(4, Slot::A, Type::Item, Due::Date(DUE)));
        let mut exchange = active(&ALL_PENDING);
        exchange.open = Some(Open {
            id: RevisionId(Uuid::from_u128(2)),
            author: Slot::A,
            expires_at: datetime!(2026-12-01 12:00 UTC),
            accepted: false,
            revision: proposed,
        });

        assert_eq!(
            kinds(&due(&exchange, &Reminded::new(), DUE, &rules())),
            [(id(1), Kind::DueSoon), (id(3), Kind::DueSoon)]
        );
    }

    #[test]
    fn a_recorded_reminder_stands_only_while_it_is_still_true() {
        let soon = told(Kind::DueSoon, DUE);
        let late = told(Kind::Overdue, DUE);
        let after = date!(2026 - 11 - 02);
        let exchange = active(&ALL_PENDING);

        assert!(stands(&exchange, &soon, date!(2026 - 10 - 31)));
        assert!(stands(&exchange, &soon, DUE));
        assert!(!stands(&exchange, &soon, after), "the date has gone by");
        assert!(stands(&exchange, &late, after));
        assert!(!stands(&exchange, &late, DUE));

        // Delivered, or otherwise settled, since it was queued.
        for status in [
            Status::Claimed,
            Status::Disputed,
            Status::Accepted,
            Status::Waived,
            Status::Removed,
        ] {
            let exchange = active(&[(1, status), (2, Status::Pending), (3, Status::Pending)]);
            assert!(!stands(&exchange, &late, after), "{status:?}");
            assert!(!stands(&exchange, &soon, DUE), "{status:?}");
        }

        // The date was moved by an amendment.
        let mut moved = active(&ALL_PENDING);
        let agreement = moved.in_force.as_mut().unwrap();
        agreement.revision.contributions[0].due = Due::Date(date!(2026 - 12 - 01));
        assert!(!stands(&moved, &late, after));

        // The contribution was taken out of the agreement.
        let mut removed = active(&[(2, Status::Pending), (3, Status::Pending)]);
        let agreement = removed.in_force.as_mut().unwrap();
        agreement.revision.contributions.remove(0);
        assert!(!stands(&removed, &late, after));

        let mut closed = active(&ALL_PENDING);
        closed.state = State::Closed(Outcome::EndedByAgreement);
        assert!(!stands(&closed, &late, after));
    }

    #[test]
    fn the_provider_is_told_it_is_due_soon_and_both_parties_that_it_is_overdue() {
        let agreement = active(&ALL_PENDING).in_force.unwrap().revision;
        let soon = [told(Kind::DueSoon, DUE)];
        assert_eq!(
            messages(&agreement, &soon),
            [Message {
                notice: Notice::DueSoon,
                to: Slot::A,
                contributions: vec![id(1)],
            }]
        );

        let late = [told(Kind::Overdue, DUE)];
        assert_eq!(
            messages(&agreement, &late),
            [
                Message {
                    notice: Notice::OverdueToDeliver,
                    to: Slot::A,
                    contributions: vec![id(1)],
                },
                Message {
                    notice: Notice::OverdueToReceive,
                    to: Slot::B,
                    contributions: vec![id(1)],
                },
            ]
        );
    }

    #[test]
    fn reminders_that_come_due_together_are_one_message_per_person_and_kind() {
        let mut revision = active(&ALL_PENDING).in_force.unwrap().revision;
        revision
            .contributions
            .push(contribution(4, Slot::A, Type::Item, Due::Date(DUE)));
        let overdue = |n| Reminder {
            contribution: id(n),
            kind: Kind::Overdue,
            due: DUE,
        };

        // A owes the repair (1) and an item (4); B owes the deposit (3).
        let found = messages(&revision, &[overdue(1), overdue(3), overdue(4)]);
        let summary: Vec<(Slot, Notice, Vec<ContributionId>)> = found
            .into_iter()
            .map(|message| (message.to, message.notice, message.contributions))
            .collect();
        assert_eq!(
            summary,
            [
                (Slot::A, Notice::OverdueToDeliver, vec![id(1), id(4)]),
                (Slot::A, Notice::OverdueToReceive, vec![id(3)]),
                (Slot::B, Notice::OverdueToDeliver, vec![id(3)]),
                (Slot::B, Notice::OverdueToReceive, vec![id(1), id(4)]),
            ]
        );
        // A reminder about something the agreement does not hold is nobody's.
        assert_eq!(messages(&revision, &[overdue(9)]), []);
    }

    #[test]
    fn every_kind_has_its_own_stored_name_and_its_messages() {
        for kind in Kind::ALL {
            assert_eq!(Kind::parse(kind.as_str()), Some(kind));
        }
        assert_eq!(Kind::parse("LATE"), None);

        let reminders: Vec<Notice> = Notice::ALL
            .into_iter()
            .filter(|notice| Kind::of(*notice).is_some())
            .collect();
        assert_eq!(
            reminders,
            [
                Notice::DueSoon,
                Notice::OverdueToDeliver,
                Notice::OverdueToReceive
            ]
        );
    }
}
