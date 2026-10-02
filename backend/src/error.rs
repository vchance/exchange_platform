use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::contribution::Refusal;

/// Stable codes for every refusal the API can return. Clients map these to
/// wording in the user's language and never parse message text
/// (DESIGN.md §13.3). Adding a code here is a contract change: the generated
/// client makes the shared wording tables fail to compile until it is covered.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    /// The revision named in the request is no longer the open one.
    StaleRevision,
    /// The action belongs to the other party.
    WrongActor,
    /// The action is not allowed in the current state.
    ActionNotAllowed,
    /// An accepted contribution cannot be changed.
    ContributionLocked,
    /// The client build is too old to act and must update.
    ClientTooOld,
    NotFound,
    ServiceUnavailable,
    Internal,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct ErrorBody {
    pub code: ErrorCode,
}

#[derive(Debug)]
pub struct ApiError {
    pub status: StatusCode,
    pub code: ErrorCode,
}

impl ApiError {
    pub const fn new(status: StatusCode, code: ErrorCode) -> Self {
        Self { status, code }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorBody { code: self.code })).into_response()
    }
}

impl From<Refusal> for ApiError {
    fn from(refusal: Refusal) -> Self {
        match refusal {
            Refusal::WrongParty => Self::new(StatusCode::FORBIDDEN, ErrorCode::WrongActor),
            Refusal::NotAllowed => Self::new(StatusCode::CONFLICT, ErrorCode::ActionNotAllowed),
        }
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database error");
        Self::new(StatusCode::INTERNAL_SERVER_ERROR, ErrorCode::Internal)
    }
}
