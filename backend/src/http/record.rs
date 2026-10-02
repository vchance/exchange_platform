//! The record endpoints: an exchange's history, and the copy a party takes
//! away. The work is in `crate::exchanges::record`; these read the request
//! and name the responses.

use axum::Json;
use axum::extract::rejection::QueryRejection;
use axum::extract::{Path, Query, State};
use uuid::Uuid;

use super::AppState;
use super::extract::Session;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::exchanges::record::dto::{
    Continuation, HistoryPage, HistoryQuery, RecordDocument, RecordQuery,
};
use crate::exchanges::record::{self, Limits};

/// An exchange the caller cannot see and one that does not exist look the same.
fn exchange_id(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse().map_err(|_| ErrorCode::NotFound.into())
}

/// A query string that does not read as what the endpoint takes.
fn query<T>(parsed: Result<Query<T>, QueryRejection>) -> Result<T, ApiError> {
    parsed
        .map(|Query(query)| query)
        .map_err(|_| ErrorCode::InvalidRequest.into())
}

/// What has happened in an exchange, oldest first, with what the parties
/// wrote along the way: the message sent with a revision, a note on a claim,
/// the reason for a dispute, a statement. Answers with the latest events, 50
/// unless `limit` says otherwise and never more than 200; `earlier` in the
/// answer is the `before` that reads the page before.
#[utoipa::path(
    get,
    path = "/v1/exchanges/{id}/history",
    params(("id" = String, Path, description = "Exchange ID"), HistoryQuery),
    responses(
        (status = 200, description = "A page of the history", body = HistoryPage),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody),
        (status = 422, description = "The query is not valid", body = ErrorBody)
    )
)]
pub async fn history(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    parsed: Result<Query<HistoryQuery>, QueryRejection>,
) -> Result<Json<HistoryPage>, ApiError> {
    let id = exchange_id(&id)?;
    let HistoryQuery { before, limit } = query(parsed)?;
    let page = record::history(&state.db, &session, id, before, limit, &Limits::default()).await?;
    Ok(Json(page))
}

/// The record of an exchange as one self-contained document, for a party to
/// keep or hand to someone else: how it stands, every revision sent with
/// what it says and what became of it, every signature and what it rests
/// on, and everything that happened, in order.
///
/// One document holds at most 500 events, and at most 50 revisions or about
/// a megabyte of their text, whichever comes first. A longer record
/// continues in further documents: `part.next` holds the two values to ask
/// for the next one with, and `part.complete` says when one document is all
/// of it.
#[utoipa::path(
    get,
    path = "/v1/exchanges/{id}/record",
    params(("id" = String, Path, description = "Exchange ID"), RecordQuery),
    responses(
        (status = 200, description = "The record, or one part of it", body = RecordDocument),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account", body = ErrorBody),
        (status = 422, description = "The query is not valid", body = ErrorBody)
    )
)]
pub async fn record(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
    parsed: Result<Query<RecordQuery>, QueryRejection>,
) -> Result<Json<RecordDocument>, ApiError> {
    let id = exchange_id(&id)?;
    let from = query(parsed)?;
    let from = Continuation {
        revisions_after: from.revisions_after.unwrap_or(0),
        events_after: from.events_after.unwrap_or(0),
    };
    let document = record::record(&state.db, &session, id, from, &Limits::default()).await?;
    Ok(Json(document))
}
