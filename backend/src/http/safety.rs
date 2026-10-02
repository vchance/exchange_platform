//! Report and block endpoints (DESIGN.md §9). The work is in `crate::safety`;
//! these read the request and name the responses.

use std::convert::Infallible;

use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::{get, post};
use axum::{Json, Router};
use uuid::Uuid;

use super::AppState;
use super::extract::{ApiJson, Session};
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::safety::{self, BlockStatus, BlockedPerson, NewInvitationReport, NewReport};

/// Mounted under `/v1`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/exchanges/{id}/reports", post(report_exchange))
        .route(
            "/exchanges/{id}/block",
            get(block_status).put(block).delete(unblock),
        )
        .route("/invitations/report", post(report_invitation))
        .route("/blocks", get(blocked_people))
}

/// An exchange the caller cannot see and one that does not exist look the same.
fn exchange_id(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse().map_err(|_| ErrorCode::NotFound.into())
}

/// The signed-in account if there is one, and nobody otherwise: for the one
/// endpoint that a person may use either way.
pub struct MaybeSession(pub Option<Session>);

impl FromRequestParts<AppState> for MaybeSession {
    type Rejection = Infallible;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, Infallible> {
        Ok(Self(Session::from_request_parts(parts, state).await.ok()))
    }
}

/// Reports an exchange, and with it the other party, for review. The other
/// party is not told. Reporting the same exchange again while the first
/// report is open changes nothing and is answered the same way.
#[utoipa::path(
    post,
    path = "/v1/exchanges/{id}/reports",
    params(("id" = String, Path, description = "Exchange ID")),
    request_body = NewReport,
    responses(
        (status = 204, description = "Received"),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody),
        (status = 409, description = "Nobody has joined the exchange yet", body = ErrorBody),
        (status = 422, description = "Details too long, or missing for `OTHER`", body = ErrorBody),
        (status = 429, description = "Too many reports today", body = ErrorBody)
    )
)]
pub async fn report_exchange(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    ApiJson(body): ApiJson<NewReport>,
) -> Result<StatusCode, ApiError> {
    safety::report_exchange(&state.db, session.account_id, exchange_id(&id)?, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Reports the proposal behind an invitation link, and with it the person
/// who sent it. No sign-in needed: the token is the proof of having received
/// it, as for the preview, and travels in the body for the same reason. A
/// link that shows no preview takes no report.
#[utoipa::path(
    post,
    path = "/v1/invitations/report",
    request_body = NewInvitationReport,
    responses(
        (status = 204, description = "Received"),
        (status = 404, description = "The link is not valid, or no longer", body = ErrorBody),
        (status = 409, description = "The link is the caller's own", body = ErrorBody),
        (status = 422, description = "Details too long, or missing for `OTHER`", body = ErrorBody),
        (status = 429, description = "Too many reports today", body = ErrorBody)
    )
)]
pub async fn report_invitation(
    State(state): State<AppState>,
    MaybeSession(session): MaybeSession,
    ApiJson(body): ApiJson<NewInvitationReport>,
) -> Result<StatusCode, ApiError> {
    let reporter = session.map(|session| session.account_id);
    safety::report_invitation(&state.db, reporter, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Whether the caller has blocked the other party of this exchange.
#[utoipa::path(
    get,
    path = "/v1/exchanges/{id}/block",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 200, description = "The caller's own block, if any", body = BlockStatus),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody)
    )
)]
pub async fn block_status(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Json<BlockStatus>, ApiError> {
    let status = safety::block_status(&state.db, session.account_id, exchange_id(&id)?).await?;
    Ok(Json(status))
}

/// Blocks the other party of this exchange. Neither of the two can then join
/// an exchange the other starts. Whatever is waiting to be signed between
/// them is withdrawn or declined; an agreement in force stays in force. The
/// other party is not told.
#[utoipa::path(
    put,
    path = "/v1/exchanges/{id}/block",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 204, description = "Blocked"),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody),
        (status = 409, description = "Nobody has joined the exchange yet", body = ErrorBody)
    )
)]
pub async fn block(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    safety::block(
        &state.db,
        &state.settings.rules,
        session.account_id,
        exchange_id(&id)?,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Removes the caller's block on the other party of this exchange.
#[utoipa::path(
    delete,
    path = "/v1/exchanges/{id}/block",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 204, description = "Not blocked any more"),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody),
        (status = 409, description = "Nobody has joined the exchange yet", body = ErrorBody)
    )
)]
pub async fn unblock(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    safety::unblock(&state.db, session.account_id, exchange_id(&id)?).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// The people the caller has blocked, most recently blocked first.
#[utoipa::path(
    get,
    path = "/v1/blocks",
    responses(
        (status = 200, description = "Each by an exchange shared with them", body = [BlockedPerson]),
        (status = 401, description = "Not signed in", body = ErrorBody)
    )
)]
pub async fn blocked_people(
    State(state): State<AppState>,
    session: Session,
) -> Result<Json<Vec<BlockedPerson>>, ApiError> {
    Ok(Json(
        safety::blocked_people(&state.db, session.account_id).await?,
    ))
}
