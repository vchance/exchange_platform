use std::sync::Arc;
use std::time::Instant;

use axum::Router;
use axum::extract::{MatchedPath, Request, State};
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, HeaderName, HeaderValue};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use sqlx::PgPool;
use tracing::Instrument;
use utoipa::OpenApi;
use uuid::Uuid;

use crate::auth::{AuthRules, CodeSender};
use crate::client_version::{self, MinimumClientVersions};
use crate::domain::Rules;
use crate::error::{ErrorBody, ErrorCode};
use crate::metrics::HttpMetrics;

pub mod account;
pub mod auth;
pub mod client_address;
pub mod deletion;
pub mod exchanges;
pub mod extract;
pub mod health;
pub mod record;
pub mod safety;
pub mod v1;
pub mod web;

pub use client_address::{ClientAddress, TrustedProxies};
pub use web::WebApp;

/// What the handlers need besides the database.
pub struct Settings {
    pub app_secret: Vec<u8>,
    pub web_origin: String,
    pub auth: AuthRules,
    pub rules: Rules,
    /// The version of the consent wording a signer must have been shown.
    pub consent_version: String,
    /// Which header, if any, names the requester's address.
    pub proxies: TrustedProxies,
    /// The oldest build of each client that may still change anything
    /// (`crate::client_version`). None required unless configured.
    pub min_client_versions: MinimumClientVersions,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub settings: Arc<Settings>,
    pub code_sender: Arc<dyn CodeSender>,
    /// Request counts and latencies, served when `METRICS_ADDR` is set.
    pub metrics: Arc<HttpMetrics>,
}

/// The whole service: the API, and the web app if there is one to serve.
/// API paths are routed first; the web app answers what is left.
pub fn router(state: AppState, web: Option<WebApp>) -> Router {
    // People sign on these pages, so the page must be ours and nobody
    // else's frame. HSTS only when the origin is HTTPS, or a development
    // setup over plain HTTP would be locked out of itself.
    let hsts = state.settings.web_origin.starts_with("https://");
    let api = Router::new()
        .route("/healthz", get(health::live))
        .route("/readyz", get(health::ready))
        .nest("/v1", v1::router())
        .layer(middleware::from_fn_with_state(
            state.clone(),
            client_version::refuse_old_clients,
        ));
    let app = match web {
        Some(web) => api.fallback_service(web.router()),
        None => api,
    };
    app.layer(middleware::from_fn(move |request, next| {
        security_headers(hsts, request, next)
    }))
    // Added with `Router::layer`, so it runs once routing has picked a
    // route, and the route's template is known.
    .layer(middleware::from_fn_with_state(state.clone(), observe))
    .with_state(state)
}

/// The header a request's ID travels in, both ways.
pub const REQUEST_ID: HeaderName = HeaderName::from_static("x-request-id");

/// The longest request ID taken from a client or proxy.
const REQUEST_ID_MAX: usize = 64;

/// The request's ID: the one a proxy or client sent in `X-Request-Id`, if it
/// is short and harmless, otherwise a new one. Harmless means 1 to 64
/// letters, digits, `-`, `_` or `.`: enough for a UUID or any proxy's own
/// format, and nothing that could forge a log line or carry an email
/// address.
pub fn request_id(headers: &HeaderMap) -> String {
    headers
        .get(&REQUEST_ID)
        .and_then(|value| value.to_str().ok())
        .filter(|id| {
            (1..=REQUEST_ID_MAX).contains(&id.len())
                && id
                    .bytes()
                    .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'))
        })
        .map(str::to_owned)
        .unwrap_or_else(|| Uuid::new_v4().to_string())
}

/// Gives each request an ID, handles it in a span that names it, writes one
/// line when it is answered, and counts it.
///
/// The span holds the method, the path and the request ID, so every line
/// logged while the request is handled carries them. The path only: a query
/// string could carry something a person typed, and never belongs in a log.
/// Nor does anything else from the request or the response: no body, no
/// other header, no token, no cookie.
async fn observe(State(state): State<AppState>, request: Request, next: Next) -> Response {
    let started = Instant::now();
    let id = request_id(request.headers());
    let method = request.method().clone();
    let route = request
        .extensions()
        .get::<MatchedPath>()
        .map(|path| path.as_str().to_owned());
    let span = tracing::info_span!(
        "request",
        method = %method,
        path = request.uri().path(),
        request_id = %id,
    );

    let mut response = next.run(request).instrument(span.clone()).await;

    let elapsed = started.elapsed();
    let status = response.status();
    state
        .metrics
        .observe(&method, route.as_deref(), status, elapsed);
    // To the microsecond: most requests take less than a millisecond.
    let latency_ms = elapsed.as_micros() as f64 / 1000.0;
    span.in_scope(|| {
        tracing::info!(status = status.as_u16(), latency_ms, "request completed");
    });
    if let Ok(value) = HeaderValue::from_str(&id) {
        response.headers_mut().insert(REQUEST_ID, value);
    }
    response
}

async fn security_headers(hsts: bool, request: Request, next: Next) -> Response {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert(X_FRAME_OPTIONS, HeaderValue::from_static("DENY"));
    headers.insert(
        CONTENT_SECURITY_POLICY,
        HeaderValue::from_static("frame-ancestors 'none'"),
    );
    headers.insert(X_CONTENT_TYPE_OPTIONS, HeaderValue::from_static("nosniff"));
    headers.insert(REFERRER_POLICY, HeaderValue::from_static("no-referrer"));
    if hsts {
        headers.insert(
            STRICT_TRANSPORT_SECURITY,
            HeaderValue::from_static("max-age=31536000"),
        );
    }
    response
}

#[derive(OpenApi)]
#[openapi(
    info(title = "Exchange API"),
    paths(
        health::live,
        health::ready,
        v1::meta,
        auth::request_code,
        auth::create_session,
        auth::delete_session,
        account::me,
        account::update_me,
        account::add_identifier,
        deletion::deletion_preview,
        deletion::request_deletion_code,
        deletion::delete_account,
        exchanges::create,
        exchanges::list,
        exchanges::get,
        exchanges::save_draft,
        exchanges::send_revision,
        exchanges::run_command,
        exchanges::leave,
        record::history,
        record::record,
        exchanges::reissue_invitation,
        exchanges::preview_invitation,
        exchanges::claim_invitation,
        safety::report_exchange,
        safety::report_invitation,
        safety::block_status,
        safety::block,
        safety::unblock,
        safety::blocked_people,
    ),
    components(schemas(ErrorBody, ErrorCode, MinimumClientVersions))
)]
struct ApiDoc;

/// The API contract. The `openapi` binary prints it, and the TypeScript client
/// used by web and mobile is generated from that output.
pub fn api_doc() -> utoipa::openapi::OpenApi {
    let mut doc = ApiDoc::openapi();
    // The derive fills these from Cargo metadata this crate does not set.
    doc.info.description = None;
    doc.info.license = None;
    doc
}
