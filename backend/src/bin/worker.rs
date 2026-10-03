//! Background worker: outbox delivery, reminders, expiries, closures, and the
//! purge of old network metadata and of old sign-in limit counts (DESIGN.md
//! §13, §14). Runs as its own process so slow jobs never stall requests.

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

use exchange_backend::auth::purge_sign_in_limits;
use exchange_backend::config::WorkerConfig;
use exchange_backend::domain::Rules;
use exchange_backend::error::Redacted;
use exchange_backend::exchanges::reminders::run_reminders;
use exchange_backend::exchanges::service::{purge_network_metadata, run_timers};
use exchange_backend::metrics::{self, Text, WorkerMetrics};
use exchange_backend::notifications::outbox::{self, Delivery, DeliveryRules};
use exchange_backend::notifications::wording::Wording;
use exchange_backend::{db, shutdown, telemetry};
use time::OffsetDateTime;
use tokio::sync::watch;

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
