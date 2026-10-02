//! The transactional outbox for notifications (DESIGN.md §13, §13.2).
//!
//! [`enqueue`] runs inside the transaction that records an event, and
//! [`enqueue_reminder`] inside the one that records a reminder.
//! [`deliver_due`] is the worker's side: claim a message, send it, record how
//! that went.
//!
//! What the columns say about a message:
//!
//! * `completed_at` empty and `attempts` below the limit: waiting, not before
//!   `available_at`;
//! * `completed_at` empty and `attempts` at the limit: given up on, and left
//!   as it is for someone to look at, with the last failure in `last_error`;
//! * `completed_at` set and `last_error` empty: sent;
//! * `completed_at` set and `last_error` set: closed without sending, because
//!   there was no longer anyone to send it to or, for a reminder, because
//!   what it said had stopped being true.

use std::sync::Arc;

use serde::Deserialize;
use serde_json::{Value, json};
use sqlx::{Connection, PgConnection, PgPool};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

use super::wording::{Links, Wording};
use super::{Email, EmailSender};
use crate::domain::notification::Notice;
use crate::domain::reminder;
use crate::domain::revision::ContributionId;
use crate::exchanges::reminders;

/// The numbers behind delivery. Placeholders: none of these is a recorded
/// design decision yet.
#[derive(Clone, Debug)]
pub struct DeliveryRules {
    /// Sends tried before a message is given up on.
    pub max_attempts: i32,
    /// The wait after a first failed send. Each further failure doubles it.
    pub retry_after: Duration,
    /// The longest wait between two tries.
    pub retry_ceiling: Duration,
    /// How long one send may take before it counts as failed.
    pub send_timeout: std::time::Duration,
    /// Messages one pass delivers at most, so a backlog cannot keep the
    /// worker from its other jobs.
    pub batch: usize,
}

impl Default for DeliveryRules {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            retry_after: Duration::minutes(1),
            retry_ceiling: Duration::hours(1),
            send_timeout: std::time::Duration::from_secs(30),
            batch: 100,
        }
    }
}

impl DeliveryRules {
    /// How long to wait after a failure, given the failures before it.
    fn backoff(&self, earlier_failures: i32) -> Duration {
        // Past the ceiling long before the shift could overflow.
        let doublings = earlier_failures.clamp(0, 20);
        let wait: Duration = self.retry_after * (1_i32 << doublings);
        wait.min(self.retry_ceiling)
    }
}

/// What the worker needs to deliver notifications.
pub struct Delivery {
    pub sender: Arc<dyn EmailSender>,
    pub wording: Wording,
    /// Where the web app is served from; messages link into it.
    pub web_origin: String,
    pub rules: DeliveryRules,
}

/// Why a message was closed unsent when its recipient was removed from the
/// exchange it is about, or left it.
pub const NO_LONGER_A_PARTY: &str = "not sent: the recipient is no longer a party to the exchange";

// ---- Writing ----------------------------------------------------------------

/// Queues an email telling `recipient` about an event. Call it in the
/// transaction that records the event, so the two are stored or lost together.
///
/// Only the kind of message is stored. The address, the language and the
/// text are read when it is sent, so a queued message holds nothing personal
/// and nothing from the agreement.
///
/// An account that cannot be emailed gets no row: one with only a phone
/// number (SMS carries one-time codes and nothing else, §12), or one that is
/// suspended or deleted.
pub async fn enqueue(
    conn: &mut PgConnection,
    exchange: Uuid,
    event_sequence: i64,
    recipient: Uuid,
    notice: Notice,
) -> Result<(), sqlx::Error> {
    let payload = json!({ "notice": notice.as_str() });
    insert(conn, exchange, Some(event_sequence), recipient, payload).await
}

/// Queues an email reminding `recipient` of contributions that are due soon
/// or overdue. Call it in the transaction that records the reminder, which
/// is what keeps it from being queued twice: no event caused it, so the
/// outbox's own key (one message per event per person) does not apply.
///
/// The contributions are stored by ID, with nothing they say. They are there
/// so that the message can be checked again when it is sent, which may be
/// after one of them was delivered.
///
/// As with [`enqueue`], an account that cannot be emailed gets no row.
pub async fn enqueue_reminder(
    conn: &mut PgConnection,
    exchange: Uuid,
    recipient: Uuid,
    notice: Notice,
    contributions: &[ContributionId],
) -> Result<(), sqlx::Error> {
    let contributions: Vec<Uuid> = contributions.iter().map(|id| id.0).collect();
    let payload = json!({ "notice": notice.as_str(), "contributions": contributions });
    insert(conn, exchange, None, recipient, payload).await
}

async fn insert(
    conn: &mut PgConnection,
    exchange: Uuid,
    event_sequence: Option<i64>,
    recipient: Uuid,
    payload: Value,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "INSERT INTO outbox (kind, recipient_account_id, exchange_id, event_sequence, payload)
         SELECT 'EMAIL', id, $2, $3, $4 FROM account
         WHERE id = $1 AND status = 'ACTIVE' AND email IS NOT NULL",
    )
    .bind(recipient)
    .bind(exchange)
    .bind(event_sequence)
    .bind(payload)
    .execute(conn)
    .await?;
    Ok(())
}

// ---- Delivering -------------------------------------------------------------

/// What one pass over the outbox did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delivered {
    pub sent: usize,
    /// Sends that failed, including those counted in `given_up`.
    pub failed: usize,
    /// Failures that were the last try.
    pub given_up: usize,
    /// Closed unsent: the recipient could no longer be emailed, or a
    /// reminder was no longer true.
    pub dropped: usize,
}

impl Delivered {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

enum Attempt {
    Sent,
    Failed(String),
    Dropped(&'static str),
}

/// A row taken for sending: id, recipient, exchange, payload, and the tries
/// made before this one.
type Claimed = (i64, Option<Uuid>, Option<Uuid>, Value, i32);

/// Sends every email that is due at `at`, up to the batch size. Safe to run
/// from several workers at once.
pub async fn deliver_due(
    db: &PgPool,
    delivery: &Delivery,
    at: OffsetDateTime,
) -> Result<Delivered, sqlx::Error> {
    let mut delivered = Delivered::default();
    for _ in 0..delivery.rules.batch {
        if !deliver_next(db, delivery, at, &mut delivered).await? {
            break;
        }
    }
    Ok(delivered)
}

/// Delivers one message, if any is due. Returns whether there was one.
async fn deliver_next(
    db: &PgPool,
    delivery: &Delivery,
    at: OffsetDateTime,
    delivered: &mut Delivered,
) -> Result<bool, sqlx::Error> {
    let rules = &delivery.rules;
    // One message per transaction, and the row stays locked while it is
    // sent. Another worker skips a locked row, so no two send the same one,
    // and a worker that dies mid-send lets go of it for the next to pick up.
    // The cost is one connection held for the length of a send, which the
    // timeout bounds.
    let mut tx = db.begin().await?;
    let claimed: Option<Claimed> = sqlx::query_as(
        "SELECT id, recipient_account_id, exchange_id, payload, attempts FROM outbox
         WHERE kind = 'EMAIL' AND completed_at IS NULL AND available_at <= $1 AND attempts < $2
         ORDER BY available_at, id
         LIMIT 1
         FOR UPDATE SKIP LOCKED",
    )
    .bind(at)
    .bind(rules.max_attempts)
    .fetch_optional(&mut *tx)
    .await?;
    let Some((id, recipient, exchange, payload, attempts)) = claimed else {
        return Ok(false);
    };

    let row = Row {
        id,
        recipient,
        exchange,
        payload: &payload,
    };
    let attempt = match prepare(&mut tx, delivery, row, at).await? {
        Ok(email) => {
            match tokio::time::timeout(rules.send_timeout, delivery.sender.send(&email)).await {
                Ok(Ok(())) => Attempt::Sent,
                Ok(Err(error)) => Attempt::Failed(format!("{error:#}")),
                Err(_) => Attempt::Failed(format!("no answer within {:?}", rules.send_timeout)),
            }
        }
        Err(attempt) => attempt,
    };

    match attempt {
        Attempt::Sent => {
            sqlx::query(
                "UPDATE outbox SET completed_at = $2, attempts = attempts + 1, last_error = NULL
                 WHERE id = $1",
            )
            .bind(id)
            .bind(at)
            .execute(&mut *tx)
            .await?;
            delivered.sent += 1;
        }
        Attempt::Failed(error) => {
            // Kept short: a provider's error can be a whole page.
            let error: String = error.chars().take(500).collect();
            sqlx::query(
                "UPDATE outbox SET attempts = attempts + 1, last_error = $2, available_at = $3
                 WHERE id = $1",
            )
            .bind(id)
            .bind(&error)
            .bind(at + rules.backoff(attempts))
            .execute(&mut *tx)
            .await?;
            delivered.failed += 1;
            if attempts + 1 >= rules.max_attempts {
                delivered.given_up += 1;
                tracing::error!(outbox = id, error, "notification given up on");
            } else {
                tracing::warn!(outbox = id, error, "notification not sent; will retry");
            }
        }
        Attempt::Dropped(reason) => {
            sqlx::query("UPDATE outbox SET completed_at = $2, last_error = $3 WHERE id = $1")
                .bind(id)
                .bind(at)
                .bind(reason)
                .execute(&mut *tx)
                .await?;
            delivered.dropped += 1;
        }
    }
    tx.commit().await?;
    Ok(true)
}

/// What a claimed row holds.
struct Row<'a> {
    id: i64,
    recipient: Option<Uuid>,
    exchange: Option<Uuid>,
    payload: &'a Value,
}

/// Builds the email for a claimed row, or says why there is none to send.
async fn prepare(
    conn: &mut PgConnection,
    delivery: &Delivery,
    row: Row<'_>,
    at: OffsetDateTime,
) -> Result<Result<Email, Attempt>, sqlx::Error> {
    let payload = row.payload;
    // A row this build cannot read may have been written by a newer one, so
    // it is retried like any failure and, at worst, left for inspection.
    let unreadable = || Attempt::Failed(format!("unreadable payload: {payload}"));
    let Some(notice) = payload["notice"].as_str().and_then(Notice::parse) else {
        return Ok(Err(unreadable()));
    };
    let code: Option<String> =
        sqlx::query_scalar("SELECT display_code FROM exchange WHERE id = $1")
            .bind(row.exchange)
            .fetch_optional(&mut *conn)
            .await?;
    let (Some(exchange), Some(code)) = (row.exchange, code) else {
        return Ok(Err(Attempt::Failed("no such exchange".to_owned())));
    };

    // Read now, not when the message was queued: the person may have changed
    // their address or language since, or left.
    let account: Option<(Option<String>, String)> =
        sqlx::query_as("SELECT email, language FROM account WHERE id = $1 AND status = 'ACTIVE'")
            .bind(row.recipient)
            .fetch_optional(&mut *conn)
            .await?;
    let Some((Some(to), language)) = account else {
        return Ok(Err(Attempt::Dropped(
            "not sent: the recipient can no longer be emailed",
        )));
    };

    // A message about an event stays true: the event happened. A reminder
    // says how things stand, and they may have changed while it waited, most
    // of all when earlier tries failed. One that is no longer true is not
    // sent.
    if let Some(kind) = reminder::Kind::of(notice) {
        let Ok(contributions) = Vec::<Uuid>::deserialize(&payload["contributions"]) else {
            return Ok(Err(unreadable()));
        };
        // In a savepoint, so that a failure here (an exchange whose timezone
        // the database cannot read, say) is this one message's failure, to be
        // retried and in the end given up on, and not the end of the pass.
        let mut check = conn.begin().await?;
        let stands = reminders::still_true(&mut check, exchange, kind, &contributions, at).await;
        match stands {
            Ok(stands) => {
                check.commit().await?;
                if !stands {
                    return Ok(Err(Attempt::Dropped(
                        "not sent: what the reminder said is no longer true",
                    )));
                }
            }
            Err(error) => {
                check.rollback().await?;
                return Ok(Err(Attempt::Failed(format!(
                    "the reminder could not be checked: {error}"
                ))));
            }
        }
    }

    let link = format!("{}/exchanges/{exchange}", delivery.web_origin);
    let links = Links {
        exchange: &link,
        record: &format!("{link}/record"),
    };
    let rendered = delivery.wording.email(&language, notice, &code, links);
    Ok(Ok(Email {
        to,
        subject: rendered.subject,
        body: rendered.body,
        reference: row.id,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_wait_doubles_with_each_failure_up_to_the_ceiling() {
        let rules = DeliveryRules {
            retry_after: Duration::minutes(1),
            retry_ceiling: Duration::minutes(10),
            ..DeliveryRules::default()
        };
        let waits: Vec<i64> = (0..6)
            .map(|failures| rules.backoff(failures).whole_minutes())
            .collect();
        assert_eq!(waits, [1, 2, 4, 8, 10, 10]);
        assert_eq!(rules.backoff(i32::MAX), Duration::minutes(10));
    }
}
