//! Wallet passes (DESIGN.md §11): each party's view of one exchange in their
//! phone's wallet, kept up to date as the exchange changes.
//!
//! A pass is a status view and nothing more. It is optional, it never stands
//! in for signing in (invariant 1), and it carries no terms: its face is made
//! by one pure function, [`pass::render`], from the exchange as its party
//! sees it, and both platforms draw that same model, so the two faces cannot
//! drift apart. What it may show follows the lock-screen rule (§11, §12):
//! the product, the exchange's display code, the other party's alias if one
//! is set, how the agreement stands in a few generic words, how many
//! contributions are still open, the next due date, and a link back to the
//! exchange, which asks its reader to sign in. Never what anyone owes, an
//! amount, a description or a name from the agreement.
//!
//! * [`apple`] builds the signed `.pkpass` and speaks Apple's pass web
//!   service protocol (the routes are in `crate::http::wallet`); a change is
//!   pushed to the devices that registered through APNs ([`apple::push`]).
//! * [`google`] builds the signed "Save to Google Wallet" link, which carries
//!   the generic pass class and object; a change is patched into the object
//!   through the Google Wallet API ([`google::objects`]).
//! * [`store`] is the `wallet_pass` table and Apple's device registrations;
//!   [`store::mark_exchange_changed`] runs in the transaction of every change
//!   to an exchange, and [`delivery`] is the worker's side.
//!
//! Nothing here runs until a deployment configures a platform ([`config`]):
//! until then `GET /v1/meta` names no platform, the clients show no button,
//! and the endpoints answer `WALLET_UNAVAILABLE`.

use std::sync::Arc;

use hmac::{Hmac, KeyInit, Mac};
use serde::Serialize;
use sha2::Sha256;
use utoipa::ToSchema;

pub mod apple;
pub mod config;
pub mod delivery;
pub mod google;
pub mod net;
pub mod pass;
mod pem;
pub mod store;
#[cfg(test)]
pub(crate) mod testkit;
pub mod wording;

pub use config::WalletConfig;

/// A wallet a pass can be added to.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum WalletPlatform {
    Apple,
    Google,
}

impl WalletPlatform {
    /// As stored in `wallet_pass.platform`.
    pub fn as_str(self) -> &'static str {
        match self {
            WalletPlatform::Apple => "APPLE",
            WalletPlatform::Google => "GOOGLE",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "APPLE" => Some(WalletPlatform::Apple),
            "GOOGLE" => Some(WalletPlatform::Google),
            _ => None,
        }
    }
}

/// The numbers behind passes. Placeholders: none is a recorded design
/// decision.
#[derive(Clone, Debug)]
pub struct WalletRules {
    /// How often one party may be handed their pass for one exchange on one
    /// platform in an hour. Signing costs little; this keeps a script from
    /// making the service sign without end.
    pub issues_per_hour: i32,
    /// How long a link to download an Apple pass works.
    pub download_link_ttl: time::Duration,
}

impl Default for WalletRules {
    fn default() -> Self {
        Self {
            issues_per_hour: 10,
            download_link_ttl: time::Duration::minutes(10),
        }
    }
}

/// Everything the service needs to issue passes, for the platforms that are
/// configured. Built once per process from [`WalletConfig`].
pub struct Wallet {
    pub apple: Option<apple::AppleIssuer>,
    pub google: Option<google::GoogleIssuer>,
    /// Where the web app is served from. A pass links into it, and Apple's
    /// web service is reached through it.
    pub web_origin: String,
    pub wording: Arc<wording::WalletWording>,
    pub rules: WalletRules,
    /// Keys the tokens this service derives: an Apple pass's authentication
    /// token and the links that download one. Taken from `APP_SECRET`; empty
    /// in the worker, which derives none.
    key: Vec<u8>,
}

impl Wallet {
    /// A wallet with no platform: the default, and what every deployment has
    /// until it configures one.
    pub fn off(web_origin: &str) -> Self {
        Self::new(&WalletConfig::default(), web_origin, None)
            .expect("the embedded wording has the default language's wallet section")
    }

    /// The configured platforms, for a process whose tokens are keyed by
    /// `secret` (the API's `APP_SECRET`; the worker passes none). Fails if
    /// the wording cannot say what a pass needs.
    pub fn new(
        config: &WalletConfig,
        web_origin: &str,
        secret: Option<&[u8]>,
    ) -> anyhow::Result<Self> {
        let web_origin = web_origin.trim_end_matches('/').to_owned();
        let key = match secret {
            Some(secret) => derive(secret, b"yuppers wallet tokens").to_vec(),
            None => Vec::new(),
        };
        Ok(Self {
            apple: config
                .apple
                .as_ref()
                .map(|apple| apple::AppleIssuer::new(apple.clone(), &web_origin)),
            google: config.google.clone(),
            web_origin,
            wording: Arc::new(wording::WalletWording::embedded()?),
            rules: WalletRules::default(),
            key,
        })
    }

    /// The platforms a pass can be added to, in a fixed order.
    pub fn platforms(&self) -> Vec<WalletPlatform> {
        let mut platforms = Vec::new();
        if self.apple.is_some() {
            platforms.push(WalletPlatform::Apple);
        }
        if self.google.is_some() {
            platforms.push(WalletPlatform::Google);
        }
        platforms
    }

    /// The link to an exchange that a pass carries. Like a notification's, it
    /// opens the exchange for someone signed in and asks anyone else to sign
    /// in; it lets nobody in by itself.
    pub fn exchange_link(&self, exchange: uuid::Uuid) -> String {
        format!("{}/exchanges/{exchange}", self.web_origin)
    }

    /// A value only this service can make from `purpose` and `subject`.
    pub(crate) fn mac(&self, purpose: &[u8], subject: &[u8]) -> [u8; 32] {
        assert!(
            !self.key.is_empty(),
            "wallet tokens are derived only where APP_SECRET is known"
        );
        let mut mac = Hmac::<Sha256>::new_from_slice(&self.key).expect("HMAC takes any key");
        mac.update(purpose);
        mac.update(&[0]);
        mac.update(subject);
        mac.finalize().into_bytes().into()
    }
}

/// HMAC-SHA-256 of `label` under `secret`: a key for one use, so that what is
/// derived for wallets never equals anything derived from the secret
/// elsewhere.
fn derive(secret: &[u8], label: &[u8]) -> [u8; 32] {
    let mut mac = Hmac::<Sha256>::new_from_slice(secret).expect("HMAC takes any key");
    mac.update(label);
    mac.finalize().into_bytes().into()
}
