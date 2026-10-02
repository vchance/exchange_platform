use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};

/// Migrations embedded at build time. Applied by the `migrate` binary, which
/// connects as the schema owner; the `api` and `worker` processes connect as
/// the restricted application role and never run them (DESIGN.md §13.2).
pub static MIGRATOR: Migrator = sqlx::migrate!("./migrations");

/// Creates the connection pool without connecting. The first query opens a
/// connection, so the process can start and report readiness on its own terms.
pub fn pool(database_url: &str) -> Result<PgPool, sqlx::Error> {
    PgPoolOptions::new()
        .max_connections(10)
        .connect_lazy(database_url)
}
