//! What a pass shows: one model, the same for both wallets, made by a pure
//! function from the exchange as its party sees it (DESIGN.md §11). Apple's
//! and Google's passes are both drawn from it.
//!
//! The lock-screen rule (§11, §12) decides what may be on it. A wallet shows
//! a pass's face without asking anyone to unlock the phone, so the face holds
//! nothing from the agreement: no terms, no amount, no description of what
//! anyone gives, no name the parties wrote. It holds the product's name, the
//! exchange's display code, how the agreement stands in a few generic words,
//! how many contributions are still open, the next due date, and the other
//! party's alias when one is set; never the display name in its place. The
//! back says what the pass is, with the link to the exchange, which asks its
//! reader to sign in.
//!
//! Changes to a pass are silent. Neither platform is asked to announce one on
//! the lock screen: the notifications already tell the person about the
//! event, once, and a second alert from the pass is what §12 says to avoid.
//! Showing the status on the lock screen is the person's choice to make in
//! the design (opt-in); nothing offers that choice yet, so it is never shown
//! there.

use serde::Serialize;
use time::Date;

use super::StatusOnFace;
use super::wording::{Language, fill};
use crate::domain::contribution::Status;
use crate::exchanges::dto::{DueDto, ExchangeView, OutcomeDto, StateDto};

/// How the agreement stands for the person holding the pass, most pressing
/// first.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
pub enum Standing {
    /// Something waits for the holder: the other party marked something
    /// delivered to them, proposed a change, proposed ending, or asked to
    /// close.
    WaitingForYou,
    /// A contribution is disputed.
    Disputed,
    /// A contribution still open was due before today.
    Overdue,
    /// A contribution still open is due within the reminder lead.
    DueSoon,
    /// In force, nothing pressing.
    InForce,
    Completed,
    Ended,
    /// Closed unresolved, or anything else that is over.
    Closed,
    /// The pass was revoked (its holder deleted their account).
    Void,
}

/// One labelled value on the pass.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Field {
    /// Stable across languages and versions, so a platform can tell which
    /// field changed.
    pub key: &'static str,
    pub label: String,
    pub value: String,
}

/// The pass, ready for a platform to draw.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct PassModel {
    pub language: &'static str,
    pub product: String,
    /// What the pass is, for screen readers.
    pub description: String,
    pub standing: Standing,
    pub status: Field,
    /// The display code. Absent on a void pass.
    pub reference: Option<Field>,
    pub next_due: Option<Field>,
    pub outstanding: Option<Field>,
    pub other_party: Option<Field>,
    pub closed_on: Option<Field>,
    /// On the back: the link to the exchange. Absent on a void pass.
    pub link: Option<Link>,
    /// On the back: what the pass is.
    pub note: String,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize)]
pub struct Link {
    pub label: String,
    pub url: String,
}

impl PassModel {
    pub fn voided(&self) -> bool {
        self.standing == Standing::Void
    }

    /// Whether the exchange is over, which both platforms show apart from a
    /// live pass.
    pub fn finished(&self) -> bool {
        matches!(
            self.standing,
            Standing::Completed | Standing::Ended | Standing::Closed
        )
    }
}

/// What the face needs besides the exchange view.
#[derive(Clone, Debug)]
pub struct PassContext {
    /// Today in the exchange's timezone, which its due dates are read in.
    pub today: Date,
    /// The day it closed, in the exchange's timezone.
    pub closed_on: Option<Date>,
    /// The other party's alias, if one is set. Their display name is never
    /// used in its place.
    pub other_party_alias: Option<String>,
    /// The link to the exchange.
    pub link: String,
    /// How many days before its date a contribution is due soon, the same
    /// lead the reminders use (`Rules::due_soon_lead`).
    pub due_soon_days: i64,
    /// How much the status line says (`WALLET_STATUS_ON_FACE`).
    pub status_on_face: StatusOnFace,
}

/// The face of `view`'s pass for the party viewing it, in `language`.
pub fn render(view: &ExchangeView, context: &PassContext, language: &Language) -> PassModel {
    let labels = &language.labels;
    let you = view.you;
    let terms = view
        .in_force_revision
        .as_ref()
        .map(|revision| &revision.terms);
    let status_of = |id| {
        view.contributions
            .iter()
            .find(|item| item.id == id)
            .map(|item| item.status)
            .unwrap_or(Status::Pending)
    };
    let open =
        |status: Status| matches!(status, Status::Pending | Status::Claimed | Status::Disputed);
    let contributions = terms.map(|terms| &terms.contributions[..]).unwrap_or(&[]);
    let still_open: Vec<_> = contributions
        .iter()
        .filter(|item| open(status_of(item.id)))
        .collect();
    let next_due = still_open
        .iter()
        .filter_map(|item| match &item.due {
            DueDto::Date { date } => parse_date(date),
            _ => None,
        })
        .min();

    let standing = match (view.state, view.closed_outcome) {
        (StateDto::Closed, Some(OutcomeDto::Completed)) => Standing::Completed,
        (StateDto::Closed, Some(OutcomeDto::EndedByAgreement)) => Standing::Ended,
        (StateDto::Active, _) => {
            let other = you.other();
            let delivered_to_you = contributions
                .iter()
                .any(|item| item.from == other && status_of(item.id) == Status::Claimed);
            let change_for_you = view
                .open_revision
                .as_ref()
                .is_some_and(|revision| revision.author == other);
            let waiting = delivered_to_you
                || change_for_you
                || view.end_proposed_by == Some(other)
                || view.close_requested_by == Some(other);
            let disputed = still_open
                .iter()
                .any(|item| status_of(item.id) == Status::Disputed);
            let pending_dates = || {
                still_open.iter().filter_map(|item| match &item.due {
                    DueDto::Date { date } if status_of(item.id) == Status::Pending => {
                        parse_date(date)
                    }
                    _ => None,
                })
            };
            let overdue = pending_dates().any(|due| due < context.today);
            let due_soon = pending_dates().any(|due| {
                (due - context.today).whole_days() <= context.due_soon_days && due >= context.today
            });
            if waiting {
                Standing::WaitingForYou
            } else if disputed {
                Standing::Disputed
            } else if overdue {
                Standing::Overdue
            } else if due_soon {
                Standing::DueSoon
            } else {
                Standing::InForce
            }
        }
        // A pass is only issued once something was agreed; anything else
        // that is over reads as closed.
        _ => Standing::Closed,
    };
    let neutral = context.status_on_face == StatusOnFace::Neutral;
    // Neutral: an agreement in force reads as in force, whatever presses.
    let standing = match standing {
        Standing::WaitingForYou | Standing::Disputed | Standing::Overdue | Standing::DueSoon
            if neutral =>
        {
            Standing::InForce
        }
        standing => standing,
    };

    let words = &language.status;
    let status_word = match standing {
        Standing::WaitingForYou => &words.waiting_for_you,
        Standing::Disputed => &words.disputed,
        Standing::Overdue => &words.overdue,
        Standing::DueSoon => &words.due_soon,
        Standing::InForce => &words.in_force,
        Standing::Completed => &words.completed,
        Standing::Ended => &words.ended,
        Standing::Closed => &words.closed,
        Standing::Void => &words.void,
    };
    let active = view.state == StateDto::Active;
    let product = language.product_name.as_str();

    PassModel {
        language: language.code,
        product: product.to_owned(),
        description: fill(
            &labels.description,
            &[("productName", product), ("code", &view.display_code)],
        ),
        standing,
        status: field("status", &labels.status, status_word),
        reference: Some(field("reference", &labels.reference, &view.display_code)),
        next_due: next_due
            .filter(|_| active && !neutral)
            .map(|due| field("next-due", &labels.next_due, &language.date(due))),
        outstanding: (active && !still_open.is_empty()).then(|| {
            field(
                "outstanding",
                &labels.outstanding,
                &still_open.len().to_string(),
            )
        }),
        other_party: context
            .other_party_alias
            .as_deref()
            .map(str::trim)
            .filter(|alias| !alias.is_empty())
            .map(|alias| field("with", &labels.with, alias)),
        closed_on: context
            .closed_on
            .filter(|_| !active)
            .map(|day| field("closed-on", &labels.closed_on, &language.date(day))),
        link: Some(Link {
            label: labels.open.clone(),
            url: context.link.clone(),
        }),
        note: fill(&labels.note, &[("productName", product)]),
    }
}

/// The face of a revoked pass: it says it is no longer in use and nothing
/// else, not even which exchange it was for: a pass on a device cannot be
/// taken back, so revoking it voids it (DESIGN.md §11).
pub fn void(language: &Language) -> PassModel {
    let product = language.product_name.as_str();
    PassModel {
        language: language.code,
        product: product.to_owned(),
        description: product.to_owned(),
        standing: Standing::Void,
        status: field("status", &language.labels.status, &language.status.void),
        reference: None,
        next_due: None,
        outstanding: None,
        other_party: None,
        closed_on: None,
        link: None,
        note: language.labels.void.clone(),
    }
}

fn field(key: &'static str, label: &str, value: &str) -> Field {
    Field {
        key,
        label: label.to_owned(),
        value: value.to_owned(),
    }
}

/// A due date as the view carries it, `YYYY-MM-DD`.
fn parse_date(text: &str) -> Option<Date> {
    let mut parts = text.splitn(3, '-');
    let year = parts.next()?.parse().ok()?;
    let month = time::Month::try_from(parts.next()?.parse::<u8>().ok()?).ok()?;
    let day = parts.next()?.parse().ok()?;
    Date::from_calendar_date(year, month, day).ok()
}

#[cfg(test)]
mod tests;
