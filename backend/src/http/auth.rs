//! Signing in with a one-time code, and signing out.

use axum::Json;
use axum::extract::State;
use axum::http::header::SET_COOKIE;
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::account::{self, Account};
use super::extract::{ApiJson, SESSION_COOKIE, Session, require_web_origin};
use super::{AppState, Settings};
use crate::auth;
use crate::domain::identity::Identifier;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::languages;

#[derive(Debug, Deserialize, ToSchema)]
pub struct RequestCode {
    /// An email address, or a phone number in international form.
    pub identifier: String,
}

/// Sends a one-time code to an email address or phone number. Answers the
/// same way whether or not an account exists for it.
#[utoipa::path(
    post,
    path = "/v1/auth/codes",
    request_body = RequestCode,
    responses(
        (status = 204, description = "A code was sent"),
        (status = 422, description = "Not an email address or phone number", body = ErrorBody),
        (status = 429, description = "Too many codes requested", body = ErrorBody)
    )
)]
pub async fn request_code(
    State(state): State<AppState>,
    ApiJson(body): ApiJson<RequestCode>,
) -> Result<StatusCode, ApiError> {
    let identifier = Identifier::parse(&body.identifier)?;
    let settings = &state.settings;
    auth::request_code(
        &state.db,
        &settings.app_secret,
        &settings.auth,
        state.code_sender.as_ref(),
        &identifier,
    )
    .await?;
    Ok(StatusCode::NO_CONTENT)
}

/// How the client wants to hold the session.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum Delivery {
    /// Web: an HTTP-only cookie the page's scripts cannot read.
    Cookie,
    /// Mobile: a token in the response, kept in the device's secure storage
    /// and sent as `Authorization: Bearer`.
    Token,
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct CreateSession {
    pub identifier: String,
    /// The one-time code sent to the identifier.
    pub code: String,
    pub delivery: Delivery,
    /// The language the client is showing, as a tag such as `es-MX`. Used only
    /// when this creates the account; an unsupported one becomes the default.
    pub language: Option<String>,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct SessionCreated {
    pub account: Account,
    /// Present for `TOKEN` delivery only.
    pub token: Option<String>,
}

/// Signs in with a one-time code, creating the account on first use.
#[utoipa::path(
    post,
    path = "/v1/auth/sessions",
    request_body = CreateSession,
    responses(
        (status = 200, description = "Signed in", body = SessionCreated),
        (status = 401, description = "The code is wrong, expired or used up", body = ErrorBody),
        (status = 403, description = "The account is suspended", body = ErrorBody),
        (status = 422, description = "Invalid request", body = ErrorBody)
    )
)]
pub async fn create_session(
    State(state): State<AppState>,
    headers: HeaderMap,
    ApiJson(body): ApiJson<CreateSession>,
) -> Result<Response, ApiError> {
    let settings = &state.settings;
    if body.delivery == Delivery::Cookie {
        // Another site must not be able to sign a browser in to an account
        // of its choosing.
        require_web_origin(&headers, &settings.web_origin)?;
    }

    let identifier = Identifier::parse(&body.identifier)?;
    auth::verify_code(
        &state.db,
        &settings.app_secret,
        &settings.auth,
        &identifier,
        &body.code,
    )
    .await?;

    let mut tx = state.db.begin().await?;

    let (column, method) = match identifier {
        Identifier::Email(_) => ("email", "EMAIL_OTP"),
        Identifier::Phone(_) => ("phone", "PHONE_OTP"),
    };
    let existing: Option<(Uuid, String)> = sqlx::query_as(sqlx::AssertSqlSafe(format!(
        "SELECT id, status FROM account WHERE {column} = $1"
    )))
    .bind(identifier.as_str())
    .fetch_optional(&mut *tx)
    .await?;

    let account_id = match existing {
        Some((_, status)) if status != "ACTIVE" => return Err(ErrorCode::AccountSuspended.into()),
        Some((id, _)) => id,
        None => {
            let language = body
                .language
                .as_deref()
                .and_then(languages::resolve)
                .unwrap_or(languages::default());
            sqlx::query_scalar(sqlx::AssertSqlSafe(format!(
                "INSERT INTO account ({column}, display_name, language)
                 VALUES ($1, '', $2)
                 RETURNING id"
            )))
            .bind(identifier.as_str())
            .bind(language)
            .fetch_one(&mut *tx)
            .await?
        }
    };

    let token = auth::generate_token();
    sqlx::query(
        "INSERT INTO account_session
            (account_id, token_hash, auth_method, authenticated_at, expires_at)
         VALUES ($1, $2, $3, now(), now() + $4 * interval '1 second')",
    )
    .bind(account_id)
    .bind(auth::token_hash(&token).as_slice())
    .bind(method)
    .bind(settings.auth.session_ttl.whole_seconds() as f64)
    .execute(&mut *tx)
    .await?;

    let account = account::load(&mut *tx, account_id).await?;
    tx.commit().await?;

    Ok(match body.delivery {
        Delivery::Token => Json(SessionCreated {
            account,
            token: Some(token),
        })
        .into_response(),
        Delivery::Cookie => (
            [(SET_COOKIE, session_cookie(settings, &token))],
            Json(SessionCreated {
                account,
                token: None,
            }),
        )
            .into_response(),
    })
}

/// Signs out: the session stops working everywhere it was held.
#[utoipa::path(
    delete,
    path = "/v1/auth/session",
    responses(
        (status = 204, description = "Signed out"),
        (status = 401, description = "Not signed in", body = ErrorBody)
    )
)]
pub async fn delete_session(
    State(state): State<AppState>,
    session: Session,
) -> Result<Response, ApiError> {
    sqlx::query("UPDATE account_session SET revoked_at = now() WHERE id = $1")
        .bind(session.id)
        .execute(&state.db)
        .await?;

    Ok((
        StatusCode::NO_CONTENT,
        [(SET_COOKIE, expired_cookie(&state.settings))],
    )
        .into_response())
}

fn session_cookie(settings: &Settings, token: &str) -> HeaderValue {
    cookie(settings, token, settings.auth.session_ttl.whole_seconds())
}

fn expired_cookie(settings: &Settings) -> HeaderValue {
    cookie(settings, "", 0)
}

fn cookie(settings: &Settings, value: &str, max_age: i64) -> HeaderValue {
    // `Secure` whenever the web app is served over HTTPS; plain HTTP is for
    // local development only.
    let secure = if settings.web_origin.starts_with("https://") {
        "; Secure"
    } else {
        ""
    };
    HeaderValue::from_str(&format!(
        "{SESSION_COOKIE}={value}; HttpOnly; SameSite=Lax; Path=/; Max-Age={max_age}{secure}"
    ))
    .expect("a hex token and fixed attributes are a valid header value")
}
