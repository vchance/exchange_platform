//! Telling a client it is too old to act (`ErrorCode::ClientTooOld`).
//!
//! `GET /v1/meta` carries the oldest version of each client the service
//! still accepts changes from, and every app compares itself against it at
//! startup. A client also names itself on each request, in an
//! `X-Client-Version` header such as `ios/1.4.0`, and a change asked for by
//! one below the minimum is refused. Both are configuration, with no minimum
//! by default: nothing is refused until a deployment says so.
//!
//! A version is dotted whole numbers, compared part by part, with a missing
//! part counting as zero: `1.4` and `1.4.0` are the same. A client may add
//! the commit it was built from after a `+`, as semantic versioning writes
//! build metadata (`web/1.4.0+abc1234`); like build metadata, it plays no
//! part in the comparison. A header that does
//! not read as `client/version`, or names a client this service does not
//! know, is ignored rather than refused: it cannot be the one an old client
//! of ours sent.

use axum::extract::{Request, State};
use axum::http::Method;
use axum::middleware::Next;
use axum::response::Response;
use serde::Serialize;
use utoipa::ToSchema;

use crate::error::{ApiError, ErrorCode};
use crate::http::AppState;

/// The header a client names itself in: `<client>/<version>`.
pub const HEADER: &str = "x-client-version";

/// The oldest version of each client that may still change anything.
/// `None` means every version may.
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, ToSchema)]
pub struct MinimumClientVersions {
    pub web: Option<String>,
    pub ios: Option<String>,
    pub android: Option<String>,
}

impl MinimumClientVersions {
    fn for_client(&self, client: &str) -> Option<&str> {
        match client {
            "web" => self.web.as_deref(),
            "ios" => self.ios.as_deref(),
            "android" => self.android.as_deref(),
            _ => None,
        }
    }

    /// Whether a client calling itself `header` is below the minimum for
    /// its kind. Anything unreadable is not.
    pub fn is_too_old(&self, header: &str) -> bool {
        let Some((client, version)) = parse_header(header) else {
            return false;
        };
        match (self.for_client(client), parse_version(version)) {
            (Some(minimum), Some(version)) => parse_version(minimum)
                .is_some_and(|minimum| compare(&version, &minimum) == std::cmp::Ordering::Less),
            _ => false,
        }
    }
}

/// `web/1.2.3` into `("web", "1.2.3")`, and `web/1.2.3+abc1234` too.
fn parse_header(header: &str) -> Option<(&str, &str)> {
    let (client, version) = header.trim().split_once('/')?;
    let client = client.trim();
    let version = version
        .split_once('+')
        .map_or(version, |(version, _)| version);
    let version = version.trim();
    (!client.is_empty() && !version.is_empty()).then_some((client, version))
}

/// Dotted whole numbers, such as `1.4.0`. Anything else is not a version.
pub fn parse_version(text: &str) -> Option<Vec<u64>> {
    text.trim()
        .split('.')
        .map(|part| part.parse::<u64>().ok())
        .collect()
}

/// Part by part, a missing part counting as zero.
fn compare(a: &[u64], b: &[u64]) -> std::cmp::Ordering {
    let width = a.len().max(b.len());
    for index in 0..width {
        let order = a
            .get(index)
            .copied()
            .unwrap_or(0)
            .cmp(&b.get(index).copied().unwrap_or(0));
        if order != std::cmp::Ordering::Equal {
            return order;
        }
    }
    std::cmp::Ordering::Equal
}

/// What an old client may still do although it changes something: sign
/// out, and delete the account (asking for the code, then deleting). A
/// person must never be stuck signed in on a device, or unable to leave,
/// because the app there has not been updated.
const ALWAYS_ALLOWED: &[(Method, &str)] = &[
    (Method::DELETE, "/v1/auth/session"),
    (Method::POST, "/v1/me/deletion"),
    (Method::POST, "/v1/me/deletion/codes"),
];

/// Whether `method` on `path` is one of [`ALWAYS_ALLOWED`]. A trailing
/// slash is not the same path: the router would not answer it either.
fn always_allowed(method: &Method, path: &str) -> bool {
    ALWAYS_ALLOWED
        .iter()
        .any(|(allowed, allowed_path)| allowed == method && *allowed_path == path)
}

/// Refuses a change asked for by a client that says it is older than the
/// minimum. Reading is still allowed: an old client can show what there is,
/// and show the person that it must be updated. So are signing out and
/// deleting the account ([`ALWAYS_ALLOWED`]).
pub async fn refuse_old_clients(
    State(state): State<AppState>,
    request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let reading = matches!(
        *request.method(),
        Method::GET | Method::HEAD | Method::OPTIONS
    );
    if !reading && !always_allowed(request.method(), request.uri().path()) {
        let header = request
            .headers()
            .get(HEADER)
            .and_then(|value| value.to_str().ok());
        if header.is_some_and(|header| state.settings.min_client_versions.is_too_old(header)) {
            return Err(ErrorCode::ClientTooOld.into());
        }
    }
    Ok(next.run(request).await)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn required() -> MinimumClientVersions {
        MinimumClientVersions {
            web: Some("2.1".to_owned()),
            ios: Some("1.4.0".to_owned()),
            android: None,
        }
    }

    #[test]
    fn versions_compare_part_by_part() {
        let minimums = required();
        assert!(minimums.is_too_old("ios/1.3.9"));
        assert!(minimums.is_too_old("ios/1.3"));
        assert!(minimums.is_too_old(" ios / 0.9.99 "));
        assert!(!minimums.is_too_old("ios/1.4"));
        assert!(!minimums.is_too_old("ios/1.4.0.1"));
        assert!(!minimums.is_too_old("ios/1.10.0"), "ten is more than four");
        assert!(minimums.is_too_old("web/2.0.5"));
        assert!(!minimums.is_too_old("web/2.1.0"));
        assert!(!minimums.is_too_old("web/3"));
    }

    #[test]
    fn the_commit_after_a_plus_plays_no_part() {
        let minimums = required();
        assert!(minimums.is_too_old("web/2.0.5+abc1234"));
        assert!(!minimums.is_too_old("web/2.1.0+abc1234"));
        assert!(minimums.is_too_old("ios/1.3+unknown"));
        assert!(
            !minimums.is_too_old("web/+abc1234"),
            "no version, not refused"
        );
    }

    #[test]
    fn nothing_is_required_unless_said() {
        assert!(!MinimumClientVersions::default().is_too_old("web/0.0.1"));
        assert!(!required().is_too_old("android/0.0.1"));
    }

    #[test]
    fn signing_out_and_deleting_are_always_allowed() {
        assert!(always_allowed(&Method::DELETE, "/v1/auth/session"));
        assert!(always_allowed(&Method::POST, "/v1/me/deletion"));
        assert!(always_allowed(&Method::POST, "/v1/me/deletion/codes"));
        assert!(!always_allowed(&Method::POST, "/v1/auth/session"));
        assert!(!always_allowed(&Method::DELETE, "/v1/me/deletion"));
        assert!(!always_allowed(&Method::POST, "/v1/auth/sessions"));
        assert!(!always_allowed(&Method::POST, "/v1/me/deletion/codes/x"));
        assert!(!always_allowed(&Method::POST, "/v1/exchanges"));
    }

    #[test]
    fn what_cannot_be_read_is_not_refused() {
        let minimums = required();
        for header in [
            "", "ios", "/1.0", "ios/", "ios/one", "ios/1..0", "tv/0.1", "ios 1.0",
        ] {
            assert!(!minimums.is_too_old(header), "{header:?}");
        }
    }
}
