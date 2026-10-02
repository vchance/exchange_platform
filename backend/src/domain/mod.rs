//! Pure domain rules. Nothing in this module touches the database or the
//! network: each state machine is a function from (current state, action,
//! actor) to a new state or a typed refusal (DESIGN.md §13.3).

use time::Duration;

pub mod amendment;
pub mod canonical;
pub mod contribution;
pub mod exchange;
pub mod identity;
pub mod invitation;
pub mod revision;
pub mod risk;

/// The tunable numbers the rules depend on. They are configuration, not
/// constants: the defaults are the decisions recorded in DESIGN.md.
#[derive(Clone, Debug)]
pub struct Rules {
    /// How long a sent revision stays open for acceptance (§6).
    pub revision_ttl: Duration,
    /// How long the other party has to answer a close request (§5.3).
    pub close_response_window: Duration,
    /// Idle time after which both parties are prompted (§5.3).
    pub inactivity_prompt_after: Duration,
    /// Time after the prompt at which an idle exchange closes (§5.3).
    pub inactivity_close_after: Duration,
    /// Longest note on a revision, in characters (§6).
    pub note_max_chars: usize,
    /// Total money in an agreement above which stronger verification is
    /// required, in minor units (§8).
    pub tier_one_threshold_minor: i64,
    /// How recent a one-time code must be to count as fresh at signing (§8).
    pub fresh_code_window: Duration,
    /// How long an invitation link can be claimed (§8).
    pub invitation_ttl: Duration,
    /// Exchanges one account may create per day (§9). A placeholder.
    pub exchanges_per_day: i64,
}

impl Default for Rules {
    fn default() -> Self {
        Self {
            revision_ttl: Duration::days(14),
            close_response_window: Duration::days(7),
            inactivity_prompt_after: Duration::days(60),
            inactivity_close_after: Duration::days(30),
            note_max_chars: 1000,
            tier_one_threshold_minor: 50_000,
            fresh_code_window: Duration::minutes(10),
            invitation_ttl: Duration::days(14),
            exchanges_per_day: 20,
        }
    }
}
