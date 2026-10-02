use time::macros::datetime;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::*;
use crate::domain::contribution::Action;
use crate::domain::revision::tests::{contribution, fence_job, id, money};
use crate::domain::revision::{Due, Kind};

const START: OffsetDateTime = datetime!(2026-10-01 12:00 UTC);
const A: Actor = Actor::Party(Slot::A);
const B: Actor = Actor::Party(Slot::B);

fn day(n: i64) -> OffsetDateTime {
    START + Duration::days(n)
}

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

/// Runs commands against one exchange, keeping the events of the last one.
struct Scenario {
    exchange: Exchange,
    events: Vec<Event>,
    rules: Rules,
}

impl Scenario {
    fn draft() -> Self {
        Self {
            exchange: Exchange::draft(START),
            events: Vec::new(),
            rules: Rules::default(),
        }
    }

    /// A has sent the fence job as revision 1; nobody has claimed slot B.
    fn negotiating() -> Self {
        let mut scenario = Self::draft();
        scenario.ok(A, send(1, fence_job()), START);
        scenario
    }

    /// B was named in the invitation, claimed it and accepted revision 1.
    fn active() -> Self {
        let mut scenario = Self::negotiating();
        scenario.ok(B, Command::ClaimCounterparty { pre_bound: true }, START);
        scenario.ok(B, Command::Accept { revision: rev(1) }, START);
        scenario
    }

    fn ok(&mut self, actor: Actor, command: Command, now: OffsetDateTime) -> &mut Self {
        let decision = decide(&self.exchange, actor, command.clone(), now, &self.rules)
            .unwrap_or_else(|refusal| panic!("{command:?} by {actor:?} was refused: {refusal}"));
        self.exchange = decision.exchange;
        self.events = decision.events;
        self
    }

    fn refused(&self, actor: Actor, command: Command, now: OffsetDateTime) -> Refusal {
        match decide(&self.exchange, actor, command.clone(), now, &self.rules) {
            Err(refusal) => refusal,
            Ok(_) => panic!("{command:?} by {actor:?} should have been refused"),
        }
    }

    fn status(&self, n: u128) -> Status {
        self.exchange.statuses[&id(n)]
    }

    fn closed(&self) -> Option<Outcome> {
        match self.exchange.state {
            State::Closed(outcome) => Some(outcome),
            _ => None,
        }
    }
}

// ---- Reaching agreement ---------------------------------------------------

#[test]
fn sending_the_first_revision_opens_the_negotiation() {
    let scenario = Scenario::negotiating();

    assert_eq!(scenario.exchange.state, State::Negotiating);
    let open = scenario.exchange.open.as_ref().unwrap();
    assert_eq!(
        (open.id, open.author, open.expires_at),
        (rev(1), Slot::A, day(14))
    );
    assert_eq!(
        scenario.events,
        vec![Event::RevisionSent {
            revision: rev(1),
            by: Slot::A,
            expires_at: day(14)
        }]
    );
}

#[test]
fn only_the_initiator_can_send_from_a_draft() {
    let scenario = Scenario::draft();
    assert_eq!(
        scenario.refused(B, send(1, fence_job()), START),
        Refusal::WrongActor
    );
}

#[test]
fn an_invalid_revision_is_not_sent() {
    let scenario = Scenario::draft();
    let mut revision = fence_job();
    revision.contributions.clear();

    assert_eq!(
        scenario.refused(A, send(1, revision), START),
        Refusal::InvalidRevision(vec![Invalid::NoRequiredContribution])
    );
}

#[test]
fn a_prebound_counterparty_accepting_makes_the_agreement_binding() {
    let scenario = Scenario::active();

    assert_eq!(scenario.exchange.state, State::Active);
    assert_eq!(scenario.exchange.open, None);
    assert_eq!(scenario.exchange.in_force.as_ref().unwrap().id, rev(1));
    assert_eq!(
        (scenario.status(1), scenario.status(2)),
        (Status::Pending, Status::Pending)
    );
    assert_eq!(
        scenario.events,
        vec![
            Event::RevisionAccepted {
                revision: rev(1),
                by: Slot::B
            },
            Event::AgreementInForce {
                revision: rev(1),
                statuses: scenario.exchange.statuses.clone(),
            },
        ]
    );
}

#[test]
fn nobody_can_act_for_slot_b_before_it_is_claimed() {
    let scenario = Scenario::negotiating();
    for command in [
        Command::Accept { revision: rev(1) },
        Command::Decline { revision: rev(1) },
        send(2, fence_job()),
    ] {
        assert_eq!(scenario.refused(B, command, START), Refusal::NotAllowed);
    }
}

#[test]
fn an_unconfirmed_acceptance_waits_for_the_initiator() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: false }, day(1));
    scenario.ok(B, Command::Accept { revision: rev(1) }, day(1));

    // Signed by both, but the initiator has not said who B is.
    assert_eq!(scenario.exchange.state, State::Negotiating);
    assert!(scenario.exchange.open.as_ref().unwrap().accepted);
    assert_eq!(
        scenario.events,
        vec![Event::RevisionAccepted {
            revision: rev(1),
            by: Slot::B
        }]
    );

    scenario.ok(A, Command::ConfirmCounterparty, day(2));
    assert_eq!(scenario.exchange.state, State::Active);
    assert!(matches!(
        scenario.events[..],
        [Event::CounterpartyConfirmed, Event::AgreementInForce { .. }]
    ));
}

#[test]
fn confirming_after_the_offer_ran_out_does_not_revive_it() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: false }, day(1));
    scenario.ok(B, Command::Accept { revision: rev(1) }, day(1));

    scenario.ok(A, Command::ConfirmCounterparty, day(14));
    assert_eq!(scenario.exchange.state, State::Negotiating);

    scenario.ok(Actor::System, Command::ExpireRevision, day(14));
    assert_eq!(
        scenario.closed(),
        Some(Outcome::NotAgreed(NotAgreed::Expired))
    );
}

#[test]
fn the_initiator_cannot_accept_a_counteroffer_from_an_unconfirmed_counterparty() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: false }, day(1));
    scenario.ok(B, send(2, fence_job()), day(1));

    assert_eq!(
        scenario.refused(A, Command::Accept { revision: rev(2) }, day(2)),
        Refusal::CounterpartyNotConfirmed
    );

    scenario.ok(A, Command::ConfirmCounterparty, day(2));
    scenario.ok(A, Command::Accept { revision: rev(2) }, day(2));
    assert_eq!(scenario.exchange.state, State::Active);
}

#[test]
fn only_the_initiator_confirms_and_only_the_invited_party_claims() {
    let mut scenario = Scenario::negotiating();
    assert_eq!(
        scenario.refused(A, Command::ClaimCounterparty { pre_bound: true }, START),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(A, Command::ConfirmCounterparty, START),
        Refusal::NotAllowed
    );

    scenario.ok(B, Command::ClaimCounterparty { pre_bound: false }, START);
    assert_eq!(
        scenario.refused(B, Command::ConfirmCounterparty, START),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(B, Command::ClaimCounterparty { pre_bound: false }, START),
        Refusal::NotAllowed
    );
}

// ---- Counteroffers, withdrawal, expiry ------------------------------------

#[test]
fn a_counteroffer_supersedes_the_open_revision() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: true }, day(1));
    scenario.ok(B, send(2, fence_job()), day(1));

    assert_eq!(
        scenario.events,
        vec![
            Event::RevisionSuperseded { revision: rev(1) },
            Event::RevisionSent {
                revision: rev(2),
                by: Slot::B,
                expires_at: day(15)
            },
        ]
    );

    // Accepting what was on screen a moment ago is refused, not applied to
    // the new terms.
    assert_eq!(
        scenario.refused(A, Command::Accept { revision: rev(1) }, day(1)),
        Refusal::StaleRevision
    );
    scenario.ok(A, Command::Accept { revision: rev(2) }, day(1));
    assert_eq!(scenario.exchange.in_force.as_ref().unwrap().id, rev(2));
}

#[test]
fn the_author_has_already_signed_and_cannot_accept_or_decline() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: true }, START);

    assert_eq!(
        scenario.refused(A, Command::Accept { revision: rev(1) }, START),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(A, Command::Decline { revision: rev(1) }, START),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(B, Command::Withdraw { revision: rev(1) }, START),
        Refusal::WrongActor
    );
}

#[test]
fn withdrawing_or_declining_during_negotiation_ends_it_without_agreement() {
    let mut withdrawn = Scenario::negotiating();
    withdrawn.ok(A, Command::Withdraw { revision: rev(1) }, day(1));
    assert_eq!(
        withdrawn.closed(),
        Some(Outcome::NotAgreed(NotAgreed::Withdrawn))
    );
    assert_eq!(
        withdrawn.events,
        vec![
            Event::RevisionWithdrawn {
                revision: rev(1),
                by: Slot::A
            },
            Event::Closed {
                outcome: Outcome::NotAgreed(NotAgreed::Withdrawn),
                waived: vec![]
            },
        ]
    );

    let mut declined = Scenario::negotiating();
    declined.ok(B, Command::ClaimCounterparty { pre_bound: true }, day(1));
    declined.ok(B, Command::Decline { revision: rev(1) }, day(1));
    assert_eq!(
        declined.closed(),
        Some(Outcome::NotAgreed(NotAgreed::Declined))
    );
}

#[test]
fn an_offer_cannot_be_accepted_once_it_has_expired() {
    let mut scenario = Scenario::negotiating();
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: true }, day(1));

    assert_eq!(
        scenario.refused(B, Command::Accept { revision: rev(1) }, day(14)),
        Refusal::RevisionExpired
    );
    // One second earlier it still could.
    scenario.ok(
        B,
        Command::Accept { revision: rev(1) },
        day(14) - Duration::seconds(1),
    );
    assert_eq!(scenario.exchange.state, State::Active);
}

#[test]
fn expiry_is_the_systems_to_run_and_only_when_due() {
    let mut scenario = Scenario::negotiating();

    assert_eq!(
        scenario.refused(A, Command::ExpireRevision, day(14)),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(Actor::System, Command::ExpireRevision, day(13)),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(Actor::System, Command::Accept { revision: rev(1) }, START),
        Refusal::WrongActor
    );

    scenario.ok(Actor::System, Command::ExpireRevision, day(14));
    assert_eq!(
        scenario.closed(),
        Some(Outcome::NotAgreed(NotAgreed::Expired))
    );
}

// ---- Fulfillment ----------------------------------------------------------

#[test]
fn claims_and_confirmations_complete_the_exchange() {
    let mut scenario = Scenario::active();

    scenario.ok(A, act(1, Action::Claim), day(20));
    assert_eq!(scenario.status(1), Status::Claimed);
    assert_eq!(
        scenario.events,
        vec![Event::ContributionChanged {
            contribution: id(1),
            action: Action::Claim,
            by: Slot::A,
            status: Status::Claimed,
        }]
    );

    scenario.ok(B, act(1, Action::Confirm), day(21));
    scenario.ok(B, act(2, Action::Claim), day(21));
    assert_eq!(scenario.exchange.state, State::Active);

    scenario.ok(A, act(2, Action::Confirm), day(22));
    assert_eq!(scenario.closed(), Some(Outcome::Completed));
    assert!(matches!(
        scenario.events[..],
        [
            Event::ContributionChanged {
                status: Status::Accepted,
                ..
            },
            Event::Closed {
                outcome: Outcome::Completed,
                ..
            }
        ]
    ));
}

#[test]
fn roles_follow_each_contribution_not_the_slot() {
    let scenario = Scenario::active();

    // A provides the repair and receives the payment.
    assert_eq!(
        scenario.refused(B, act(1, Action::Claim), day(1)),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(A, act(2, Action::Claim), day(1)),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(A, act(1, Action::Confirm), day(1)),
        Refusal::WrongActor
    );
}

#[test]
fn a_dispute_can_be_remedied_or_waived() {
    let mut scenario = Scenario::active();
    scenario.ok(A, act(1, Action::Claim), day(20));
    scenario.ok(B, act(1, Action::Dispute), day(21));
    assert_eq!(scenario.status(1), Status::Disputed);
    assert_eq!(scenario.exchange.state, State::Active);

    scenario.ok(A, act(1, Action::Claim), day(22));
    scenario.ok(B, act(1, Action::Confirm), day(23));
    scenario.ok(A, act(2, Action::Waive), day(23));
    assert_eq!(scenario.closed(), Some(Outcome::Completed));
}

#[test]
fn optional_contributions_do_not_hold_up_completion() {
    let mut revision = fence_job();
    let mut extra = contribution(3, Slot::A, Kind::Task, Due::OnAgreement);
    extra.required = false;
    revision.contributions.push(extra);

    let mut scenario = Scenario::draft();
    scenario.ok(A, send(1, revision), START);
    scenario.ok(B, Command::ClaimCounterparty { pre_bound: true }, START);
    scenario.ok(B, Command::Accept { revision: rev(1) }, START);

    scenario.ok(B, act(1, Action::Confirm), day(1));
    scenario.ok(A, act(2, Action::Confirm), day(1));
    assert_eq!(scenario.closed(), Some(Outcome::Completed));
    assert_eq!(scenario.status(3), Status::Pending);
}

#[test]
fn fulfillment_needs_an_agreement_in_force_and_a_known_contribution() {
    let negotiating = Scenario::negotiating();
    assert_eq!(
        negotiating.refused(A, act(1, Action::Claim), START),
        Refusal::NotAllowed
    );

    let active = Scenario::active();
    assert_eq!(
        active.refused(A, act(9, Action::Claim), START),
        Refusal::UnknownContribution(id(9))
    );
}

// ---- Amendments -----------------------------------------------------------

/// The fence job with a higher price and a clean-up task added.
fn amended() -> Revision {
    let mut revision = fence_job();
    revision.contributions[1].kind = money(60_000);
    revision
        .contributions
        .push(contribution(3, Slot::A, Kind::Task, Due::After(id(1))));
    revision
}

#[test]
fn a_proposed_amendment_changes_nothing_until_accepted() {
    let mut scenario = Scenario::active();
    scenario.ok(A, act(1, Action::Claim), day(5));
    scenario.ok(A, send(2, amended()), day(6));

    assert_eq!(scenario.exchange.state, State::Active);
    assert_eq!(scenario.exchange.in_force.as_ref().unwrap().id, rev(1));
    assert_eq!(scenario.exchange.open.as_ref().unwrap().id, rev(2));
    assert_eq!(scenario.status(1), Status::Claimed);

    // Fulfillment continues under the agreement in force.
    scenario.ok(B, act(1, Action::Dispute), day(7));
    assert_eq!(scenario.status(1), Status::Disputed);
}

#[test]
fn an_accepted_amendment_replaces_the_agreement_and_resets_what_changed() {
    let mut scenario = Scenario::active();
    scenario.ok(A, act(1, Action::Claim), day(5));
    scenario.ok(B, act(2, Action::Claim), day(5));
    scenario.ok(A, send(2, amended()), day(6));
    scenario.ok(B, Command::Accept { revision: rev(2) }, day(7));

    assert_eq!(scenario.exchange.in_force.as_ref().unwrap().id, rev(2));
    assert_eq!(scenario.exchange.open, None);
    assert_eq!(scenario.status(1), Status::Claimed, "untouched");
    assert_eq!(scenario.status(2), Status::Pending, "price changed");
    assert_eq!(scenario.status(3), Status::Pending, "new");
}

#[test]
fn an_amendment_can_remove_a_contribution() {
    let mut scenario = Scenario::active();
    let mut revision = fence_job();
    revision.contributions.truncate(1);

    scenario.ok(B, send(2, revision), day(1));
    scenario.ok(A, Command::Accept { revision: rev(2) }, day(1));
    assert_eq!(scenario.status(2), Status::Removed);

    assert_eq!(
        scenario.refused(B, act(2, Action::Claim), day(2)),
        Refusal::UnknownContribution(id(2))
    );
    // Its ID is retired.
    assert_eq!(
        scenario.refused(A, send(3, fence_job()), day(2)),
        Refusal::InvalidRevision(vec![Invalid::ReusedContribution(id(2))])
    );
}

#[test]
fn an_accepted_contribution_is_locked_against_amendment() {
    let mut scenario = Scenario::active();
    scenario.ok(B, act(1, Action::Confirm), day(5));

    let mut revision = fence_job();
    revision.contributions[0].description = "Repair and paint".into();
    assert_eq!(
        scenario.refused(A, send(2, revision), day(6)),
        Refusal::ContributionLocked(id(1))
    );
}

#[test]
fn the_lock_is_checked_again_when_the_amendment_is_accepted() {
    let mut scenario = Scenario::active();
    let mut revision = fence_job();
    revision.contributions[0].description = "Repair and paint".into();
    scenario.ok(A, send(2, revision), day(5));

    // The repair is accepted under the old terms while the amendment waits.
    scenario.ok(B, act(1, Action::Confirm), day(6));
    assert_eq!(
        scenario.refused(B, Command::Accept { revision: rev(2) }, day(7)),
        Refusal::ContributionLocked(id(1))
    );
    assert_eq!(scenario.exchange.in_force.as_ref().unwrap().id, rev(1));
}

#[test]
fn an_amendment_that_goes_nowhere_leaves_the_agreement_as_it_was() {
    let outcomes: [(Actor, Command, OffsetDateTime); 3] = [
        (A, Command::Withdraw { revision: rev(2) }, day(2)),
        (B, Command::Decline { revision: rev(2) }, day(2)),
        (Actor::System, Command::ExpireRevision, day(15)),
    ];
    for (actor, command, now) in outcomes {
        let mut scenario = Scenario::active();
        scenario.ok(A, send(2, amended()), day(1));
        scenario.ok(actor, command.clone(), now);

        assert_eq!(scenario.exchange.state, State::Active, "{command:?}");
        assert_eq!(scenario.exchange.open, None, "{command:?}");
        assert_eq!(
            scenario.exchange.in_force.as_ref().unwrap().id,
            rev(1),
            "{command:?}"
        );
    }
}

#[test]
fn an_amendment_that_settles_everything_completes_the_exchange() {
    let mut scenario = Scenario::active();
    scenario.ok(B, act(1, Action::Confirm), day(5));

    // The parties drop the payment; only the accepted repair remains.
    let mut revision = fence_job();
    revision.contributions.truncate(1);
    scenario.ok(A, send(2, revision), day(6));
    scenario.ok(B, Command::Accept { revision: rev(2) }, day(6));

    assert_eq!(scenario.closed(), Some(Outcome::Completed));
}

#[test]
fn completing_the_exchange_voids_a_pending_amendment() {
    let mut scenario = Scenario::active();
    scenario.ok(B, act(1, Action::Confirm), day(5));
    let mut revision = fence_job();
    revision.contributions[1].kind = money(60_000);
    scenario.ok(A, send(2, revision), day(5));

    scenario.ok(A, act(2, Action::Confirm), day(6));
    assert_eq!(scenario.closed(), Some(Outcome::Completed));
    assert_eq!(scenario.exchange.open, None);
    assert!(
        scenario
            .events
            .contains(&Event::RevisionSuperseded { revision: rev(2) })
    );
}

// ---- Ending by agreement --------------------------------------------------

#[test]
fn ending_by_agreement_releases_what_is_outstanding() {
    let mut scenario = Scenario::active();
    scenario.ok(B, act(1, Action::Confirm), day(5));
    scenario.ok(B, act(2, Action::Claim), day(5));

    scenario.ok(A, Command::ProposeEnd, day(6));
    assert_eq!(
        scenario.refused(A, Command::AcceptEnd, day(6)),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(B, Command::ProposeEnd, day(6)),
        Refusal::NotAllowed
    );

    scenario.ok(B, Command::AcceptEnd, day(7));
    assert_eq!(scenario.closed(), Some(Outcome::EndedByAgreement));
    assert_eq!(
        scenario.status(1),
        Status::Accepted,
        "what was accepted stays accepted"
    );
    assert_eq!(scenario.status(2), Status::Waived);
    assert_eq!(
        scenario.events,
        vec![Event::Closed {
            outcome: Outcome::EndedByAgreement,
            waived: vec![id(2)]
        }]
    );
}

#[test]
fn an_end_proposal_can_be_cancelled_by_either_party() {
    for canceller in [A, B] {
        let mut scenario = Scenario::active();
        scenario.ok(A, Command::ProposeEnd, day(1));
        scenario.ok(canceller, Command::CancelEnd, day(2));

        assert_eq!(scenario.exchange.end_proposed_by, None);
        assert_eq!(
            scenario.refused(B, Command::AcceptEnd, day(3)),
            Refusal::NotAllowed
        );
    }
    assert_eq!(
        Scenario::active().refused(A, Command::CancelEnd, day(1)),
        Refusal::NotAllowed
    );
}

// ---- Closing without agreement --------------------------------------------

#[test]
fn a_close_request_closes_unresolved_when_the_window_lapses() {
    let mut scenario = Scenario::active();
    scenario.ok(A, act(1, Action::Claim), day(20));
    scenario.ok(A, Command::RequestClose, day(40));
    scenario.ok(B, Command::AddStatement, day(41));
    scenario.ok(A, Command::AddStatement, day(42));

    assert_eq!(
        scenario.refused(Actor::System, Command::LapseCloseRequest, day(46)),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(A, Command::LapseCloseRequest, day(47)),
        Refusal::WrongActor
    );

    scenario.ok(Actor::System, Command::LapseCloseRequest, day(47));
    assert_eq!(
        scenario.closed(),
        Some(Outcome::Unresolved(Unresolved::CloseRequest))
    );
    // The record shows "claimed, not confirmed".
    assert_eq!(scenario.status(1), Status::Claimed);
    assert_eq!(scenario.status(2), Status::Pending);
}

#[test]
fn only_the_requester_can_retract_a_close_request() {
    let mut scenario = Scenario::active();
    assert_eq!(
        scenario.refused(A, Command::RetractClose, day(1)),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(A, Command::AddStatement, day(1)),
        Refusal::NotAllowed
    );

    scenario.ok(A, Command::RequestClose, day(1));
    assert_eq!(
        scenario.refused(B, Command::RetractClose, day(2)),
        Refusal::WrongActor
    );
    assert_eq!(
        scenario.refused(B, Command::RequestClose, day(2)),
        Refusal::NotAllowed
    );

    scenario.ok(A, Command::RetractClose, day(2));
    assert_eq!(scenario.exchange.close_request, None);
    assert_eq!(
        scenario.refused(Actor::System, Command::LapseCloseRequest, day(30)),
        Refusal::NotAllowed
    );
}

#[test]
fn the_other_party_can_answer_a_close_request_by_agreeing_to_end() {
    let mut scenario = Scenario::active();
    scenario.ok(A, Command::RequestClose, day(1));
    assert_eq!(
        scenario.refused(A, Command::AcceptEnd, day(2)),
        Refusal::NotAllowed
    );

    scenario.ok(B, Command::AcceptEnd, day(2));
    assert_eq!(scenario.closed(), Some(Outcome::EndedByAgreement));
}

#[test]
fn resolving_the_open_items_during_the_window_completes_instead() {
    let mut scenario = Scenario::active();
    scenario.ok(A, Command::RequestClose, day(1));
    scenario.ok(B, act(1, Action::Confirm), day(2));
    scenario.ok(A, act(2, Action::Confirm), day(2));

    assert_eq!(scenario.closed(), Some(Outcome::Completed));
}

#[test]
fn an_idle_exchange_is_prompted_then_closed() {
    let mut scenario = Scenario::active();
    let system = Actor::System;

    // The repair is due on 1 November, so the clock starts when that day ends.
    let idle_from = datetime!(2026-11-02 00:00 UTC);
    assert_eq!(scenario.exchange.idle_since(), idle_from);
    assert_eq!(
        scenario.refused(
            system,
            Command::PromptInactivity,
            idle_from + Duration::days(59)
        ),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(
            system,
            Command::CloseInactive,
            idle_from + Duration::days(200)
        ),
        Refusal::NotAllowed,
        "never closed without a prompt first"
    );

    let prompted = idle_from + Duration::days(60);
    scenario.ok(system, Command::PromptInactivity, prompted);
    assert_eq!(scenario.events, vec![Event::InactivityPrompted]);
    assert_eq!(
        scenario.refused(
            system,
            Command::PromptInactivity,
            prompted + Duration::days(1)
        ),
        Refusal::NotAllowed
    );
    assert_eq!(
        scenario.refused(
            system,
            Command::CloseInactive,
            prompted + Duration::days(29)
        ),
        Refusal::NotAllowed
    );

    scenario.ok(
        system,
        Command::CloseInactive,
        prompted + Duration::days(30),
    );
    assert_eq!(
        scenario.closed(),
        Some(Outcome::Unresolved(Unresolved::Inactive))
    );
}

#[test]
fn any_activity_after_the_prompt_restarts_the_clock() {
    let mut scenario = Scenario::active();
    let prompted = datetime!(2027-01-01 00:00 UTC);
    scenario.ok(Actor::System, Command::PromptInactivity, prompted);

    let acted = prompted + Duration::days(10);
    scenario.ok(A, act(1, Action::Claim), acted);
    assert_eq!(scenario.exchange.inactivity_prompted_at, None);
    assert_eq!(scenario.exchange.idle_since(), acted);
    assert_eq!(
        scenario.refused(
            Actor::System,
            Command::CloseInactive,
            prompted + Duration::days(30)
        ),
        Refusal::NotAllowed
    );
}

// ---- Closed is final ------------------------------------------------------

#[test]
fn a_closed_exchange_accepts_nothing() {
    let mut scenario = Scenario::active();
    scenario.ok(A, Command::ProposeEnd, day(1));
    scenario.ok(B, Command::AcceptEnd, day(1));

    let commands = [
        (A, send(2, fence_job())),
        (A, act(1, Action::Claim)),
        (B, Command::ProposeEnd),
        (B, Command::RequestClose),
        (Actor::System, Command::PromptInactivity),
        (Actor::System, Command::ExpireRevision),
    ];
    for (actor, command) in commands {
        assert_eq!(
            scenario.refused(actor, command, day(400)),
            Refusal::NotAllowed
        );
    }
}

#[test]
fn a_refusal_changes_nothing() {
    let scenario = Scenario::active();
    let before = scenario.exchange.clone();
    scenario.refused(A, act(1, Action::Confirm), day(1));
    scenario.refused(B, Command::Accept { revision: rev(9) }, day(1));
    assert_eq!(scenario.exchange, before);
}
