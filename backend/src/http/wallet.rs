//! Wallet passes over HTTP (DESIGN.md §11; the work is in `crate::wallet`).
//!
//! For the clients, each answering only a party to the exchange, and anyone
//! else as if it did not exist:
//!
//! * `POST /v1/exchanges/{id}/wallet/apple`: the caller's `.pkpass`;
//! * `POST /v1/exchanges/{id}/wallet/apple/link`: a short-lived link that
//!   downloads it without a session, for opening in Safari, which is what
//!   adds a pass to Wallet from a page or an app;
//! * `POST /v1/exchanges/{id}/wallet/google`: the "Save to Google Wallet"
//!   link.
//!
//! A platform that is not configured answers `WALLET_UNAVAILABLE` (404).
//!
//! For Apple's devices, under the `webServiceURL` a pass names
//! (`/v1/wallet/apple`), Apple's pass web service protocol: registering and
//! unregistering a device for a pass's updates, listing which passes
//! changed, fetching the latest pass, and taking the devices' logs. A device
//! proves it holds a pass with the pass's authentication token, sent as
//! `Authorization: ApplePass <token>`. These are not part of the API
//! description the clients are generated from.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{Path, Query, State};
use axum::http::header::{
    AUTHORIZATION, CACHE_CONTROL, CONTENT_DISPOSITION, CONTENT_TYPE, IF_MODIFIED_SINCE,
    LAST_MODIFIED,
};
use axum::http::{HeaderMap, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use serde::{Deserialize, Serialize};
use time::OffsetDateTime;
use utoipa::ToSchema;
use uuid::Uuid;

use super::AppState;
use super::extract::Session;
use crate::error::{ApiError, ErrorBody, ErrorCode};
use crate::exchanges::dto::rfc3339;
use crate::exchanges::service;
use crate::wallet::apple::{self, AppleIssuer};
use crate::wallet::pass::PassModel;
use crate::wallet::store::{self, PassRow, Registered};
use crate::wallet::{Wallet, WalletPlatform};

/// Where Apple's devices call: the pass web service. Their paths name a
/// device, so the request log writes their route's template instead
/// (`super::observe`).
pub const DEVICE_ROUTES: &str = "/v1/wallet/apple/v1/";

pub fn routes() -> Router<AppState> {
    let web_service = apple::WEB_SERVICE_PATH.trim_start_matches("/v1");
    Router::new()
        .route("/exchanges/{id}/wallet/apple", post(apple_pass))
        .route("/exchanges/{id}/wallet/apple/link", post(apple_link))
        .route("/exchanges/{id}/wallet/google", post(google_link))
        .route(&format!("{web_service}/pass"), get(download))
        .route(
            &format!("{web_service}/v1/devices/{{device}}/registrations/{{pass_type}}/{{serial}}"),
            post(register).delete(unregister),
        )
        .route(
            &format!("{web_service}/v1/devices/{{device}}/registrations/{{pass_type}}"),
            get(changed),
        )
        .route(
            &format!("{web_service}/v1/passes/{{pass_type}}/{{serial}}"),
            get(latest),
        )
        .route(&format!("{web_service}/v1/log"), post(log))
}

/// A signed Apple pass, a zip archive. Only describes the answer for the
/// API description; the handler sends the bytes themselves.
#[derive(ToSchema)]
#[schema(value_type = String, format = Binary)]
pub struct Pkpass(#[allow(dead_code)] Vec<u8>);

/// A link to follow to add a pass.
#[derive(Debug, Serialize, ToSchema)]
pub struct WalletLink {
    pub url: String,
    /// When the link stops working, RFC 3339. Absent for a link that does
    /// not expire.
    pub expires_at: Option<String>,
}

fn exchange_id(raw: &str) -> Result<Uuid, ApiError> {
    raw.parse().map_err(|_| ErrorCode::NotFound.into())
}

fn unavailable() -> ApiError {
    ErrorCode::WalletUnavailable.into()
}

fn internal(error: anyhow::Error) -> ApiError {
    // A signing or packaging failure is the service's own and holds
    // nothing personal; the chain says which step failed.
    tracing::error!(error = %format!("{error:#}"), "a wallet pass could not be made");
    ErrorCode::Internal.into()
}

/// The caller's pass on `platform` for exchange `id`: only for a party to
/// it (anyone else is told it does not exist), and only once something has
/// been agreed (DESIGN.md §11: offered once the exchange is active).
async fn issue(
    state: &AppState,
    wallet: &Wallet,
    session: &Session,
    raw: &str,
    platform: WalletPlatform,
) -> Result<PassRow, ApiError> {
    let id = exchange_id(raw)?;
    let view = service::get(&state.db, &state.settings.rules, session, id).await?;
    if view.in_force_revision.is_none() {
        return Err(ErrorCode::ActionNotAllowed.into());
    }
    store::issue(&state.db, wallet, session.account_id, id, platform).await
}

/// The pass's face as it is now, with its Last-Modified time.
async fn current(
    state: &AppState,
    wallet: &Wallet,
    pass: &PassRow,
) -> Result<(PassModel, OffsetDateTime), ApiError> {
    let (model, _) = store::face(&state.db, &state.settings.rules, wallet, pass).await?;
    // Drawn just now: if it differs from the face last recorded, its time is
    // now, so that what a device is given and its Last-Modified agree.
    let changed_at = store::publish_face(&state.db, pass.id, &store::face_hash(&model)).await?;
    Ok((model, changed_at))
}

/// The signed pass for `model`.
fn package(
    wallet: &Wallet,
    issuer: &AppleIssuer,
    pass: &PassRow,
    model: &PassModel,
) -> Result<Vec<u8>, ApiError> {
    issuer
        .package(
            model,
            &pass.serial,
            &wallet.apple_auth_token(pass.id),
            OffsetDateTime::now_utc(),
        )
        .map_err(internal)
}

fn pkpass(bytes: Vec<u8>, changed_at: OffsetDateTime) -> Response {
    let mut response = bytes.into_response();
    let headers = response.headers_mut();
    headers.insert(CONTENT_TYPE, HeaderValue::from_static(apple::CONTENT_TYPE));
    headers.insert(
        CONTENT_DISPOSITION,
        HeaderValue::from_static("attachment; filename=\"yuppers.pkpass\""),
    );
    if let Ok(value) = HeaderValue::from_str(&http_date(changed_at)) {
        headers.insert(LAST_MODIFIED, value);
    }
    // Only for the device that asked.
    headers.insert(CACHE_CONTROL, HeaderValue::from_static("private, no-cache"));
    response
}

/// Adds the caller's pass to Apple Wallet: the signed `.pkpass`. A party to
/// the exchange only, and only once something is agreed.
#[utoipa::path(
    post,
    path = "/v1/exchanges/{id}/wallet/apple",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 200, description = "The pass", content_type = "application/vnd.apple.pkpass", body = Pkpass),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account, or Apple Wallet passes are not available (WALLET_UNAVAILABLE)", body = ErrorBody),
        (status = 409, description = "Nothing has been agreed yet", body = ErrorBody),
        (status = 429, description = "Asked for too often", body = ErrorBody)
    )
)]
pub async fn apple_pass(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Response, ApiError> {
    let issuer = wallet.apple.as_ref().ok_or_else(unavailable)?;
    let pass = issue(&state, &wallet, &session, &id, WalletPlatform::Apple).await?;
    let (model, changed_at) = current(&state, &wallet, &pass).await?;
    Ok(pkpass(package(&wallet, issuer, &pass, &model)?, changed_at))
}

/// A link that downloads the caller's Apple pass for a few minutes, without
/// a session: opened in Safari, it adds the pass to Wallet. For the app and
/// the web page, which hold the session themselves.
#[utoipa::path(
    post,
    path = "/v1/exchanges/{id}/wallet/apple/link",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 200, description = "The link", body = WalletLink),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account, or Apple Wallet passes are not available (WALLET_UNAVAILABLE)", body = ErrorBody),
        (status = 409, description = "Nothing has been agreed yet", body = ErrorBody),
        (status = 429, description = "Asked for too often", body = ErrorBody)
    )
)]
pub async fn apple_link(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Json<WalletLink>, ApiError> {
    wallet.apple.as_ref().ok_or_else(unavailable)?;
    let pass = issue(&state, &wallet, &session, &id, WalletPlatform::Apple).await?;
    let expires = OffsetDateTime::now_utc() + wallet.rules.download_link_ttl;
    let token = wallet.apple_download_token(pass.id, expires);
    Ok(Json(WalletLink {
        // In the query string, which the service never logs.
        url: format!(
            "{}{}/pass?token={token}",
            wallet.web_origin,
            apple::WEB_SERVICE_PATH
        ),
        expires_at: Some(rfc3339(expires)),
    }))
}

/// The "Save to Google Wallet" link for the caller's pass. A party to the
/// exchange only, and only once something is agreed.
#[utoipa::path(
    post,
    path = "/v1/exchanges/{id}/wallet/google",
    params(("id" = String, Path, description = "Exchange ID")),
    responses(
        (status = 200, description = "The link", body = WalletLink),
        (status = 401, description = "Not signed in", body = ErrorBody),
        (status = 404, description = "No such exchange for this account, or Google Wallet passes are not available (WALLET_UNAVAILABLE)", body = ErrorBody),
        (status = 409, description = "Nothing has been agreed yet", body = ErrorBody),
        (status = 429, description = "Asked for too often", body = ErrorBody)
    )
)]
pub async fn google_link(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    session: Session,
    Path(id): Path<String>,
) -> Result<Json<WalletLink>, ApiError> {
    let issuer = wallet.google.as_ref().ok_or_else(unavailable)?;
    let pass = issue(&state, &wallet, &session, &id, WalletPlatform::Google).await?;
    let (model, _) = store::face(&state.db, &state.settings.rules, &wallet, &pass).await?;
    let url = issuer
        .save_url(
            &model,
            &pass.serial,
            &wallet.web_origin,
            OffsetDateTime::now_utc(),
        )
        .map_err(internal)?;
    Ok(Json(WalletLink {
        url,
        expires_at: None,
    }))
}

#[derive(Deserialize)]
pub struct DownloadQuery {
    token: String,
}

/// The pass behind a download link from [`apple_link`].
async fn download(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    Query(query): Query<DownloadQuery>,
) -> Result<Response, ApiError> {
    let issuer = wallet.apple.as_ref().ok_or_else(unavailable)?;
    let not_found = || ApiError::from(ErrorCode::NotFound);
    let id = wallet
        .apple_download_pass(&query.token, OffsetDateTime::now_utc())
        .ok_or_else(not_found)?;
    let pass = store::pass_by_id(&state.db, id)
        .await?
        .filter(|pass| pass.platform == WalletPlatform::Apple && !pass.voided)
        .ok_or_else(not_found)?;
    let (model, changed_at) = current(&state, &wallet, &pass).await?;
    Ok(pkpass(package(&wallet, issuer, &pass, &model)?, changed_at))
}

// ---- Apple's pass web service ---------------------------------------------------

/// The pass a device names, if the token it sent is that pass's. Anything
/// else, a pass type that is not ours included, is 401, as Apple asks.
async fn authorized(
    state: &AppState,
    issuer: &AppleIssuer,
    headers: &HeaderMap,
    pass_type: &str,
    serial: &str,
) -> Result<PassRow, StatusCode> {
    let token = headers
        .get(AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("ApplePass "))
        .map(str::trim)
        .ok_or(StatusCode::UNAUTHORIZED)?;
    if pass_type != issuer.pass_type_id() {
        return Err(StatusCode::UNAUTHORIZED);
    }
    store::apple_pass(&state.db, serial, token)
        .await
        .map_err(|error| ApiError::from(error).status)?
        .ok_or(StatusCode::UNAUTHORIZED)
}

fn device_id(device: &str) -> Result<(), StatusCode> {
    let fits = (1..=128).contains(&device.len())
        && device
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'-' | b'_' | b'.'));
    fits.then_some(()).ok_or(StatusCode::BAD_REQUEST)
}

#[derive(Deserialize)]
struct Registration {
    #[serde(rename = "pushToken")]
    push_token: String,
}

/// A device asks for a pass's updates. 201 when new, 200 when it already
/// had, 401 without the pass's token.
async fn register(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    Path((device, pass_type, serial)): Path<(String, String, String)>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<StatusCode, StatusCode> {
    let issuer = wallet.apple.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let pass = authorized(&state, issuer, &headers, &pass_type, &serial).await?;
    device_id(&device)?;
    // A void pass takes no new devices.
    if pass.voided {
        return Err(StatusCode::UNAUTHORIZED);
    }
    let registration: Registration =
        serde_json::from_slice(&body).map_err(|_| StatusCode::BAD_REQUEST)?;
    let token = registration.push_token.trim();
    if !(1..=256).contains(&token.len()) || !token.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(StatusCode::BAD_REQUEST);
    }
    match store::register(&state.db, pass.id, &device, token)
        .await
        .map_err(|error| ApiError::from(error).status)?
    {
        Registered::New => Ok(StatusCode::CREATED),
        Registered::Already => Ok(StatusCode::OK),
        Registered::TooMany => Err(StatusCode::FORBIDDEN),
    }
}

/// A device stops asking for a pass's updates, usually because the pass was
/// removed from it.
async fn unregister(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    Path((device, pass_type, serial)): Path<(String, String, String)>,
    headers: HeaderMap,
) -> Result<StatusCode, StatusCode> {
    let issuer = wallet.apple.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let pass = authorized(&state, issuer, &headers, &pass_type, &serial).await?;
    store::unregister(&state.db, pass.id, &device)
        .await
        .map_err(|error| ApiError::from(error).status)?;
    Ok(StatusCode::OK)
}

#[derive(Deserialize)]
struct ChangedQuery {
    #[serde(rename = "passesUpdatedSince")]
    since: Option<String>,
}

#[derive(Serialize)]
struct Changed {
    #[serde(rename = "serialNumbers")]
    serial_numbers: Vec<String>,
    #[serde(rename = "lastUpdated")]
    last_updated: String,
}

/// Which of a device's passes changed since the tag it got last time (all
/// of them without one). 204 when none did. Apple sends no token here: the
/// device's library ID, which only the device knows, is what it asks with.
async fn changed(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    Path((device, pass_type)): Path<(String, String)>,
    Query(query): Query<ChangedQuery>,
) -> Result<Response, StatusCode> {
    let issuer = wallet.apple.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    if pass_type != issuer.pass_type_id() {
        return Ok(StatusCode::NO_CONTENT.into_response());
    }
    device_id(&device)?;
    // The tag is ours: Unix seconds. One that does not read as that is as
    // good as none.
    let since = query
        .since
        .as_deref()
        .and_then(|tag| tag.parse::<i64>().ok())
        .and_then(|seconds| OffsetDateTime::from_unix_timestamp(seconds).ok());
    let (serials, latest) = store::changed_for_device(&state.db, &device, since)
        .await
        .map_err(|error| ApiError::from(error).status)?;
    match latest {
        Some(latest) if !serials.is_empty() => Ok(Json(Changed {
            serial_numbers: serials,
            last_updated: latest.unix_timestamp().to_string(),
        })
        .into_response()),
        _ => Ok(StatusCode::NO_CONTENT.into_response()),
    }
}

/// The latest version of a pass, or 304 when it has not changed since the
/// device's copy.
async fn latest(
    State(state): State<AppState>,
    Extension(wallet): Extension<Arc<Wallet>>,
    Path((pass_type, serial)): Path<(String, String)>,
    headers: HeaderMap,
) -> Result<Response, StatusCode> {
    let issuer = wallet.apple.as_ref().ok_or(StatusCode::NOT_FOUND)?;
    let pass = authorized(&state, issuer, &headers, &pass_type, &serial).await?;
    let (model, changed_at) = current(&state, &wallet, &pass)
        .await
        .map_err(|error| error.status)?;
    let since = headers
        .get(IF_MODIFIED_SINCE)
        .and_then(|value| value.to_str().ok())
        .and_then(parse_http_date);
    if since.is_some_and(|since| changed_at <= since) {
        return Ok(StatusCode::NOT_MODIFIED.into_response());
    }
    let bytes = package(&wallet, issuer, &pass, &model).map_err(|error| error.status)?;
    Ok(pkpass(bytes, changed_at))
}

#[derive(Deserialize)]
struct Logs {
    #[serde(default)]
    logs: Vec<String>,
}

/// What a device reports about errors with the web service. Written to the
/// log, a few lines and a few hundred characters at most: anyone may send
/// them.
async fn log(body: Bytes) -> StatusCode {
    if let Ok(Logs { logs }) = serde_json::from_slice::<Logs>(&body) {
        for line in logs.iter().take(5) {
            let line: String = line.chars().take(300).collect();
            tracing::warn!(line, "a Wallet device reported");
        }
    }
    StatusCode::OK
}

// ---- HTTP dates ---------------------------------------------------------------

const MONTHS: [&str; 12] = [
    "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
];

/// An instant as an HTTP date (RFC 9110, IMF-fixdate), to the second.
pub fn http_date(at: OffsetDateTime) -> String {
    let at = at.to_offset(time::UtcOffset::UTC);
    let weekday = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"]
        [usize::from(at.weekday().number_days_from_monday())];
    format!(
        "{weekday}, {:02} {} {} {:02}:{:02}:{:02} GMT",
        at.day(),
        MONTHS[usize::from(u8::from(at.month())) - 1],
        at.year(),
        at.hour(),
        at.minute(),
        at.second()
    )
}

/// An IMF-fixdate (`Sun, 06 Nov 1994 08:49:37 GMT`), the form this service
/// sends and so the form a device sends back. Anything else is ignored.
pub fn parse_http_date(text: &str) -> Option<OffsetDateTime> {
    let rest = text.trim().split_once(", ")?.1;
    let mut parts = rest.split(' ');
    let day: u8 = parts.next()?.parse().ok()?;
    let month = parts.next()?;
    let month = MONTHS.iter().position(|name| *name == month)?;
    let year: i32 = parts.next()?.parse().ok()?;
    let mut clock = parts.next()?.split(':');
    let (hour, minute, second): (u8, u8, u8) = (
        clock.next()?.parse().ok()?,
        clock.next()?.parse().ok()?,
        clock.next()?.parse().ok()?,
    );
    if parts.next()? != "GMT" || parts.next().is_some() {
        return None;
    }
    let date = time::Date::from_calendar_date(
        year,
        time::Month::try_from(u8::try_from(month + 1).ok()?).ok()?,
        day,
    )
    .ok()?;
    let time = time::Time::from_hms(hour, minute, second).ok()?;
    Some(date.with_time(time).assume_utc())
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    #[test]
    fn every_route_a_device_calls_is_logged_by_its_template() {
        assert!(DEVICE_ROUTES.starts_with(apple::WEB_SERVICE_PATH));
        assert_eq!(&DEVICE_ROUTES[apple::WEB_SERVICE_PATH.len()..], "/v1/");
    }

    #[test]
    fn http_dates_go_out_and_come_back_to_the_second() {
        let at = datetime!(2026-10-05 08:49:37.900 UTC);
        assert_eq!(http_date(at), "Mon, 05 Oct 2026 08:49:37 GMT");
        assert_eq!(
            parse_http_date("Mon, 05 Oct 2026 08:49:37 GMT"),
            Some(datetime!(2026-10-05 08:49:37 UTC))
        );
        for wrong in [
            "",
            "Monday, 05-Oct-26 08:49:37 GMT",
            "Mon, 05 Oct 2026 08:49:37 PST",
            "Mon, 32 Oct 2026 08:49:37 GMT",
            "Mon, 05 Oct 2026 08:49 GMT",
        ] {
            assert_eq!(parse_http_date(wrong), None, "{wrong}");
        }
    }
}
