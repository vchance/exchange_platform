use axum::routing::get;
use axum::{Json, Router};
use serde::Serialize;
use utoipa::ToSchema;

use super::AppState;

pub fn router() -> Router<AppState> {
    Router::new().route("/meta", get(meta))
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
