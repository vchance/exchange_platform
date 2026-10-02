//! Who is told what when an exchange changes (DESIGN.md §12).
//!
//! [`notification`] reads a decision and says which message it calls for and
//! which parties should get it. Whether a party can be reached at all (the
//! slot may be unclaimed, the account may have no email address) is not known
//! here; that is decided where the message is stored.

use super::contribution::Action;
use super::exchange::{Actor, Event, Exchange, NotAgreed, Outcome, Unresolved};
use super::revision::Slot;

/// One thing a party can be told. The name is what is stored with a queued
/// message and what the wording files key the text by, so renaming one is a
/// change to both.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Notice {
    /// The person the invitation named has joined.
    InvitationClaimed,
    /// Someone joined through an unbound link; the initiator must confirm them.
    InvitationClaimedUnconfirmed,
    CounterpartyConfirmed,
    /// A proposal or counteroffer during negotiation.
    RevisionSent,
    AmendmentProposed,
    /// The other party signed, but it waits on the initiator confirming them.
    AcceptanceWaiting,
    AgreementInForce,
    AmendmentInForce,
    AmendmentDeclined,
    AmendmentWithdrawn,
    AmendmentExpired,
    DeliveryClaimed,
    ClaimRetracted,
    DeliveryConfirmed,
    DisputeOpened,
    ContributionWaived,
    EndProposed,
    EndProposalCancelled,
    CloseRequested,
    CloseRequestRetracted,
    StatementAdded,
    InactivityPrompted,
    ClosedWithdrawn,
    ClosedDeclined,
    ClosedExpired,
    ClosedCompleted,
    ClosedEndedByAgreement,
    ClosedUnresolved,
    ClosedInactive,
}

impl Notice {
    pub const ALL: [Notice; 29] = [
        Notice::InvitationClaimed,
        Notice::InvitationClaimedUnconfirmed,
        Notice::CounterpartyConfirmed,
        Notice::RevisionSent,
        Notice::AmendmentProposed,
        Notice::AcceptanceWaiting,
        Notice::AgreementInForce,
        Notice::AmendmentInForce,
        Notice::AmendmentDeclined,
        Notice::AmendmentWithdrawn,
        Notice::AmendmentExpired,
        Notice::DeliveryClaimed,
        Notice::ClaimRetracted,
        Notice::DeliveryConfirmed,
        Notice::DisputeOpened,
        Notice::ContributionWaived,
        Notice::EndProposed,
        Notice::EndProposalCancelled,
        Notice::CloseRequested,
        Notice::CloseRequestRetracted,
        Notice::StatementAdded,
        Notice::InactivityPrompted,
        Notice::ClosedWithdrawn,
        Notice::ClosedDeclined,
        Notice::ClosedExpired,
        Notice::ClosedCompleted,
        Notice::ClosedEndedByAgreement,
        Notice::ClosedUnresolved,
        Notice::ClosedInactive,
    ];

    pub fn as_str(self) -> &'static str {
        match self {
            Notice::InvitationClaimed => "INVITATION_CLAIMED",
            Notice::InvitationClaimedUnconfirmed => "INVITATION_CLAIMED_UNCONFIRMED",
            Notice::CounterpartyConfirmed => "COUNTERPARTY_CONFIRMED",
            Notice::RevisionSent => "REVISION_SENT",
            Notice::AmendmentProposed => "AMENDMENT_PROPOSED",
            Notice::AcceptanceWaiting => "ACCEPTANCE_WAITING",
            Notice::AgreementInForce => "AGREEMENT_IN_FORCE",
            Notice::AmendmentInForce => "AMENDMENT_IN_FORCE",
            Notice::AmendmentDeclined => "AMENDMENT_DECLINED",
            Notice::AmendmentWithdrawn => "AMENDMENT_WITHDRAWN",
            Notice::AmendmentExpired => "AMENDMENT_EXPIRED",
            Notice::DeliveryClaimed => "DELIVERY_CLAIMED",
            Notice::ClaimRetracted => "CLAIM_RETRACTED",
            Notice::DeliveryConfirmed => "DELIVERY_CONFIRMED",
            Notice::DisputeOpened => "DISPUTE_OPENED",
            Notice::ContributionWaived => "CONTRIBUTION_WAIVED",
            Notice::EndProposed => "END_PROPOSED",
            Notice::EndProposalCancelled => "END_PROPOSAL_CANCELLED",
            Notice::CloseRequested => "CLOSE_REQUESTED",
            Notice::CloseRequestRetracted => "CLOSE_REQUEST_RETRACTED",
            Notice::StatementAdded => "STATEMENT_ADDED",
            Notice::InactivityPrompted => "INACTIVITY_PROMPTED",
            Notice::ClosedWithdrawn => "CLOSED_WITHDRAWN",
            Notice::ClosedDeclined => "CLOSED_DECLINED",
            Notice::ClosedExpired => "CLOSED_EXPIRED",
            Notice::ClosedCompleted => "CLOSED_COMPLETED",
            Notice::ClosedEndedByAgreement => "CLOSED_ENDED_BY_AGREEMENT",
            Notice::ClosedUnresolved => "CLOSED_UNRESOLVED",
            Notice::ClosedInactive => "CLOSED_INACTIVE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|notice| notice.as_str() == text)
    }
}

/// The message a decision calls for.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Notification {
    /// Position, among the decision's events, of the one the message is about.
    pub event: usize,
    pub notice: Notice,
    pub to: Vec<Slot>,
}

/// What one event is worth saying, if anything. `before` is the exchange as
/// it stood when the command arrived: the same event reads differently in a
/// negotiation and on an agreement already in force.
fn notice(before: &Exchange, event: &Event) -> Option<Notice> {
    let amending = before.in_force.is_some();
    Some(match event {
        Event::CounterpartyClaimed { confirmed: true } => Notice::InvitationClaimed,
        Event::CounterpartyClaimed { confirmed: false } => Notice::InvitationClaimedUnconfirmed,
        Event::CounterpartyConfirmed => Notice::CounterpartyConfirmed,
        Event::RevisionSent { .. } if amending => Notice::AmendmentProposed,
        Event::RevisionSent { .. } => Notice::RevisionSent,
        // Nobody needs telling by itself: it only ever accompanies the new
        // revision or the closure that replaced it.
        Event::RevisionSuperseded { .. } => return None,
        Event::RevisionAccepted { .. } => Notice::AcceptanceWaiting,
        Event::RevisionDeclined { .. } => Notice::AmendmentDeclined,
        Event::RevisionWithdrawn { .. } => Notice::AmendmentWithdrawn,
        Event::RevisionExpired { .. } => Notice::AmendmentExpired,
        Event::AgreementInForce { .. } if amending => Notice::AmendmentInForce,
        Event::AgreementInForce { .. } => Notice::AgreementInForce,
        Event::ContributionChanged { action, .. } => match action {
            Action::Claim => Notice::DeliveryClaimed,
            Action::RetractClaim => Notice::ClaimRetracted,
            Action::Confirm => Notice::DeliveryConfirmed,
            Action::Dispute => Notice::DisputeOpened,
            Action::Waive => Notice::ContributionWaived,
        },
        Event::EndProposed { .. } => Notice::EndProposed,
        Event::EndProposalCancelled { .. } => Notice::EndProposalCancelled,
        Event::CloseRequested { .. } => Notice::CloseRequested,
        Event::CloseRequestRetracted { .. } => Notice::CloseRequestRetracted,
        Event::StatementAdded { .. } => Notice::StatementAdded,
        Event::InactivityPrompted => Notice::InactivityPrompted,
        Event::Closed { outcome, .. } => match outcome {
            Outcome::NotAgreed(NotAgreed::Withdrawn) => Notice::ClosedWithdrawn,
            Outcome::NotAgreed(NotAgreed::Declined) => Notice::ClosedDeclined,
            Outcome::NotAgreed(NotAgreed::Expired) => Notice::ClosedExpired,
            Outcome::Completed => Notice::ClosedCompleted,
            Outcome::EndedByAgreement => Notice::ClosedEndedByAgreement,
            Outcome::Unresolved(Unresolved::CloseRequest) => Notice::ClosedUnresolved,
            Outcome::Unresolved(Unresolved::Inactive) => Notice::ClosedInactive,
        },
    })
}

/// The one message a decision calls for, and who should get it.
///
/// One command can record several events (a confirmation that completes the
/// exchange is a contribution change and a closure), but a person wants one
/// message about it, not one per event. The events are recorded in the order
/// they follow from each other, so the last one worth saying is where things
/// ended up, and that is what is said.
///
/// Everyone but the actor is told: the actor already knows what they did, and
/// what the worker does on a timer is news to both parties.
pub fn notification(before: &Exchange, actor: Actor, events: &[Event]) -> Option<Notification> {
    let (event, notice) = events
        .iter()
        .enumerate()
        .rev()
        .find_map(|(index, event)| Some((index, notice(before, event)?)))?;
    let to = match actor {
        Actor::Party(slot) => vec![slot.other()],
        Actor::System => vec![Slot::A, Slot::B],
    };
    Some(Notification { event, notice, to })
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use time::macros::datetime;
    use time::{Duration, OffsetDateTime};
    use uuid::Uuid;

    use super::*;
    use crate::domain::Rules;
    use crate::domain::exchange::{Command, decide};
    use crate::domain::revision::tests::{contribution, fence_job, id};
    use crate::domain::revision::{Due, Kind, Revision, RevisionId};

    const START: OffsetDateTime = datetime!(2026-10-01 12:00 UTC);
    const A: Actor = Actor::Party(Slot::A);
    const B: Actor = Actor::Party(Slot::B);

    fn rev(n: u128) -> RevisionId {
        RevisionId(Uuid::from_u128(n))
    }

    fn send(n: u128, revision: Revision) -> Command {
        Command::Send {
            id: rev(n),
            revision,
        }
    }

    fn act(n: u128, action: Action) -> Command {
        Command::Contribution { id: id(n), action }
    }

    /// Runs commands against one exchange and reports what each called for.
    struct Scenario {
        exchange: Exchange,
        rules: Rules,
    }

    impl Scenario {
        fn draft() -> Self {
            Self {
                exchange: Exchange::draft(START),
                rules: Rules::default(),
            }
        }

        /// A sent the fence job as revision 1 and B, named in the invitation,
        /// has joined.
        fn negotiating() -> Self {
            let mut scenario = Self::draft();
            scenario.run(A, send(1, fence_job()));
            scenario.run(B, Command::ClaimCounterparty { pre_bound: true });
            scenario
        }

        fn active() -> Self {
            let mut scenario = Self::negotiating();
            scenario.run(B, Command::Accept { revision: rev(1) });
            scenario
        }

        fn run_at(
            &mut self,
            actor: Actor,
            command: Command,
            now: OffsetDateTime,
        ) -> Option<(Notice, Vec<Slot>)> {
            let decision = decide(&self.exchange, actor, command.clone(), now, &self.rules)
                .unwrap_or_else(|refusal| panic!("{command:?} was refused: {refusal}"));
            let found = notification(&self.exchange, actor, &decision.events);
            if let Some(found) = &found {
                assert_eq!(
                    notice(&self.exchange, &decision.events[found.event]),
                    Some(found.notice),
                    "the message names the event it is about"
                );
            }
            self.exchange = decision.exchange;
            found.map(|found| (found.notice, found.to))
        }

        fn run(&mut self, actor: Actor, command: Command) -> Option<(Notice, Vec<Slot>)> {
            self.run_at(actor, command, START)
        }
    }

    fn to_a(notice: Notice) -> Option<(Notice, Vec<Slot>)> {
        Some((notice, vec![Slot::A]))
    }

    fn to_b(notice: Notice) -> Option<(Notice, Vec<Slot>)> {
        Some((notice, vec![Slot::B]))
    }

    fn to_both(notice: Notice) -> Option<(Notice, Vec<Slot>)> {
        Some((notice, vec![Slot::A, Slot::B]))
    }

    #[test]
    fn a_negotiation_tells_the_party_who_did_not_act() {
        let mut s = Scenario::draft();
        // Slot B is addressed here even while it is empty; whether anyone is
        // there to be told is decided where the message is stored.
        assert_eq!(
            s.run(A, send(1, fence_job())),
            to_b(Notice::RevisionSent),
            "a first proposal"
        );
        assert_eq!(
            s.run(B, Command::ClaimCounterparty { pre_bound: false }),
            to_a(Notice::InvitationClaimedUnconfirmed)
        );
        assert_eq!(
            s.run(B, send(2, fence_job())),
            to_a(Notice::RevisionSent),
            "a counteroffer, which also supersedes revision 1"
        );
        assert_eq!(
            s.run(A, send(3, fence_job())),
            to_b(Notice::RevisionSent),
            "the initiator may counter before confirming"
        );
        assert_eq!(
            s.run(B, Command::Accept { revision: rev(3) }),
            to_a(Notice::AcceptanceWaiting)
        );
        assert_eq!(
            s.run(A, Command::ConfirmCounterparty),
            to_b(Notice::AgreementInForce),
            "confirming brought the waiting acceptance into force"
        );
    }

    #[test]
    fn claiming_and_confirming_say_what_the_other_party_can_now_do() {
        let mut s = Scenario::draft();
        s.run(A, send(1, fence_job()));
        assert_eq!(
            s.run(B, Command::ClaimCounterparty { pre_bound: true }),
            to_a(Notice::InvitationClaimed)
        );

        let mut s = Scenario::draft();
        s.run(A, send(1, fence_job()));
        s.run(B, Command::ClaimCounterparty { pre_bound: false });
        assert_eq!(
            s.run(A, Command::ConfirmCounterparty),
            to_b(Notice::CounterpartyConfirmed),
            "nothing was waiting on the confirmation"
        );
        assert_eq!(
            s.run(B, Command::Accept { revision: rev(1) }),
            to_a(Notice::AgreementInForce)
        );
    }

    #[test]
    fn a_negotiation_that_ends_is_reported_as_closed_with_the_reason() {
        let mut s = Scenario::negotiating();
        assert_eq!(
            s.run(B, Command::Decline { revision: rev(1) }),
            to_a(Notice::ClosedDeclined)
        );

        let mut s = Scenario::negotiating();
        assert_eq!(
            s.run(A, Command::Withdraw { revision: rev(1) }),
            to_b(Notice::ClosedWithdrawn)
        );

        let mut s = Scenario::negotiating();
        assert_eq!(
            s.run_at(
                Actor::System,
                Command::ExpireRevision,
                START + Duration::days(15)
            ),
            to_both(Notice::ClosedExpired)
        );
    }

    #[test]
    fn fulfillment_tells_the_other_party_each_step() {
        let mut s = Scenario::active();
        assert_eq!(
            s.run(A, act(1, Action::Claim)),
            to_b(Notice::DeliveryClaimed)
        );
        assert_eq!(
            s.run(A, act(1, Action::RetractClaim)),
            to_b(Notice::ClaimRetracted)
        );
        s.run(A, act(1, Action::Claim));
        assert_eq!(
            s.run(B, act(1, Action::Dispute)),
            to_a(Notice::DisputeOpened)
        );
        assert_eq!(
            s.run(B, act(1, Action::Confirm)),
            to_a(Notice::DeliveryConfirmed)
        );
        assert_eq!(
            s.run(A, act(2, Action::Waive)),
            to_b(Notice::ClosedCompleted),
            "the waiver completed the exchange, and that is the news"
        );
    }

    #[test]
    fn a_waiver_that_leaves_work_outstanding_is_reported_as_a_waiver() {
        let mut s = Scenario::active();
        assert_eq!(
            s.run(B, act(1, Action::Waive)),
            to_a(Notice::ContributionWaived)
        );
    }

    #[test]
    fn an_amendment_is_reported_as_a_change_to_the_agreement() {
        let mut amended = fence_job();
        amended
            .contributions
            .push(contribution(3, Slot::A, Kind::Item, Due::OnAgreement));

        let mut s = Scenario::active();
        assert_eq!(
            s.run(A, send(2, amended.clone())),
            to_b(Notice::AmendmentProposed)
        );
        assert_eq!(
            s.run(B, Command::Decline { revision: rev(2) }),
            to_a(Notice::AmendmentDeclined)
        );
        s.run(A, send(3, amended.clone()));
        assert_eq!(
            s.run(A, Command::Withdraw { revision: rev(3) }),
            to_b(Notice::AmendmentWithdrawn)
        );
        s.run(A, send(4, amended.clone()));
        assert_eq!(
            s.run_at(
                Actor::System,
                Command::ExpireRevision,
                START + Duration::days(15)
            ),
            to_both(Notice::AmendmentExpired)
        );
        s.run_at(A, send(5, amended), START + Duration::days(15));
        assert_eq!(
            s.run_at(
                B,
                Command::Accept { revision: rev(5) },
                START + Duration::days(15)
            ),
            to_a(Notice::AmendmentInForce)
        );
    }

    #[test]
    fn ending_and_closing_tell_the_other_party_and_timers_tell_both() {
        let mut s = Scenario::active();
        assert_eq!(s.run(A, Command::ProposeEnd), to_b(Notice::EndProposed));
        assert_eq!(
            s.run(B, Command::CancelEnd),
            to_a(Notice::EndProposalCancelled)
        );
        s.run(A, Command::ProposeEnd);
        assert_eq!(
            s.run(B, Command::AcceptEnd),
            to_a(Notice::ClosedEndedByAgreement)
        );

        let mut s = Scenario::active();
        assert_eq!(
            s.run(B, Command::RequestClose),
            to_a(Notice::CloseRequested)
        );
        assert_eq!(
            s.run(A, Command::AddStatement),
            to_b(Notice::StatementAdded)
        );
        assert_eq!(
            s.run(B, Command::RetractClose),
            to_a(Notice::CloseRequestRetracted)
        );
        s.run(B, Command::RequestClose);
        assert_eq!(
            s.run_at(
                Actor::System,
                Command::LapseCloseRequest,
                START + Duration::days(8)
            ),
            to_both(Notice::ClosedUnresolved)
        );

        // The fence is due a month after the start, and the idle clock runs
        // from then.
        let mut s = Scenario::active();
        assert_eq!(
            s.run_at(
                Actor::System,
                Command::PromptInactivity,
                START + Duration::days(100)
            ),
            to_both(Notice::InactivityPrompted)
        );
        assert_eq!(
            s.run_at(
                Actor::System,
                Command::CloseInactive,
                START + Duration::days(131)
            ),
            to_both(Notice::ClosedInactive)
        );
    }

    #[test]
    fn every_notice_has_its_own_stored_name() {
        let names: BTreeSet<&str> = Notice::ALL.iter().map(|notice| notice.as_str()).collect();
        assert_eq!(names.len(), Notice::ALL.len());
        for notice in Notice::ALL {
            assert_eq!(Notice::parse(notice.as_str()), Some(notice));
        }
        assert_eq!(Notice::parse("NO_SUCH_NOTICE"), None);
    }
}
