//! One-time codes and sessions (DESIGN.md §8).
//!
//! A person proves control of an email address or phone number by entering a
//! code sent to it. That creates or signs into an account and yields a
//! session. Neither the code nor the session token is ever stored.
//!
//! **Limits** (DESIGN.md §8, §18 item 4). Asking for a code never ends the
//! codes already sent: the most recent few for an identifier and purpose
//! stay live until each expires, a code offered is checked against all of
//! them, and using one uses them all. So someone who knows an address can no
//! longer keep its owner out by asking for codes; the newest code the owner
//! received works. Guessing is bounded per code, per identifier per day and
//! per requester's address per hour, and asking per identifier and per
//! address per hour. Deletion codes are counted apart, against the account
//! (see [`Requester`]). The numbers are in [`AuthRules`].
//!
//! The counts live in `sign_in_limit`, one row per thing counted and window,
//! rather than behind advisory locks alone: an advisory lock can make
//! counting and inserting atomic, but something has to hold the count, and
//! failed guesses and requests by address leave no row of their own to count.
//! The row is locked while a request is decided, which makes each limit
//! exact under concurrency. Windows are fixed hours and UTC days, so a burst
//! straddling the turn of a window can reach twice a limit.

use std::future::Future;
use std::net::IpAddr;
use std::pin::Pin;

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use sqlx::{PgConnection, PgPool};
use time::Duration;
use uuid::Uuid;

use crate::domain::identity::Identifier;
use crate::error::{ApiError, ErrorCode};
use crate::languages;

/// The numbers behind code and session handling. Placeholders, every one:
/// none is a recorded design decision yet (DESIGN.md §8, §18 item 4).
#[derive(Clone, Debug)]
pub struct AuthRules {
    /// How long a code works. A placeholder.
    pub code_ttl: Duration,
    /// Wrong guesses allowed against one code before it is dead. A guess is
    /// checked against every live code, so it counts against each of them.
    /// A placeholder.
    pub code_max_failed_attempts: i16,
    /// Sign-in codes one identifier may be sent per hour. A placeholder.
    pub codes_per_hour: i64,
    /// How many of the most recent codes for one identifier and purpose stay
    /// live, each until it expires. A placeholder.
    pub live_codes: i64,
    /// Failed sign-in guesses one identifier may take per UTC day, and failed
    /// guesses one account may make at its deletion codes. Past that, codes
    /// are refused, right or wrong, until the day ends. A placeholder.
    pub failed_guesses_per_day: i64,
    /// Sign-in codes one requester's network address may ask for per hour,
    /// whatever the identifiers. A placeholder.
    pub code_requests_per_address_per_hour: i64,
    /// Failed sign-in guesses one requester's network address may make per
    /// hour, whatever the identifiers. A placeholder.
    pub failed_guesses_per_address_per_hour: i64,
    /// Deletion codes one account may ask for per hour. A placeholder.
    pub deletion_codes_per_hour: i64,
    /// How long a session lasts. A placeholder.
    pub session_ttl: Duration,
}

impl Default for AuthRules {
    fn default() -> Self {
        Self {
            code_ttl: Duration::minutes(10),
            code_max_failed_attempts: 5,
            codes_per_hour: 5,
            live_codes: 3,
            failed_guesses_per_day: 20,
            code_requests_per_address_per_hour: 10,
            failed_guesses_per_address_per_hour: 30,
            deletion_codes_per_hour: 5,
            session_ttl: Duration::days(30),
        }
    }
}

/// What a code was asked for. A code does only that: one sent to sign in
/// cannot delete the account, and one sent to delete it cannot sign in. The
/// message that carries a code says which it is, so that nobody is talked
/// into reading out a "sign-in code" that would delete their account.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Purpose {
    /// Proving control of an identifier: signing in, or attaching it to an
    /// account.
    SignIn,
    /// Confirming that the account is to be deleted (`crate::deletion`).
    DeleteAccount,
}

impl Purpose {
    pub fn as_str(self) -> &'static str {
        match self {
            Purpose::SignIn => "sign-in",
            Purpose::DeleteAccount => "delete-account",
        }
    }
}

/// Who is asking for a code, or offering one back. That decides what the
/// code is for and which limits count the request.
///
/// Signing in is open to anyone, so it is limited by the identifier and by
/// the requester's network address. Deleting is open only to the account's
/// own session, so it is limited by the account, and by nothing someone
/// without that session can use up: not the identifier's sign-in limits, and
/// not an address, which the owner may share with whoever is flooding it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Requester {
    /// Someone proving control of an identifier, to sign in or to attach it
    /// to their account, known by the address the request came from
    /// (`crate::http::ClientAddress`). `None` only for a request with no
    /// connection behind it; all such requests share one count.
    SignIn { address: Option<IpAddr> },
    /// A signed-in account confirming that it is to be deleted.
    DeleteAccount { account: Uuid },
}

impl Requester {
    pub fn purpose(self) -> Purpose {
        match self {
            Requester::SignIn { .. } => Purpose::SignIn,
            Requester::DeleteAccount { .. } => Purpose::DeleteAccount,
        }
    }
}

// ---- Delivery ---------------------------------------------------------------

pub type SendFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

/// A one-time code on its way to someone.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CodeMessage<'a> {
    pub to: &'a Identifier,
    pub code: &'a str,
    /// What the code is for. The message must say so.
    pub purpose: Purpose,
    /// The language to write the message in: a supported language tag, or
    /// whatever the client asked for, to be resolved by the wording.
    pub language: &'a str,
}

/// Delivers a one-time code by email or SMS.
pub trait CodeSender: Send + Sync {
    fn send<'a>(&'a self, message: CodeMessage<'a>) -> SendFuture<'a>;
}

/// Development delivery: writes the code to the service log. Never configured
/// in production, where anyone who can read logs could sign in as anyone.
pub struct LogSender;

impl CodeSender for LogSender {
    fn send<'a>(&'a self, message: CodeMessage<'a>) -> SendFuture<'a> {
        Box::pin(async move {
            tracing::info!(
                to = message.to.as_str(),
                code = message.code,
                purpose = message.purpose.as_str(),
                language = message.language,
                "one-time code (development delivery)"
            );
            Ok(())
        })
    }
}

/// The language to write to someone in: their account's preference if an
/// account has this identifier, otherwise the first supported language among
/// those the client asked for in `Accept-Language`, otherwise the default.
/// Looked up as a side query, so a failure here costs the language, not the
/// code.
pub async fn language_for(
    db: &PgPool,
    identifier: &Identifier,
    accept_language: Option<&str>,
) -> String {
    let column = match identifier {
        Identifier::Email(_) => "email",
        Identifier::Phone(_) => "phone",
    };
    let preference: Option<String> = sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
        "SELECT language FROM account WHERE {column} = $1 AND status = 'ACTIVE'"
    )))
    .bind(identifier.as_str())
    .fetch_optional(db)
    .await
    .unwrap_or_default();
    preference
        .or_else(|| {
            accept_language?
                .split(',')
                .map(|entry| entry.split(';').next().unwrap_or("").trim())
                .find_map(|tag| languages::resolve(tag).map(str::to_owned))
        })
        .unwrap_or_else(|| languages::default().to_owned())
}

// ---- Codes and tokens -------------------------------------------------------

/// Six decimal digits, uniformly distributed.
pub fn generate_code() -> String {
    // Reject the top sliver of the range so the remainder is unbiased.
    const LIMIT: u32 = u32::MAX - u32::MAX % 1_000_000;
    loop {
        let n = getrandom::u32().expect("the operating system provides randomness");
        if n < LIMIT {
            return format!("{:06}", n % 1_000_000);
        }
    }
}

fn code_mac(secret: &[u8], purpose: Purpose, identifier: &Identifier, code: &str) -> Hmac<Sha256> {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
    // The purpose is part of what is hashed, which is what ties a stored code
    // to it. A sign-in code is hashed as it always was. No identifier
    // contains a NUL, so the two forms cannot be confused.
    if purpose != Purpose::SignIn {
        mac.update(purpose.as_str().as_bytes());
        mac.update(b"\0");
    }
    mac.update(identifier.as_str().as_bytes());
    mac.update(b"\0");
    mac.update(code.as_bytes());
    mac
}

pub fn code_hash(secret: &[u8], purpose: Purpose, identifier: &Identifier, code: &str) -> [u8; 32] {
    code_mac(secret, purpose, identifier, code)
        .finalize()
        .into_bytes()
        .into()
}

/// A session or invitation token: 256 random bits as 64 hex characters.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    getrandom::fill(&mut bytes).expect("the operating system provides randomness");
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn token_hash(token: &str) -> [u8; 32] {
    Sha256::digest(token.as_bytes()).into()
}

// ---- Limits -----------------------------------------------------------------

/// What `sign_in_limit` counts. Each is counted in fixed windows: the hour,
/// or the UTC day.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Counted {
    CodeRequestsByAddress,
    FailedGuessesByAddress,
    FailedGuessesByIdentifier,
    CodeRequestsByAccount,
    FailedGuessesByAccount,
}

impl Counted {
    fn scope(self) -> &'static str {
        match self {
            Counted::CodeRequestsByAddress => "code-requests-by-address",
            Counted::FailedGuessesByAddress => "failed-guesses-by-address",
            Counted::FailedGuessesByIdentifier => "failed-guesses-by-identifier",
            Counted::CodeRequestsByAccount => "code-requests-by-account",
            Counted::FailedGuessesByAccount => "failed-guesses-by-account",
        }
    }

    /// The window, as `date_trunc` names it.
    fn window(self) -> &'static str {
        match self {
            Counted::FailedGuessesByIdentifier | Counted::FailedGuessesByAccount => "day",
            _ => "hour",
        }
    }
}

/// One count in `sign_in_limit`: what is counted, and of whom. Whom is kept
/// only as a keyed hash, so the table holds no address or identifier.
struct Counter {
    counted: Counted,
    subject: [u8; 32],
}

impl Counter {
    fn new(secret: &[u8], counted: Counted, subject: &str) -> Self {
        let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC accepts any key length");
        mac.update(b"sign-in-limit\0");
        mac.update(counted.scope().as_bytes());
        mac.update(b"\0");
        mac.update(subject.as_bytes());
        Self {
            counted,
            subject: mac.finalize().into_bytes().into(),
        }
    }

    fn address(secret: &[u8], counted: Counted, address: Option<IpAddr>) -> Self {
        // Not an address, so it cannot be mistaken for one.
        let subject = address.map_or_else(|| "unknown".to_owned(), |address| address.to_string());
        Self::new(secret, counted, &subject)
    }

    /// The count so far in the current window. The row stays locked until
    /// the transaction ends, so nobody else decides on it meanwhile.
    async fn hold(&self, conn: &mut PgConnection) -> Result<i64, sqlx::Error> {
        self.add(conn, 0).await
    }

    /// Adds to the count in the current window and returns the new count.
    async fn add(&self, conn: &mut PgConnection, by: i32) -> Result<i64, sqlx::Error> {
        let count: i32 = sqlx::query_scalar(
            "INSERT INTO sign_in_limit (scope, subject, window_start, count)
             VALUES ($1, $2, date_trunc($3, now(), 'UTC'), $4)
             ON CONFLICT (scope, subject, window_start)
             DO UPDATE SET count = sign_in_limit.count + EXCLUDED.count
             RETURNING count",
        )
        .bind(self.counted.scope())
        .bind(self.subject.as_slice())
        .bind(self.counted.window())
        .bind(by)
        .fetch_one(conn)
        .await?;
        Ok(i64::from(count))
    }
}

/// One request at a time per identifier and purpose, so that counting,
/// inserting and checking codes cannot be raced. Sign-in and deletion take
/// different locks, so neither waits on the other.
async fn lock_codes(
    conn: &mut PgConnection,
    identifier: &Identifier,
    purpose: Purpose,
) -> Result<(), sqlx::Error> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(format!(
            "one-time-code:{}:{}",
            purpose.as_str(),
            identifier.as_str()
        ))
        .execute(conn)
        .await?;
    Ok(())
}

/// Forgets counts whose window ended more than a day ago; none of them can
/// matter any more. Returns how many were removed. Called by the worker.
pub async fn purge_sign_in_limits(db: &PgPool) -> Result<u64, sqlx::Error> {
    let removed =
        sqlx::query("DELETE FROM sign_in_limit WHERE window_start < now() - interval '2 days'")
            .execute(db)
            .await?
            .rows_affected();
    Ok(removed)
}

// ---- Requesting and verifying a code ----------------------------------------

/// Issues a new code for an identifier and sends it, in `language` (see
/// [`language_for`]). Codes already sent for the same identifier and purpose
/// keep working until they expire, except that only the newest
/// [`AuthRules::live_codes`] are kept: older ones stop working.
///
/// Refused with `TOO_MANY_REQUESTS` when the requester's address has asked
/// for too many sign-in codes this hour, when the identifier has been sent
/// too many, or, for deletion, when the account has asked for too many.
pub async fn request_code(
    db: &PgPool,
    secret: &[u8],
    rules: &AuthRules,
    sender: &dyn CodeSender,
    identifier: &Identifier,
    requester: Requester,
    language: &str,
) -> Result<(), ApiError> {
    let purpose = requester.purpose();
    let mut tx = db.begin().await?;

    // The requester first, and then the identifier, always in that order.
    let (requests, limit) = match requester {
        Requester::SignIn { address } => (
            Counter::address(secret, Counted::CodeRequestsByAddress, address),
            rules.code_requests_per_address_per_hour,
        ),
        Requester::DeleteAccount { account } => (
            Counter::new(secret, Counted::CodeRequestsByAccount, &account.to_string()),
            rules.deletion_codes_per_hour,
        ),
    };
    if requests.hold(&mut tx).await? >= limit {
        return Err(ErrorCode::TooManyRequests.into());
    }
    // Counted whether or not the identifier's own limit then refuses it.
    requests.add(&mut tx, 1).await?;

    lock_codes(&mut tx, identifier, purpose).await?;

    // A deletion code goes only to the account's own identifier, at the
    // account's own request, so the account's count above is what limits it.
    // Counting it against the identifier as well would let anyone asking
    // for sign-in codes use up the owner's way to delete.
    if purpose == Purpose::SignIn {
        let recent: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM one_time_code
             WHERE identifier = $1 AND purpose = $2 AND created_at > now() - interval '1 hour'",
        )
        .bind(identifier.as_str())
        .bind(purpose.as_str())
        .fetch_one(&mut *tx)
        .await?;
        if recent >= rules.codes_per_hour {
            tx.commit().await?;
            return Err(ErrorCode::TooManyRequests.into());
        }
    }

    let code = generate_code();
    // The clock, not the transaction's start: requests for one identifier
    // are ordered by the lock above, and so are their codes.
    sqlx::query(
        "INSERT INTO one_time_code (identifier, purpose, code_hash, expires_at, created_at)
         VALUES ($1, $2, $3, clock_timestamp() + $4 * interval '1 second', clock_timestamp())",
    )
    .bind(identifier.as_str())
    .bind(purpose.as_str())
    .bind(code_hash(secret, purpose, identifier, &code).as_slice())
    .bind(rules.code_ttl.whole_seconds() as f64)
    .execute(&mut *tx)
    .await?;

    // Only the newest few stay live.
    sqlx::query(
        "UPDATE one_time_code SET consumed_at = now()
         WHERE identifier = $1 AND purpose = $2 AND consumed_at IS NULL
           AND id NOT IN (
               SELECT id FROM one_time_code
               WHERE identifier = $1 AND purpose = $2
               ORDER BY created_at DESC
               LIMIT $3)",
    )
    .bind(identifier.as_str())
    .bind(purpose.as_str())
    .bind(rules.live_codes)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    sender
        .send(CodeMessage {
            to: identifier,
            code: &code,
            purpose,
            language,
        })
        .await
        .map_err(|error| {
            tracing::error!(%error, "one-time code could not be delivered");
            ApiError::from(ErrorCode::ServiceUnavailable)
        })
}

/// Checks a code against every live code for the identifier and purpose,
/// and if it matches one, uses them all up. A wrong guess is counted against
/// each live code, and against the identifier's day and the requester's
/// hour (for deletion, the account's day), before the refusal is returned,
/// so guessing cannot be retried for free. A code asked for one purpose and
/// offered for another is a wrong guess.
///
/// Refused with `TOO_MANY_GUESSES`, right or wrong, once the identifier (for
/// deletion, the account) has used up its failed guesses for the day, and
/// with `TOO_MANY_REQUESTS` once the requester's address has for the hour.
pub async fn verify_code(
    db: &PgPool,
    secret: &[u8],
    rules: &AuthRules,
    identifier: &Identifier,
    code: &str,
    requester: Requester,
) -> Result<(), ApiError> {
    let purpose = requester.purpose();
    let mut tx = db.begin().await?;

    // The requester's address first, then the identifier, as when asking.
    // A deletion is guessed at only through the account's own session, so
    // it is counted against the account and against no address.
    let (by_address, by_owner) = match requester {
        Requester::SignIn { address } => (
            Some(Counter::address(
                secret,
                Counted::FailedGuessesByAddress,
                address,
            )),
            Counter::new(
                secret,
                Counted::FailedGuessesByIdentifier,
                identifier.as_str(),
            ),
        ),
        Requester::DeleteAccount { account } => (
            None,
            Counter::new(
                secret,
                Counted::FailedGuessesByAccount,
                &account.to_string(),
            ),
        ),
    };
    if let Some(by_address) = &by_address
        && by_address.hold(&mut tx).await? >= rules.failed_guesses_per_address_per_hour
    {
        return Err(ErrorCode::TooManyRequests.into());
    }

    lock_codes(&mut tx, identifier, purpose).await?;
    if by_owner.hold(&mut tx).await? >= rules.failed_guesses_per_day {
        return Err(ErrorCode::TooManyGuesses.into());
    }

    let live: Vec<(Uuid, Vec<u8>)> = sqlx::query_as(
        "SELECT id, code_hash FROM one_time_code
         WHERE identifier = $1 AND purpose = $2
           AND consumed_at IS NULL AND expires_at > now() AND failed_attempts < $3
         FOR UPDATE",
    )
    .bind(identifier.as_str())
    .bind(purpose.as_str())
    .bind(rules.code_max_failed_attempts)
    .fetch_all(&mut *tx)
    .await?;

    // Constant-time comparison against each, and no stopping at the first
    // match, so the time taken says nothing about which one it was.
    let offered = code.trim();
    let matches = live
        .iter()
        .map(|(_, expected)| {
            code_mac(secret, purpose, identifier, offered)
                .verify_slice(expected)
                .is_ok()
        })
        .fold(false, |any, this| any | this);

    if matches {
        sqlx::query(
            "UPDATE one_time_code SET consumed_at = now()
             WHERE identifier = $1 AND purpose = $2 AND consumed_at IS NULL",
        )
        .bind(identifier.as_str())
        .bind(purpose.as_str())
        .execute(&mut *tx)
        .await?;
        tx.commit().await?;
        return Ok(());
    }

    let ids: Vec<Uuid> = live.iter().map(|(id, _)| *id).collect();
    sqlx::query(
        "UPDATE one_time_code SET failed_attempts = failed_attempts + 1 WHERE id = ANY($1)",
    )
    .bind(&ids)
    .execute(&mut *tx)
    .await?;
    // With no live code there was nothing to guess at, so the identifier's
    // day is not charged: otherwise anyone could use it up without a code
    // ever being sent. The requester's hour is.
    if !live.is_empty() {
        by_owner.add(&mut tx, 1).await?;
    }
    if let Some(by_address) = &by_address {
        by_address.add(&mut tx, 1).await?;
    }
    tx.commit().await?;
    Err(ErrorCode::InvalidCode.into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_are_six_digits() {
        for _ in 0..200 {
            let code = generate_code();
            assert_eq!(code.len(), 6);
            assert!(code.bytes().all(|b| b.is_ascii_digit()), "{code}");
        }
    }

    #[test]
    fn tokens_are_long_and_distinct() {
        let (a, b) = (generate_token(), generate_token());
        assert_eq!(a.len(), 64);
        assert_ne!(a, b);
    }

    #[test]
    fn a_code_hash_depends_on_the_secret_the_identifier_the_code_and_the_purpose() {
        let ana = Identifier::parse("ana@example.com").unwrap();
        let ben = Identifier::parse("ben@example.com").unwrap();
        let sign_in = Purpose::SignIn;
        let base = code_hash(b"secret", sign_in, &ana, "123456");

        assert_eq!(code_hash(b"secret", sign_in, &ana, "123456"), base);
        assert_ne!(code_hash(b"other", sign_in, &ana, "123456"), base);
        assert_ne!(code_hash(b"secret", sign_in, &ben, "123456"), base);
        assert_ne!(code_hash(b"secret", sign_in, &ana, "123457"), base);
        assert_ne!(
            code_hash(b"secret", Purpose::DeleteAccount, &ana, "123456"),
            base
        );
    }
}
