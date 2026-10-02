//! Background worker: outbox delivery, reminders, expiries and closures
//! (DESIGN.md §13). Runs as its own process so slow jobs never stall requests.

use std::time::Duration;

use exchange_backend::domain::Rules;
use exchange_backend::exchanges::service::run_timers;
use exchange_backend::{config, db, telemetry};
use time::OffsetDateTime;

const TICK: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
    let db = db::pool(&config::database_url()?)?;
    let rules = Rules::default();

    let mut ticker = tokio::time::interval(TICK);
    tracing::info!("worker started");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Still to come here: draining the outbox and sending reminders.
                match run_timers(&db, &rules, OffsetDateTime::now_utc()).await {
                    Ok(0) => {}
                    Ok(changed) => tracing::info!(changed, "timers ran"),
                    Err(error) => tracing::error!(%error, "timers failed"),
                }
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("worker shutting down");
                return Ok(());
            }
        }
    }
}
