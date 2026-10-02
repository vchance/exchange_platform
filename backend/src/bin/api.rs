use exchange_backend::config::Config;
use exchange_backend::http::{self, AppState};
use exchange_backend::{db, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
    let config = Config::from_env()?;

    let state = AppState {
        db: db::pool(&config.database_url)?,
    };

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "api listening");

    axum::serve(listener, http::router(state))
        .with_graceful_shutdown(shutdown())
        .await?;
    Ok(())
}

async fn shutdown() {
    tokio::signal::ctrl_c().await.ok();
    tracing::info!("api shutting down");
}
