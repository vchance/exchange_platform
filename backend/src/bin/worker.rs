//! Background worker: outbox delivery, reminders, expiries, closures, and the
//! purge of old network metadata and of old sign-in limit counts (DESIGN.md
//! §13, §14). Runs as its own process so slow jobs never stall requests.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use time::OffsetDateTime;
use tokio::sync::watch;
use yuppers_backend::auth::purge_sign_in_limits;
use yuppers_backend::config::WorkerConfig;
use yuppers_backend::domain::Rules;
use yuppers_backend::error::Redacted;
use yuppers_backend::exchanges::reminders::run_reminders;
use yuppers_backend::exchanges::service::{purge_network_metadata, run_timers};
use yuppers_backend::metrics::{self, Text, WorkerMetrics};
use yuppers_backend::notifications::outbox::{self, Delivery, DeliveryRules};
use yuppers_backend::notifications::push::{self, PushDelivery, ReceiptRules};
use yuppers_backend::notifications::wording::Wording;
use yuppers_backend::{db, shutdown, telemetry};

const TICK: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init()?;
    let config = WorkerConfig::from_env()?;
    let db = db::pool(&config.database_url)?;
    let rules = Rules::default();
    let delivery = Delivery {
        sender: config.email_sender,
        // Checked now, so wording that cannot say everything stops the
        // worker from starting instead of failing one message at a time.
        wording: Wording::embedded()?,
        web_origin: config.web_origin,
        rules: DeliveryRules::default(),
    };
    let push_delivery = PushDelivery {
        sender: config.push_sender,
        wording: Wording::embedded()?,
        rules: DeliveryRules::default(),
    };
    let receipt_rules = ReceiptRules::default();
    // When the push service was last asked for receipts.
    let mut last_receipts: Option<std::time::Instant> = None;
    if push_delivery.sender.is_none() {
        tracing::info!("push notifications are off (PUSH_DELIVERY)");
    }

    // On a listener of its own, and only when asked for (docs/operations.md).
    let metrics = Arc::new(WorkerMetrics::default());
    if let Some(addr) = config.metrics_addr {
        let (jobs, db) = (metrics.clone(), db.clone());
        let max_attempts = delivery.rules.max_attempts;
        metrics::serve(addr, move || {
            let (jobs, db) = (jobs.clone(), db.clone());
            async move {
                let mut text = Text::new();
                jobs.render(&mut text);
                metrics::render_pool(&mut text, &db);
                metrics::render_outbox(&mut text, &db, max_attempts).await;
                text.finish()
            }
        })
        .await?;
    }

    let mut ticker = tokio::time::interval(TICK);
    // Set once the process is asked to stop. Delivery looks at it between
    // two messages, so a long batch does not keep the worker from stopping.
    let stopping = Arc::new(AtomicBool::new(false));
    let (stop_tx, mut stop_rx) = watch::channel(false);
    {
        let stopping = stopping.clone();
        tokio::spawn(async move {
            shutdown::signal().await;
            stopping.store(true, Ordering::Relaxed);
            let _ = stop_tx.send(true);
        });
    }
    tracing::info!("worker started");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let timers = run_timers(&db, &rules, OffsetDateTime::now_utc()).await;
                metrics.timers(&timers);
                match timers {
                    Ok(0) => {}
                    Ok(changed) => tracing::info!(changed, "timers ran"),
                    Err(error) => tracing::error!(error = %Redacted(&error), "timers failed"),
                }
                // After the timers, so an exchange they have just closed is
                // not reminded of anything.
                let reminders = run_reminders(&db, &rules, OffsetDateTime::now_utc()).await;
                metrics.reminders(&reminders);
                match reminders {
                    Ok(0) => {}
                    Ok(reminders) => tracing::info!(reminders, "reminders queued"),
                    Err(error) => tracing::error!(error = %Redacted(&error), "reminders failed"),
                }
                match purge_network_metadata(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::info!(removed, "network metadata purged"),
                    Err(error) => tracing::error!(error = %Redacted(&error), "network metadata purge failed"),
                }
                match purge_sign_in_limits(&db).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::info!(removed, "old sign-in limit counts removed"),
                    Err(error) => tracing::error!(error = %Redacted(&error), "sign-in limit purge failed"),
                }
                // After both, so what they just caused goes out in the same
                // pass.
                let delivered = outbox::deliver_due_until(
                    &db,
                    &delivery,
                    OffsetDateTime::now_utc(),
                    || stopping.load(Ordering::Relaxed),
                )
                .await;
                if let Ok(delivered) = &delivered {
                    metrics.delivered(delivered);
                }
                match delivered {
                    Ok(delivered) if delivered.is_empty() => {}
                    Ok(delivered) => {
                        tracing::info!(
                            sent = delivered.sent,
                            failed = delivered.failed,
                            given_up = delivered.given_up,
                            dropped = delivered.dropped,
                            "notifications delivered"
                        );
                        // A full batch, or one cut short by its time
                        // budget, means more may be waiting. Go round again
                        // now rather than a tick later, so a burst drains as
                        // fast as it can be sent instead of one batch per
                        // tick (README, "Load check"). The timers still run
                        // first on every round.
                        if delivered.handled() >= delivery.rules.batch || delivered.cut_short {
                            ticker.reset_immediately();
                        }
                    }
                    Err(error) => tracing::error!(error = %Redacted(&error), "notification delivery failed"),
                }
                // The same notices by push, to those with the app.
                let pushed = push::deliver_push_due_until(
                    &db,
                    &push_delivery,
                    OffsetDateTime::now_utc(),
                    || stopping.load(Ordering::Relaxed),
                )
                .await;
                if let Ok(pushed) = &pushed {
                    metrics.pushed(pushed);
                }
                match pushed {
                    Ok(pushed) if pushed.rows.is_empty() => {}
                    Ok(pushed) => {
                        tracing::info!(
                            sent = pushed.rows.sent,
                            failed = pushed.rows.failed,
                            given_up = pushed.rows.given_up,
                            dropped = pushed.rows.dropped,
                            devices_removed = pushed.devices_removed,
                            "push notifications delivered"
                        );
                        if pushed.rows.handled() >= push_delivery.rules.batch || pushed.rows.cut_short {
                            ticker.reset_immediately();
                        }
                    }
                    Err(error) => tracing::error!(error = %Redacted(&error), "push delivery failed"),
                }
                let receipts_due = last_receipts
                    .is_none_or(|last: std::time::Instant| last.elapsed() >= receipt_rules.every);
                if let Some(sender) = push_delivery.sender.as_ref().filter(|_| receipts_due) {
                    last_receipts = Some(std::time::Instant::now());
                    let read = push::check_receipts(
                        &db,
                        sender.as_ref(),
                        &receipt_rules,
                        OffsetDateTime::now_utc(),
                    )
                    .await;
                    metrics.receipts(&read);
                    match read {
                        Ok(read) if read.devices_removed > 0 => tracing::info!(
                            devices_removed = read.devices_removed,
                            "push receipts read"
                        ),
                        Ok(_) => {}
                        // The service's own words are never in it.
                        Err(error) => tracing::warn!(%error, "push receipts could not be read"),
                    }
                }
                match push::purge_devices(&db).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::info!(removed, "devices of ended sessions removed"),
                    Err(error) => tracing::error!(error = %Redacted(&error), "device purge failed"),
                }
                let now = OffsetDateTime::now_utc().unix_timestamp();
                metrics.pass_finished(u64::try_from(now).unwrap_or(0));
            }
            _ = stop_rx.wait_for(|stop| *stop) => {
                tracing::info!("worker shutting down");
                return Ok(());
            }
        }
    }
}
