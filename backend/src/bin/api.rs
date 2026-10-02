use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use exchange_backend::auth::AuthRules;
use exchange_backend::config::ApiConfig;
use exchange_backend::domain::Rules;
use exchange_backend::http::{self, AppState, Settings, WebApp};
use exchange_backend::{db, shutdown, telemetry};

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
            proxies: config.proxies,
        }),
        code_sender: config.code_sender,
    };

    let web = match &config.web_dir {
        Some(directory) => {
            let web = WebApp::open(directory)
                .with_context(|| format!("WEB_DIR={}", directory.display()))?;
            tracing::info!(
                directory = %directory.display(),
                languages = ?web.languages(),
                "serving the web app"
            );
            Some(web)
        }
        None => None,
    };

    let listener = tokio::net::TcpListener::bind(config.bind_addr).await?;
    tracing::info!(addr = %config.bind_addr, "api listening");

    // With the peer address of each connection, which is the client's unless
    // a trusted proxy header says otherwise.
    let app = http::router(state, web).into_make_service_with_connect_info::<SocketAddr>();
    axum::serve(listener, app)
        .with_graceful_shutdown(async {
            shutdown::signal().await;
            tracing::info!("api shutting down");
        })
        .await?;
    Ok(())
}
