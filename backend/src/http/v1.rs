use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use super::{AppState, account, auth, exchanges, record, safety};

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meta", get(meta))
        .route("/auth/codes", post(auth::request_code))
        .route("/auth/sessions", post(auth::create_session))
        .route("/auth/session", delete(auth::delete_session))
        .route("/me", get(account::me).patch(account::update_me))
        .route("/me/identifiers", post(account::add_identifier))
        .route("/exchanges", get(exchanges::list).post(exchanges::create))
        .route("/exchanges/{id}", get(exchanges::get))
        .route("/exchanges/{id}/draft", put(exchanges::save_draft))
        .route("/exchanges/{id}/revisions", post(exchanges::send_revision))
        .route("/exchanges/{id}/commands", post(exchanges::run_command))
        .route("/exchanges/{id}/leave", post(exchanges::leave))
        .route("/exchanges/{id}/history", get(record::history))
        .route("/exchanges/{id}/record", get(record::record))
        .route(
            "/exchanges/{id}/invitation",
            post(exchanges::reissue_invitation),
        )
        .route("/invitations/preview", post(exchanges::preview_invitation))
        .route("/invitations/claim", post(exchanges::claim_invitation))
        .merge(safety::routes())
}

#[derive(Serialize, ToSchema)]
pub struct Meta {
    pub service: String,
    pub version: String,
}

/// Identifies the service and its build.
#[utoipa::path(get, path = "/v1/meta", responses((status = 200, description = "Service identity", body = Meta)))]
pub async fn meta() -> Json<Meta> {
    Json(Meta {
        service: env!("CARGO_PKG_NAME").to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
    })
}
