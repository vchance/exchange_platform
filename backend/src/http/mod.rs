use std::sync::Arc;

use axum::Router;
use axum::routing::get;
use sqlx::PgPool;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;

use crate::auth::{AuthRules, CodeSender};
use crate::domain::Rules;
use crate::error::{ErrorBody, ErrorCode};

pub mod account;
pub mod auth;
pub mod exchanges;
pub mod extract;
pub mod health;
pub mod record;
pub mod v1;

/// What the handlers need besides the database.
pub struct Settings {
    pub app_secret: Vec<u8>,
    pub web_origin: String,
    pub auth: AuthRules,
    pub rules: Rules,
    /// The version of the consent wording a signer must have been shown.
    pub consent_version: String,
}

#[derive(Clone)]
pub struct AppState {
    pub db: PgPool,
    pub settings: Arc<Settings>,
    pub code_sender: Arc<dyn CodeSender>,
}

pub fn router(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health::live))
        .route("/readyz", get(health::ready))
        .nest("/v1", v1::router())
        .layer(TraceLayer::new_for_http())
        .with_state(state)
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
        exchanges::create,
        exchanges::list,
        exchanges::get,
        exchanges::save_draft,
        exchanges::send_revision,
        exchanges::run_command,
        record::history,
        record::record,
        exchanges::reissue_invitation,
        exchanges::preview_invitation,
        exchanges::claim_invitation,
    ),
    components(schemas(ErrorBody, ErrorCode))
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
