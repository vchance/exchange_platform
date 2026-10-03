//! Telling a device that one of its passes changed (DESIGN.md §11, §13.1).
//!
//! The push carries nothing: an empty JSON object, sent through Apple's push
//! service (APNs) to the push token the device registered with, under the
//! pass type identifier as the topic. The device then asks the pass web
//! service which of its passes changed, and fetches them. This is not the
//! app's push: it reaches the device whether or not the app is installed,
//! it is authenticated by the pass type certificate rather than the app's
//! key, and it shares no token or code with the app's notifications.

use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Context;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Method, Request, StatusCode};
use rustls_pki_types::CertificateDer;

use super::Signer;
use crate::wallet::{net, pem};

/// What came of one push.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushOutcome {
    Sent,
    /// Apple says the token is no longer valid for this pass type: the
    /// registration goes.
    Unregistered,
}

pub type PushFuture<'a> = Pin<Box<dyn Future<Output = anyhow::Result<PushOutcome>> + Send + 'a>>;

/// Sends a pass update push to one device.
pub trait PassPush: Send + Sync {
    fn push<'a>(&'a self, push_token: &'a str) -> PushFuture<'a>;
}

/// Development delivery (`WALLET_DELIVERY=log`): writes that a push would
/// have gone to the worker's log. Only the end of the token is written; it
/// identifies a device.
pub struct LogPush;

impl PassPush for LogPush {
    fn push<'a>(&'a self, push_token: &'a str) -> PushFuture<'a> {
        Box::pin(async move {
            let tail: String = push_token
                .chars()
                .rev()
                .take(6)
                .collect::<Vec<_>>()
                .into_iter()
                .rev()
                .collect();
            tracing::info!(token_ends = tail, "wallet pass push (development delivery)");
            Ok(PushOutcome::Sent)
        })
    }
}

/// Apple's push service. Wallet's pushes always go to the production
/// service, even from a development device.
pub const APNS_ORIGIN: &str = "https://api.push.apple.com";

/// Pushes through APNs over HTTP/2, presenting the pass type certificate.
pub struct ApnsPush {
    client: net::HttpsClient,
    topic: String,
    origin: String,
    timeout: Duration,
}

impl ApnsPush {
    pub fn new(signer: &Arc<Signer>, pass_type_id: &str) -> anyhow::Result<Self> {
        let chain = vec![CertificateDer::from(signer.certificate_der.clone())];
        let key = pem::tls_key("APPLE_PASS_KEY", &signer.key_pem)?;
        Ok(Self {
            client: net::client_with_certificate(chain, key)
                .context("the APNs client, with the pass type certificate")?,
            topic: pass_type_id.to_owned(),
            origin: APNS_ORIGIN.to_owned(),
            timeout: Duration::from_secs(20),
        })
    }
}

/// The request for one push.
pub fn request(
    origin: &str,
    topic: &str,
    push_token: &str,
) -> anyhow::Result<Request<Full<Bytes>>> {
    // A token is hex; anything else in the path is refused, not escaped.
    if push_token.is_empty() || !push_token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        anyhow::bail!("a push token is hexadecimal");
    }
    Ok(Request::builder()
        .method(Method::POST)
        .uri(format!("{origin}/3/device/{push_token}"))
        .header("apns-topic", topic)
        .header("content-type", "application/json")
        .body(Full::new(Bytes::from_static(b"{}")))?)
}

/// What APNs's answer means for the registration.
pub fn outcome(status: StatusCode, body: &[u8]) -> anyhow::Result<PushOutcome> {
    let reason = serde_json::from_slice::<serde_json::Value>(body)
        .ok()
        .and_then(|body| body["reason"].as_str().map(str::to_owned))
        .unwrap_or_default();
    match status {
        StatusCode::OK => Ok(PushOutcome::Sent),
        StatusCode::GONE => Ok(PushOutcome::Unregistered),
        StatusCode::BAD_REQUEST
            if matches!(
                reason.as_str(),
                "BadDeviceToken" | "DeviceTokenNotForTopic" | "Unregistered"
            ) =>
        {
            Ok(PushOutcome::Unregistered)
        }
        _ => anyhow::bail!("APNs answered {status} {reason}"),
    }
}

impl PassPush for ApnsPush {
    fn push<'a>(&'a self, push_token: &'a str) -> PushFuture<'a> {
        Box::pin(async move {
            let request = request(&self.origin, &self.topic, push_token)?;
            let answer = net::send(&self.client, request, self.timeout).await?;
            outcome(answer.status, &answer.body)
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_push_is_an_empty_object_to_the_token_under_the_pass_type() {
        let push = request(APNS_ORIGIN, "pass.app.yuppers", "00ab").unwrap();
        assert_eq!(push.method(), Method::POST);
        assert_eq!(
            push.uri().to_string(),
            "https://api.push.apple.com/3/device/00ab"
        );
        assert_eq!(push.headers()["apns-topic"], "pass.app.yuppers");
        assert!(push.headers().get("apns-priority").is_none());
        assert!(request(APNS_ORIGIN, "pass.app.yuppers", "../x").is_err());
        assert!(request(APNS_ORIGIN, "pass.app.yuppers", "").is_err());
    }

    #[test]
    fn apple_saying_a_token_is_dead_removes_it_and_anything_else_is_a_failure() {
        assert_eq!(outcome(StatusCode::OK, b"").unwrap(), PushOutcome::Sent);
        assert_eq!(
            outcome(StatusCode::GONE, br#"{"reason":"Unregistered"}"#).unwrap(),
            PushOutcome::Unregistered
        );
        assert_eq!(
            outcome(StatusCode::BAD_REQUEST, br#"{"reason":"BadDeviceToken"}"#).unwrap(),
            PushOutcome::Unregistered
        );
        assert!(outcome(StatusCode::BAD_REQUEST, br#"{"reason":"BadTopic"}"#).is_err());
        assert!(outcome(StatusCode::FORBIDDEN, br#"{"reason":"BadCertificate"}"#).is_err());
        assert!(outcome(StatusCode::TOO_MANY_REQUESTS, b"").is_err());
    }
}
