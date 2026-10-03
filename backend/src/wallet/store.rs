//! The `wallet_pass` table, Apple's device registrations, the passes'
//! authentication tokens and their download links (`migrations/0010_wallet.sql`,
//! `0012_wallet_tokens.sql`).
//!
//! **Updates.** A change to an exchange marks its passes `PENDING` in the
//! transaction that records the change ([`mark_exchange_changed`], called
//! from `exchanges::repo::persist`), the way the outbox queues a message with
//! its event: a pass is marked exactly when something happened. A face also
//! changes with the date ("Due soon", "Overdue"), so the worker marks again
//! the passes whose exchange's day has moved on ([`mark_new_days`]). The pass
//! row is its own queue entry, so many changes in a row make one update, and
//! every update sends the latest face, never one in between: what reaches a
//! device cannot go backwards. The worker takes marked passes with a short
//! lease rather than a lock held while it sends, so marking a pass never
//! waits on a slow push ([`claim_due`]).
//!
//! **Accounts that are not active.** A deleted account's passes are voided
//! ([`revoke_for_account`]). A suspended account's are frozen: they are not
//! marked and not updated, and devices are told nothing new about them, but
//! they are not voided. Whether suspension should void them is the owner's
//! decision (docs/wallet.md).

use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::pass::{self, PassContext, PassModel};
use super::{Wallet, WalletPlatform, random_token};
use crate::domain::Rules;
use crate::error::{ApiError, ErrorCode};
use crate::exchanges::service;

/// Marks every live pass of an exchange for an update, except those of an
/// account that is not active. Runs in the transaction of the change; cheap
/// when the exchange has no pass.
pub async fn mark_exchange_changed(
    conn: &mut PgConnection,
    exchange: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE wallet_pass
         SET update_status = 'PENDING', mark_seq = mark_seq + 1, available_at = now(),
             attempts = 0, updated_at = now()
         WHERE exchange_id = $1 AND voided_at IS NULL
           AND account_id IN (SELECT id FROM account WHERE status = 'ACTIVE')",
    )
    .bind(exchange)
    .execute(conn)
    .await?;
    Ok(())
}

/// Marks the passes whose face may have changed with the date: those of an
/// exchange in force with a pending contribution due on a date, last drawn
/// for a day before the exchange's today at `at` (in its timezone, as the
/// reminders read due dates). Run by the worker on every pass; once a pass's
/// new day has been drawn it is not marked again until the next. A face that
/// comes out the same is not sent (`super::delivery`). Returns how many were
/// marked.
pub async fn mark_new_days(db: &PgPool, at: OffsetDateTime) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE wallet_pass p
         SET update_status = 'PENDING', mark_seq = mark_seq + 1, available_at = now(),
             attempts = 0, updated_at = now()
         FROM exchange e, account a
         WHERE e.id = p.exchange_id AND a.id = p.account_id AND a.status = 'ACTIVE'
           AND p.voided_at IS NULL AND p.update_status = 'CURRENT' AND e.state = 'ACTIVE'
           AND (p.face_date IS NULL OR p.face_date < ($1::timestamptz AT TIME ZONE e.timezone)::date)
           AND EXISTS (
               SELECT 1 FROM contribution_snapshot s
               JOIN contribution c ON c.id = s.contribution_id AND c.status = 'PENDING'
               WHERE s.revision_id = e.in_force_revision_id AND s.due_kind = 'DATE')",
    )
    .bind(at)
    .execute(db)
    .await?
    .rows_affected())
}

/// Revokes every pass an account holds, as part of deleting it (DESIGN.md
/// §11: revoking voids). Each is marked for one last update, which tells its
/// devices, or Google, that it is void and strips it; after that it is never
/// updated again. Apple's devices are forgotten once they have been told, or
/// after a while ([`purge_void_registrations`]).
pub async fn revoke_for_account(
    conn: &mut PgConnection,
    account: Uuid,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "UPDATE wallet_pass
         SET voided_at = now(), update_status = 'PENDING', mark_seq = mark_seq + 1,
             available_at = now(), attempts = 0, updated_at = now()
         WHERE account_id = $1 AND voided_at IS NULL",
    )
    .bind(account)
    .execute(conn)
    .await?
    .rows_affected())
}

/// A pass as stored.
#[derive(Clone, Debug)]
pub struct PassRow {
    pub id: Uuid,
    pub account: Uuid,
    pub exchange: Uuid,
    pub platform: WalletPlatform,
    /// The Apple serial number, or the end of the Google object ID.
    pub serial: String,
    pub voided: bool,
    /// Its account is not active (suspended): the pass is not updated.
    pub frozen: bool,
    /// Apple's Last-Modified.
    pub changed_at: OffsetDateTime,
}

/// A `wallet_pass` row as [`COLUMNS`] reads it.
#[derive(sqlx::FromRow)]
struct Stored {
    id: Uuid,
    account_id: Uuid,
    exchange_id: Uuid,
    platform: String,
    external_id: String,
    voided: bool,
    frozen: bool,
    changed_at: OffsetDateTime,
}

impl From<Stored> for PassRow {
    fn from(row: Stored) -> Self {
        PassRow {
            id: row.id,
            account: row.account_id,
            exchange: row.exchange_id,
            platform: WalletPlatform::parse(&row.platform).unwrap_or(WalletPlatform::Apple),
            serial: row.external_id,
            voided: row.voided,
            frozen: row.frozen,
            changed_at: row.changed_at,
        }
    }
}

const COLUMNS: &str = "id, account_id, exchange_id, platform, external_id, \
                       voided_at IS NOT NULL AS voided, \
                       EXISTS (SELECT 1 FROM account WHERE account.id = wallet_pass.account_id \
                               AND account.status <> 'ACTIVE') AS frozen, \
                       changed_at";

/// A random serial number: 128 bits in hex. It says nothing about the
/// exchange or the person.
fn new_serial() -> String {
    let mut bytes = [0u8; 16];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

/// The account's pass for an exchange on a platform, made the first time it
/// is asked for and the same one after. Counts the asking against
/// `issues_per_hour` and refuses past it.
pub async fn issue(
    db: &PgPool,
    wallet: &Wallet,
    account: Uuid,
    exchange: Uuid,
    platform: WalletPlatform,
) -> Result<PassRow, ApiError> {
    let (id, serial, voided, changed_at, issued): (Uuid, String, bool, OffsetDateTime, i32) =
        sqlx::query_as(
            "INSERT INTO wallet_pass
                (account_id, exchange_id, platform, external_id, issues_in_window,
                 issue_window_started_at, face_date)
             -- Its first face is drawn now, for today.
             VALUES ($1, $2, $3, $4, 1, now(),
                     (SELECT (now() AT TIME ZONE timezone)::date FROM exchange WHERE id = $2))
             ON CONFLICT (account_id, exchange_id, platform) DO UPDATE SET
                 issues_in_window = CASE
                     WHEN wallet_pass.issue_window_started_at > now() - interval '1 hour'
                     THEN wallet_pass.issues_in_window + 1 ELSE 1 END,
                 issue_window_started_at = CASE
                     WHEN wallet_pass.issue_window_started_at > now() - interval '1 hour'
                     THEN wallet_pass.issue_window_started_at ELSE now() END
             RETURNING id, external_id, voided_at IS NOT NULL, changed_at, issues_in_window",
        )
        .bind(account)
        .bind(exchange)
        .bind(platform.as_str())
        .bind(new_serial())
        .fetch_one(db)
        .await?;
    // Counted even when refused, so a script that keeps asking stays out.
    if voided {
        return Err(ErrorCode::NotFound.into());
    }
    if issued > wallet.rules.issues_per_hour {
        return Err(ErrorCode::TooManyRequests.into());
    }
    Ok(PassRow {
        id,
        account,
        exchange,
        platform,
        serial,
        voided,
        // Only an active account has a session to ask with.
        frozen: false,
        changed_at,
    })
}

/// A new authentication token for an Apple pass, for the copy about to be
/// handed out. Random; only its hash is kept, with those of the latest
/// `kept - 1` before it, so the copies on a person's other devices keep
/// updating while older tokens stop working.
pub async fn new_auth_token(db: &PgPool, pass: Uuid, kept: i64) -> Result<String, sqlx::Error> {
    let token = random_token();
    let mut tx = db.begin().await?;
    sqlx::query("INSERT INTO wallet_auth_token (token_hash, wallet_pass_id) VALUES ($1, $2)")
        .bind(token_hash(&token).as_slice())
        .bind(pass)
        .execute(&mut *tx)
        .await?;
    sqlx::query(
        "DELETE FROM wallet_auth_token
         WHERE wallet_pass_id = $1 AND token_hash NOT IN (
             SELECT token_hash FROM wallet_auth_token WHERE wallet_pass_id = $1
             ORDER BY created_at DESC LIMIT $2)",
    )
    .bind(pass)
    .bind(kept.max(1))
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(token)
}

/// A link token that downloads an Apple pass once, until `expires`. Random;
/// only its hash is stored.
pub async fn new_download_link(
    db: &PgPool,
    pass: Uuid,
    expires: OffsetDateTime,
) -> Result<String, sqlx::Error> {
    let token = random_token();
    sqlx::query(
        "INSERT INTO wallet_download_link (token_hash, wallet_pass_id, expires_at)
         VALUES ($1, $2, $3)",
    )
    .bind(token_hash(&token).as_slice())
    .bind(pass)
    .bind(expires)
    .execute(db)
    .await?;
    Ok(token)
}

/// The pass a download link is for, if it has not expired at `now` and has
/// not been used; it is used up by this. An unknown, expired and used link
/// look the same.
pub async fn redeem_download_link(
    db: &PgPool,
    token: &str,
    now: OffsetDateTime,
) -> Result<Option<Uuid>, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE wallet_download_link SET used_at = $2
         WHERE token_hash = $1 AND used_at IS NULL AND expires_at > $2
         RETURNING wallet_pass_id",
    )
    .bind(token_hash(token.trim()).as_slice())
    .bind(now)
    .fetch_optional(db)
    .await
}

/// SHA-256 of a token, as stored.
pub fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

/// SHA-256 of a face, to tell whether it changed.
pub fn face_hash(model: &PassModel) -> [u8; 32] {
    let bytes = serde_json::to_vec(model).expect("a pass model serializes");
    Sha256::digest(&bytes).into()
}

/// A face, drawn.
pub struct Face {
    pub model: PassModel,
    /// The exchange's version it was drawn from.
    pub version: i64,
    /// The day in the exchange's timezone it was drawn for.
    pub day: Date,
}

/// The face of a pass as it should be at `at`: the void face for a revoked
/// one, otherwise drawn from the exchange as its holder sees it.
pub async fn face(
    db: &PgPool,
    rules: &Rules,
    wallet: &Wallet,
    pass: &PassRow,
    at: OffsetDateTime,
) -> Result<Face, ApiError> {
    let language: Option<String> = sqlx::query_scalar("SELECT language FROM account WHERE id = $1")
        .bind(pass.account)
        .fetch_optional(db)
        .await?;
    let language = wallet.wording.language(language.as_deref().unwrap_or(""));
    if pass.voided {
        return Ok(Face {
            model: pass::void(language),
            version: 0,
            day: at.date(),
        });
    }
    let view = service::view_for(db, rules, pass.exchange, pass.account).await?;
    let context: (Date, Option<Date>, Option<String>) = sqlx::query_as(
        "SELECT ($3::timestamptz AT TIME ZONE e.timezone)::date,
                (e.closed_at AT TIME ZONE e.timezone)::date,
                other.alias
         FROM exchange e
         JOIN participant mine ON mine.exchange_id = e.id AND mine.account_id = $2
         JOIN participant other ON other.exchange_id = e.id AND other.slot <> mine.slot
         WHERE e.id = $1",
    )
    .bind(pass.exchange)
    .bind(pass.account)
    .bind(at)
    .fetch_optional(db)
    .await?
    .ok_or(ErrorCode::NotFound)?;
    let (today, closed_on, alias) = context;
    let model = pass::render(
        &view,
        &PassContext {
            today,
            closed_on,
            other_party_alias: alias,
            link: wallet.exchange_link(pass.exchange),
            due_soon_days: rules.due_soon_lead.whole_days(),
            status_on_face: wallet.status_on_face,
        },
        language,
    );
    Ok(Face {
        model,
        version: view.version,
        day: today,
    })
}

/// Records `hash` as the current face: when it differs from the last, the
/// pass's Last-Modified moves on to now, or a second past the last, whichever
/// is later, so that every face has a time of its own. Returns that time.
pub async fn publish_face(
    db: &PgPool,
    pass: Uuid,
    hash: &[u8; 32],
) -> Result<OffsetDateTime, sqlx::Error> {
    sqlx::query_scalar(
        "UPDATE wallet_pass
         SET changed_at = CASE WHEN face_hash IS DISTINCT FROM $2
                               THEN greatest(date_trunc('second', clock_timestamp()),
                                             changed_at + interval '1 second')
                               ELSE changed_at END,
             face_hash = $2
         WHERE id = $1
         RETURNING changed_at",
    )
    .bind(pass)
    .bind(hash.as_slice())
    .fetch_one(db)
    .await
}

// ---- Apple's pass web service -------------------------------------------------

/// An Apple pass by its serial number, if `token` is one of its valid
/// authentication tokens. A wrong token and an unknown serial look the same.
/// Looked up by the token's hash, so how long the lookup takes says nothing
/// about the token.
pub async fn apple_pass(
    db: &PgPool,
    serial: &str,
    token: &str,
) -> Result<Option<PassRow>, sqlx::Error> {
    let found: Option<Stored> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM wallet_pass
         WHERE platform = 'APPLE' AND external_id = $1
           AND id = (SELECT wallet_pass_id FROM wallet_auth_token WHERE token_hash = $2)"
    )))
    .bind(serial)
    .bind(token_hash(token).as_slice())
    .fetch_optional(db)
    .await?;
    Ok(found.map(PassRow::from))
}

/// A pass by its ID.
pub async fn pass_by_id(db: &PgPool, id: Uuid) -> Result<Option<PassRow>, sqlx::Error> {
    let found: Option<Stored> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT {COLUMNS} FROM wallet_pass WHERE id = $1"
    )))
    .bind(id)
    .fetch_optional(db)
    .await?;
    Ok(found.map(PassRow::from))
}

/// The most devices one pass may be registered on. A person has a handful;
/// past it, a new device takes the place of the one heard from longest ago.
pub const DEVICES_PER_PASS: i64 = 20;

/// What came of a registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registered {
    New,
    Already,
    /// More new devices this hour than the pass takes.
    TooOften,
}

/// Registers a device for a pass's updates, or updates its push token. A
/// pass takes `per_hour` new devices an hour; one at [`DEVICES_PER_PASS`]
/// forgets its oldest registration for the new one, so a person whose
/// devices came and went is never locked out by the ones that went.
pub async fn register(
    db: &PgPool,
    pass: Uuid,
    device: &str,
    push_token: &str,
    per_hour: i32,
) -> Result<Registered, sqlx::Error> {
    let mut tx = db.begin().await?;
    // One registration at a time per pass, so the counts hold.
    sqlx::query("SELECT 1 FROM wallet_pass WHERE id = $1 FOR UPDATE")
        .bind(pass)
        .execute(&mut *tx)
        .await?;
    let updated = sqlx::query(
        "UPDATE wallet_device_registration SET push_token = $3, updated_at = now()
         WHERE wallet_pass_id = $1 AND device_library_id = $2",
    )
    .bind(pass)
    .bind(device)
    .bind(push_token)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated > 0 {
        tx.commit().await?;
        return Ok(Registered::Already);
    }
    let in_window: i32 = sqlx::query_scalar(
        "UPDATE wallet_pass SET
             registrations_in_window = CASE
                 WHEN registration_window_started_at > now() - interval '1 hour'
                 THEN registrations_in_window + 1 ELSE 1 END,
             registration_window_started_at = CASE
                 WHEN registration_window_started_at > now() - interval '1 hour'
                 THEN registration_window_started_at ELSE now() END
         WHERE id = $1
         RETURNING registrations_in_window",
    )
    .bind(pass)
    .fetch_one(&mut *tx)
    .await?;
    if in_window > per_hour {
        // Counted all the same, so a script that keeps trying stays out.
        tx.commit().await?;
        return Ok(Registered::TooOften);
    }
    sqlx::query(
        "DELETE FROM wallet_device_registration
         WHERE wallet_pass_id = $1 AND device_library_id IN (
             SELECT device_library_id FROM wallet_device_registration
             WHERE wallet_pass_id = $1
             ORDER BY updated_at DESC, device_library_id
             OFFSET $2)",
    )
    .bind(pass)
    .bind(DEVICES_PER_PASS - 1)
    .execute(&mut *tx)
    .await?;
    sqlx::query(
        "INSERT INTO wallet_device_registration (wallet_pass_id, device_library_id, push_token)
         VALUES ($1, $2, $3)",
    )
    .bind(pass)
    .bind(device)
    .bind(push_token)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;
    Ok(Registered::New)
}

/// Forgets a device's registration for a pass. Returns whether there was one.
pub async fn unregister(db: &PgPool, pass: Uuid, device: &str) -> Result<bool, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM wallet_device_registration WHERE wallet_pass_id = $1 AND device_library_id = $2",
    )
    .bind(pass)
    .bind(device)
    .execute(db)
    .await?
    .rows_affected()
        > 0)
}

/// How far back the list of changed passes looks before the tag a device
/// sends. A pass's time is set by the worker before it pushes, and two
/// workers' transactions can commit out of order; looking back a minute
/// means a pass whose time was set just before another's but committed just
/// after it is still listed. A pass listed again is harmless: the device
/// asks for it with If-Modified-Since and is told it has not changed.
const SINCE_OVERLAP: time::Duration = time::Duration::minutes(1);

/// The serial numbers of a device's passes of this type that changed after
/// `since` (all of them without one), and the tag to send next time. A
/// frozen pass is left out. A voided one is listed, which is how its device
/// learns to fetch the void face; the registration notes when, and is
/// forgotten a while after ([`purge_void_registrations`]).
pub async fn changed_for_device(
    db: &PgPool,
    device: &str,
    since: Option<OffsetDateTime>,
) -> Result<(Vec<String>, Option<OffsetDateTime>), sqlx::Error> {
    let rows: Vec<(String, OffsetDateTime)> = sqlx::query_as(
        "WITH listed AS (
             SELECT p.id, p.external_id, p.changed_at, p.voided_at IS NOT NULL AS voided
             FROM wallet_device_registration r
             JOIN wallet_pass p ON p.id = r.wallet_pass_id
             JOIN account a ON a.id = p.account_id
             WHERE r.device_library_id = $1 AND p.platform = 'APPLE'
               AND (p.voided_at IS NOT NULL OR a.status = 'ACTIVE')
               AND ($2::timestamptz IS NULL OR p.changed_at > $2)
         ), told AS (
             UPDATE wallet_device_registration r SET void_listed_at = now()
             FROM listed
             WHERE listed.voided AND r.wallet_pass_id = listed.id
               AND r.device_library_id = $1 AND r.void_listed_at IS NULL
         )
         SELECT external_id, changed_at FROM listed ORDER BY external_id",
    )
    .bind(device)
    .bind(since.map(|since| since - SINCE_OVERLAP))
    .fetch_all(db)
    .await?;
    let latest = rows.iter().map(|(_, at)| *at).max();
    Ok((rows.into_iter().map(|(serial, _)| serial).collect(), latest))
}

/// Forgets the registrations of voided passes once their device has been
/// told of the void face `grace` ago, or `kept` after the voiding whether
/// told or not: nothing more is ever sent for a voided pass. Returns how many
/// were removed.
pub async fn purge_void_registrations(
    db: &PgPool,
    now: OffsetDateTime,
    grace: time::Duration,
    kept: time::Duration,
) -> Result<u64, sqlx::Error> {
    Ok(sqlx::query(
        "DELETE FROM wallet_device_registration r
         USING wallet_pass p
         WHERE p.id = r.wallet_pass_id AND p.voided_at IS NOT NULL
           AND (r.void_listed_at < $1 OR p.voided_at < $2)",
    )
    .bind(now - grace)
    .bind(now - kept)
    .execute(db)
    .await?
    .rows_affected())
}

/// Forgets download links that expired a day ago or more. Returns how many.
pub async fn purge_download_links(db: &PgPool, now: OffsetDateTime) -> Result<u64, sqlx::Error> {
    Ok(
        sqlx::query("DELETE FROM wallet_download_link WHERE expires_at < $1")
            .bind(now - time::Duration::days(1))
            .execute(db)
            .await?
            .rows_affected(),
    )
}

// ---- The worker ---------------------------------------------------------------

/// A pass taken by the worker to update.
#[derive(Clone, Debug)]
pub struct Claimed {
    pub pass: PassRow,
    /// The mark the worker saw; a newer one keeps the pass pending.
    pub mark_seq: i64,
    pub attempts: i32,
    /// The face last delivered to the platform.
    pub delivered_hash: Option<Vec<u8>>,
}

#[derive(sqlx::FromRow)]
struct ClaimedRow {
    #[sqlx(flatten)]
    stored: Stored,
    mark_seq: i64,
    attempts: i32,
    delivered_hash: Option<Vec<u8>>,
}

/// Takes up to `batch` passes due for an update at `now`, each for `lease`.
/// Another worker skips them until the lease runs out, which is also what
/// hands a pass on if this worker dies while sending.
pub async fn claim_due(
    db: &PgPool,
    now: OffsetDateTime,
    batch: i64,
    lease: time::Duration,
    platforms: &[WalletPlatform],
) -> Result<Vec<Claimed>, sqlx::Error> {
    let platforms: Vec<&str> = platforms.iter().map(|platform| platform.as_str()).collect();
    let rows: Vec<ClaimedRow> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "UPDATE wallet_pass SET claimed_until = $1 + $3
         WHERE id IN (SELECT id FROM wallet_pass
                      WHERE update_status = 'PENDING' AND available_at <= $1
                        AND (claimed_until IS NULL OR claimed_until < $1)
                        AND platform = ANY($4)
                      ORDER BY available_at
                      LIMIT $2
                      FOR UPDATE SKIP LOCKED)
         RETURNING {COLUMNS}, mark_seq, attempts, delivered_hash"
    )))
    .bind(now)
    .bind(batch)
    .bind(lease)
    .bind(&platforms)
    .fetch_all(db)
    .await?;
    Ok(rows
        .into_iter()
        .map(|row| Claimed {
            pass: row.stored.into(),
            mark_seq: row.mark_seq,
            attempts: row.attempts,
            delivered_hash: row.delivered_hash,
        })
        .collect())
}

/// The devices registered for a pass: (device, push token).
pub async fn registrations(db: &PgPool, pass: Uuid) -> Result<Vec<(String, String)>, sqlx::Error> {
    sqlx::query_as(
        "SELECT device_library_id, push_token FROM wallet_device_registration
         WHERE wallet_pass_id = $1 ORDER BY device_library_id",
    )
    .bind(pass)
    .fetch_all(db)
    .await
}

/// Forgets one device's registration, which Apple says is gone.
pub async fn forget_device(db: &PgPool, pass: Uuid, device: &str) -> Result<(), sqlx::Error> {
    unregister(db, pass, device).await.map(|_| ())
}

/// The update went out, or there was nothing to send: `hash`, drawn for
/// `day`, is what the platform has. With no hash, nothing reached the
/// platform (Google has no such object, or the pass is frozen), and what was
/// last delivered stays as it was, so a later update sends the face again.
/// The pass is current, unless it was marked again meanwhile.
pub async fn delivered(
    db: &PgPool,
    claimed: &Claimed,
    hash: Option<&[u8; 32]>,
    version: i64,
    day: Date,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE wallet_pass
         SET update_status = CASE WHEN mark_seq = $2 THEN 'CURRENT' ELSE 'PENDING' END,
             delivered_hash = coalesce($3, delivered_hash),
             display_version = CASE WHEN $3 IS NULL THEN display_version
                                    ELSE greatest(display_version, $4) END,
             face_date = $5,
             claimed_until = NULL, attempts = 0, last_error = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(claimed.pass.id)
    .bind(claimed.mark_seq)
    .bind(hash.map(|hash| hash.as_slice()))
    .bind(version)
    .bind(day)
    .execute(db)
    .await?;
    Ok(())
}

/// `hash` reached the platform outside the worker: a Google object
/// created or updated when its save link was made.
pub async fn record_delivered(db: &PgPool, pass: Uuid, hash: &[u8; 32]) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE wallet_pass SET delivered_hash = $2 WHERE id = $1")
        .bind(pass)
        .bind(hash.as_slice())
        .execute(db)
        .await?;
    Ok(())
}

/// The update failed: tried again at `retry_at`, or given up on after the
/// last attempt and left `FAILED` with the error, until the exchange changes
/// again.
pub async fn failed(
    db: &PgPool,
    claimed: &Claimed,
    error: &str,
    retry_at: OffsetDateTime,
    max_attempts: i32,
) -> Result<bool, sqlx::Error> {
    let error: String = error.chars().take(500).collect();
    let given_up: bool = sqlx::query_scalar(
        "UPDATE wallet_pass
         SET attempts = attempts + 1, last_error = $2, available_at = $3, claimed_until = NULL,
             update_status = CASE WHEN mark_seq = $5 AND attempts + 1 >= $4 THEN 'FAILED'
                                  ELSE 'PENDING' END,
             updated_at = now()
         WHERE id = $1
         RETURNING update_status = 'FAILED'",
    )
    .bind(claimed.pass.id)
    .bind(&error)
    .bind(retry_at)
    .bind(max_attempts)
    .bind(claimed.mark_seq)
    .fetch_one(db)
    .await?;
    Ok(given_up)
}
