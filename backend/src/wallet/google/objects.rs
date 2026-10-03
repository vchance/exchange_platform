//! Google Wallet objects through the Google Wallet API, with an access token
//! for the issuer's service account (the OAuth 2.0 JWT bearer grant): the
//! class and the object are created when a save link is made, so the link
//! names an object that already exists, and the object is patched as the
//! exchange changes.
//!
//! What Google says when it refuses is reduced to the HTTP status and its
//! error code and reason ([`describe_refusal`]), as for the other providers:
//! its message can quote what was sent, and it goes into logs and
//! `wallet_pass.last_error`.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use anyhow::{Context, bail};
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use serde::Deserialize;
use serde_json::{Value, json};
use time::OffsetDateTime;
use tokio::sync::Mutex;

use super::ServiceAccount;
use crate::wallet::net;

/// What came of one update.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PatchOutcome {
    Updated,
    /// Google has no such object. Nothing is updated, and the face is not
    /// counted as delivered, so the next update tries again.
    NotSaved,
}

pub type PatchFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<PatchOutcome>> + Send + 'a>>;
pub type CreateFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<()>> + Send + 'a>>;

/// Writes generic classes and objects to Google.
pub trait WalletObjects: Send + Sync {
    /// Writes an object's new state.
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a>;
    /// Makes sure the class exists, and creates the object, or writes its
    /// state when it exists already.
    fn upsert<'a>(&'a self, class: &'a Value, object: &'a Value) -> CreateFuture<'a>;
}

/// Development delivery (`WALLET_DELIVERY=log`): writes that an object would
/// have been written to the log, with its state and size. Not what it says,
/// which includes the alias of the other party.
pub struct LogObjects;

impl WalletObjects for LogObjects {
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a> {
        Box::pin(async move {
            tracing::info!(
                object = object_id,
                state = object["state"].as_str().unwrap_or(""),
                bytes = object.to_string().len(),
                "google wallet object update (development delivery)"
            );
            Ok(PatchOutcome::Updated)
        })
    }

    fn upsert<'a>(&'a self, _class: &'a Value, object: &'a Value) -> CreateFuture<'a> {
        Box::pin(async move {
            tracing::info!(
                object = object["id"].as_str().unwrap_or(""),
                state = object["state"].as_str().unwrap_or(""),
                "google wallet object created (development delivery)"
            );
            Ok(())
        })
    }
}

/// The Google Wallet API.
pub const API_ORIGIN: &str = "https://walletobjects.googleapis.com";
/// What the service account's access token may do: manage this issuer's
/// classes and objects, nothing else.
pub const SCOPE: &str = "https://www.googleapis.com/auth/wallet_object.issuer";

/// Writes classes and objects through the Google Wallet API.
pub struct ApiObjects {
    client: net::HttpsClient,
    account: Arc<ServiceAccount>,
    origin: String,
    timeout: Duration,
    /// The access token and when to stop using it.
    token: Mutex<Option<(String, OffsetDateTime)>>,
    /// Set once the class is known to exist.
    class_exists: AtomicBool,
}

impl ApiObjects {
    pub fn new(account: Arc<ServiceAccount>) -> anyhow::Result<Self> {
        Ok(Self {
            client: net::client()?,
            account,
            origin: API_ORIGIN.to_owned(),
            timeout: Duration::from_secs(20),
            token: Mutex::new(None),
            class_exists: AtomicBool::new(false),
        })
    }

    /// An access token, asked for again a minute before the last runs out.
    async fn access_token(&self) -> anyhow::Result<String> {
        let mut held = self.token.lock().await;
        let now = OffsetDateTime::now_utc();
        if let Some((token, until)) = held.as_ref()
            && *until > now
        {
            return Ok(token.clone());
        }
        let request = token_request(&self.account, now)?;
        let answer = net::send(&self.client, request, self.timeout).await?;
        if answer.status != StatusCode::OK {
            bail!(
                "the access token was refused: {}",
                describe_refusal(answer.status, &answer.body)
            );
        }
        #[derive(Deserialize)]
        struct Granted {
            access_token: String,
            expires_in: i64,
        }
        let granted: Granted =
            serde_json::from_slice(&answer.body).context("an unreadable access token answer")?;
        let until = now + time::Duration::seconds((granted.expires_in - 60).max(0));
        *held = Some((granted.access_token.clone(), until));
        Ok(granted.access_token)
    }

    async fn send(&self, request: Request<Full<Bytes>>) -> anyhow::Result<net::Answer> {
        let answer = net::send(&self.client, request, self.timeout).await?;
        if answer.status == StatusCode::UNAUTHORIZED {
            // Asked for again on the next try.
            *self.token.lock().await = None;
            bail!("the Wallet API refused the access token");
        }
        Ok(answer)
    }

    /// Creates `body` under `kind` (`genericClass`, `genericObject`).
    /// Returns false when it exists already.
    async fn insert(&self, kind: &str, body: &Value) -> anyhow::Result<bool> {
        let token = self.access_token().await?;
        let answer = self
            .send(insert_request(&self.origin, &token, kind, body)?)
            .await?;
        match answer.status {
            status if status.is_success() => Ok(true),
            StatusCode::CONFLICT => Ok(false),
            status => bail!(
                "the Wallet API did not create the {kind}: {}",
                describe_refusal(status, &answer.body)
            ),
        }
    }
}

/// The request for an access token: a JWT signed by the service account,
/// exchanged at its token endpoint.
pub fn token_request(
    account: &ServiceAccount,
    now: OffsetDateTime,
) -> anyhow::Result<Request<Full<Bytes>>> {
    let claims = json!({
        "iss": account.client_email,
        "scope": SCOPE,
        "aud": account.token_uri,
        "iat": now.unix_timestamp(),
        "exp": now.unix_timestamp() + 3600,
    });
    // The JWT is base64url and dots, and needs no escaping in a form.
    let body = format!(
        "grant_type=urn%3Aietf%3Aparams%3Aoauth%3Agrant-type%3Ajwt-bearer&assertion={}",
        account.sign_jwt(&claims)?
    );
    Ok(Request::builder()
        .method(Method::POST)
        .uri(&account.token_uri)
        .header("content-type", "application/x-www-form-urlencoded")
        .body(Full::new(Bytes::from(body)))?)
}

fn valid_id(id: &str) -> bool {
    !id.is_empty()
        && id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
}

/// The request that patches one object.
pub fn patch_request(
    origin: &str,
    access_token: &str,
    object_id: &str,
    object: &Value,
) -> anyhow::Result<Request<Full<Bytes>>> {
    if !valid_id(object_id) {
        bail!("an object ID is letters, digits, dots, underscores and hyphens");
    }
    Ok(Request::builder()
        .method(Method::PATCH)
        .uri(format!(
            "{origin}/walletobjects/v1/genericObject/{object_id}"
        ))
        .header("authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(serde_json::to_vec(object)?)))?)
}

/// The request that creates a class or an object (`kind` is `genericClass`
/// or `genericObject`).
pub fn insert_request(
    origin: &str,
    access_token: &str,
    kind: &str,
    body: &Value,
) -> anyhow::Result<Request<Full<Bytes>>> {
    if !matches!(kind, "genericClass" | "genericObject")
        || !body["id"].as_str().is_some_and(valid_id)
    {
        bail!(
            "a generic class or object, with an ID of letters, digits, dots, underscores and hyphens"
        );
    }
    Ok(Request::builder()
        .method(Method::POST)
        .uri(format!("{origin}/walletobjects/v1/{kind}"))
        .header("authorization", format!("Bearer {access_token}"))
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from(serde_json::to_vec(body)?)))?)
}

/// What a refusal from Google may say in a log: the HTTP status, and the
/// error's code and reason when Google gave them as short codes, never its
/// message. The Wallet API answers `{"error": {"code", "status", "errors":
/// [{"reason"}], "message"}}`, the token endpoint `{"error",
/// "error_description"}`.
pub fn describe_refusal(status: StatusCode, body: &[u8]) -> String {
    let body: Value = serde_json::from_slice(body).unwrap_or(Value::Null);
    let error = &body["error"];
    let code = |value: &Value| {
        value
            .as_str()
            .filter(|text| {
                (1..=64).contains(&text.len())
                    && text
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
            })
            .map(str::to_owned)
    };
    let mut parts: Vec<String> = Vec::new();
    for found in [
        code(error),
        code(&error["status"]),
        code(&error["errors"][0]["reason"]),
    ]
    .into_iter()
    .flatten()
    {
        if !parts.contains(&found) {
            parts.push(found);
        }
    }
    if parts.is_empty() {
        format!("HTTP {}", status.as_u16())
    } else {
        format!("HTTP {}, error {}", status.as_u16(), parts.join(" "))
    }
}

impl WalletObjects for ApiObjects {
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a> {
        Box::pin(async move {
            let token = self.access_token().await?;
            let answer = self
                .send(patch_request(&self.origin, &token, object_id, object)?)
                .await?;
            match answer.status {
                status if status.is_success() => Ok(PatchOutcome::Updated),
                StatusCode::NOT_FOUND => Ok(PatchOutcome::NotSaved),
                status => bail!(
                    "the Wallet API did not update the object: {}",
                    describe_refusal(status, &answer.body)
                ),
            }
        })
    }

    fn upsert<'a>(&'a self, class: &'a Value, object: &'a Value) -> CreateFuture<'a> {
        Box::pin(async move {
            if !self.class_exists.load(Ordering::Relaxed) {
                self.insert("genericClass", class).await?;
                self.class_exists.store(true, Ordering::Relaxed);
            }
            if !self.insert("genericObject", object).await? {
                let id = object["id"].as_str().unwrap_or_default();
                if self.patch(id, object).await? == PatchOutcome::NotSaved {
                    bail!("the Wallet API has the object and does not have it");
                }
            }
            Ok(())
        })
    }
}
