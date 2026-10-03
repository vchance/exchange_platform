//! Deleting an account (DESIGN.md §4.1, §14): the account stops existing as
//! something a person can sign in to or be reached through.
//!
//! What is deleted is the account and its working data. What the person
//! wrote into agreements, and signed, is history that the other party keeps:
//! revisions, signatures and events are not touched here, and the database
//! would refuse it (§13.2). What must become of that text is a separate
//! question and deliberately not answered in this module.
//!
//! **The account.** The row stays, marked `DELETED`, because history refers
//! to it. It loses its email address and phone number, so the same person
//! can sign up again as a new account that has nothing to do with this one,
//! and its display name and language, which are profile data and appear in
//! no agreement. It keeps when it was created and when its holder confirmed
//! being an adult: the second is what the signatures it gave rest on.
//!
//! **Its working data** is removed: every session, on every device; the
//! devices registered for its push notifications; codes sent to its
//! identifiers; unsent working copies of terms; idempotency keys; queued
//! notifications to it; the blocks it made. Invitation links it issued
//! that nobody took are revoked, and forget whom they were for. A block
//! someone else made against it stays theirs to remove. Reports stay, both
//! those it made and those about it: leaving must not erase a complaint.
//!
//! **Its exchanges.** Whatever must happen to an exchange happens through the
//! rules, as commands in the departing party's name, recorded and notified
//! like any other (the same way a block ends open offers, `crate::safety`):
//!
//! * A draft never sent has no history and nobody else in it. Its working
//!   copy is deleted, the name on it blanked, and the draft is discarded in
//!   the departing account's name (`Command::Discard`), exactly as its
//!   initiator could have done: it closes with nothing agreed, reason
//!   `DISCARDED`, and is nobody's to open. The row is not deleted. The
//!   service's role may not delete an exchange or a participant, and the
//!   record of who held a slot cannot be deleted by any role (migration
//!   0006), so deleting the shell would mean loosening the append-only design
//!   for the one case where closing it through the rules does the same job.
//!   What becomes of never-agreed exchanges after that is the retention
//!   question (DESIGN.md §18), which covers these with all the others.
//! * In a negotiation, the offer on the table is withdrawn if the departing
//!   party sent it and declined if the other did, which closes the exchange
//!   with nothing agreed. The one exception is someone who had opened an
//!   invitation and not yet been confirmed by its sender (§8): they can
//!   neither decline nor take a signature back, so they leave the exchange
//!   instead. Whatever they signed is void, the offer stays open, and its
//!   sender is told and can invite someone else.
//! * On an agreement in force, an amendment still waiting is withdrawn or
//!   declined in the same way, and then the departing party asks to close
//!   unresolved (§5.3), unless a request to close is already pending. The
//!   other party is told, has the usual window to confirm what they received,
//!   waive, or add their statement, and the worker then closes the exchange
//!   with every contribution keeping the status it had. Deleting an account
//!   is not refused because of what is outstanding: the platform records and
//!   does not enforce (§3, invariant 6), holding the account open would give
//!   the other party nothing, and the agreement and its record stay with
//!   them whatever the account does. Nothing is waived or accepted on the
//!   departing party's behalf.
//! * A closed exchange is left exactly as it is.
//!
//! The other party is never left waiting on someone who cannot answer: every
//! exchange above either closes at once or is on a timer that closes it
//! (invariant 5), and while it is still open the exchange view tells them
//! that the other party is no longer here (`ExchangeView::other_party_left`).
//!
//! **The deletion log.** The same transaction adds the account's ID and the
//! time to `deletion_log` (migration 0015), and nothing else about it. A
//! backup restored later would bring the account back; the log, exported
//! beside every backup, is what lets `replay` delete it again, through this
//! same code (docs/operations.md, "Restoring").

use serde::{Deserialize, Serialize};
use sqlx::{PgConnection, PgPool};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use crate::auth::{CodeCheck, OfferedCode};
use crate::domain::Rules;
use crate::domain::exchange::{Actor, Command, Counterparty, State, decide};
use crate::domain::identity::Identifier;
use crate::domain::revision::Slot;
use crate::error::{ApiError, ErrorCode};
use crate::exchanges::repo;
use crate::{languages, wallet};

/// Which of the account's identifiers a code is sent to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CodeChannel {
    Email,
    Phone,
}

/// What deleting the account would do to the exchanges it is in, counted, so
/// that the person can be told before they confirm.
#[derive(Debug, PartialEq, Eq, Serialize, ToSchema)]
pub struct DeletionPreview {
    /// Drafts never sent. They are discarded.
    pub drafts: i64,
    /// Negotiations with an offer on the table. They end with nothing agreed.
    pub open_proposals: i64,
    /// Agreements in force. Each gets a request to close, in the account's name.
    pub agreements_in_force: i64,
}

/// The account's own email address or phone number, to send a code to or to
/// check one against. The client never names the address: the proof asked
/// for is control of an identifier the account already has.
pub async fn identifier(
    db: &PgPool,
    account: Uuid,
    channel: CodeChannel,
) -> Result<Identifier, ApiError> {
    let found: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT status, email, phone FROM account WHERE id = $1")
            .bind(account)
            .fetch_optional(db)
            .await?;
    let (email, phone) = match found {
        Some((status, email, phone)) if status == "ACTIVE" => (email, phone),
        // Deleted a moment ago by another request.
        _ => return Err(ErrorCode::Unauthenticated.into()),
    };
    match channel {
        CodeChannel::Email => email.map(Identifier::Email),
        CodeChannel::Phone => phone.map(Identifier::Phone),
    }
    .ok_or_else(|| ErrorCode::InvalidRequest.into())
}

pub async fn preview(db: &PgPool, account: Uuid) -> Result<DeletionPreview, ApiError> {
    let (drafts, open_proposals, agreements_in_force): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE e.state = 'DRAFT'),
                count(*) FILTER (WHERE e.state = 'NEGOTIATING'),
                count(*) FILTER (WHERE e.state = 'ACTIVE')
         FROM participant p JOIN exchange e ON e.id = p.exchange_id
         WHERE p.account_id = $1",
    )
    .bind(account)
    .fetch_one(db)
    .await?;
    Ok(DeletionPreview {
        drafts,
        open_proposals,
        agreements_in_force,
    })
}

/// How often the whole deletion is tried before giving up, and the first
/// wait between tries, which doubles.
const ATTEMPTS: u32 = 6;
const FIRST_WAIT: std::time::Duration = std::time::Duration::from_millis(20);

enum Attempt {
    Done,
    /// Another transaction held the account row; nothing was changed, and
    /// the code offered with the request, if any, was not used up.
    Busy,
}

/// Deletes the account once the one-time code offered for it checks out.
///
/// The code is checked and used up in the transaction that deletes the
/// account (`auth::OfferedCode::check`), so the two stand or fall together.
/// When the account is busy and the deletion gives up (`SERVICE_UNAVAILABLE`),
/// the code is still live and the person can try again with it. A wrong code
/// is charged, committed and refused at once, before anything is tried, so
/// trying again costs a guess like any other: only a code that matched is
/// ever checked more than once.
///
/// Everything happens in one transaction, so an account is never half
/// deleted.
pub async fn delete_account_with_code(
    db: &PgPool,
    rules: &Rules,
    account: Uuid,
    code: &OfferedCode<'_>,
) -> Result<(), ApiError> {
    retry(db, rules, account, Some(code), None).await
}

/// Deletes the account, for a caller that has confirmed it some other way.
/// Deleting an account that is already deleted changes nothing and succeeds:
/// that is what a repeat finds.
pub async fn delete_account(db: &PgPool, rules: &Rules, account: Uuid) -> Result<(), ApiError> {
    retry(db, rules, account, None, None).await
}

/// What `replay` found and did for one account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Replayed {
    /// It was live in this database, and is now deleted.
    Deleted,
    /// It was deleted here already. Its line in the log is there too.
    AlreadyDeleted,
    /// This database never held it: it was made after the backup.
    NotHere,
    /// It is suspended here, and a suspended account is not deleted
    /// (`attempt`). Left as it is, for a person to look at.
    Suspended,
}

/// Applies again, to a database restored from a backup, a deletion that the
/// log says happened at `deleted_at` (`replay-deletions`, docs/operations.md,
/// "Restoring"). It deletes through the same code as the person did, so
/// every rule runs again: sessions end, identifiers go, exchanges are left
/// through the rules. Its line in the log keeps the time it first happened.
/// Replaying an account twice changes nothing the second time.
pub async fn replay(
    db: &PgPool,
    rules: &Rules,
    account: Uuid,
    deleted_at: OffsetDateTime,
) -> Result<Replayed, ApiError> {
    let status: Option<String> = sqlx::query_scalar("SELECT status FROM account WHERE id = $1")
        .bind(account)
        .fetch_optional(db)
        .await?;
    match status.as_deref() {
        None => Ok(Replayed::NotHere),
        Some("SUSPENDED") => Ok(Replayed::Suspended),
        Some("DELETED") => {
            // Deleted before the backup, whose log already says so; a
            // database older than the log may not.
            log_deletion(db, account, Some(deleted_at)).await?;
            Ok(Replayed::AlreadyDeleted)
        }
        Some(_) => {
            retry(db, rules, account, None, Some(deleted_at)).await?;
            Ok(Replayed::Deleted)
        }
    }
}

/// Adds the account to the deletion log, unless it is there already.
async fn log_deletion<'c, E>(
    conn: E,
    account: Uuid,
    deleted_at: Option<OffsetDateTime>,
) -> Result<(), sqlx::Error>
where
    E: sqlx::PgExecutor<'c>,
{
    sqlx::query(
        "INSERT INTO deletion_log (account_id, deleted_at) VALUES ($1, coalesce($2, now()))
         ON CONFLICT (account_id) DO NOTHING",
    )
    .bind(account)
    .bind(deleted_at)
    .execute(conn)
    .await?;
    Ok(())
}

async fn retry(
    db: &PgPool,
    rules: &Rules,
    account: Uuid,
    code: Option<&OfferedCode<'_>>,
    logged_at: Option<OffsetDateTime>,
) -> Result<(), ApiError> {
    let mut wait = FIRST_WAIT;
    for _ in 0..ATTEMPTS {
        match attempt(db, rules, account, code, logged_at).await? {
            Attempt::Done => return Ok(()),
            Attempt::Busy => {
                tokio::time::sleep(wait).await;
                wait *= 2;
            }
        }
    }
    Err(ErrorCode::ServiceUnavailable.into())
}

/// `logged_at` is the time for the deletion log: now, unless this replays a
/// deletion that happened earlier.
async fn attempt(
    db: &PgPool,
    rules: &Rules,
    account: Uuid,
    code: Option<&OfferedCode<'_>>,
    logged_at: Option<OffsetDateTime>,
) -> Result<Attempt, ApiError> {
    let mut tx = db.begin().await?;

    // The code before anything else, so that a refusal is recorded without
    // waiting on the account. It is used up only if this transaction
    // commits; a busy attempt rolls back and leaves it live.
    if let Some(code) = code
        && let CodeCheck::Refused(error) = code.check(&mut tx).await?
    {
        // The wrong guess is counted, as for any code.
        tx.commit().await?;
        return Err(error);
    }

    // The account row first, held to the end. Whatever the account is doing
    // from another device at this moment either finished before this or
    // waits here and then finds the account gone (`service::acting`), and a
    // second deletion waits here and then finds nothing left to do.
    //
    // This strength of lock does not get in the way of other people's
    // transactions that merely refer to the account, such as the other party
    // acting in a shared exchange and queueing a message for it. Those may be
    // holding the lock on an exchange this transaction needs next, so making
    // them wait here would be a deadlock.
    let held: Option<(String, Option<String>, Option<String>)> =
        sqlx::query_as("SELECT status, email, phone FROM account WHERE id = $1 FOR NO KEY UPDATE")
            .bind(account)
            .fetch_optional(&mut *tx)
            .await?;
    let (email, phone) = match held {
        Some((status, email, phone)) if status == "ACTIVE" => (email, phone),
        Some((status, ..)) if status == "SUSPENDED" => {
            // A suspended account has no session to ask with; if one gets
            // here, the suspension stands and so does the account.
            return Err(ErrorCode::AccountSuspended.into());
        }
        _ => return Ok(Attempt::Done),
    };

    // Working copies of terms never sent, and the name on drafts that were
    // never sent: no revision carries it, so nothing signed refers to it.
    // Before the drafts are discarded below, while they can still be told
    // apart from exchanges that closed after something was sent.
    sqlx::query("DELETE FROM exchange_draft WHERE account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "UPDATE participant p SET display_name = '', alias = ''
         FROM exchange e
         WHERE e.id = p.exchange_id AND e.state = 'DRAFT' AND p.account_id = $1",
    )
    .bind(account)
    .execute(&mut *tx)
    .await?;

    // Exchanges, through the rules. Always in the same order, so two people
    // who share exchanges and leave at the same moment cannot each hold a
    // lock the other is waiting for.
    let open: Vec<Uuid> = sqlx::query_scalar(
        "SELECT e.id FROM exchange e JOIN participant p ON p.exchange_id = e.id
         WHERE p.account_id = $1 AND e.state IN ('DRAFT', 'NEGOTIATING', 'ACTIVE')
         ORDER BY e.id",
    )
    .bind(account)
    .fetch_all(&mut *tx)
    .await?;
    for exchange in open {
        leave(&mut tx, rules, exchange, account).await?;
    }

    // Invitation links the account issued that nobody took: they stop
    // working, and forget whom they were for, which is an address the
    // account supplied about someone else.
    sqlx::query(
        "UPDATE invitation i
         SET revoked_at = coalesce(i.revoked_at, now()), bound_email = NULL, bound_phone = NULL
         FROM participant p
         WHERE p.exchange_id = i.exchange_id AND p.slot = 'A' AND p.account_id = $1
           AND i.claimed_by IS NULL",
    )
    .bind(account)
    .execute(&mut *tx)
    .await?;
    // An invitation that named this account's own address, and that it took.
    // That it was named is in the history already (the claim was recorded as
    // confirmed); the address itself is not needed again.
    sqlx::query(
        "UPDATE invitation SET bound_email = NULL, bound_phone = NULL WHERE claimed_by = $1",
    )
    .bind(account)
    .execute(&mut *tx)
    .await?;
    // An invitation someone else bound to this account's address and that
    // nobody has taken: it was for a person who is leaving, so it stops
    // working and forgets the address. Its sender sees a dead link and can
    // issue a new one.
    sqlx::query(
        "UPDATE invitation
         SET revoked_at = coalesce(revoked_at, now()), bound_email = NULL, bound_phone = NULL
         WHERE claimed_by IS NULL
           AND ((bound_email IS NOT NULL AND bound_email = $1)
             OR (bound_phone IS NOT NULL AND bound_phone = $2))",
    )
    .bind(email.as_deref())
    .bind(phone.as_deref())
    .execute(&mut *tx)
    .await?;

    // Its devices first: nothing more is pushed to them. (Removing the
    // sessions would take them too; this says so.)
    sqlx::query("DELETE FROM device WHERE account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM account_session WHERE account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    // Codes are stored under the address they were sent to.
    let identifiers: Vec<String> = [email, phone].into_iter().flatten().collect();
    sqlx::query("DELETE FROM one_time_code WHERE identifier = ANY($1)")
        .bind(&identifiers)
        .execute(&mut *tx)
        .await?;
    sqlx::query("DELETE FROM idempotency_key WHERE account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;
    // A message the worker is sending at this instant is locked by it and is
    // passed over: it is too late to stop, and waiting for a slow send here
    // would hold up everyone in the exchanges above. Anything queued for the
    // account later is not written, and anything left is dropped unsent
    // (`notifications::outbox`).
    sqlx::query(
        "DELETE FROM outbox WHERE id IN (
             SELECT id FROM outbox WHERE recipient_account_id = $1 FOR UPDATE SKIP LOCKED)",
    )
    .bind(account)
    .execute(&mut *tx)
    .await?;
    // Its Wallet passes are revoked: each is told once that it is void, and
    // never updated after (`crate::wallet::store::revoke_for_account`).
    wallet::store::revoke_for_account(&mut tx, account).await?;
    sqlx::query("DELETE FROM account_block WHERE blocker_account_id = $1")
        .bind(account)
        .execute(&mut *tx)
        .await?;

    // Giving up the identifiers needs the row to itself, which a transaction
    // that merely refers to the account also prevents. Such a transaction
    // may itself be waiting for an exchange locked above (a block does: it
    // stores the block, then ends open offers), so this must not wait for
    // it. It asks once, and if the row is not free, everything above is
    // undone and tried again from the start.
    let free = sqlx::query("SELECT 1 FROM account WHERE id = $1 FOR UPDATE NOWAIT")
        .bind(account)
        .execute(&mut *tx)
        .await;
    match free {
        Ok(_) => {}
        Err(error) if is_lock_not_available(&error) => return Ok(Attempt::Busy),
        Err(error) => return Err(error.into()),
    }
    sqlx::query(
        "UPDATE account
         SET status = 'DELETED', email = NULL, phone = NULL, display_name = '', language = $2
         WHERE id = $1",
    )
    .bind(account)
    .bind(languages::default())
    .execute(&mut *tx)
    .await?;
    // Committed with the deletion or not at all.
    log_deletion(&mut *tx, account, logged_at).await?;

    tx.commit().await?;
    Ok(Attempt::Done)
}

fn is_lock_not_available(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .is_some_and(|e| e.code().as_deref() == Some("55P03"))
}

/// What the departing party does in an exchange on the way out, in order.
#[derive(Clone, Copy)]
enum Parting {
    /// Discard a draft that was never sent, as its initiator.
    DiscardDraft,
    /// End what is waiting to be signed: withdraw their own, decline the
    /// other party's.
    EndOpenRevision,
    /// Ask to close an agreement in force.
    RequestClose,
}

/// Takes the departing account out of one open exchange, in its own name and
/// through the rules. Runs in the transaction that deletes the account.
async fn leave(
    conn: &mut PgConnection,
    rules: &Rules,
    exchange: Uuid,
    account: Uuid,
) -> Result<(), sqlx::Error> {
    for step in [
        Parting::DiscardDraft,
        Parting::EndOpenRevision,
        Parting::RequestClose,
    ] {
        // Loaded again for the second step: the first may have changed it.
        let Some(aggregate) = repo::load(conn, exchange, true).await? else {
            return Ok(());
        };
        let Some(slot) = aggregate.slot_of(account) else {
            return Ok(());
        };
        let current = &aggregate.exchange;

        let mut commands = Vec::new();
        match step {
            // Only its initiator has a draft, and nobody else is in it yet.
            Parting::DiscardDraft => {
                if current.state == State::Draft {
                    commands.push(Command::Discard);
                }
            }
            Parting::EndOpenRevision => {
                // Someone the initiator has not confirmed can neither decline
                // nor take a signature back. They leave, which undoes both:
                // whatever they signed is void, and the offer stays open for
                // whoever the initiator invites next.
                if slot == Slot::B && current.counterparty == Counterparty::Claimed {
                    commands.push(Command::ReleaseClaim);
                }
                if let Some(open) = &current.open {
                    let revision = open.id;
                    commands.push(if open.author == slot {
                        Command::Withdraw { revision }
                    } else {
                        Command::Decline { revision }
                    });
                }
            }
            // A request already pending, from either party, closes the
            // exchange when its window ends; a second one is not needed.
            Parting::RequestClose => {
                if current.state == State::Active && current.close_request.is_none() {
                    commands.push(Command::RequestClose);
                }
            }
        }

        let at = OffsetDateTime::now_utc();
        let actor = Actor::Party(slot);
        // The rules decide, as for any command, and the first they allow is
        // what happens. Where they allow none, the exchange is left as it
        // stands.
        let decision = commands
            .into_iter()
            .find_map(|command| decide(current, actor, command, at, rules).ok());
        let Some(decision) = decision else {
            continue;
        };
        repo::persist(conn, &aggregate, &decision, actor, None, at).await?;
    }
    Ok(())
}
