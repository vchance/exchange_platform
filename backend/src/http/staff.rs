//! The staff review endpoints (DESIGN.md §9). The work is in
//! `crate::review`; these read the request and name the responses.
//!
//! Every path here is for reviewers only. To anyone else, signed in or not,
//! each one answers `404 NOT_FOUND`, exactly as a path that does not exist,
//! before anything in the request is looked at. So does every other path
//! under `/v1/staff`, and every method a path does not take, so that nothing
//! tells the paths that exist from those that do not. A reviewer whose one-time
//! code was entered too long ago is told `SESSION_TOO_OLD`, and signs in
//! again.

use axum::extract::{FromRequestParts, Path, State};
use axum::http::StatusCode;
use axum::http::request::Parts;
use axum::routing::{any, get, post};
use axum::{Json, Router};
use time::OffsetDateTime;
use uuid::Uuid;

use super::AppState;
use super::extract::{ApiJson, Session};
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::review::{
    self, HiddenContent, ReportDetail, Resolution, RestoreContent, ReviewQueue, StaffNote,
    Suspension,
};

/// Mounted under `/v1`.
pub fn routes() -> Router<AppState> {
    Router::new()
        .route("/staff/reports", get(queue))
        .route("/staff/reports/{id}", get(open_report))
        .route("/staff/reports/{id}/resolution", post(resolve))
        .route("/staff/suspensions", get(suspensions))
        .route("/staff/suspensions/{account}/lift", post(lift))
        .route("/staff/hidden", get(hidden))
        .route("/staff/hidden/restore", post(restore))
        // After the routes above, so that it covers each of them.
        .method_not_allowed_fallback(not_found)
        // Every other path under /staff. A route of its own rather than a
        // fallback, which would not reach paths under one prefix only.
        .route("/staff", any(not_found))
        .route("/staff/", any(not_found))
        .route("/staff/{*rest}", any(not_found))
}

/// What every unknown path and method under `/v1/staff` answers: the same
/// as a staff path answers anyone who is not a reviewer.
async fn not_found() -> ApiError {
    ErrorCode::NotFound.into()
}

/// A signed-in reviewer whose one-time code is recent enough.
pub struct Staff(pub Session);

impl FromRequestParts<AppState> for Staff {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        // Nobody signed in, and anyone who is not a reviewer, learns nothing,
        // not even that the path exists.
        let session = Session::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::from(ErrorCode::NotFound))?;
        if !review::is_staff(&state.db, session.account_id).await? {
            return Err(ErrorCode::NotFound.into());
        }
        if !review::signed_in_recently(session.authenticated_at, OffsetDateTime::now_utc()) {
            return Err(ErrorCode::SessionTooOld.into());
        }
        Ok(Staff(session))
    }
}

fn id(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse().map_err(|_| ErrorCode::NotFound.into())
}

/// Staff only. The open reports, oldest first, each with its age, its
/// reason and its exchange's code, and nothing of what the reporter wrote or
/// who they are: those are read by opening the report. Reports the reviewer
/// takes part in are left out. Limited per reviewer, with the other lists.
#[utoipa::path(
    get,
    path = "/v1/staff/reports",
    tag = "staff",
    responses(
        (status = 200, description = "The queue", body = ReviewQueue),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer", body = ErrorBody),
        (status = 429, description = "Too many lists read in the last hour", body = ErrorBody)
    )
)]
pub async fn queue(
    State(state): State<AppState>,
    Staff(session): Staff,
) -> Result<Json<ReviewQueue>, ApiError> {
    Ok(Json(review::queue(&state.db, session.account_id).await?))
}

/// Staff only. Opens an open report: what it says, who it is about, the
/// reported exchange's record with nothing hidden, and what review has done
/// so far. Recorded in the audit history. A resolved report shows nothing
/// more. A report the reviewer takes part in (they made it, it is about
/// them, or they hold or held a place in its exchange) is "not found".
#[utoipa::path(
    get,
    path = "/v1/staff/reports/{id}",
    tag = "staff",
    params(("id" = String, Path, description = "Report ID")),
    responses(
        (status = 200, description = "The report and the exchange", body = ReportDetail),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer, or no such report", body = ErrorBody),
        (status = 409, description = "Already resolved (`REPORT_RESOLVED`)", body = ErrorBody),
        (status = 429, description = "Too many reports opened in the last hour", body = ErrorBody)
    )
)]
pub async fn open_report(
    State(state): State<AppState>,
    Staff(session): Staff,
    Path(report): Path<String>,
) -> Result<Json<ReportDetail>, ApiError> {
    Ok(Json(
        review::open_report(&state.db, session.account_id, id(&report)?).await?,
    ))
}

/// Staff only. Resolves an open report, once: dismissed, the exchange's
/// content hidden from the person reported, their account suspended, or
/// both. A note is required for every outcome but `DISMISSED`. A report the
/// reviewer takes part in is "not found"; suspending another reviewer is
/// refused (`SUBJECT_IS_REVIEWER`).
#[utoipa::path(
    post,
    path = "/v1/staff/reports/{id}/resolution",
    tag = "staff",
    params(("id" = String, Path, description = "Report ID")),
    request_body = Resolution,
    responses(
        (status = 204, description = "Resolved"),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer, or no such report", body = ErrorBody),
        (status = 409, description = "Already resolved (`REPORT_RESOLVED`), the outcome cannot apply (`ACTION_NOT_ALLOWED`), or the person reported is a reviewer, whom only the owner can suspend (`SUBJECT_IS_REVIEWER`)", body = ErrorBody),
        (status = 422, description = "The note is missing or too long", body = ErrorBody),
        (status = 429, description = "Too many actions in the last hour", body = ErrorBody)
    )
)]
pub async fn resolve(
    State(state): State<AppState>,
    Staff(session): Staff,
    Path(report): Path<String>,
    ApiJson(body): ApiJson<Resolution>,
) -> Result<StatusCode, ApiError> {
    review::resolve(
        &state.db,
        &state.settings.rules,
        session.account_id,
        id(&report)?,
        body,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Staff only. The suspended accounts, most recently suspended first,
/// leaving out the reviewer's own and those a report they take part in led
/// to. Limited per reviewer, with the other lists.
#[utoipa::path(
    get,
    path = "/v1/staff/suspensions",
    tag = "staff",
    responses(
        (status = 200, description = "Suspended accounts", body = [Suspension]),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer", body = ErrorBody),
        (status = 429, description = "Too many lists read in the last hour", body = ErrorBody)
    )
)]
pub async fn suspensions(
    State(state): State<AppState>,
    Staff(session): Staff,
) -> Result<Json<Vec<Suspension>>, ApiError> {
    Ok(Json(
        review::suspensions(&state.db, session.account_id).await?,
    ))
}

/// Staff only. Lifts a suspension; the account can sign in again. A note is
/// required. The reviewer's own suspension, and one that a report they take
/// part in led to, is "not found"; a reviewer's is the owner's to lift
/// (`SUBJECT_IS_REVIEWER`).
#[utoipa::path(
    post,
    path = "/v1/staff/suspensions/{account}/lift",
    tag = "staff",
    params(("account" = String, Path, description = "Account ID")),
    request_body = StaffNote,
    responses(
        (status = 204, description = "Lifted"),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer, or no such suspended account", body = ErrorBody),
        (status = 409, description = "The account is a reviewer's (`SUBJECT_IS_REVIEWER`)", body = ErrorBody),
        (status = 422, description = "The note is missing or too long", body = ErrorBody),
        (status = 429, description = "Too many actions in the last hour", body = ErrorBody)
    )
)]
pub async fn lift(
    State(state): State<AppState>,
    Staff(session): Staff,
    Path(account): Path<String>,
    ApiJson(body): ApiJson<StaffNote>,
) -> Result<StatusCode, ApiError> {
    review::lift(&state.db, session.account_id, id(&account)?, body).await?;
    Ok(StatusCode::NO_CONTENT)
}

/// Staff only. Content hidden by review, most recently hidden first,
/// leaving out what is hidden from the reviewer and what a report they take
/// part in hid. Limited per reviewer, with the other lists.
#[utoipa::path(
    get,
    path = "/v1/staff/hidden",
    tag = "staff",
    responses(
        (status = 200, description = "Hidden content", body = [HiddenContent]),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer", body = ErrorBody),
        (status = 429, description = "Too many lists read in the last hour", body = ErrorBody)
    )
)]
pub async fn hidden(
    State(state): State<AppState>,
    Staff(session): Staff,
) -> Result<Json<Vec<HiddenContent>>, ApiError> {
    Ok(Json(review::hidden(&state.db, session.account_id).await?))
}

/// Staff only. Shows hidden content to the account again. A note is
/// required. Content hidden from the reviewer, or by a report they take part
/// in, is "not found".
#[utoipa::path(
    post,
    path = "/v1/staff/hidden/restore",
    tag = "staff",
    request_body = RestoreContent,
    responses(
        (status = 204, description = "Shown again"),
        (status = 401, description = "The reviewer must sign in again (`SESSION_TOO_OLD`)", body = ErrorBody),
        (status = 404, description = "Not a reviewer, or nothing hidden there", body = ErrorBody),
        (status = 422, description = "The note is missing or too long", body = ErrorBody),
        (status = 429, description = "Too many actions in the last hour", body = ErrorBody)
    )
)]
pub async fn restore(
    State(state): State<AppState>,
    Staff(session): Staff,
    ApiJson(body): ApiJson<RestoreContent>,
) -> Result<StatusCode, ApiError> {
    review::restore(&state.db, session.account_id, body).await?;
    Ok(StatusCode::NO_CONTENT)
}
