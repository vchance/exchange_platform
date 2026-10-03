use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use super::{AppState, account, auth, deletion, devices, exchanges, record, safety};
use crate::client_version::MinimumClientVersions;

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/meta", get(meta))
        .route("/auth/codes", post(auth::request_code))
        .route("/auth/sessions", post(auth::create_session))
        .route("/auth/session", delete(auth::delete_session))
        .route("/me", get(account::me).patch(account::update_me))
        .route("/me/identifiers", post(account::add_identifier))
        .route(
            "/me/deletion",
            get(deletion::deletion_preview).post(deletion::delete_account),
        )
        .route("/me/deletion/codes", post(deletion::request_deletion_code))
        .route("/me/devices", put(devices::register_device))
        .route("/me/devices/{id}", delete(devices::remove_device))
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
    /// The oldest build of each client that may still change anything. A
    /// client below its minimum shows that it must be updated; its changes
    /// are refused with `CLIENT_TOO_OLD`. Absent for a client with no minimum.
    pub minimum_client_versions: MinimumClientVersions,
    /// Whether the service sends push notifications. An app offers them, and
    /// registers its device (`PUT /v1/me/devices`), only when it does.
    pub push_notifications: bool,
}

/// Identifies the service and its build, and says how old a client may be.
#[utoipa::path(get, path = "/v1/meta", responses((status = 200, description = "Service identity", body = Meta)))]
pub async fn meta(State(state): State<AppState>) -> Json<Meta> {
    Json(Meta {
        service: env!("CARGO_PKG_NAME").to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        minimum_client_versions: state.settings.min_client_versions.clone(),
        push_notifications: state.settings.push_notifications,
    })
}
