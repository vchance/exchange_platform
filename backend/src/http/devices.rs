//! The devices an account receives push notifications on (DESIGN.md §12,
//! §13): the app registers its push token here once the person has allowed
//! notifications, and removes it when they turn them off.
//!
//! A device belongs to the session it was registered under. Signing out of
//! that session removes it (`auth::delete_session`), and so does deleting
//! the account; one whose session ends any other way is not sent to, and
//! the worker removes it (`notifications::push::purge_devices`).

use axum::Json;
use axum::extract::{Path, State};
use axum::http::StatusCode;
use serde::{Deserialize, Serialize};
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::extract::{ApiJson, Session};
use crate::client_version::parse_version;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::languages;

/// The most devices an account keeps. Registering one more forgets the one
/// least recently registered: a person does not use more phones and tablets
/// than this, and the table cannot grow without bound by re-installs.
pub const DEVICES_PER_ACCOUNT: i64 = 10;

/// The platform a device runs.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, ToSchema)]
#[serde(rename_all = "lowercase")]
pub enum DevicePlatform {
    Ios,
    Android,
}

impl DevicePlatform {
    fn as_str(self) -> &'static str {
        match self {
            DevicePlatform::Ios => "ios",
            DevicePlatform::Android => "android",
        }
    }
}

#[derive(Debug, Deserialize, ToSchema)]
pub struct RegisterDevice {
    /// The Expo push token the app was given, `ExponentPushToken[…]`.
    pub token: String,
    pub platform: DevicePlatform,
    /// The app's version, such as `0.1.0`.
    pub app_version: String,
    /// The language the app is showing, as a language tag. Notifications are
    /// written in the account's language, like every email.
    pub language: String,
}

#[derive(Debug, Serialize, ToSchema)]
pub struct DeviceRegistered {
    /// The device's ID, for removing it again.
    pub id: String,
}

/// Whether `token` is an Expo push token: `ExponentPushToken[…]` or
/// `ExpoPushToken[…]`, short, and printable ASCII only.
fn is_expo_token(token: &str) -> bool {
    let inner = token
        .strip_prefix("ExponentPushToken[")
        .or_else(|| token.strip_prefix("ExpoPushToken["))
        .and_then(|rest| rest.strip_suffix(']'));
    token.len() <= 256
        && inner.is_some_and(|inner| {
            !inner.is_empty()
                && inner
                    .bytes()
                    .all(|byte| byte.is_ascii_graphic() && byte != b']')
        })
}

/// Registers this device for push notifications, under the session making
/// the request, or updates it if it is registered already.
///
/// A token is one device, and it moves to this session only if it is not
/// registered, is registered by the same account, or is registered under a
/// session that has ended (signed out, revoked or expired): the same phone
/// signed into another account afterwards. Registered under another
/// account's live session, it stays where it is, so that someone who has
/// learned a token cannot take another person's notifications. That is
/// answered exactly as a registration is, with the device's ID, which this
/// account cannot use; a different answer would say that the token belongs
/// to someone. The cost falls on a phone handed on without signing out: it
/// is notified for the old account until that session ends (at most the
/// session's lifetime), and the app's next registration after that takes it.
#[utoipa::path(
    put,
    path = "/v1/me/devices",
    request_body = RegisterDevice,
    responses(
        (status = 200, description = "The device, registered (or, if another account's live session holds the token, left as it was and answered the same)", body = DeviceRegistered),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 422, description = "Not an Expo push token, a platform, a version or a language", body = ErrorBody)
    )
)]
pub async fn register_device(
    State(state): State<AppState>,
    session: Session,
    ApiJson(body): ApiJson<RegisterDevice>,
) -> Result<Json<DeviceRegistered>, ApiError> {
    let token = body.token.trim();
    let app_version = body.app_version.trim();
    let language = languages::resolve(&body.language).ok_or(ErrorCode::InvalidRequest)?;
    if !is_expo_token(token) || app_version.len() > 32 || parse_version(app_version).is_none() {
        return Err(ErrorCode::InvalidRequest.into());
    }

    let mut tx = state.db.begin().await?;
    // The condition is decided on the conflicting row, locked, so two
    // registrations at once cannot both move it.
    let moved: Option<Uuid> = sqlx::query_scalar(
        "INSERT INTO device (account_id, session_id, service, token, platform, app_version, language)
         VALUES ($1, $2, 'EXPO', $3, $4, $5, $6)
         ON CONFLICT (service, token) DO UPDATE
         SET account_id = EXCLUDED.account_id, session_id = EXCLUDED.session_id,
             platform = EXCLUDED.platform, app_version = EXCLUDED.app_version,
             language = EXCLUDED.language, updated_at = now()
         WHERE device.account_id = EXCLUDED.account_id
            OR NOT EXISTS (
                SELECT 1 FROM account_session s
                WHERE s.id = device.session_id
                  AND s.revoked_at IS NULL AND s.expires_at > now())
         RETURNING id",
    )
    .bind(session.account_id)
    .bind(session.id)
    .bind(token)
    .bind(body.platform.as_str())
    .bind(app_version)
    .bind(language)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(id) = moved else {
        // Another account's live session holds it. Left alone, and answered
        // as if registered (above).
        let id: Uuid =
            sqlx::query_scalar("SELECT id FROM device WHERE service = 'EXPO' AND token = $1")
                .bind(token)
                .fetch_one(&mut *tx)
                .await?;
        tx.commit().await?;
        return Ok(Json(DeviceRegistered { id: id.to_string() }));
    };
    sqlx::query(
        "DELETE FROM device WHERE account_id = $1 AND id NOT IN (
             SELECT id FROM device WHERE account_id = $1
             ORDER BY updated_at DESC, id LIMIT $2)",
    )
    .bind(session.account_id)
    .bind(DEVICES_PER_ACCOUNT)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    Ok(Json(DeviceRegistered { id: id.to_string() }))
}

/// Stops sending push notifications to a device of the caller's. Answers
/// the same whether or not there was such a device, so a request repeated,
/// or one about a device already removed, is fine.
#[utoipa::path(
    delete,
    path = "/v1/me/devices/{id}",
    params(("id" = String, Path, description = "Device ID")),
    responses(
        (status = 204, description = "Not registered any more"),
        (status = 401, description = "Not signed in", body = ErrorBody)
    )
)]
pub async fn remove_device(
    State(state): State<AppState>,
    session: Session,
    Path(id): Path<String>,
) -> Result<StatusCode, ApiError> {
    if let Ok(id) = id.parse::<Uuid>() {
        sqlx::query("DELETE FROM device WHERE id = $1 AND account_id = $2")
            .bind(id)
            .bind(session.account_id)
            .execute(&state.db)
            .await?;
    }
    Ok(StatusCode::NO_CONTENT)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_an_expo_push_token_is_taken() {
        for token in [
            "ExponentPushToken[xxxxxxxxxxxxxxxxxxxxxx]",
            "ExpoPushToken[abc-123_DEF]",
        ] {
            assert!(is_expo_token(token), "{token}");
        }
        for token in [
            "",
            "ExponentPushToken[]",
            "ExponentPushToken[abc",
            "abc]",
            "ExponentPushToken[a b]",
            "ExponentPushToken[a]b]",
            "fcm:0123456789",
            &format!("ExponentPushToken[{}]", "x".repeat(300)),
        ] {
            assert!(!is_expo_token(token), "{token}");
        }
    }
}
