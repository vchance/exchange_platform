use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Context;
use yuppers_backend::config::ApiConfig;
use yuppers_backend::domain::Rules;
use yuppers_backend::http::{self, AppState, Settings, WebApp};
use yuppers_backend::metrics::{self, HttpMetrics, Text};
use yuppers_backend::notifications::outbox::DeliveryRules;
use yuppers_backend::{db, shutdown, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    telemetry::init()?;
    let config = ApiConfig::from_env()?;
    if !config.proxies.trusts_a_header() {
        tracing::warn!(
            "TRUSTED_PROXY_HEADER is not set, so each connection's peer is taken as the \
             requester; behind a reverse proxy or CDN every user then shares the proxy's \
             address and its sign-in limits. Name the proxy's header if there is one."
        );
    }

    let state = AppState {
        db: db::pool(&config.database_url)?,
        settings: Arc::new(Settings {
            app_secret: config.app_secret,
            web_origin: config.web_origin,
            auth: config.auth,
            rules: Rules::default(),
            // A stand-in until counsel-approved consent wording exists
            // (DESIGN.md §14.1); the real version replaces it then.
            consent_version: "draft-1".to_owned(),
            proxies: config.proxies,
            min_client_versions: config.min_client_versions,
        }),
        code_sender: config.code_sender,
        metrics: Arc::new(HttpMetrics::default()),
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

    // On a listener of its own, and only when asked for (docs/operations.md).
    if let Some(addr) = config.metrics_addr {
        let (requests, db) = (state.metrics.clone(), state.db.clone());
        let max_attempts = DeliveryRules::default().max_attempts;
        metrics::serve(addr, move || {
            let (requests, db) = (requests.clone(), db.clone());
            async move {
                let mut text = Text::new();
                requests.render(&mut text);
                metrics::render_pool(&mut text, &db);
                metrics::render_outbox(&mut text, &db, max_attempts).await;
                text.finish()
            }
        })
        .await?;
    }

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
