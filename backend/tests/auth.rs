//! Signing in, sessions and the account endpoints, exercised through the HTTP
//! router against a real database.
//!
//! Needs PostgreSQL and the connection strings from `.env`. Each test uses
//! identifiers of its own under a reserved test domain and number range, and
//! removes what it created.

use std::sync::{Arc, Mutex};

use axum::Router;
use axum::body::Body;
use axum::http::header::{AUTHORIZATION, CONTENT_TYPE, COOKIE, ORIGIN, SET_COOKIE};
use axum::http::{HeaderMap, Method, Request, StatusCode};
use exchange_backend::auth::{AuthRules, CodeSender, Purpose, SendFuture, token_hash};
use exchange_backend::db;
use exchange_backend::domain::identity::Identifier;
use exchange_backend::http::{self, AppState, Settings};
use http_body_util::BodyExt;
use serde_json::{Value, json};
use sqlx::postgres::{PgPool, PgPoolOptions};
use tower::ServiceExt;
use uuid::Uuid;

const WEB_ORIGIN: &str = "https://app.test";
const EMAIL_DOMAIN: &str = "auth-test.invalid";
const PHONE_PREFIX: &str = "+1999";

/// Keeps the codes the service "sent", so tests can read them back.
#[derive(Default)]
struct Outbox(Mutex<Vec<(String, String)>>);

impl CodeSender for Outbox {
    fn send<'a>(&'a self, to: &'a Identifier, code: &'a str, _: Purpose) -> SendFuture<'a> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .push((to.as_str().to_owned(), code.to_owned()));
            Ok(())
        })
    }
}

struct Reply {
    status: StatusCode,
    headers: HeaderMap,
    body: Value,
}

impl Reply {
    fn code(&self) -> &str {
        self.body["code"].as_str().unwrap_or("")
    }
}

struct App {
    router: Router,
    outbox: Arc<Outbox>,
    owner: PgPool,
}

fn env(name: &str) -> String {
    dotenvy::dotenv().ok();
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set; see .env.example"))
}

async fn connect(url_var: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(3)
        .connect(&env(url_var))
        .await
        .unwrap_or_else(|error| panic!("cannot connect using {url_var}: {error}"))
}

fn email() -> String {
    format!("{}@{EMAIL_DOMAIN}", Uuid::new_v4().simple())
}

fn phone() -> String {
    format!("{PHONE_PREFIX}{:07}", Uuid::new_v4().as_u128() % 10_000_000)
}

impl App {
    async fn start() -> Self {
        let owner = connect("MIGRATION_DATABASE_URL").await;
        db::MIGRATOR.run(&owner).await.expect("migrations apply");

        let outbox = Arc::new(Outbox::default());
        let state = AppState {
            db: connect("DATABASE_URL").await,
            settings: Arc::new(Settings {
                app_secret: b"test-secret-test-secret-test-secret".to_vec(),
                web_origin: WEB_ORIGIN.to_owned(),
                auth: AuthRules::default(),
                rules: Default::default(),
                consent_version: "test".to_owned(),
            }),
            code_sender: outbox.clone(),
        };

        let app = Self {
            router: http::router(state),
            outbox,
            owner,
        };
        // Clear what an earlier, interrupted run may have left.
        app.remove_test_rows("created_at < now() - interval '10 minutes'")
            .await;
        app
    }

    async fn send(
        &self,
        method: Method,
        path: &str,
        body: Option<Value>,
        headers: &[(axum::http::HeaderName, &str)],
    ) -> Reply {
        let mut request = Request::builder().method(method).uri(path);
        for (name, value) in headers {
            request = request.header(name, *value);
        }
        let request = match body {
            Some(body) => request
                .header(CONTENT_TYPE, "application/json")
                .body(Body::from(body.to_string())),
            None => request.body(Body::empty()),
        }
        .unwrap();

        let response = self.router.clone().oneshot(request).await.unwrap();
        let status = response.status();
        let headers = response.headers().clone();
        let bytes = response.into_body().collect().await.unwrap().to_bytes();
        let body = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
        Reply {
            status,
            headers,
            body,
        }
    }

    async fn post(&self, path: &str, body: Value) -> Reply {
        self.send(Method::POST, path, Some(body), &[]).await
    }

    async fn get_as(&self, token: &str, path: &str) -> Reply {
        self.send(
            Method::GET,
            path,
            None,
            &[(AUTHORIZATION, &format!("Bearer {token}"))],
        )
        .await
    }

    async fn request_code(&self, identifier: &str) -> String {
        let reply = self
            .post("/v1/auth/codes", json!({ "identifier": identifier }))
            .await;
        assert_eq!(reply.status, StatusCode::NO_CONTENT, "{:?}", reply.body);
        self.last_code(identifier)
    }

    fn last_code(&self, identifier: &str) -> String {
        let sent = self.outbox.0.lock().unwrap();
        let (_, code) = sent
            .iter()
            .rev()
            .find(|(to, _)| to == identifier)
            .unwrap_or_else(|| panic!("no code was sent to {identifier}"));
        code.clone()
    }

    async fn create_session(&self, identifier: &str, code: &str) -> Reply {
        self.post(
            "/v1/auth/sessions",
            json!({ "identifier": identifier, "code": code, "delivery": "TOKEN" }),
        )
        .await
    }

    /// Signs in and returns the session token and account ID.
    async fn sign_in(&self, identifier: &str) -> (String, String) {
        let code = self.request_code(identifier).await;
        let reply = self.create_session(identifier, &code).await;
        assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
        (
            reply.body["token"].as_str().unwrap().to_owned(),
            reply.body["account"]["id"].as_str().unwrap().to_owned(),
        )
    }

    async fn remove_test_rows(&self, condition: &str) {
        let accounts = format!(
            "SELECT id FROM account
             WHERE (email LIKE '%@{EMAIL_DOMAIN}' OR phone LIKE '{PHONE_PREFIX}%') AND {condition}"
        );
        let statements = [
            format!("DELETE FROM account_session WHERE account_id IN ({accounts})"),
            format!("DELETE FROM account WHERE id IN ({accounts})"),
            format!(
                "DELETE FROM one_time_code
                 WHERE (identifier LIKE '%@{EMAIL_DOMAIN}' OR identifier LIKE '{PHONE_PREFIX}%')
                   AND {condition}"
            ),
        ];
        for statement in statements {
            sqlx::query(sqlx::AssertSqlSafe(statement))
                .execute(&self.owner)
                .await
                .unwrap();
        }
    }

    /// Removes the rows belonging to the identifiers this test used.
    async fn finish(self, identifiers: &[&str]) {
        for identifier in identifiers {
            for statement in [
                "DELETE FROM account_session WHERE account_id IN
                    (SELECT id FROM account WHERE email = $1 OR phone = $1)",
                "DELETE FROM account WHERE email = $1 OR phone = $1",
                "DELETE FROM one_time_code WHERE identifier = $1",
            ] {
                sqlx::query(statement)
                    .bind(identifier)
                    .execute(&self.owner)
                    .await
                    .unwrap();
            }
        }
    }
}

#[tokio::test]
async fn signing_in_creates_an_account_and_a_working_session() {
    let app = App::start().await;
    let email = email();

    let code = app.request_code(&email).await;
    let reply = app.create_session(&email, &code).await;

    assert_eq!(reply.status, StatusCode::OK);
    let account = &reply.body["account"];
    assert_eq!(account["email"], email.as_str());
    assert_eq!(account["phone"], Value::Null);
    assert_eq!(account["display_name"], "");
    assert_eq!(account["language"], "en");
    assert_eq!(account["adult_confirmed"], false);

    let token = reply.body["token"].as_str().unwrap();
    let me = app.get_as(token, "/v1/me").await;
    assert_eq!(me.status, StatusCode::OK);
    assert_eq!(me.body["id"], account["id"]);

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn signing_in_again_reaches_the_same_account() {
    let app = App::start().await;
    let email = email();

    let (_, first) = app.sign_in(&email).await;
    // Typed differently, same address.
    let typed = format!("  {} ", email.to_uppercase());
    let reply = app
        .post("/v1/auth/codes", json!({ "identifier": typed }))
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    let reply = app.create_session(&typed, &app.last_code(&email)).await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["account"]["id"], first.as_str());

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn a_phone_number_signs_in_too() {
    let app = App::start().await;
    let phone = phone();

    let code = app.request_code(&phone).await;
    let reply = app.create_session(&phone, &code).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["account"]["phone"], phone.as_str());
    assert_eq!(reply.body["account"]["email"], Value::Null);

    app.finish(&[&phone]).await;
}

#[tokio::test]
async fn a_code_works_once() {
    let app = App::start().await;
    let email = email();

    let code = app.request_code(&email).await;
    assert_eq!(
        app.create_session(&email, &code).await.status,
        StatusCode::OK
    );

    let again = app.create_session(&email, &code).await;
    assert_eq!(
        (again.status, again.code()),
        (StatusCode::UNAUTHORIZED, "INVALID_CODE")
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn five_wrong_guesses_kill_a_code() {
    let app = App::start().await;
    let email = email();
    let code = app.request_code(&email).await;
    let wrong = if code == "000000" { "000001" } else { "000000" };

    for _ in 0..5 {
        let reply = app.create_session(&email, wrong).await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNAUTHORIZED, "INVALID_CODE")
        );
    }
    // The right code no longer helps.
    let reply = app.create_session(&email, &code).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "INVALID_CODE")
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn a_new_code_replaces_the_old_one() {
    let app = App::start().await;
    let email = email();

    let first = app.request_code(&email).await;
    let second = app.request_code(&email).await;

    if first != second {
        assert_eq!(
            app.create_session(&email, &first).await.status,
            StatusCode::UNAUTHORIZED
        );
    }
    assert_eq!(
        app.create_session(&email, &second).await.status,
        StatusCode::OK
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn an_expired_code_is_refused() {
    let app = App::start().await;
    let email = email();
    let code = app.request_code(&email).await;

    sqlx::query("UPDATE one_time_code SET expires_at = now() WHERE identifier = $1")
        .bind(&email)
        .execute(&app.owner)
        .await
        .unwrap();

    let reply = app.create_session(&email, &code).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNAUTHORIZED, "INVALID_CODE")
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn code_requests_are_limited_per_identifier() {
    let app = App::start().await;
    let email = email();

    for _ in 0..5 {
        app.request_code(&email).await;
    }
    let reply = app
        .post("/v1/auth/codes", json!({ "identifier": email }))
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS")
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn bad_input_gets_a_typed_refusal() {
    let app = App::start().await;

    let reply = app
        .post("/v1/auth/codes", json!({ "identifier": "not an address" }))
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_IDENTIFIER")
    );

    let reply = app
        .post("/v1/auth/codes", json!({ "wrong": "shape" }))
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST")
    );

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/codes",
            None,
            &[(CONTENT_TYPE, "application/json")],
        )
        .await;
    assert_eq!(reply.code(), "INVALID_REQUEST");
}

#[tokio::test]
async fn account_endpoints_need_a_session() {
    let app = App::start().await;

    let none = app.send(Method::GET, "/v1/me", None, &[]).await;
    assert_eq!(
        (none.status, none.code()),
        (StatusCode::UNAUTHORIZED, "UNAUTHENTICATED")
    );

    let garbage = app.get_as("not-a-token", "/v1/me").await;
    assert_eq!(
        (garbage.status, garbage.code()),
        (StatusCode::UNAUTHORIZED, "UNAUTHENTICATED")
    );
}

#[tokio::test]
async fn signing_out_ends_the_session() {
    let app = App::start().await;
    let email = email();
    let (token, _) = app.sign_in(&email).await;
    let bearer = format!("Bearer {token}");

    let reply = app
        .send(
            Method::DELETE,
            "/v1/auth/session",
            None,
            &[(AUTHORIZATION, &bearer)],
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    assert_eq!(
        app.get_as(&token, "/v1/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn an_expired_session_stops_working() {
    let app = App::start().await;
    let email = email();
    let (token, _) = app.sign_in(&email).await;

    sqlx::query("UPDATE account_session SET expires_at = now() WHERE token_hash = $1")
        .bind(token_hash(&token).as_slice())
        .execute(&app.owner)
        .await
        .unwrap();
    assert_eq!(
        app.get_as(&token, "/v1/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn a_web_session_is_a_cookie_scripts_cannot_read() {
    let app = App::start().await;
    let email = email();
    let code = app.request_code(&email).await;
    let body = json!({ "identifier": email, "code": code, "delivery": "COOKIE" });

    // Not from our web app: refused before the code is even looked at.
    let foreign = app
        .send(
            Method::POST,
            "/v1/auth/sessions",
            Some(body.clone()),
            &[(ORIGIN, "https://elsewhere.test")],
        )
        .await;
    assert_eq!(foreign.status, StatusCode::UNAUTHORIZED);

    let reply = app
        .send(
            Method::POST,
            "/v1/auth/sessions",
            Some(body),
            &[(ORIGIN, WEB_ORIGIN)],
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(
        reply.body["token"],
        Value::Null,
        "the token is not exposed to the page"
    );

    let set_cookie = reply.headers[SET_COOKIE].to_str().unwrap();
    for attribute in ["HttpOnly", "SameSite=Lax", "Secure", "Path=/"] {
        assert!(set_cookie.contains(attribute), "{set_cookie}");
    }
    let cookie = set_cookie.split(';').next().unwrap().to_owned();

    // Reading needs only the cookie.
    let me = app
        .send(Method::GET, "/v1/me", None, &[(COOKIE, &cookie)])
        .await;
    assert_eq!(me.status, StatusCode::OK);

    // Changing anything also needs to come from our web app.
    let update = json!({ "display_name": "Ana" });
    let forged = app
        .send(
            Method::PATCH,
            "/v1/me",
            Some(update.clone()),
            &[(COOKIE, &cookie)],
        )
        .await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);

    let forged = app
        .send(
            Method::PATCH,
            "/v1/me",
            Some(update.clone()),
            &[(COOKIE, &cookie), (ORIGIN, "https://elsewhere.test")],
        )
        .await;
    assert_eq!(forged.status, StatusCode::UNAUTHORIZED);

    let genuine = app
        .send(
            Method::PATCH,
            "/v1/me",
            Some(update),
            &[(COOKIE, &cookie), (ORIGIN, WEB_ORIGIN)],
        )
        .await;
    assert_eq!(genuine.status, StatusCode::OK);
    assert_eq!(genuine.body["display_name"], "Ana");

    // Signing out clears the cookie.
    let out = app
        .send(
            Method::DELETE,
            "/v1/auth/session",
            None,
            &[(COOKIE, &cookie), (ORIGIN, WEB_ORIGIN)],
        )
        .await;
    assert_eq!(out.status, StatusCode::NO_CONTENT);
    assert!(
        out.headers[SET_COOKIE]
            .to_str()
            .unwrap()
            .contains("Max-Age=0")
    );
    let me = app
        .send(Method::GET, "/v1/me", None, &[(COOKIE, &cookie)])
        .await;
    assert_eq!(me.status, StatusCode::UNAUTHORIZED);

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn the_profile_can_be_edited() {
    let app = App::start().await;
    let email = email();
    let (token, _) = app.sign_in(&email).await;
    let bearer = format!("Bearer {token}");
    let patch = |body: Value| {
        let (app, bearer) = (&app, &bearer);
        async move {
            app.send(
                Method::PATCH,
                "/v1/me",
                Some(body),
                &[(AUTHORIZATION, bearer)],
            )
            .await
        }
    };

    let reply = patch(json!({
        "display_name": "  Ana Ruiz ",
        "language": "es",
        "adult_confirmed": true,
    }))
    .await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["display_name"], "Ana Ruiz");
    assert_eq!(reply.body["language"], "es");
    assert_eq!(reply.body["adult_confirmed"], true);

    // A confirmation of adulthood cannot be taken back, and omitted fields stay.
    let reply = patch(json!({ "adult_confirmed": false })).await;
    assert_eq!(reply.body["adult_confirmed"], true);
    assert_eq!(reply.body["display_name"], "Ana Ruiz");

    for bad in [
        json!({ "display_name": "   " }),
        json!({ "display_name": "x".repeat(101) }),
    ] {
        let reply = patch(bad).await;
        assert_eq!(
            (reply.status, reply.code()),
            (StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST")
        );
    }
    // A language the product does not speak is refused; a regional variant of
    // one it does falls back to the base language.
    let reply = patch(json!({ "language": "tlh" })).await;
    assert_eq!(reply.code(), "INVALID_REQUEST");
    let reply = patch(json!({ "language": "en-GB" })).await;
    assert_eq!(reply.body["language"], "en");

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn a_second_identifier_can_be_verified_and_then_signs_in() {
    let app = App::start().await;
    let (email, phone) = (email(), phone());
    let (token, account_id) = app.sign_in(&email).await;

    let code = app.request_code(&phone).await;
    let reply = app
        .send(
            Method::POST,
            "/v1/me/identifiers",
            Some(json!({ "identifier": phone, "code": code })),
            &[(AUTHORIZATION, &format!("Bearer {token}"))],
        )
        .await;
    assert_eq!(reply.status, StatusCode::OK, "{:?}", reply.body);
    assert_eq!(reply.body["email"], email.as_str());
    assert_eq!(reply.body["phone"], phone.as_str());

    let (_, by_phone) = app.sign_in(&phone).await;
    assert_eq!(by_phone, account_id);

    app.finish(&[&email, &phone]).await;
}

#[tokio::test]
async fn an_identifier_belongs_to_one_account() {
    let app = App::start().await;
    let (ana, ben) = (email(), email());
    app.sign_in(&ana).await;
    let (ben_token, _) = app.sign_in(&ben).await;

    // Ben holds a valid code for Ana's address, but it is already hers.
    let code = app.request_code(&ana).await;
    let reply = app
        .send(
            Method::POST,
            "/v1/me/identifiers",
            Some(json!({ "identifier": ana, "code": code })),
            &[(AUTHORIZATION, &format!("Bearer {ben_token}"))],
        )
        .await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::CONFLICT, "IDENTIFIER_IN_USE")
    );

    app.finish(&[&ana, &ben]).await;
}

#[tokio::test]
async fn a_suspended_account_is_shut_out() {
    let app = App::start().await;
    let email = email();
    let (token, _) = app.sign_in(&email).await;

    sqlx::query("UPDATE account SET status = 'SUSPENDED' WHERE email = $1")
        .bind(&email)
        .execute(&app.owner)
        .await
        .unwrap();

    assert_eq!(
        app.get_as(&token, "/v1/me").await.status,
        StatusCode::UNAUTHORIZED
    );

    let code = app.request_code(&email).await;
    let reply = app.create_session(&email, &code).await;
    assert_eq!(
        (reply.status, reply.code()),
        (StatusCode::FORBIDDEN, "ACCOUNT_SUSPENDED")
    );

    app.finish(&[&email]).await;
}

#[tokio::test]
async fn neither_codes_nor_tokens_are_stored() {
    let app = App::start().await;
    let email = email();
    let code = app.request_code(&email).await;

    let stored: Vec<u8> =
        sqlx::query_scalar("SELECT code_hash FROM one_time_code WHERE identifier = $1")
            .bind(&email)
            .fetch_one(&app.owner)
            .await
            .unwrap();
    assert_eq!(stored.len(), 32);
    assert_ne!(stored, code.as_bytes());
    // Keyed: knowing the code is not enough to reproduce the stored hash.
    assert_ne!(stored, token_hash(&code));

    let reply = app.create_session(&email, &code).await;
    let token = reply.body["token"].as_str().unwrap();
    let hashes: Vec<Vec<u8>> = sqlx::query_scalar(
        "SELECT s.token_hash FROM account_session s JOIN account a ON a.id = s.account_id
         WHERE a.email = $1",
    )
    .bind(&email)
    .fetch_all(&app.owner)
    .await
    .unwrap();
    assert_eq!(hashes, vec![token_hash(token).to_vec()]);

    app.finish(&[&email]).await;
}
