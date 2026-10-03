//! Background worker: outbox delivery, reminders, expiries, closures and the
//! purge of old network metadata (DESIGN.md §13, §14). Runs as its own process so slow jobs never stall requests.

use std::time::Duration;

use exchange_backend::config::WorkerConfig;
use exchange_backend::domain::Rules;
use exchange_backend::exchanges::reminders::run_reminders;
use exchange_backend::exchanges::service::{purge_network_metadata, run_timers};
use exchange_backend::notifications::outbox::{self, Delivery, DeliveryRules};
use exchange_backend::notifications::wording::Wording;
use exchange_backend::{db, shutdown, telemetry};
use time::OffsetDateTime;

const TICK: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
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

    let mut ticker = tokio::time::interval(TICK);
    let mut stop = std::pin::pin!(shutdown::signal());
    tracing::info!("worker started");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                match run_timers(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(changed) => tracing::info!(changed, "timers ran"),
                    Err(error) => tracing::error!(%error, "timers failed"),
                }
                // After the timers, so an exchange they have just closed is
                // not reminded of anything.
                match run_reminders(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(reminders) => tracing::info!(reminders, "reminders queued"),
                    Err(error) => tracing::error!(%error, "reminders failed"),
                }
                match purge_network_metadata(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(removed) => tracing::info!(removed, "network metadata purged"),
                    Err(error) => tracing::error!(%error, "network metadata purge failed"),
                }
                // After both, so what they just caused goes out in the same
                // pass.
                match outbox::deliver_due(&db, &delivery, OffsetDateTime::now_utc()).await {
                    Ok(delivered) if delivered.is_empty() => {}
                    Ok(delivered) => {
                        tracing::info!(
                            sent = delivered.sent,
                            failed = delivered.failed,
                            given_up = delivered.given_up,
                            dropped = delivered.dropped,
                            "notifications delivered"
                        );
                        // A full batch means more are waiting. Go round again
                        // now rather than a tick later, so a burst drains as
                        // fast as it can be sent instead of one batch per
                        // tick (README, "Load check"). The timers still run
                        // first on every round.
                        if delivered.handled() >= delivery.rules.batch {
                            ticker.reset_immediately();
                        }
                    }
                    Err(error) => tracing::error!(%error, "notification delivery failed"),
                }
            }
            _ = &mut stop => {
                tracing::info!("worker shutting down");
                return Ok(());
            }
        }
    }
}
