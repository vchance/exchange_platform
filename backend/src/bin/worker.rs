//! Background worker: outbox delivery, reminders, expiries and closures
//! (DESIGN.md §13). Runs as its own process so slow jobs never stall requests.

use std::time::Duration;

use exchange_backend::config::WorkerConfig;
use exchange_backend::domain::Rules;
use exchange_backend::exchanges::service::run_timers;
use exchange_backend::notifications::outbox::{self, Delivery, DeliveryRules};
use exchange_backend::notifications::wording::Wording;
use exchange_backend::{db, telemetry};
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
    tracing::info!("worker started");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Still to come here: sending reminders.
                match run_timers(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(changed) => tracing::info!(changed, "timers ran"),
                    Err(error) => tracing::error!(%error, "timers failed"),
                }
                // After the timers, so what they just caused goes out in the
                // same pass.
                match outbox::deliver_due(&db, &delivery, OffsetDateTime::now_utc()).await {
                    Ok(delivered) if delivered.is_empty() => {}
                    Ok(delivered) => tracing::info!(
                        sent = delivered.sent,
                        failed = delivered.failed,
                        given_up = delivered.given_up,
                        dropped = delivered.dropped,
                        "notifications delivered"
                    ),
                    Err(error) => tracing::error!(%error, "notification delivery failed"),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("worker shutting down");
                return Ok(());
            }
        }
    }
}
