//! Applies database migrations. Connects with `MIGRATION_DATABASE_URL` (the
//! schema owner), falling back to `DATABASE_URL` in development.
//!
//! With `MIGRATE_CREATE_APP_ROLE=true` it first creates the application role
//! `exchange_app` with the password in `APP_DB_PASSWORD`, if there is no such
//! role, and checks that the role can do nothing but log in
//! (`yuppers_backend::app_role`). For a managed database where nobody runs
//! `psql` before the first deploy (docs/deploy-render.md); off otherwise.

use sqlx::postgres::PgPoolOptions;
use yuppers_backend::app_role::{self, APP_ROLE, Outcome};
use yuppers_backend::build_info::BuildInfo;
use yuppers_backend::config::MigrateConfig;
use yuppers_backend::{db, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init()?;
    BuildInfo::current().log_start("migrate");
    let config = MigrateConfig::from_env()?;

    let pool = PgPoolOptions::new()
        .max_connections(1)
        .connect(&config.database_url)
        .await?;
    if let Some(password) = &config.create_app_role {
        match app_role::ensure(&pool, APP_ROLE, password).await? {
            Outcome::Created => tracing::info!(role = APP_ROLE, "application role created"),
            Outcome::AlreadyExisted => tracing::info!(
                role = APP_ROLE,
                "application role already exists; its password is left as it is"
            ),
        }
    }
    db::MIGRATOR.run(&pool).await?;
    tracing::info!("migrations applied");
    Ok(())
}
