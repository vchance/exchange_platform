//! Request extractors that answer with typed error codes.

use axum::body::Bytes;
use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::header::{AUTHORIZATION, COOKIE, ORIGIN};
use axum::http::request::Parts;
use axum::http::{HeaderMap, Method};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use time::OffsetDateTime;
use uuid::Uuid;

use super::AppState;
use crate::auth::token_hash;
use crate::error::{ApiError, ErrorCode};

pub const SESSION_COOKIE: &str = "exchange_session";

/// A JSON body. Unlike axum's own extractor, a bad body is refused with an
/// error code the clients understand.
pub struct ApiJson<T>(pub T);

impl<T: DeserializeOwned> FromRequest<AppState> for ApiJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, ApiError> {
        match axum::Json::<T>::from_request(request, state).await {
            Ok(axum::Json(value)) => Ok(Self(value)),
            Err(_) => Err(ErrorCode::InvalidRequest.into()),
        }
    }
}

/// A JSON body together with the SHA-256 of its bytes, for endpoints that
/// honor an idempotency key: the same key must come with the same request.
pub struct DigestedJson<T> {
    pub body: T,
    pub digest: [u8; 32],
}

impl<T: DeserializeOwned> FromRequest<AppState> for DigestedJson<T> {
    type Rejection = ApiError;

    async fn from_request(request: Request, state: &AppState) -> Result<Self, ApiError> {
        let bytes = Bytes::from_request(request, state)
            .await
            .map_err(|_| ApiError::from(ErrorCode::InvalidRequest))?;
        let body = serde_json::from_slice(&bytes).map_err(|_| ErrorCode::InvalidRequest)?;
        Ok(Self {
            body,
            digest: Sha256::digest(&bytes).into(),
        })
    }
}

/// The signed-in account making the request.
#[derive(Clone, Debug)]
pub struct Session {
    pub id: Uuid,
    pub account_id: Uuid,
    pub auth_method: String,
    pub authenticated_at: OffsetDateTime,
}

impl FromRequestParts<AppState> for Session {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &AppState) -> Result<Self, ApiError> {
        let (token, from_cookie) = match bearer_token(&parts.headers) {
            Some(token) => (token, false),
            None => (
                cookie(&parts.headers, SESSION_COOKIE).ok_or(ErrorCode::Unauthenticated)?,
                true,
            ),
        };

        // A browser attaches cookies to requests other sites cause it to
        // make. Anything that changes state must come from our own web app.
        if from_cookie && !is_safe(&parts.method) {
            require_web_origin(&parts.headers, &state.settings.web_origin)?;
        }

        let row: Option<(Uuid, Uuid, String, OffsetDateTime)> = sqlx::query_as(
            "SELECT s.id, s.account_id, s.auth_method, s.authenticated_at
             FROM account_session s
             JOIN account a ON a.id = s.account_id
             WHERE s.token_hash = $1
               AND s.revoked_at IS NULL
               AND s.expires_at > now()
               AND a.status = 'ACTIVE'",
        )
        .bind(token_hash(token).as_slice())
        .fetch_optional(&state.db)
        .await?;

        let (id, account_id, auth_method, authenticated_at) =
            row.ok_or(ErrorCode::Unauthenticated)?;
        Ok(Self {
            id,
            account_id,
            auth_method,
            authenticated_at,
        })
    }
}

fn is_safe(method: &Method) -> bool {
    matches!(*method, Method::GET | Method::HEAD | Method::OPTIONS)
}

pub fn require_web_origin(headers: &HeaderMap, web_origin: &str) -> Result<(), ApiError> {
    let origin = headers.get(ORIGIN).and_then(|value| value.to_str().ok());
    if origin == Some(web_origin) {
        Ok(())
    } else {
        Err(ErrorCode::Unauthenticated.into())
    }
}

fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    headers
        .get(AUTHORIZATION)?
        .to_str()
        .ok()?
        .strip_prefix("Bearer ")
}

fn cookie<'a>(headers: &'a HeaderMap, name: &str) -> Option<&'a str> {
    headers
        .get_all(COOKIE)
        .iter()
        .filter_map(|value| value.to_str().ok())
        .flat_map(|value| value.split(';'))
        .filter_map(|pair| pair.trim().split_once('='))
        .find_map(|(key, value)| (key == name).then_some(value))
}
