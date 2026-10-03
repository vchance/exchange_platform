//! The worker's side of Wallet passes: take the passes marked for an update,
//! draw each one's face as it now is, and when it changed, tell the
//! platform (DESIGN.md §11).
//!
//! * **Apple.** The pass's Last-Modified moves on, then every device
//!   registered for it gets an empty push; each then fetches the pass from
//!   the web service. A device Apple says is gone is forgotten.
//! * **Google.** The object is patched with the new face. The object is
//!   created when the save link is made, so Google has it; should it not,
//!   the face is not counted as delivered, and the next update sends it
//!   again.
//!
//! A revoked pass gets its void face once. Its devices are kept until each
//! has been told, in the list of its changed passes, to fetch it (Apple's
//! protocol is a push that wakes the device, then the list, then the pass),
//! or for a bounded time, and are forgotten after: nothing more is ever sent
//! for it. A pass whose account is suspended is not updated at all.
//!
//! Each pass of the worker also marks the passes whose face may have
//! changed with the date ("Due soon", "Overdue"), once a day in each
//! exchange's timezone, and forgets what has had its time.
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
        let wallet = Arc::new(Wallet::new(config, web_origin)?);
        let apple: Option<Arc<dyn PassPush>> = match (&config.apple, delivery) {
            (None, _) => None,
            (Some(_), UpdateDelivery::Log) => Some(Arc::new(super::apple::push::LogPush)),
            (Some(apple), UpdateDelivery::Live) => Some(Arc::new(
                super::apple::push::ApnsPush::new(&apple.signer, &apple.pass_type_id)?,
            )),
        };
        let google = wallet.google_objects.clone();
        Ok(Some(Self {
            wallet,
            apple,
            google,
            rules: UpdateRules::default(),
        }))
    }

    fn platforms(&self) -> Vec<WalletPlatform> {
        let mut platforms = Vec::new();
        if self.apple.is_some() && self.wallet.apple().is_some() {
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
    housekeeping(db, delivery, now).await?;
    let claimed = store::claim_due(
        db,
        now,
        delivery.rules.batch,
        delivery.rules.lease,
        &platforms,
    )
    .await?;
    for pass in claimed {
        match update(db, rules, delivery, &pass, now).await {
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

/// What every pass of the worker does besides sending: mark the passes
/// whose day moved on, and forget void registrations and download links
/// that have had their time.
async fn housekeeping(
    db: &PgPool,
    delivery: &WalletDelivery,
    now: OffsetDateTime,
) -> Result<(), sqlx::Error> {
    let rules = &delivery.wallet.rules;
    store::mark_new_days(db, now).await?;
    store::purge_void_registrations(
        db,
        now,
        rules.void_fetch_grace,
        rules.void_registration_kept,
    )
    .await?;
    store::purge_download_links(db, now).await?;
    Ok(())
}

/// Updates one pass, its face drawn as at `now`. Returns whether anything
/// was sent; an error is the failure to record and retry.
async fn update(
    db: &PgPool,
    rules: &Rules,
    delivery: &WalletDelivery,
    claimed: &Claimed,
    now: OffsetDateTime,
) -> Result<bool, String> {
    let wallet = &delivery.wallet;
    let pass = &claimed.pass;
    let database = |error: sqlx::Error| Redacted(&error).to_string();
    // Suspended: not updated (a deleted account's passes are voided, and
    // get their void face).
    if pass.frozen && !pass.voided {
        store::delivered(db, claimed, None, 0, now.date())
            .await
            .map_err(database)?;
        return Ok(false);
    }
    let face = store::face(db, rules, wallet, pass, now)
        .await
        .map_err(|error| format!("the face could not be drawn: {:?}", error.code))?;
    let hash = store::face_hash(&face.model);

    if claimed.delivered_hash.as_deref() == Some(&hash[..]) {
        store::delivered(db, claimed, Some(&hash), face.version, face.day)
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
                    Ok(PushOutcome::Unregistered) => store::forget_device(db, pass.id, device)
                        .await
                        .map_err(database)?,
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
            // A voided pass's devices are kept until each has asked for the
            // list that names it (`store::purge_void_registrations`).
        }
        WalletPlatform::Google => {
            let objects = delivery.google.as_ref().ok_or("Google is not configured")?;
            let issuer = wallet.google.as_ref().ok_or("Google is not configured")?;
            let object_id = issuer.object_id(&pass.serial);
            let object = issuer.object(&face.model, &pass.serial);
            match objects.patch(&object_id, &object).await {
                Ok(PatchOutcome::Updated) => {}
                Ok(PatchOutcome::NotSaved) => {
                    // Not delivered: the next update, or the next link,
                    // carries the face then.
                    store::delivered(db, claimed, None, face.version, face.day)
                        .await
                        .map_err(database)?;
                    return Ok(false);
                }
                Err(error) => return Err(format!("{error:#}")),
            }
        }
    }
    store::delivered(db, claimed, Some(&hash), face.version, face.day)
        .await
        .map_err(database)?;
    Ok(true)
}
