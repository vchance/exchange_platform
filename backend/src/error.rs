use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use utoipa::ToSchema;

use crate::domain::exchange::Refusal;
use crate::domain::identity::InvalidIdentifier;

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
    /// The revision ran out before it was accepted.
    RevisionExpired,
    /// The initiator must confirm who the counterparty is first.
    CounterpartyNotConfirmed,
    /// The caller opened an invitation that named nobody, and the initiator
    /// has not confirmed them yet. Until then they can sign or leave.
    AwaitingConfirmation,
    /// The revision breaks a rule and was not sent.
    InvalidRevision,
    /// The request body is missing, malformed or out of range.
    InvalidRequest,
    /// Not a usable email address or phone number.
    InvalidIdentifier,
    /// The one-time code is wrong, expired or used up. Deliberately one code
    /// for all three, so a guesser learns nothing.
    InvalidCode,
    /// Too many one-time codes requested for this identifier.
    TooManyRequests,
    /// No valid session.
    Unauthenticated,
    AccountSuspended,
    /// The email address or phone number belongs to another account.
    IdentifierInUse,
    /// The exchange changed since the client last read it.
    VersionConflict,
    /// A display name and confirmation of age are needed before signing.
    ProfileIncomplete,
    /// The consent wording shown is not the current version.
    ConsentOutdated,
    /// The invitation link is not valid, or no longer.
    InvitationUnavailable,
    /// The invitation names someone else.
    InvitationNotForYou,
    /// The idempotency key was already used for a different request.
    IdempotencyKeyReused,
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

impl From<ErrorCode> for ApiError {
    fn from(code: ErrorCode) -> Self {
        use ErrorCode::*;
        let status = match code {
            InvalidRequest | InvalidIdentifier | InvalidRevision | IdempotencyKeyReused => {
                StatusCode::UNPROCESSABLE_ENTITY
            }
            InvalidCode | Unauthenticated => StatusCode::UNAUTHORIZED,
            WrongActor | AccountSuspended | InvitationNotForYou => StatusCode::FORBIDDEN,
            NotFound | InvitationUnavailable => StatusCode::NOT_FOUND,
            TooManyRequests => StatusCode::TOO_MANY_REQUESTS,
            StaleRevision
            | ActionNotAllowed
            | ContributionLocked
            | RevisionExpired
            | CounterpartyNotConfirmed
            | AwaitingConfirmation
            | IdentifierInUse
            | VersionConflict
            | ProfileIncomplete
            | ConsentOutdated => StatusCode::CONFLICT,
            ClientTooOld => StatusCode::UPGRADE_REQUIRED,
            ServiceUnavailable => StatusCode::SERVICE_UNAVAILABLE,
            Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        Self::new(status, code)
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        (self.status, Json(ErrorBody { code: self.code })).into_response()
    }
}

impl From<Refusal> for ApiError {
    fn from(refusal: Refusal) -> Self {
        let code = match refusal {
            Refusal::WrongActor => ErrorCode::WrongActor,
            Refusal::NotAllowed => ErrorCode::ActionNotAllowed,
            Refusal::StaleRevision => ErrorCode::StaleRevision,
            Refusal::RevisionExpired => ErrorCode::RevisionExpired,
            Refusal::CounterpartyNotConfirmed => ErrorCode::CounterpartyNotConfirmed,
            Refusal::AwaitingConfirmation => ErrorCode::AwaitingConfirmation,
            Refusal::ContributionLocked(_) => ErrorCode::ContributionLocked,
            Refusal::UnknownContribution(_) => ErrorCode::NotFound,
            Refusal::InvalidRevision(_) => ErrorCode::InvalidRevision,
        };
        code.into()
    }
}

impl From<InvalidIdentifier> for ApiError {
    fn from(_: InvalidIdentifier) -> Self {
        ErrorCode::InvalidIdentifier.into()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database error");
        ErrorCode::Internal.into()
    }
}
