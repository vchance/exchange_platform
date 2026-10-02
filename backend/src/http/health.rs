use axum::extract::State;
use axum::http::StatusCode;

use super::AppState;
use crate::error::{ApiError, ErrorBody, ErrorCode};

/// The process is running.
#[utoipa::path(get, path = "/healthz", responses((status = 204, description = "Running")))]
pub async fn live() -> StatusCode {
    StatusCode::NO_CONTENT
}

/// The process can reach the database.
#[utoipa::path(
    get,
    path = "/readyz",
    responses(
        (status = 204, description = "Ready"),
        (status = 503, description = "Database unreachable", body = ErrorBody)
    )
)]
pub async fn ready(State(state): State<AppState>) -> Result<StatusCode, ApiError> {
    sqlx::query("SELECT 1")
        .execute(&state.db)
        .await
        .map_err(|error| {
            tracing::warn!(%error, "readiness check failed");
            ApiError::new(
                StatusCode::SERVICE_UNAVAILABLE,
                ErrorCode::ServiceUnavailable,
            )
        })?;
    Ok(StatusCode::NO_CONTENT)
}
