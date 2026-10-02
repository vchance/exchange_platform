use std::sync::Arc;

use axum::Router;
use axum::extract::Request;
use axum::http::HeaderValue;
use axum::http::header::{
    CONTENT_SECURITY_POLICY, REFERRER_POLICY, STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS,
    X_FRAME_OPTIONS,
};
use axum::middleware::{self, Next};
use axum::response::Response;
use axum::routing::get;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

use crate::auth::{AuthRules, CodeSender};
use crate::client_version::{self, MinimumClientVersions};
use crate::domain::Rules;
use crate::error::{ErrorBody, ErrorCode};

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
    .layer(TraceLayer::new_for_http().make_span_with(request_span))
    .with_state(state)
}

/// The span a request is handled in. The path only: a query string could
/// carry something a person typed, and never belongs in a log.
fn request_span(request: &Request) -> tracing::Span {
    tracing::info_span!(
        "request",
        method = %request.method(),
        path = request.uri().path(),
    )
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
