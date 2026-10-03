//! Serving the built web app from the same origin as the API (DESIGN.md
//! §13.5).
//!
//! The build is static files: `index.html`, the app's own entry page; one
//! entry page per language at `{language}/i/index.html`, which an invitation
//! link points at; hashed files under `assets/`; and whatever else is in the
//! app's public directory. Three rules turn that into a site:
//!
//! * `/{language}/i`, with or without a trailing slash, is answered with that
//!   language's page directly, without a redirect that would change the link
//!   a messaging app previews;
//! * a path that is no file is answered with `index.html`, so a route the app
//!   handles in the browser can be opened or reloaded directly;
//! * the API's paths are never answered with a page.
//!
//! A path that starts with several slashes is read as if it had one, and a
//! directory is never answered with a redirect: `//assets` redirected to
//! `//assets/` would be a link to a host named `assets`.
//!
//! Hashed assets may be cached for a year; everything else must be checked
//! each time, or a new build would be a page pointing at files that are gone.
//!
//! The token of an invitation link is in the URL fragment, which a browser
//! never sends. Nothing here reads the query string either, and the request
//! span logs the path alone.

use std::path::{Path, PathBuf};

use axum::Router;
use axum::extract::Request;
use axum::http::header::{CACHE_CONTROL, CONTENT_TYPE};
use axum::http::{HeaderValue, StatusCode, Uri};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use tower_http::services::{ServeDir, ServeFile};

use crate::error::{ApiError, ErrorCode};

/// Paths that belong to the API, whatever is or is not routed under them.
const API_PATHS: &[&str] = &["/v1", "/healthz", "/readyz"];

/// Whether a path is one of the API's, or below one of them.
fn is_api_path(path: &str) -> bool {
    API_PATHS.iter().any(|api| {
        path.strip_prefix(api)
            .is_some_and(|rest| rest.is_empty() || rest.starts_with('/'))
    })
}

/// A built web app, ready to serve.
#[derive(Clone, Debug)]
pub struct WebApp {
    directory: PathBuf,
    /// The languages that have an entry page, from the build itself.
    languages: Vec<String>,
}

impl WebApp {
    /// Reads the build at `directory`. Fails if it is not one.
    pub fn open(directory: &Path) -> std::io::Result<Self> {
        let index = directory.join("index.html");
        if !index.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!(
                    "{} has no index.html; is it a web build?",
                    directory.display()
                ),
            ));
        }
        let mut languages: Vec<String> = std::fs::read_dir(directory)?
            .filter_map(|entry| entry.ok())
            .filter(|entry| entry.path().join("i").join("index.html").is_file())
            .filter_map(|entry| entry.file_name().into_string().ok())
            .collect();
        languages.sort();
        Ok(Self {
            directory: directory.to_owned(),
            languages,
        })
    }

    pub fn languages(&self) -> &[String] {
        &self.languages
    }

    /// The language whose entry page `path` names, if it names one.
    fn entry_page(&self, path: &str) -> Option<&str> {
        let rest = path.strip_prefix('/')?;
        let rest = rest.strip_suffix('/').unwrap_or(rest);
        let language = rest.strip_suffix("/i")?;
        self.languages
            .iter()
            .find(|known| known.as_str() == language)
            .map(String::as_str)
    }

    /// The service for everything the API does not route.
    pub fn router(self) -> Router {
        // No redirect from a directory to its path with a slash: the entry
        // pages are routed above without one, and a redirect built from the
        // request's own path can point at another host.
        let files = ServeDir::new(&self.directory)
            .append_index_html_on_directories(true)
            .redirect_to_trailing_slash(false)
            .fallback(ServeFile::new(self.directory.join("index.html")));
        Router::new()
            .fallback_service(files)
            .layer(middleware::from_fn(cache_control))
            .layer(middleware::from_fn_with_state(self, route))
    }
}

/// `uri` with any run of slashes at the start of its path made one, or
/// `None` if it has no such run.
fn single_leading_slash(uri: &Uri) -> Option<Uri> {
    let path = uri.path();
    if !path.starts_with("//") {
        return None;
    }
    let path = format!("/{}", path.trim_start_matches('/'));
    let path_and_query = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path,
    };
    path_and_query.parse().ok()
}

/// Keeps API paths away from the pages and sends an entry-page path to its
/// file.
async fn route(
    axum::extract::State(web): axum::extract::State<WebApp>,
    mut request: Request,
    next: Next,
) -> Response {
    if request.uri().path().starts_with("//") {
        let Some(uri) = single_leading_slash(request.uri()) else {
            return StatusCode::NOT_FOUND.into_response();
        };
        *request.uri_mut() = uri;
    }
    let path = request.uri().path();
    if is_api_path(path) {
        return ApiError::from(ErrorCode::NotFound).into_response();
    }
    if let Some(language) = web.entry_page(path) {
        let Ok(uri) = format!("/{language}/i/index.html").parse::<Uri>() else {
            return StatusCode::NOT_FOUND.into_response();
        };
        *request.uri_mut() = uri;
    }
    next.run(request).await
}

/// Hashed assets for a year, everything else checked every time. A page is
/// never cached, even when it was served for an asset's path because the
/// asset is gone: a later build may have the asset.
async fn cache_control(request: Request, next: Next) -> Response {
    let hashed = request.uri().path().starts_with("/assets/");
    let mut response = next.run(request).await;
    if response.status().is_success() || response.status() == StatusCode::NOT_MODIFIED {
        let page = response
            .headers()
            .get(CONTENT_TYPE)
            .and_then(|value| value.to_str().ok())
            .is_some_and(|value| value.starts_with("text/html"));
        let value = if hashed && !page {
            "public, max-age=31536000, immutable"
        } else {
            "no-cache"
        };
        response
            .headers_mut()
            .insert(CACHE_CONTROL, HeaderValue::from_static(value));
    }
    response
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(languages: &[&str]) -> WebApp {
        WebApp {
            directory: PathBuf::from("/nowhere"),
            languages: languages.iter().map(|code| (*code).to_owned()).collect(),
        }
    }

    #[test]
    fn api_paths_are_the_api_and_everything_below_them() {
        for api in ["/v1", "/v1/", "/v1/meta", "/healthz", "/readyz", "/readyz/"] {
            assert!(is_api_path(api), "{api}");
        }
        for page in [
            "/",
            "/v10",
            "/v1x/meta",
            "/healthzz",
            "/es/i",
            "/exchanges/v1",
        ] {
            assert!(!is_api_path(page), "{page}");
        }
    }

    #[test]
    fn leading_slashes_are_made_one() {
        let one =
            |text: &str| single_leading_slash(&text.parse().unwrap()).map(|uri| uri.to_string());
        assert_eq!(one("//assets").as_deref(), Some("/assets"));
        assert_eq!(
            one("///evil.example/x?a=b").as_deref(),
            Some("/evil.example/x?a=b")
        );
        assert_eq!(one("//").as_deref(), Some("/"));
        assert_eq!(one("/assets"), None);
        assert_eq!(one("/a//b"), None);
    }

    #[test]
    fn an_entry_page_path_names_a_language_the_build_has() {
        let web = web(&["en", "es", "pt-BR"]);
        assert_eq!(web.entry_page("/es/i"), Some("es"));
        assert_eq!(web.entry_page("/es/i/"), Some("es"));
        assert_eq!(web.entry_page("/pt-BR/i"), Some("pt-BR"));
        for other in [
            "/fr/i",
            "/es",
            "/es/i/index.html",
            "/es/invite",
            "/i",
            "/",
            "es/i",
        ] {
            assert_eq!(web.entry_page(other), None, "{other:?}");
        }
    }
}
