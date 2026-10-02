//! One-time codes and sessions (DESIGN.md §8).
//!
//! A person proves control of an email address or phone number by entering a
//! code sent to it. That creates or signs into an account and yields a
//! session. Neither the code nor the session token is ever stored.

use std::future::Future;
use std::pin::Pin;

use hmac::{Hmac, KeyInit, Mac};
use sha2::{Digest, Sha256};
use sqlx::PgPool;
use time::Duration;
use uuid::Uuid;

use crate::domain::identity::Identifier;
use crate::error::{ApiError, ErrorCode};
use crate::languages;

/// The numbers behind code and session handling. Placeholders: none of these
/// is a recorded design decision yet.
#[derive(Clone, Debug)]
pub struct AuthRules {
    pub code_ttl: Duration,
    /// Wrong guesses allowed against one code before it is dead.
    pub code_max_failed_attempts: i16,
    /// Codes one identifier may be sent per hour.
    pub codes_per_hour: i64,
    pub session_ttl: Duration,
}

impl Default for AuthRules {
    fn default() -> Self {
        Self {
            code_ttl: Duration::minutes(10),
            code_max_failed_attempts: 5,
            codes_per_hour: 5,
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

// ---- Requesting and verifying a code ----------------------------------------

/// Issues a new code for an identifier and sends it, in `language` (see
/// [`language_for`]). Any earlier code for the same identifier stops working,
/// whatever it was for.
pub async fn request_code(
    db: &PgPool,
    secret: &[u8],
    rules: &AuthRules,
    sender: &dyn CodeSender,
    identifier: &Identifier,
    purpose: Purpose,
    language: &str,
) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;

    // One request at a time per identifier, so that counting and inserting
    // cannot be raced past the hourly limit.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
        .bind(identifier.as_str())
        .execute(&mut *tx)
        .await?;

    let recent: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM one_time_code
         WHERE identifier = $1 AND created_at > now() - interval '1 hour'",
    )
    .bind(identifier.as_str())
    .fetch_one(&mut *tx)
    .await?;
    if recent >= rules.codes_per_hour {
        return Err(ErrorCode::TooManyRequests.into());
    }

    sqlx::query(
        "UPDATE one_time_code SET consumed_at = now()
         WHERE identifier = $1 AND consumed_at IS NULL",
    )
    .bind(identifier.as_str())
    .execute(&mut *tx)
    .await?;

    let code = generate_code();
    sqlx::query(
        "INSERT INTO one_time_code (identifier, code_hash, expires_at)
         VALUES ($1, $2, now() + $3 * interval '1 second')",
    )
    .bind(identifier.as_str())
    .bind(code_hash(secret, purpose, identifier, &code).as_slice())
    .bind(rules.code_ttl.whole_seconds() as f64)
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

/// Checks a code and uses it up. A wrong guess is counted against the code
/// before the refusal is returned, so guessing cannot be retried for free. A
/// code asked for one purpose and offered for another is a wrong guess.
pub async fn verify_code(
    db: &PgPool,
    secret: &[u8],
    rules: &AuthRules,
    identifier: &Identifier,
    code: &str,
    purpose: Purpose,
) -> Result<(), ApiError> {
    let mut tx = db.begin().await?;

    let current: Option<(Uuid, Vec<u8>, i16)> = sqlx::query_as(
        "SELECT id, code_hash, failed_attempts FROM one_time_code
         WHERE identifier = $1 AND consumed_at IS NULL AND expires_at > now()
         ORDER BY created_at DESC
         LIMIT 1
         FOR UPDATE",
    )
    .bind(identifier.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let Some((id, expected, failed_attempts)) = current else {
        return Err(ErrorCode::InvalidCode.into());
    };
    if failed_attempts >= rules.code_max_failed_attempts {
        return Err(ErrorCode::InvalidCode.into());
    }

    // Constant-time comparison.
    let matches = code_mac(secret, purpose, identifier, code.trim())
        .verify_slice(&expected)
        .is_ok();
    let update = if matches {
        "UPDATE one_time_code SET consumed_at = now() WHERE id = $1"
    } else {
        "UPDATE one_time_code SET failed_attempts = failed_attempts + 1 WHERE id = $1"
    };
    sqlx::query(update).bind(id).execute(&mut *tx).await?;
    tx.commit().await?;

    if matches {
        Ok(())
    } else {
        Err(ErrorCode::InvalidCode.into())
    }
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
