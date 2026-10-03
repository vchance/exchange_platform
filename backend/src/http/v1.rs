use std::sync::Arc;

use axum::extract::State;
use axum::routing::{delete, get, post, put};
use axum::{Extension, Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use super::{AppState, account, auth, deletion, exchanges, record, safety, wallet};
use crate::client_version::MinimumClientVersions;
use crate::wallet::{Wallet, WalletPlatform};

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
        .merge(wallet::routes())
}

#[derive(Serialize, ToSchema)]
pub struct Meta {
    pub service: String,
    pub version: String,
    /// The oldest build of each client that may still change anything. A
    /// client below its minimum shows that it must be updated; its changes
    /// are refused with `CLIENT_TOO_OLD`. Absent for a client with no minimum.
    pub minimum_client_versions: MinimumClientVersions,
    /// The wallets a pass can be added to here (DESIGN.md §11). Empty until
    /// a deployment configures one; a client shows no Wallet button then.
    pub wallet_platforms: Vec<WalletPlatform>,
}

/// Identifies the service and its build, and says how old a client may be.
#[utoipa::path(get, path = "/v1/meta", responses((status = 200, description = "Service identity", body = Meta)))]
pub async fn meta(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
) -> Json<Meta> {
    Json(Meta {
        service: env!("CARGO_PKG_NAME").to_owned(),
        version: env!("CARGO_PKG_VERSION").to_owned(),
        minimum_client_versions: state.settings.min_client_versions.clone(),
        wallet_platforms: wallet.platforms(),
    })
}
