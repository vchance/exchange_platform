//! The worker's side of Wallet passes: take the passes marked for an update,
//! draw each one's face as it now is, and when it changed, tell the
//! platform (DESIGN.md §11).
//!
//! * **Apple.** The pass's Last-Modified moves on, then every device
//!   registered for it gets an empty push; each then fetches the pass from
//!   the web service. A device Apple says is gone is forgotten.
//! * **Google.** The object is patched with the new face. One the person
//!   never saved does not exist at Google, which is fine: the save link
//!   carries the latest face whenever it is used.
//!
//! A revoked pass gets its void face once, and then its devices are
//! forgotten: nothing more is ever sent for it.
//!
//! Delivery is at least once, like the outbox's: a worker that dies after a
//! push and before recording it pushes again, and a device that is pushed
//! twice asks twice and is told the second time that nothing changed.

use std::sync::Arc;

use sqlx::PgPool;
use time::{Duration, OffsetDateTime};

use super::apple::push::{PassPush, PushOutcome};
use super::config::{UpdateDelivery, WalletConfig};
use super::google::objects::{PatchOutcome, WalletObjects};
use super::store::{self, Claimed};
use super::{Wallet, WalletPlatform};
use crate::domain::Rules;
use crate::error::Redacted;

/// The numbers behind sending updates. Placeholders.
#[derive(Clone, Debug)]
pub struct UpdateRules {
    pub max_attempts: i32,
    pub retry_after: Duration,
    pub retry_ceiling: Duration,
    /// Passes one pass of the worker takes at most.
    pub batch: i64,
    /// How long a taken pass stays the worker's.
    pub lease: Duration,
}

impl Default for UpdateRules {
    fn default() -> Self {
        Self {
            max_attempts: 8,
            retry_after: Duration::minutes(1),
            retry_ceiling: Duration::hours(1),
            batch: 50,
            lease: Duration::minutes(2),
        }
    }
}

impl UpdateRules {
    fn backoff(&self, earlier_failures: i32) -> Duration {
        let doublings = earlier_failures.clamp(0, 20);
        (self.retry_after * (1_i32 << doublings)).min(self.retry_ceiling)
    }
}

/// What the worker needs to keep passes up to date.
pub struct WalletDelivery {
    pub wallet: Arc<Wallet>,
    pub apple: Option<Arc<dyn PassPush>>,
    pub google: Option<Arc<dyn WalletObjects>>,
    pub rules: UpdateRules,
}

impl WalletDelivery {
    /// The delivery a configuration asks for, or none when no platform is
    /// configured.
    pub fn from_config(config: &WalletConfig, web_origin: &str) -> anyhow::Result<Option<Self>> {
        let Some(delivery) = config.delivery else {
            return Ok(None);
        };
        let wallet = Arc::new(Wallet::new(config, web_origin, None)?);
        let apple: Option<Arc<dyn PassPush>> = match (&config.apple, delivery) {
            (None, _) => None,
            (Some(_), UpdateDelivery::Log) => Some(Arc::new(super::apple::push::LogPush)),
            (Some(apple), UpdateDelivery::Live) => Some(Arc::new(
                super::apple::push::ApnsPush::new(&apple.signer, &apple.pass_type_id)?,
            )),
        };
        let google: Option<Arc<dyn WalletObjects>> = match (&config.google, delivery) {
            (None, _) => None,
            (Some(_), UpdateDelivery::Log) => Some(Arc::new(super::google::objects::LogObjects)),
            (Some(google), UpdateDelivery::Live) => Some(Arc::new(
                super::google::objects::ApiObjects::new(google.account.clone())?,
            )),
        };
        Ok(Some(Self {
            wallet,
            apple,
            google,
            rules: UpdateRules::default(),
        }))
    }

    fn platforms(&self) -> Vec<WalletPlatform> {
        let mut platforms = Vec::new();
        if self.apple.is_some() && self.wallet.apple.is_some() {
            platforms.push(WalletPlatform::Apple);
        }
        if self.google.is_some() && self.wallet.google.is_some() {
            platforms.push(WalletPlatform::Google);
        }
        platforms
    }
}

/// What one pass over the marked passes did.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Updated {
    /// Passes whose new face went out.
    pub sent: usize,
    /// Passes whose face had not changed, so nothing was sent.
    pub unchanged: usize,
    pub failed: usize,
    pub given_up: usize,
}

impl Updated {
    pub fn handled(&self) -> usize {
        self.sent + self.unchanged + self.failed
    }
}

/// Updates the passes due at `now`, up to a batch. Safe to run from several
/// workers at once.
pub async fn deliver_due(
    db: &PgPool,
    rules: &Rules,
    delivery: &WalletDelivery,
    now: OffsetDateTime,
) -> Result<Updated, sqlx::Error> {
    let platforms = delivery.platforms();
    let mut updated = Updated::default();
    if platforms.is_empty() {
        return Ok(updated);
    }
    let claimed = store::claim_due(
        db,
        now,
        delivery.rules.batch,
        delivery.rules.lease,
        &platforms,
    )
    .await?;
    for pass in claimed {
        match update(db, rules, delivery, &pass).await {
            Ok(true) => updated.sent += 1,
            Ok(false) => updated.unchanged += 1,
            Err(error) => {
                updated.failed += 1;
                let retry_at = OffsetDateTime::now_utc() + delivery.rules.backoff(pass.attempts);
                let given_up =
                    store::failed(db, &pass, &error, retry_at, delivery.rules.max_attempts).await?;
                if given_up {
                    updated.given_up += 1;
                    tracing::error!(pass = %pass.pass.id, error, "wallet pass update given up on");
                } else {
                    tracing::warn!(pass = %pass.pass.id, error, "wallet pass not updated; will retry");
                }
            }
        }
    }
    Ok(updated)
}

/// Updates one pass. Returns whether anything was sent; an error is the
/// failure to record and retry.
async fn update(
    db: &PgPool,
    rules: &Rules,
    delivery: &WalletDelivery,
    claimed: &Claimed,
) -> Result<bool, String> {
    let wallet = &delivery.wallet;
    let pass = &claimed.pass;
    let (model, version) = store::face(db, rules, wallet, pass)
        .await
        .map_err(|error| format!("the face could not be drawn: {:?}", error.code))?;
    let hash = store::face_hash(&model);
    let database = |error: sqlx::Error| Redacted(&error).to_string();

    if claimed.delivered_hash.as_deref() == Some(&hash[..]) {
        store::delivered(db, claimed, &hash, version)
            .await
            .map_err(database)?;
        return Ok(false);
    }

    match pass.platform {
        WalletPlatform::Apple => {
            let push = delivery.apple.as_ref().ok_or("Apple is not configured")?;
            // The new face's time first, committed, so that a device asking
            // after the push finds it changed.
            store::publish_face(db, pass.id, &hash)
                .await
                .map_err(database)?;
            let devices = store::registrations(db, pass.id).await.map_err(database)?;
            let mut failures = Vec::new();
            for (device, token) in &devices {
                match push.push(token).await {
                    Ok(PushOutcome::Sent) => {}
                    Ok(PushOutcome::Unregistered) => {
                        store::forget_devices(db, pass.id, Some(device))
                            .await
                            .map_err(database)?
                    }
                    Err(error) => failures.push(format!("{error:#}")),
                }
            }
            if !failures.is_empty() {
                return Err(format!(
                    "{} of {} pushes failed: {}",
                    failures.len(),
                    devices.len(),
                    failures.join("; ")
                ));
            }
            // Told it is void; nothing more will ever be sent for it.
            if pass.voided {
                store::forget_devices(db, pass.id, None)
                    .await
                    .map_err(database)?;
            }
        }
        WalletPlatform::Google => {
            let objects = delivery.google.as_ref().ok_or("Google is not configured")?;
            let issuer = wallet.google.as_ref().ok_or("Google is not configured")?;
            let object_id = issuer.object_id(&pass.serial);
            let object = issuer.object(&model, &pass.serial);
            match objects.patch(&object_id, &object).await {
                Ok(PatchOutcome::Updated | PatchOutcome::NotSaved) => {}
                Err(error) => return Err(format!("{error:#}")),
            }
        }
    }
    store::delivered(db, claimed, &hash, version)
        .await
        .map_err(database)?;
    Ok(true)
}
