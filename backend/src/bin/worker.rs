//! Background worker: outbox delivery, reminders, expiries and closures
//! (DESIGN.md §13). Runs as its own process so slow jobs never stall requests.

use std::time::Duration;

use exchange_backend::{config, db, telemetry};

const TICK: Duration = Duration::from_secs(5);

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
    let _db = db::pool(&config::database_url()?)?;

    let mut ticker = tokio::time::interval(TICK);
    tracing::info!("worker started");

    loop {
        tokio::select! {
            _ = ticker.tick() => {
                // Jobs are added here as they are built: drain the outbox,
                // send reminders, expire revisions and invitations, close
                // exchanges whose windows have lapsed.
                tracing::debug!("worker tick");
            }
            _ = tokio::signal::ctrl_c() => {
                tracing::info!("worker shutting down");
                return Ok(());
            }
        }
    }
}
