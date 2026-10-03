//! The `wallet_pass` table and Apple's device registrations
//! (`migrations/0010_wallet.sql`).
//!
//! **Updates.** A change to an exchange marks its passes `PENDING` in the
//! transaction that records the change ([`mark_exchange_changed`], called
//! from `exchanges::repo::persist`), the way the outbox queues a message with
//! its event: a pass is marked exactly when something happened. The pass row
//! is its own queue entry, so many changes in a row make one update, and
//! every update sends the latest face, never one in between: what reaches a
//! device cannot go backwards. The worker takes marked passes with a short
//! lease rather than a lock held while it sends, so marking a pass never
//! waits on a slow push ([`claim_due`]).

use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use time::{Date, OffsetDateTime};
use uuid::Uuid;

use super::pass::{self, PassContext, PassModel};
use super::{Wallet, WalletPlatform};
use crate::domain::Rules;
use crate::error::{ApiError, ErrorCode};
use crate::exchanges::service;

/// Marks every live pass of an exchange for an update. Runs in the
/// transaction of the change; cheap when the exchange has no pass.
pub async fn mark_exchange_changed(
    conn: &mut PgConnection,
    exchange: Uuid,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE wallet_pass
         SET update_status = 'PENDING', mark_seq = mark_seq + 1, available_at = now(),
             attempts = 0, updated_at = now()
         WHERE exchange_id = $1 AND voided_at IS NULL",
    )
    .bind(exchange)
    .execute(conn)
    .await?;
    Ok(())
}

/// Revokes every pass an account holds, as part of deleting it (DESIGN.md
/// §11: revoking voids). Each is marked for one last update, which tells its
/// devices, or Google, that it is void and strips it; after that it is never
/// updated again, and Apple's devices are forgotten once told.
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
            changed_at: row.changed_at,
        }
    }
}

const COLUMNS: &str = "id, account_id, exchange_id, platform, external_id, \
                       voided_at IS NOT NULL AS voided, changed_at";

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
    let mut tx = db.begin().await?;
    let (id, serial, voided, changed_at, issued): (Uuid, String, bool, OffsetDateTime, i32) =
        sqlx::query_as(
            "INSERT INTO wallet_pass
                (account_id, exchange_id, platform, external_id, issues_in_window,
                 issue_window_started_at)
             VALUES ($1, $2, $3, $4, 1, now())
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
        .fetch_one(&mut *tx)
        .await?;
    let pass = PassRow {
        id,
        account,
        exchange,
        platform,
        serial,
        voided,
        changed_at,
    };
    if platform == WalletPlatform::Apple {
        let hash = token_hash(&wallet.apple_auth_token(pass.id));
        sqlx::query(
            "UPDATE wallet_pass SET auth_token_hash = $2
             WHERE id = $1 AND auth_token_hash IS DISTINCT FROM $2",
        )
        .bind(pass.id)
        .bind(hash.as_slice())
        .execute(&mut *tx)
        .await?;
    }
    // Counted even when refused, so a script that keeps asking stays out.
    tx.commit().await?;
    if voided {
        return Err(ErrorCode::NotFound.into());
    }
    if issued > wallet.rules.issues_per_hour {
        return Err(ErrorCode::TooManyRequests.into());
    }
    Ok(pass)
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

/// The face of a pass as it should be now: the void face for a revoked one,
/// otherwise drawn from the exchange as its holder sees it.
pub async fn face(
    db: &PgPool,
    rules: &Rules,
    wallet: &Wallet,
    pass: &PassRow,
) -> Result<(PassModel, i64), ApiError> {
    let language: Option<String> = sqlx::query_scalar("SELECT language FROM account WHERE id = $1")
        .bind(pass.account)
        .fetch_optional(db)
        .await?;
    let language = wallet.wording.language(language.as_deref().unwrap_or(""));
    if pass.voided {
        return Ok((pass::void(language), 0));
    }
    let view = service::view_for(db, rules, pass.exchange, pass.account).await?;
    let context: (Date, Option<Date>, Option<String>) = sqlx::query_as(
        "SELECT (now() AT TIME ZONE e.timezone)::date,
                (e.closed_at AT TIME ZONE e.timezone)::date,
                other.alias
         FROM exchange e
         JOIN participant mine ON mine.exchange_id = e.id AND mine.account_id = $2
         JOIN participant other ON other.exchange_id = e.id AND other.slot <> mine.slot
         WHERE e.id = $1",
    )
    .bind(pass.exchange)
    .bind(pass.account)
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
        },
        language,
    );
    Ok((model, view.version))
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

/// An Apple pass by its serial number, if `token` is its authentication
/// token. A wrong token and an unknown serial look the same.
pub async fn apple_pass(
    db: &PgPool,
    serial: &str,
    token: &str,
) -> Result<Option<PassRow>, sqlx::Error> {
    let found: Option<(Option<Vec<u8>>, Uuid)> = sqlx::query_as(
        "SELECT auth_token_hash, id FROM wallet_pass WHERE platform = 'APPLE' AND external_id = $1",
    )
    .bind(serial)
    .fetch_optional(db)
    .await?;
    let Some((hash, id)) = found else {
        return Ok(None);
    };
    let presented = token_hash(token);
    let matches = hash.is_some_and(|hash| {
        hash.len() == presented.len()
            && hash
                .iter()
                .zip(presented)
                .fold(0u8, |diff, (a, b)| diff | (a ^ b))
                == 0
    });
    if !matches {
        return Ok(None);
    }
    pass_by_id(db, id).await
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
/// the bound keeps a leaked token from growing the table without end.
const DEVICES_PER_PASS: i64 = 20;

/// What came of a registration.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Registered {
    New,
    Already,
    TooMany,
}

/// Registers a device for a pass's updates, or updates its push token.
pub async fn register(
    db: &PgPool,
    pass: Uuid,
    device: &str,
    push_token: &str,
) -> Result<Registered, sqlx::Error> {
    let mut tx = db.begin().await?;
    // One registration at a time per pass, so the count holds.
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
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wallet_device_registration WHERE wallet_pass_id = $1",
    )
    .bind(pass)
    .fetch_one(&mut *tx)
    .await?;
    if count >= DEVICES_PER_PASS {
        return Ok(Registered::TooMany);
    }
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
/// `since` (all of them without one), and the tag to send next time.
pub async fn changed_for_device(
    db: &PgPool,
    device: &str,
    since: Option<OffsetDateTime>,
) -> Result<(Vec<String>, Option<OffsetDateTime>), sqlx::Error> {
    let rows: Vec<(String, OffsetDateTime)> = sqlx::query_as(
        "SELECT p.external_id, p.changed_at
         FROM wallet_device_registration r
         JOIN wallet_pass p ON p.id = r.wallet_pass_id
         WHERE r.device_library_id = $1 AND p.platform = 'APPLE'
           AND ($2::timestamptz IS NULL OR p.changed_at > $2)
         ORDER BY p.external_id",
    )
    .bind(device)
    .bind(since.map(|since| since - SINCE_OVERLAP))
    .fetch_all(db)
    .await?;
    let latest = rows.iter().map(|(_, at)| *at).max();
    Ok((rows.into_iter().map(|(serial, _)| serial).collect(), latest))
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

/// Forgets registrations: one device's, or with `None` every one.
pub async fn forget_devices(
    db: &PgPool,
    pass: Uuid,
    device: Option<&str>,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "DELETE FROM wallet_device_registration
         WHERE wallet_pass_id = $1 AND ($2::text IS NULL OR device_library_id = $2)",
    )
    .bind(pass)
    .bind(device)
    .execute(db)
    .await?;
    Ok(())
}

/// The update went out (or there was nothing to send). The pass is current,
/// unless it was marked again meanwhile.
pub async fn delivered(
    db: &PgPool,
    claimed: &Claimed,
    hash: &[u8; 32],
    version: i64,
) -> Result<(), sqlx::Error> {
    sqlx::query(
        "UPDATE wallet_pass
         SET update_status = CASE WHEN mark_seq = $2 THEN 'CURRENT' ELSE 'PENDING' END,
             delivered_hash = $3, display_version = greatest(display_version, $4),
             claimed_until = NULL, attempts = 0, last_error = NULL, updated_at = now()
         WHERE id = $1",
    )
    .bind(claimed.pass.id)
    .bind(claimed.mark_seq)
    .bind(hash.as_slice())
    .bind(version)
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
