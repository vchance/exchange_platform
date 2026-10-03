//! The service serving the web app from the same origin as the API
//! (DESIGN.md §13.5), against a directory laid out as the web build writes
//! it: the app's `index.html`, one entry page per language at
//! `{language}/i/index.html`, hashed files under `assets/`, and the public
//! files beside them. Needs no database: nothing here reaches one.

use std::path::{Path, PathBuf};
use std::sync::Arc;

use axum::Router;
use axum::body::Body;
use axum::http::header::{
    CACHE_CONTROL, CONTENT_SECURITY_POLICY, CONTENT_TYPE, LOCATION, REFERRER_POLICY,
    STRICT_TRANSPORT_SECURITY, X_CONTENT_TYPE_OPTIONS, X_FRAME_OPTIONS,
};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use exchange_backend::auth::{AuthRules, LogSender};
use exchange_backend::db;
use exchange_backend::http::{self, AppState, Settings, TrustedProxies, WebApp};
use http_body_util::BodyExt;
use tower::ServiceExt;

const HOME: &str = "<!doctype html><html lang=\"en\"><title>Exchange</title>home</html>";
const EN: &str = "<!doctype html><html lang=\"en\"><title>Invitation</title>en</html>";
const ES: &str = "<!doctype html><html lang=\"es\"><title>Invitación</title>es</html>";
const SCRIPT: &str = "console.log('hashed')";
const ICON: &str = "<svg xmlns=\"http://www.w3.org/2000/svg\"/>";

/// A build in a directory of its own, removed when dropped.
struct Build(PathBuf);

impl Build {
    fn write() -> Self {
        let directory = std::env::temp_dir().join(format!(
            "exchange-web-test-{}-{}",
            std::process::id(),
            uuid::Uuid::new_v4().simple()
        ));
        let write = |path: &str, text: &str| {
            let file = directory.join(path);
            std::fs::create_dir_all(file.parent().unwrap()).unwrap();
            std::fs::write(file, text).unwrap();
        };
        write("index.html", HOME);
        write("en/i/index.html", EN);
        write("es/i/index.html", ES);
        write("assets/index-DCaXBRW7.js", SCRIPT);
        write("favicon.svg", ICON);
        Self(directory)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for Build {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).ok();
    }
}

fn service(web_origin: &str, web: Option<WebApp>) -> Router {
    let state = AppState {
        // Never connected to: nothing here needs a database.
        db: db::pool("postgres://nobody@127.0.0.1:1/nothing").unwrap(),
        settings: Arc::new(Settings {
            app_secret: b"test-secret-test-secret-test-secret".to_vec(),
            web_origin: web_origin.to_owned(),
            auth: AuthRules::default(),
            rules: Default::default(),
            consent_version: "test".to_owned(),
            proxies: TrustedProxies::none(),
            min_client_versions: Default::default(),
        }),
        code_sender: Arc::new(LogSender),
    };
    http::router(state, web)
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: String,
}

impl Reply {
    fn header(&self, name: impl axum::http::header::AsHeaderName) -> &str {
        self.headers
            .get(name)
            .map(|value| value.to_str().unwrap())
            .unwrap_or("")
    }
}

async fn fetch(app: &Router, method: Method, path: &str) -> Reply {
    let request = Request::builder()
        .method(method)
        .uri(path)
        .body(Body::empty())
        .unwrap();
    let response = app.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let headers = response.headers().clone();
    let bytes = response.into_body().collect().await.unwrap().to_bytes();
    Reply {
        status,
        headers,
        body: String::from_utf8_lossy(&bytes).into_owned(),
    }
}

async fn get(app: &Router, path: &str) -> Reply {
    fetch(app, Method::GET, path).await
}

#[tokio::test]
async fn the_entry_pages_and_the_app_are_served_without_redirects() {
    let build = Build::write();
    let web = WebApp::open(build.path()).unwrap();
    assert_eq!(web.languages(), ["en", "es"]);
    let app = service("http://localhost:8080", Some(web));

    let home = get(&app, "/").await;
    assert_eq!(home.status, StatusCode::OK);
    assert!(home.header(CONTENT_TYPE).starts_with("text/html"));
    assert_eq!(home.body, HOME);

    // An invitation link, with or without the slash, is that language's page.
    for path in ["/es/i", "/es/i/"] {
        let page = get(&app, path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert_eq!(page.header(LOCATION), "", "{path}: no redirect");
        assert_eq!(page.body, ES, "{path}");
    }
    assert_eq!(get(&app, "/en/i").await.body, EN);

    // A route the app handles in the browser opens on the app's own page,
    // and so does a language the build does not have.
    for path in [
        "/exchanges/some-id",
        "/exchanges/some-id/record",
        "/fr/i",
        "/account",
    ] {
        let page = get(&app, path).await;
        assert_eq!(page.status, StatusCode::OK, "{path}");
        assert_eq!(page.body, HOME, "{path}");
    }

    // Files are files.
    let script = get(&app, "/assets/index-DCaXBRW7.js").await;
    assert_eq!(script.status, StatusCode::OK);
    assert!(
        script.header(CONTENT_TYPE).contains("javascript"),
        "{}",
        script.header(CONTENT_TYPE)
    );
    assert_eq!(script.body, SCRIPT);
    let icon = get(&app, "/favicon.svg").await;
    assert!(icon.header(CONTENT_TYPE).starts_with("image/svg+xml"));

    // HEAD works as GET does, without a body.
    let head = fetch(&app, Method::HEAD, "/es/i").await;
    assert_eq!(head.status, StatusCode::OK);
    assert_eq!(head.body, "");
}

#[tokio::test]
async fn hashed_assets_are_cached_for_a_year_and_pages_are_not() {
    let build = Build::write();
    let app = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );

    assert_eq!(
        get(&app, "/assets/index-DCaXBRW7.js")
            .await
            .header(CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    for path in ["/", "/es/i", "/exchanges/some-id", "/favicon.svg"] {
        assert_eq!(
            get(&app, path).await.header(CACHE_CONTROL),
            "no-cache",
            "{path}"
        );
    }
    // A missing asset is the app's page, which must not be cached as the
    // asset: a later build may have it.
    let missing = get(&app, "/assets/gone-00000000.js").await;
    assert_eq!(missing.body, HOME);
    assert_eq!(missing.header(CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn api_paths_keep_precedence_and_are_never_answered_with_a_page() {
    let build = Build::write();
    let app = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );

    assert_eq!(get(&app, "/healthz").await.status, StatusCode::NO_CONTENT);
    let meta = get(&app, "/v1/meta").await;
    assert_eq!(meta.status, StatusCode::OK);
    assert!(meta.header(CONTENT_TYPE).starts_with("application/json"));

    for path in ["/v1/nothing", "/v1/exchanges/x/y/z", "/v1/", "/v1"] {
        let reply = get(&app, path).await;
        assert_eq!(reply.status, StatusCode::NOT_FOUND, "{path}");
        assert!(
            !reply.header(CONTENT_TYPE).starts_with("text/html"),
            "{path}: {}",
            reply.header(CONTENT_TYPE)
        );
    }
}

#[tokio::test]
async fn nothing_outside_the_build_is_served() {
    let build = Build::write();
    let app = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );
    for path in [
        "/../Cargo.toml",
        "/%2e%2e/Cargo.toml",
        "/assets/../../Cargo.toml",
    ] {
        let reply = get(&app, path).await;
        assert!(!reply.body.contains("[package]"), "{path}");
    }
}

#[tokio::test]
async fn every_response_carries_the_headers_a_signing_page_needs() {
    let build = Build::write();
    let plain = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );
    for path in [
        "/",
        "/es/i",
        "/assets/index-DCaXBRW7.js",
        "/v1/meta",
        "/healthz",
    ] {
        let reply = get(&plain, path).await;
        assert_eq!(reply.header(X_FRAME_OPTIONS), "DENY", "{path}");
        assert_eq!(
            reply.header(CONTENT_SECURITY_POLICY),
            "default-src 'self'; img-src 'self' data:; object-src 'none'; \
             base-uri 'none'; form-action 'self'; frame-ancestors 'none'",
            "{path}"
        );
        assert_eq!(reply.header(X_CONTENT_TYPE_OPTIONS), "nosniff", "{path}");
        assert_eq!(reply.header(REFERRER_POLICY), "no-referrer", "{path}");
        // Not over plain HTTP, or a development setup locks itself out.
        assert_eq!(reply.header(STRICT_TRANSPORT_SECURITY), "", "{path}");
    }

    let secure = service(
        "https://app.example.test",
        Some(WebApp::open(build.path()).unwrap()),
    );
    assert_eq!(
        get(&secure, "/").await.header(STRICT_TRANSPORT_SECURITY),
        "max-age=31536000"
    );
    assert_eq!(
        get(&secure, "/v1/meta")
            .await
            .header(STRICT_TRANSPORT_SECURITY),
        "max-age=31536000"
    );
}

#[tokio::test]
async fn api_answers_are_never_stored_and_pages_keep_their_own_caching() {
    let build = Build::write();
    let app = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );
    // Answered, refused or not found: none of it may be kept.
    for path in ["/v1/meta", "/v1/me", "/v1/nothing", "/v1"] {
        assert_eq!(
            get(&app, path).await.header(CACHE_CONTROL),
            "no-store",
            "{path}"
        );
    }
    assert_eq!(
        fetch(&app, Method::POST, "/v1/exchanges")
            .await
            .header(CACHE_CONTROL),
        "no-store"
    );
    // Pages and assets keep what the web app's own rules say.
    assert_eq!(get(&app, "/").await.header(CACHE_CONTROL), "no-cache");
    assert_eq!(
        get(&app, "/assets/index-DCaXBRW7.js")
            .await
            .header(CACHE_CONTROL),
        "public, max-age=31536000, immutable"
    );
    assert_eq!(get(&app, "/v1x").await.header(CACHE_CONTROL), "no-cache");
}

#[tokio::test]
async fn a_path_with_several_leading_slashes_never_redirects_to_another_host() {
    let build = Build::write();
    let app = service(
        "http://localhost:8080",
        Some(WebApp::open(build.path()).unwrap()),
    );
    // `//assets/` in a Location header is a link to a host named `assets`.
    for path in [
        "//assets",
        "///assets",
        "//evil.example",
        "/assets",
        "//es",
        "/es",
    ] {
        let reply = get(&app, path).await;
        assert!(
            !reply.status.is_redirection(),
            "{path}: {} to {}",
            reply.status,
            reply.header(LOCATION)
        );
        assert_eq!(reply.header(LOCATION), "", "{path}");
        assert_eq!(reply.status, StatusCode::OK, "{path}");
        assert_eq!(reply.body, HOME, "{path}");
    }
    // Read as the one-slash path it means.
    assert_eq!(get(&app, "//es/i").await.body, ES);
    assert_eq!(get(&app, "//assets/index-DCaXBRW7.js").await.body, SCRIPT);
    // An API path is still the API's, and never a page.
    let reply = get(&app, "//v1/meta").await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
    assert!(!reply.header(CONTENT_TYPE).starts_with("text/html"));
}

#[tokio::test]
async fn without_a_web_directory_the_api_stands_alone() {
    let app = service("http://localhost:8080", None);
    assert_eq!(get(&app, "/healthz").await.status, StatusCode::NO_CONTENT);
    assert_eq!(get(&app, "/").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/es/i").await.status, StatusCode::NOT_FOUND);
    assert_eq!(get(&app, "/healthz").await.header(X_FRAME_OPTIONS), "DENY");
}

#[tokio::test]
async fn a_directory_that_is_not_a_build_is_refused() {
    let empty = std::env::temp_dir().join(format!(
        "exchange-not-a-build-{}",
        uuid::Uuid::new_v4().simple()
    ));
    std::fs::create_dir_all(&empty).unwrap();
    assert!(WebApp::open(&empty).is_err());
    assert!(WebApp::open(&empty.join("missing")).is_err());
    std::fs::remove_dir_all(&empty).ok();
}
