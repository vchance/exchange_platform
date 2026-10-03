//! The Wallet settings (`.env.example`, docs/wallet.md). Every platform is
//! off until all of its settings are given, and a platform half configured
//! stops the process at start rather than leaving passes half working.
//!
//! | Setting | |
//! |---|---|
//! | `APPLE_PASS_TYPE_ID`, `APPLE_TEAM_ID` | the pass type identifier and the team |
//! | `APPLE_PASS_CERT`, `APPLE_PASS_KEY` | the pass type certificate and its key, PEM, or paths to PEM files |
//! | `APPLE_WWDR_CERT` | Apple's WWDR intermediate certificate, PEM or a path |
//! | `GOOGLE_WALLET_ISSUER_ID` | the issuer ID |
//! | `GOOGLE_WALLET_SERVICE_ACCOUNT` | the service account's JSON key file, a path or the JSON |
//! | `WALLET_DELIVERY` | `log` or `live`: how the worker sends pass updates. Required once a platform is on |
//! | `WALLET_STATUS_ON_FACE` | `detailed` (the default) or `neutral`: how much the status line says |
//!
//! Two problems with Apple's certificates take Apple off, with an error in
//! the log, rather than stopping the process: a WWDR certificate that has
//! expired or did not issue the pass type certificate, and a pass type
//! certificate that has expired (`super::Wallet::apple`). Google and
//! everything else go on.

use std::sync::Arc;

use anyhow::{Context, bail};
use time::OffsetDateTime;

use super::apple::{AppleSettings, Signer};
use super::google::{GoogleIssuer, ServiceAccount};
use super::{StatusOnFace, pem};

/// Reads a setting: the environment in production, a table in the tests.
pub type Lookup<'a> = &'a dyn Fn(&str) -> Option<String>;

/// How the worker sends pass updates. No default, like the other deliveries
/// (`crate::config`): a deployment that turns a platform on says which.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UpdateDelivery {
    /// Write each push and object update to the worker's log. Development.
    Log,
    /// Push through APNs and patch through the Google Wallet API.
    Live,
}

/// What a deployment configured for Wallet passes.
#[derive(Clone, Debug, Default)]
pub struct WalletConfig {
    pub apple: Option<AppleSettings>,
    pub google: Option<GoogleIssuer>,
    /// Set whenever a platform is.
    pub delivery: Option<UpdateDelivery>,
    pub status_on_face: StatusOnFace,
}

impl WalletConfig {
    pub fn from_lookup(get: Lookup<'_>) -> anyhow::Result<Self> {
        let apple = apple(get)?;
        let google = google(get)?;
        let delivery = match optional(get, "WALLET_DELIVERY").as_deref() {
            None if apple.is_none() && google.is_none() => None,
            None => bail!(
                "WALLET_DELIVERY is not set: with a Wallet platform configured, say how the \
                 worker sends pass updates, `live` or `log`"
            ),
            Some("log") => Some(UpdateDelivery::Log),
            Some("live") => Some(UpdateDelivery::Live),
            Some(other) => bail!("WALLET_DELIVERY={other} is not supported; use `live` or `log`"),
        };
        let status_on_face = match optional(get, "WALLET_STATUS_ON_FACE").as_deref() {
            None | Some("detailed") => StatusOnFace::Detailed,
            Some("neutral") => StatusOnFace::Neutral,
            Some(other) => {
                bail!("WALLET_STATUS_ON_FACE={other} is not supported; use `detailed` or `neutral`")
            }
        };
        Ok(Self {
            apple,
            google,
            delivery,
            status_on_face,
        })
    }
}

fn optional(get: Lookup<'_>, name: &str) -> Option<String> {
    get(name)
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// All of a platform's settings, or none. Names the ones missing.
fn all_or_none<const N: usize>(
    get: Lookup<'_>,
    names: [&str; N],
) -> anyhow::Result<Option<[String; N]>> {
    let values = names.map(|name| optional(get, name));
    if values.iter().all(Option::is_none) {
        return Ok(None);
    }
    let missing: Vec<&str> = names
        .iter()
        .zip(&values)
        .filter(|(_, value)| value.is_none())
        .map(|(name, _)| *name)
        .collect();
    if !missing.is_empty() {
        bail!(
            "{} must be set too, or none of {}",
            missing.join(", "),
            names.join(", ")
        );
    }
    Ok(Some(values.map(|value| value.expect("checked above"))))
}

fn apple(get: Lookup<'_>) -> anyhow::Result<Option<AppleSettings>> {
    let Some([pass_type_id, team_id, cert, key, wwdr]) = all_or_none(
        get,
        [
            "APPLE_PASS_TYPE_ID",
            "APPLE_TEAM_ID",
            "APPLE_PASS_CERT",
            "APPLE_PASS_KEY",
            "APPLE_WWDR_CERT",
        ],
    )?
    else {
        return Ok(None);
    };
    let reverse_domain = pass_type_id.strip_prefix("pass.").is_some_and(|rest| {
        rest.split('.').count() >= 2
            && rest.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
    });
    if !reverse_domain {
        bail!(
            "APPLE_PASS_TYPE_ID={pass_type_id} is not a pass type identifier such as pass.app.yuppers"
        );
    }
    if team_id.len() != 10 || !team_id.bytes().all(|byte| byte.is_ascii_alphanumeric()) {
        bail!("APPLE_TEAM_ID={team_id} is not a team ID, ten letters and digits");
    }
    let signer = Signer::from_pem(
        &pem::text("APPLE_PASS_CERT", &cert)?,
        &pem::text("APPLE_PASS_KEY", &key)?,
        &pem::text("APPLE_WWDR_CERT", &wwdr)?,
    )?;
    // A certificate Apple issued for another pass type or team would sign
    // passes Wallet then refuses.
    let (uid, team) = signer.names();
    if uid.as_deref().is_some_and(|uid| uid != pass_type_id) {
        bail!(
            "APPLE_PASS_CERT is for {}, not APPLE_PASS_TYPE_ID={pass_type_id}",
            uid.unwrap_or_default()
        );
    }
    if team.as_deref().is_some_and(|team| team != team_id) {
        bail!(
            "APPLE_PASS_CERT is for team {}, not APPLE_TEAM_ID={team_id}",
            team.unwrap_or_default()
        );
    }
    let now = OffsetDateTime::now_utc();
    // The WWDR certificate must be valid and must have issued the pass type
    // certificate, or Wallet refuses every pass. Apple is off then; the rest
    // of the service is not stopped by it.
    if let Err(problem) = signer.check_issuer(now) {
        tracing::error!(
            problem,
            "APPLE_WWDR_CERT does not vouch for APPLE_PASS_CERT: Apple Wallet passes are off (docs/wallet.md)"
        );
        return Ok(None);
    }
    // An expired pass type certificate takes Apple off too, here and
    // whenever it expires while the process runs (`super::Wallet::apple`).
    let expires = signer.not_after();
    if expires <= now {
        tracing::error!(
            %expires,
            "APPLE_PASS_CERT has expired: Apple Wallet passes are off until it is renewed (docs/wallet.md)"
        );
    } else if expires - now < time::Duration::days(30) {
        tracing::warn!(%expires, "APPLE_PASS_CERT expires within 30 days; renew it (docs/wallet.md)");
    }
    Ok(Some(AppleSettings {
        pass_type_id,
        team_id,
        signer: Arc::new(signer),
    }))
}

fn google(get: Lookup<'_>) -> anyhow::Result<Option<GoogleIssuer>> {
    let Some([issuer_id, account]) = all_or_none(
        get,
        ["GOOGLE_WALLET_ISSUER_ID", "GOOGLE_WALLET_SERVICE_ACCOUNT"],
    )?
    else {
        return Ok(None);
    };
    if !issuer_id.bytes().all(|byte| byte.is_ascii_digit()) {
        bail!("GOOGLE_WALLET_ISSUER_ID={issuer_id} is not an issuer ID, which is digits");
    }
    let json = if account.starts_with('{') {
        account
    } else {
        std::fs::read_to_string(&account).with_context(|| {
            format!("GOOGLE_WALLET_SERVICE_ACCOUNT: cannot read the file {account}")
        })?
    };
    Ok(Some(GoogleIssuer {
        issuer_id,
        account: Arc::new(ServiceAccount::from_json(&json)?),
    }))
}
