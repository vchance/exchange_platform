//! Sending the "due soon" and "overdue" reminders (DESIGN.md §12).
//!
//! `domain::reminder` says what a reminder is and when one is due. This is
//! the worker's side: find the exchanges that may need one, and for each,
//! with the exchange locked, record the reminder and queue its messages in
//! one transaction.
//!
//! A reminder is not part of the exchange's history. The history is what the
//! parties did and what became of their agreement, kept for good and handed
//! to them as their record; a reminder is the platform's own message, says
//! nothing the due dates and statuses in that record do not already say, and
//! may never have reached anyone. So no event is written, and the exchange
//! row is locked but not changed: its version stays where it was, and a
//! party's next action is not refused as stale because of an email. With no
//! event there is also nothing for the rate limit or the inactivity clock to
//! count. What is kept is the `contribution_reminder` row, which is what
//! stops the reminder being sent again.

use sqlx::{PgConnection, PgExecutor, PgPool};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::repo;
use crate::domain::Rules;
use crate::domain::reminder::{self, Kind, Reminded, Reminder};
use crate::domain::revision::ContributionId;
use crate::notifications::outbox;

/// The date it is at `at` in a timezone.
///
/// Due dates are plain dates read in the exchange's timezone, and turning an
/// instant into a date there takes a timezone database. The service has none
/// of its own; PostgreSQL does, and it is the one that accepted the
/// exchange's timezone when the exchange was created, so it is asked.
async fn local_date(
    db: impl PgExecutor<'_>,
    timezone: &str,
    at: OffsetDateTime,
) -> Result<Date, sqlx::Error> {
    sqlx::query_scalar("SELECT ($1::timestamptz AT TIME ZONE $2)::date")
        .bind(at)
        .bind(timezone)
        .fetch_one(db)
        .await
}

/// What an exchange's contributions have already been reminded about.
async fn reminded(conn: &mut PgConnection, exchange: Uuid) -> Result<Reminded, sqlx::Error> {
    let rows: Vec<(Uuid, String, Date)> = sqlx::query_as(
        "SELECT contribution_id, kind, due_date FROM contribution_reminder
         WHERE exchange_id = $1",
    )
    .bind(exchange)
    .fetch_all(conn)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(contribution, kind, due)| {
            Some(Reminder {
                contribution: ContributionId(contribution),
                kind: Kind::parse(&kind)?,
                due,
            })
        })
        .collect())
}

/// Sends every reminder that has come due at `at`: records it, and queues
/// the messages for the worker's delivery to send. Returns how many
/// reminders were recorded. Called by the worker, and safe to run from
/// several workers at once.
pub async fn run_reminders(
    db: &PgPool,
    rules: &Rules,
    at: OffsetDateTime,
) -> Result<usize, sqlx::Error> {
    // One timezone at a time, because "today" is a different date in each.
    let timezones: Vec<String> =
        sqlx::query_scalar("SELECT DISTINCT timezone FROM exchange WHERE state = 'ACTIVE'")
            .fetch_all(db)
            .await?;

    let mut recorded = 0;
    for timezone in timezones {
        // A timezone the database cannot read, or a shortlist that fails, is
        // logged and passed over: the exchanges in every other timezone are
        // still reminded.
        let (today, exchanges) = match shortlist(db, rules, &timezone, at).await {
            Ok(found) => found,
            Err(error) => {
                tracing::error!(%error, timezone, "reminders failed for one timezone");
                continue;
            }
        };
        for id in exchanges {
            // Each exchange on a task of its own, as the timers do: one that
            // fails, or even panics, is logged and passed over.
            let (db, rules) = (db.clone(), rules.clone());
            let outcome =
                tokio::spawn(async move { remind(&db, &rules, id, today, at).await }).await;
            match outcome {
                Ok(Ok(count)) => recorded += count,
                Ok(Err(error)) => {
                    tracing::error!(%error, exchange = %id, "reminders failed for one exchange")
                }
                Err(error) => {
                    tracing::error!(%error, exchange = %id, "reminders panicked for one exchange")
                }
            }
        }
    }
    Ok(recorded)
}

/// Today's date in a timezone, and the active exchanges there that may have
/// a reminder to send.
async fn shortlist(
    db: &PgPool,
    rules: &Rules,
    timezone: &str,
    at: OffsetDateTime,
) -> Result<(Date, Vec<Uuid>), sqlx::Error> {
    let today = local_date(db, timezone, at).await?;
    // A day too far ahead to compute is further off than any due date.
    let soon = today.checked_add(rules.due_soon_lead).unwrap_or(Date::MAX);

    // A pending contribution of the agreement in force whose due date is
    // near or past, and which has not had the reminder that goes with that.
    // Only a shortlist: `remind` asks the rules, with the exchange locked.
    let exchanges = sqlx::query_scalar(
        "SELECT DISTINCT e.id
         FROM exchange e
         JOIN contribution_snapshot s
           ON s.revision_id = e.in_force_revision_id AND s.due_kind = 'DATE'
         JOIN contribution c ON c.id = s.contribution_id AND c.status = 'PENDING'
         WHERE e.state = 'ACTIVE' AND e.timezone = $1 AND s.due_date <= $3
           AND NOT EXISTS (
               SELECT 1 FROM contribution_reminder r
               WHERE r.exchange_id = e.id AND r.contribution_id = s.contribution_id
                 AND r.due_date = s.due_date
                 AND r.kind = CASE WHEN s.due_date < $2 THEN 'OVERDUE' ELSE 'DUE_SOON' END)",
    )
    .bind(timezone)
    .bind(today)
    .bind(soon)
    .fetch_all(db)
    .await?;
    Ok((today, exchanges))
}

/// Records and queues the reminders one exchange calls for. Returns how many
/// there were.
async fn remind(
    db: &PgPool,
    rules: &Rules,
    id: Uuid,
    today: Date,
    at: OffsetDateTime,
) -> Result<usize, sqlx::Error> {
    let mut tx = db.begin().await?;
    // Locked, as for every change to an exchange. A claim, a confirmation or
    // an amendment either finished before this and is seen here, or waits
    // until this is done: a reminder is never queued for something that has
    // just been delivered. Another worker reminding the same exchange waits
    // too, and then finds the reminders already recorded.
    let Some(aggregate) = repo::load(&mut tx, id, true).await? else {
        return Ok(0);
    };
    let reminded = reminded(&mut tx, id).await?;
    // The shortlist was only that; the rules have the last word.
    let due = reminder::due(&aggregate.exchange, &reminded, today, rules);
    let Some(agreement) = aggregate
        .exchange
        .in_force
        .as_ref()
        .filter(|_| !due.is_empty())
    else {
        return Ok(0);
    };

    for reminder in &due {
        // The primary key refuses a reminder recorded twice, whatever the
        // code above came to believe.
        sqlx::query(
            "INSERT INTO contribution_reminder
                (exchange_id, contribution_id, kind, due_date, reminded_at)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(id)
        .bind(reminder.contribution.0)
        .bind(reminder.kind.as_str())
        .bind(reminder.due)
        .bind(at)
        .execute(&mut *tx)
        .await?;
    }
    for message in reminder::messages(&agreement.revision, &due) {
        // Both slots of an agreement in force are held, but an account that
        // has since been closed or has no email address is not messaged.
        if let Some(account) = aggregate.account_of(message.to) {
            outbox::enqueue_reminder(&mut tx, id, account, message.notice, &message.contributions)
                .await?;
        }
    }
    // Nothing was written to the exchange row itself: its version, its last
    // activity and its history are as they were.
    tx.commit().await?;
    Ok(due.len())
}

/// Whether a queued reminder of `kind` about `contributions` is still true
/// at `at`. Asked by the outbox just before it sends one.
///
/// Read without a lock: a party who delivers in the instant between this and
/// the send can still get the reminder, as they could with any email.
pub async fn still_true(
    conn: &mut PgConnection,
    exchange: Uuid,
    kind: Kind,
    contributions: &[Uuid],
    at: OffsetDateTime,
) -> Result<bool, sqlx::Error> {
    let Some(aggregate) = repo::load(conn, exchange, false).await? else {
        return Ok(false);
    };
    let today = local_date(&mut *conn, &aggregate.timezone, at).await?;
    let reminded = reminded(conn, exchange).await?;
    Ok(reminded.iter().any(|reminder| {
        reminder.kind == kind
            && contributions.contains(&reminder.contribution.0)
            && reminder::stands(&aggregate.exchange, reminder, today)
    }))
}
