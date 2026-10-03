//! Updating a saved Google Wallet pass: the generic object is patched
//! through the Google Wallet API, with an access token for the issuer's
//! service account (the OAuth 2.0 JWT bearer grant).

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
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
    /// Google has no such object: the person never saved the pass. There is
    /// nothing to update; the link they may still use carries the latest.
    NotSaved,
}

pub type PatchFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<PatchOutcome>> + Send + 'a>>;

/// Writes a generic object's new state to Google.
pub trait WalletObjects: Send + Sync {
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a>;
}

/// Development delivery (`WALLET_DELIVERY=log`): writes that an object would
/// have been updated to the worker's log, with its state and size. Not what
/// it says, which includes the alias of the other party.
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
}

/// The Google Wallet API.
pub const API_ORIGIN: &str = "https://walletobjects.googleapis.com";
/// What the service account's access token may do: manage this issuer's
/// classes and objects, nothing else.
pub const SCOPE: &str = "https://www.googleapis.com/auth/wallet_object.issuer";

/// Patches objects through the Google Wallet API.
pub struct ApiObjects {
    client: net::HttpsClient,
    account: Arc<ServiceAccount>,
    origin: String,
    timeout: Duration,
    /// The access token and when to stop using it.
    token: Mutex<Option<(String, OffsetDateTime)>>,
}

impl ApiObjects {
    pub fn new(account: Arc<ServiceAccount>) -> anyhow::Result<Self> {
        Ok(Self {
            client: net::client()?,
            account,
            origin: API_ORIGIN.to_owned(),
            timeout: Duration::from_secs(20),
            token: Mutex::new(None),
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
            // Google's error names the problem and holds nothing secret.
            bail!(
                "the access token was refused: {} {}",
                answer.status,
                String::from_utf8_lossy(&answer.body)
                    .chars()
                    .take(300)
                    .collect::<String>()
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

/// The request that patches one object.
pub fn patch_request(
    origin: &str,
    access_token: &str,
    object_id: &str,
    object: &Value,
) -> anyhow::Result<Request<Full<Bytes>>> {
    if object_id.is_empty()
        || !object_id
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'.' | b'_' | b'-'))
    {
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

impl WalletObjects for ApiObjects {
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a> {
        Box::pin(async move {
            let token = self.access_token().await?;
            let request = patch_request(&self.origin, &token, object_id, object)?;
            let answer = net::send(&self.client, request, self.timeout).await?;
            match answer.status {
                status if status.is_success() => Ok(PatchOutcome::Updated),
                StatusCode::NOT_FOUND => Ok(PatchOutcome::NotSaved),
                StatusCode::UNAUTHORIZED => {
                    // Asked for again on the next try.
                    *self.token.lock().await = None;
                    bail!("the Wallet API refused the access token")
                }
                status => bail!(
                    "the Wallet API answered {status}: {}",
                    String::from_utf8_lossy(&answer.body)
                        .chars()
                        .take(300)
                        .collect::<String>()
                ),
            }
        })
    }
}
