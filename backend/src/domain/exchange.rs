//! Exchange state machine (DESIGN.md §5.1, §5.3, §6, §7).
//!
//! [`decide`] takes the current exchange, who is acting, what they want and
//! the time, and returns the events that happened and the resulting exchange,
//! or a typed refusal. It reads no clock and no database.

use time::OffsetDateTime;

use super::Rules;
use super::amendment::{self, Statuses};
use super::contribution::{self, Role, Status};
use super::revision::{self, ContributionId, Due, Invalid, Revision, RevisionId, Slot};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NotAgreed {
    Withdrawn,
    Declined,
    Expired,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Unresolved {
    /// One party asked to close and the response window lapsed.
    CloseRequest,
    Inactive,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Outcome {
    /// Nothing was ever in force.
    NotAgreed(NotAgreed),
    /// Every required contribution was accepted or waived.
    Completed,
    EndedByAgreement,
    /// Closed without the other party's cooperation. Each contribution keeps
    /// the status it had.
    Unresolved(Unresolved),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum State {
    /// Visible to the initiator only.
    Draft,
    /// A revision has been sent. Nothing is binding yet.
    Negotiating,
    /// Both parties accepted the same revision, which is now in force.
    Active,
    Closed(Outcome),
}

/// Whether the invited party's slot is filled, and whether the initiator
/// knows by whom (DESIGN.md §8).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Counterparty {
    Unclaimed,
    /// Claimed through an unbound invitation; the initiator has not yet
    /// confirmed who it is. No acceptance takes effect in this state.
    Claimed,
    Confirmed,
}

/// The one revision awaiting acceptance. Its author signed it by sending it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Open {
    pub id: RevisionId,
    pub author: Slot,
    pub expires_at: OffsetDateTime,
    /// The other party has accepted, but the acceptance is waiting for the
    /// initiator to confirm the counterparty before it takes effect.
    pub accepted: bool,
    pub revision: Revision,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InForce {
    pub id: RevisionId,
    pub revision: Revision,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct CloseRequest {
    pub by: Slot,
    pub at: OffsetDateTime,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exchange {
    pub state: State,
    pub counterparty: Counterparty,
    pub open: Option<Open>,
    pub in_force: Option<InForce>,
    /// Every contribution that has ever been in force, including removed ones.
    pub statuses: Statuses,
    pub end_proposed_by: Option<Slot>,
    pub close_request: Option<CloseRequest>,
    pub inactivity_prompted_at: Option<OffsetDateTime>,
    /// When a party last did anything.
    pub last_activity_at: OffsetDateTime,
}

impl Exchange {
    pub fn draft(now: OffsetDateTime) -> Self {
        Self {
            state: State::Draft,
            counterparty: Counterparty::Unclaimed,
            open: None,
            in_force: None,
            statuses: Statuses::new(),
            end_proposed_by: None,
            close_request: None,
            inactivity_prompted_at: None,
            last_activity_at: now,
        }
    }

    /// The moment the inactivity clock starts: the later of the last activity
    /// and the last due date. A due date counts until the end of that day in
    /// UTC, which is precise enough for a window measured in weeks.
    pub fn idle_since(&self) -> OffsetDateTime {
        let last_due = self.in_force.iter().flat_map(|in_force| {
            in_force
                .revision
                .contributions
                .iter()
                .filter_map(|c| match c.due {
                    Due::Date(date) => date.next_day().map(|day| day.midnight().assume_utc()),
                    _ => None,
                })
        });
        last_due.fold(self.last_activity_at, OffsetDateTime::max)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Actor {
    Party(Slot),
    /// The worker, acting on a timer.
    System,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Command {
    /// The invited party fills slot B. `pre_bound` means the invitation named
    /// them, so the initiator already knows who they are.
    ClaimCounterparty {
        pre_bound: bool,
    },
    /// The initiator confirms who claimed slot B.
    ConfirmCounterparty,
    /// Send a revision, which signs it. In a draft or negotiation this is a
    /// proposal or counteroffer; on an active exchange it proposes an amendment.
    Send {
        id: RevisionId,
        revision: Revision,
    },
    Accept {
        revision: RevisionId,
    },
    Decline {
        revision: RevisionId,
    },
    Withdraw {
        revision: RevisionId,
    },
    Contribution {
        id: ContributionId,
        action: contribution::Action,
    },
    ProposeEnd,
    /// Agree to end, answering either an end proposal or a close request.
    AcceptEnd,
    CancelEnd,
    RequestClose,
    RetractClose,
    AddStatement,

    // Timers, run by the system.
    ExpireRevision,
    LapseCloseRequest,
    PromptInactivity,
    CloseInactive,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Event {
    CounterpartyClaimed {
        confirmed: bool,
    },
    CounterpartyConfirmed,
    RevisionSent {
        revision: RevisionId,
        by: Slot,
        expires_at: OffsetDateTime,
    },
    /// Replaced by a newer revision before it was accepted.
    RevisionSuperseded {
        revision: RevisionId,
    },
    RevisionAccepted {
        revision: RevisionId,
        by: Slot,
    },
    RevisionDeclined {
        revision: RevisionId,
        by: Slot,
    },
    RevisionWithdrawn {
        revision: RevisionId,
        by: Slot,
    },
    RevisionExpired {
        revision: RevisionId,
    },
    /// A revision became binding: the first agreement, or an amendment.
    /// `statuses` is every contribution's status as a result.
    AgreementInForce {
        revision: RevisionId,
        statuses: Statuses,
    },
    ContributionChanged {
        contribution: ContributionId,
        action: contribution::Action,
        by: Slot,
        status: Status,
    },
    EndProposed {
        by: Slot,
    },
    EndProposalCancelled {
        by: Slot,
    },
    CloseRequested {
        by: Slot,
    },
    CloseRequestRetracted {
        by: Slot,
    },
    StatementAdded {
        by: Slot,
    },
    InactivityPrompted,
    /// `waived` lists contributions released because the parties agreed to end.
    Closed {
        outcome: Outcome,
        waived: Vec<ContributionId>,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum Refusal {
    #[error("this action belongs to someone else")]
    WrongActor,
    #[error("this action is not allowed in the exchange's current state")]
    NotAllowed,
    #[error("the revision named is not the one currently open")]
    StaleRevision,
    #[error("the revision has expired")]
    RevisionExpired,
    #[error("the initiator has not yet confirmed the counterparty")]
    CounterpartyNotConfirmed,
    #[error("contribution {0:?} has been accepted and is locked")]
    ContributionLocked(ContributionId),
    #[error("contribution {0:?} is not part of the agreement in force")]
    UnknownContribution(ContributionId),
    #[error("the revision is not valid")]
    InvalidRevision(Vec<Invalid>),
}

impl From<contribution::Refusal> for Refusal {
    fn from(refusal: contribution::Refusal) -> Self {
        match refusal {
            contribution::Refusal::WrongParty => Refusal::WrongActor,
            contribution::Refusal::NotAllowed => Refusal::NotAllowed,
        }
    }
}

impl From<amendment::Refusal> for Refusal {
    fn from(refusal: amendment::Refusal) -> Self {
        match refusal {
            amendment::Refusal::Locked(id) => Refusal::ContributionLocked(id),
            amendment::Refusal::Invalid(invalid) => Refusal::InvalidRevision(vec![invalid]),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Decision {
    pub events: Vec<Event>,
    pub exchange: Exchange,
}

/// Applies one command. On refusal nothing has changed.
pub fn decide(
    exchange: &Exchange,
    actor: Actor,
    command: Command,
    now: OffsetDateTime,
    rules: &Rules,
) -> Result<Decision, Refusal> {
    let mut step = Step {
        exchange: exchange.clone(),
        events: Vec::new(),
        now,
        rules,
    };

    if matches!(step.exchange.state, State::Closed(_)) {
        return Err(Refusal::NotAllowed);
    }

    match (actor, command) {
        (Actor::System, Command::ExpireRevision) => step.expire_revision()?,
        (Actor::System, Command::LapseCloseRequest) => step.lapse_close_request()?,
        (Actor::System, Command::PromptInactivity) => step.prompt_inactivity()?,
        (Actor::System, Command::CloseInactive) => step.close_inactive()?,
        (Actor::System, _) => return Err(Refusal::WrongActor),

        (Actor::Party(by), command) => {
            match command {
                Command::ClaimCounterparty { pre_bound } => step.claim(by, pre_bound)?,
                Command::ConfirmCounterparty => step.confirm(by)?,
                Command::Send { id, revision } => step.send(by, id, revision)?,
                Command::Accept { revision } => step.accept(by, revision)?,
                Command::Decline { revision } => step.decline(by, revision)?,
                Command::Withdraw { revision } => step.withdraw(by, revision)?,
                Command::Contribution { id, action } => step.contribution(by, id, action)?,
                Command::ProposeEnd => step.propose_end(by)?,
                Command::AcceptEnd => step.accept_end(by)?,
                Command::CancelEnd => step.cancel_end(by)?,
                Command::RequestClose => step.request_close(by)?,
                Command::RetractClose => step.retract_close(by)?,
                Command::AddStatement => step.add_statement(by)?,
                Command::ExpireRevision
                | Command::LapseCloseRequest
                | Command::PromptInactivity
                | Command::CloseInactive => return Err(Refusal::WrongActor),
            }
            // Anything a party does restarts the inactivity clock.
            step.exchange.last_activity_at = now;
            step.exchange.inactivity_prompted_at = None;
        }
    }

    Ok(Decision {
        events: step.events,
        exchange: step.exchange,
    })
}

struct Step<'a> {
    exchange: Exchange,
    events: Vec<Event>,
    now: OffsetDateTime,
    rules: &'a Rules,
}

impl Step<'_> {
    // ---- Counterparty -----------------------------------------------------

    fn claim(&mut self, by: Slot, pre_bound: bool) -> Result<(), Refusal> {
        if by != Slot::B {
            return Err(Refusal::WrongActor);
        }
        if self.exchange.state != State::Negotiating
            || self.exchange.counterparty != Counterparty::Unclaimed
        {
            return Err(Refusal::NotAllowed);
        }
        self.exchange.counterparty = if pre_bound {
            Counterparty::Confirmed
        } else {
            Counterparty::Claimed
        };
        self.events.push(Event::CounterpartyClaimed {
            confirmed: pre_bound,
        });
        Ok(())
    }

    fn confirm(&mut self, by: Slot) -> Result<(), Refusal> {
        if by != Slot::A {
            return Err(Refusal::WrongActor);
        }
        if self.exchange.counterparty != Counterparty::Claimed {
            return Err(Refusal::NotAllowed);
        }
        self.exchange.counterparty = Counterparty::Confirmed;
        self.events.push(Event::CounterpartyConfirmed);

        // An acceptance that was waiting on this confirmation now takes
        // effect, unless the offer ran out in the meantime.
        let waiting = self
            .exchange
            .open
            .as_ref()
            .is_some_and(|open| open.accepted && self.now < open.expires_at);
        if waiting {
            self.bring_into_force()?;
        }
        Ok(())
    }

    // ---- Revisions --------------------------------------------------------

    fn send(&mut self, by: Slot, id: RevisionId, revision: Revision) -> Result<(), Refusal> {
        match self.exchange.state {
            State::Draft if by != Slot::A => return Err(Refusal::WrongActor),
            State::Draft => {}
            State::Negotiating | State::Active => self.require_present(by)?,
            State::Closed(_) => return Err(Refusal::NotAllowed),
        }

        revision::validate(&revision, self.rules).map_err(Refusal::InvalidRevision)?;
        if let Some(in_force) = &self.exchange.in_force {
            amendment::effects(&in_force.revision, &self.exchange.statuses, &revision)?;
        }

        if let Some(previous) = self.exchange.open.take() {
            self.events.push(Event::RevisionSuperseded {
                revision: previous.id,
            });
        }
        if self.exchange.state == State::Draft {
            self.exchange.state = State::Negotiating;
        }

        let expires_at = self.now + self.rules.revision_ttl;
        self.exchange.open = Some(Open {
            id,
            author: by,
            expires_at,
            accepted: false,
            revision,
        });
        self.events.push(Event::RevisionSent {
            revision: id,
            by,
            expires_at,
        });
        Ok(())
    }

    fn accept(&mut self, by: Slot, revision: RevisionId) -> Result<(), Refusal> {
        self.require_present(by)?;
        let open = self.open(revision)?;
        if by == open.author || open.accepted {
            // The author signed by sending; nobody signs twice.
            return Err(Refusal::NotAllowed);
        }
        if self.now >= open.expires_at {
            return Err(Refusal::RevisionExpired);
        }
        // The initiator is never bound to someone they have not confirmed.
        let confirmed = self.exchange.counterparty == Counterparty::Confirmed;
        if by == Slot::A && !confirmed {
            return Err(Refusal::CounterpartyNotConfirmed);
        }

        self.events.push(Event::RevisionAccepted { revision, by });
        if confirmed {
            self.bring_into_force()
        } else {
            self.exchange.open.as_mut().expect("checked above").accepted = true;
            Ok(())
        }
    }

    fn decline(&mut self, by: Slot, revision: RevisionId) -> Result<(), Refusal> {
        self.require_present(by)?;
        let open = self.open(revision)?;
        if by == open.author {
            return Err(Refusal::WrongActor);
        }
        if open.accepted {
            return Err(Refusal::NotAllowed);
        }
        self.events.push(Event::RevisionDeclined { revision, by });
        self.drop_open(NotAgreed::Declined);
        Ok(())
    }

    fn withdraw(&mut self, by: Slot, revision: RevisionId) -> Result<(), Refusal> {
        let open = self.open(revision)?;
        if by != open.author {
            return Err(Refusal::WrongActor);
        }
        self.events.push(Event::RevisionWithdrawn { revision, by });
        self.drop_open(NotAgreed::Withdrawn);
        Ok(())
    }

    fn expire_revision(&mut self) -> Result<(), Refusal> {
        let open = self.exchange.open.as_ref().ok_or(Refusal::NotAllowed)?;
        if self.now < open.expires_at {
            return Err(Refusal::NotAllowed);
        }
        self.events
            .push(Event::RevisionExpired { revision: open.id });
        self.drop_open(NotAgreed::Expired);
        Ok(())
    }

    /// The open revision, provided it is the one the caller named.
    fn open(&self, revision: RevisionId) -> Result<&Open, Refusal> {
        match &self.exchange.open {
            Some(open) if open.id == revision => Ok(open),
            _ => Err(Refusal::StaleRevision),
        }
    }

    /// Slot B cannot act until someone has claimed it.
    fn require_present(&self, by: Slot) -> Result<(), Refusal> {
        if by == Slot::B && self.exchange.counterparty == Counterparty::Unclaimed {
            Err(Refusal::NotAllowed)
        } else {
            Ok(())
        }
    }

    /// An open revision went away without being accepted. During negotiation
    /// that ends the exchange; an amendment just leaves the agreement as it was.
    fn drop_open(&mut self, reason: NotAgreed) {
        self.exchange.open = None;
        if self.exchange.state == State::Negotiating {
            self.close(Outcome::NotAgreed(reason), Vec::new());
        }
    }

    /// Both parties have signed the open revision and the counterparty is
    /// confirmed: it becomes the agreement in force.
    fn bring_into_force(&mut self) -> Result<(), Refusal> {
        let open = self
            .exchange
            .open
            .take()
            .expect("caller checked there is an open revision");

        let statuses = match &self.exchange.in_force {
            // Checked again here because a contribution may have been
            // accepted since the amendment was sent.
            Some(in_force) => {
                amendment::effects(&in_force.revision, &self.exchange.statuses, &open.revision)?
            }
            None => open
                .revision
                .contributions
                .iter()
                .map(|c| (c.id, Status::Pending))
                .collect(),
        };

        self.exchange.state = State::Active;
        self.exchange.statuses = statuses.clone();
        self.exchange.in_force = Some(InForce {
            id: open.id,
            revision: open.revision,
        });
        self.events.push(Event::AgreementInForce {
            revision: open.id,
            statuses,
        });
        self.complete_if_done();
        Ok(())
    }

    // ---- Fulfillment ------------------------------------------------------

    fn contribution(
        &mut self,
        by: Slot,
        id: ContributionId,
        action: contribution::Action,
    ) -> Result<(), Refusal> {
        let in_force = self.active()?;
        let terms = in_force
            .revision
            .contribution(id)
            .ok_or(Refusal::UnknownContribution(id))?;
        let role = if terms.from == by {
            Role::Provider
        } else {
            Role::Recipient
        };
        let current = self.exchange.statuses[&id];

        let status = contribution::transition(current, action, role)?;
        self.exchange.statuses.insert(id, status);
        self.events.push(Event::ContributionChanged {
            contribution: id,
            action,
            by,
            status,
        });
        self.complete_if_done();
        Ok(())
    }

    fn complete_if_done(&mut self) {
        let Some(in_force) = &self.exchange.in_force else {
            return;
        };
        let done = in_force
            .revision
            .contributions
            .iter()
            .filter(|c| c.required)
            .all(|c| {
                matches!(
                    self.exchange.statuses[&c.id],
                    Status::Accepted | Status::Waived
                )
            });
        if done {
            self.close(Outcome::Completed, Vec::new());
        }
    }

    // ---- Ending by agreement ----------------------------------------------

    fn propose_end(&mut self, by: Slot) -> Result<(), Refusal> {
        self.active()?;
        if self.exchange.end_proposed_by.is_some() {
            return Err(Refusal::NotAllowed);
        }
        self.exchange.end_proposed_by = Some(by);
        self.events.push(Event::EndProposed { by });
        Ok(())
    }

    fn accept_end(&mut self, by: Slot) -> Result<(), Refusal> {
        let in_force = self.active()?;
        let other = Some(by.other());
        let invited = self.exchange.end_proposed_by == other
            || self.exchange.close_request.map(|request| request.by) == other;
        if !invited {
            return Err(Refusal::NotAllowed);
        }

        // Whatever is still outstanding is released by both.
        let waived: Vec<ContributionId> = in_force
            .revision
            .contributions
            .iter()
            .map(|c| c.id)
            .filter(|id| {
                matches!(
                    self.exchange.statuses[id],
                    Status::Pending | Status::Claimed | Status::Disputed
                )
            })
            .collect();
        for id in &waived {
            self.exchange.statuses.insert(*id, Status::Waived);
        }
        self.close(Outcome::EndedByAgreement, waived);
        Ok(())
    }

    fn cancel_end(&mut self, by: Slot) -> Result<(), Refusal> {
        self.active()?;
        if self.exchange.end_proposed_by.take().is_none() {
            return Err(Refusal::NotAllowed);
        }
        self.events.push(Event::EndProposalCancelled { by });
        Ok(())
    }

    // ---- Closing without agreement ----------------------------------------

    fn request_close(&mut self, by: Slot) -> Result<(), Refusal> {
        self.active()?;
        if self.exchange.close_request.is_some() {
            return Err(Refusal::NotAllowed);
        }
        self.exchange.close_request = Some(CloseRequest { by, at: self.now });
        self.events.push(Event::CloseRequested { by });
        Ok(())
    }

    fn retract_close(&mut self, by: Slot) -> Result<(), Refusal> {
        match self.exchange.close_request {
            Some(request) if request.by == by => {
                self.exchange.close_request = None;
                self.events.push(Event::CloseRequestRetracted { by });
                Ok(())
            }
            Some(_) => Err(Refusal::WrongActor),
            None => Err(Refusal::NotAllowed),
        }
    }

    fn add_statement(&mut self, by: Slot) -> Result<(), Refusal> {
        if self.exchange.close_request.is_none() {
            return Err(Refusal::NotAllowed);
        }
        self.events.push(Event::StatementAdded { by });
        Ok(())
    }

    fn lapse_close_request(&mut self) -> Result<(), Refusal> {
        let request = self.exchange.close_request.ok_or(Refusal::NotAllowed)?;
        if self.now < request.at + self.rules.close_response_window {
            return Err(Refusal::NotAllowed);
        }
        self.close(Outcome::Unresolved(Unresolved::CloseRequest), Vec::new());
        Ok(())
    }

    fn prompt_inactivity(&mut self) -> Result<(), Refusal> {
        self.active()?;
        if self.exchange.inactivity_prompted_at.is_some()
            || self.now < self.exchange.idle_since() + self.rules.inactivity_prompt_after
        {
            return Err(Refusal::NotAllowed);
        }
        self.exchange.inactivity_prompted_at = Some(self.now);
        self.events.push(Event::InactivityPrompted);
        Ok(())
    }

    fn close_inactive(&mut self) -> Result<(), Refusal> {
        let prompted_at = self
            .exchange
            .inactivity_prompted_at
            .ok_or(Refusal::NotAllowed)?;
        if self.now < prompted_at + self.rules.inactivity_close_after {
            return Err(Refusal::NotAllowed);
        }
        self.close(Outcome::Unresolved(Unresolved::Inactive), Vec::new());
        Ok(())
    }

    // ---- Shared -----------------------------------------------------------

    fn active(&self) -> Result<&InForce, Refusal> {
        match (&self.exchange.state, &self.exchange.in_force) {
            (State::Active, Some(in_force)) => Ok(in_force),
            _ => Err(Refusal::NotAllowed),
        }
    }

    /// Closing voids anything still pending: an unaccepted amendment, an end
    /// proposal, a close request.
    fn close(&mut self, outcome: Outcome, waived: Vec<ContributionId>) {
        if let Some(open) = self.exchange.open.take() {
            self.events
                .push(Event::RevisionSuperseded { revision: open.id });
        }
        self.exchange.state = State::Closed(outcome);
        self.exchange.end_proposed_by = None;
        self.exchange.close_request = None;
        self.exchange.inactivity_prompted_at = None;
        self.events.push(Event::Closed { outcome, waived });
    }
}

#[cfg(test)]
mod tests;
