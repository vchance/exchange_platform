//! Applies database migrations. Connects with `MIGRATION_DATABASE_URL` (the
//! schema owner), falling back to `DATABASE_URL` in development.

use anyhow::Context;
use exchange_backend::{db, telemetry};
use sqlx::postgres::PgPoolOptions;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
    dotenvy::dotenv().ok();

    let url = std::env::var("MIGRATION_DATABASE_URL")
        .or_else(|_| std::env::var("DATABASE_URL"))
        .context("set MIGRATION_DATABASE_URL or DATABASE_URL")?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&url)
        .await?;
    db::MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");
    Ok(())
}
