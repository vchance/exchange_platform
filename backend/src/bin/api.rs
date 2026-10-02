use std::sync::Arc;

use exchange_backend::auth::AuthRules;
use exchange_backend::config::ApiConfig;
use exchange_backend::domain::Rules;
use exchange_backend::http::{self, AppState, Settings};
use exchange_backend::{db, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init();
    let config = ApiConfig::from_env()?;

    let state = AppState {
        db: db::pool(&config.database_url)?,
        settings: Arc::new(Settings {
            app_secret: config.app_secret,
            web_origin: config.web_origin,
            auth: AuthRules::default(),
            rules: Rules::default(),
            // A stand-in until counsel-approved consent wording exists
            // (DESIGN.md §14.1); the real version replaces it then.
            consent_version: "draft-1".to_owned(),
        }),
        code_sender: config.code_sender,
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
