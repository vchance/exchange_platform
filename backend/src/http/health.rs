use axum::extract::State;
use axum::http::StatusCode;

use super::AppState;
use crate::error::{ApiError, ErrorBody, ErrorCode};

/// The process is running.
#[utoipa::path(get, path = "/healthz", responses((status = 204, description = "Running")))]
pub async fn live() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// The process can reach the database, and the database is not a restored
/// copy still waiting for its deletion log to be replayed.
#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 204, description = "Ready"),
        (status = 503, description = "Database unreachable, or a restored copy whose deletion log is still to be replayed", body = ErrorBody)
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    let unavailable = || {
        ApiError::new(
            StatusCode::SERVICE_UNAVAILABLE,
            ErrorCode::ServiceUnavailable,
        )
    };
    let pending = crate::db::replay_pending(&state.db)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "readiness check failed");
            unavailable()
        })?;
    if pending {
        tracing::warn!("{}", crate::db::REPLAY_PENDING);
        return Err(unavailable());
    }
    Ok(StatusCode::NO_CONTENT)
}
