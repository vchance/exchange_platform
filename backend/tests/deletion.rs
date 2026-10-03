//! Deleting an account, end to end: HTTP requests in, and what is left of
//! the account, its working data and its exchanges afterwards.
//!
//! These tests create exchanges, whose history cannot be deleted, so they use
//! the throwaway database of the exchange tests. They take turns: several of
//! them run the worker's timers, which act on every exchange in the database.

mod common;

use std::sync::{Arc, Mutex};

use axum::http::header::SET_COOKIE;
use axum::http::{HeaderName, Method, StatusCode};
use common::{App, Reply, User, accept, consent, fence_job};
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};
use tokio::sync::MutexGuard;
use uuid::Uuid;
use yuppers_backend::auth::{CodeMessage, CodeSender, Purpose, SendFuture};
use yuppers_backend::deletion;
use yuppers_backend::domain::Rules;
use yuppers_backend::exchanges::service::run_timers;
use yuppers_backend::http::TrustedProxies;
use yuppers_backend::notifications::outbox::{Delivery, DeliveryRules, deliver_due};
use yuppers_backend::notifications::wording::Wording;
use yuppers_backend::notifications::{Email, EmailSender};

const DATABASE: &str = "exchange_test_deletion";
const WEB_ORIGIN: &str = "https://app.test";

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Keeps the codes the service "sent", with what each was for.
#[derive(Default)]
struct Codes(Mutex<Vec<(String, String, Purpose)>>);

impl CodeSender for Codes {
    fn send<'a>(&'a self, message: CodeMessage<'a>) -> SendFuture<'a> {
        Box::pin(async move {
            self.0.lock().unwrap().push((
                message.to.as_str().to_owned(),
                message.code.to_owned(),
                message.purpose,
            ));
            Ok(())
        })
    }
}

impl Codes {
    /// The latest code sent to an identifier, and what it was for.
    fn last(&self, identifier: &str) -> (String, Purpose) {
        let sent = self.0.lock().unwrap();
        let (_, code, purpose) = sent
            .iter()
            .rev()
            .find(|(to, ..)| to == identifier)
            .unwrap_or_else(|| panic!("no code was sent to {identifier}"));
        (code.clone(), *purpose)
    }
}

struct Test {
    app: App,
    codes: Arc<Codes>,
    /// Keeps the other tests waiting until this one is done.
    _turn: MutexGuard<'static, ()>,
}

async fn start() -> Test {
    let turn = TURN.lock().await;
    let codes = Arc::new(Codes::default());
    let app = App::start_sending(DATABASE, Rules::default(), codes.clone()).await;
    Test {
        app,
        codes,
        _turn: turn,
    }
}

fn id(exchange: &str) -> Uuid {
    exchange.parse().unwrap()
}

#[track_caller]
fn done(reply: &Reply) {
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
}

impl Test {
    /// Asks for a code to delete the account with, and reads it back.
    async fn deletion_code(&self, user: &User, channel: &str, sent_to: &str) -> String {
        let reply = self
            .app
            .post(user, "/v1/me/deletion/codes", json!({ "channel": channel }))
            .await;
        done(&reply);
        let (code, purpose) = self.codes.last(sent_to);
        assert_eq!(purpose, Purpose::DeleteAccount);
        code
    }

    async fn delete_with(&self, user: &User, channel: &str, code: &str) -> Reply {
        self.app
            .post(
                user,
                "/v1/me/deletion",
                json!({ "channel": channel, "code": code }),
            )
            .await
    }

    /// The whole thing: a code to the account's email address, then the deletion.
    async fn delete(&self, user: &User) {
        let code = self.deletion_code(user, "EMAIL", &user.email).await;
        done(&self.delete_with(user, "EMAIL", &code).await);
    }

    /// Deletes the account and checks that nothing already in the history
    /// of these exchanges changed: only events were added.
    async fn leave(&self, user: &User, exchanges: &[&str]) {
        let mut before = Vec::new();
        for exchange in exchanges {
            before.push(history(&self.app, exchange).await);
        }
        self.delete(user).await;
        for (exchange, before) in exchanges.iter().zip(before) {
            let after = history(&self.app, exchange).await;
            assert_eq!(after.fixed, before.fixed, "revisions and signatures");
            assert!(
                after.events.starts_with(&before.events),
                "events already recorded"
            );
        }
    }

    /// Signs in the way a person does, and returns the session token.
    async fn sign_in(&self, identifier: &str) -> (String, Value) {
        let requested = self
            .app
            .call(
                None,
                Method::POST,
                "/v1/auth/codes",
                Some(json!({ "identifier": identifier })),
                &[],
            )
            .await;
        done(&requested);
        let (code, purpose) = self.codes.last(identifier);
        assert_eq!(purpose, Purpose::SignIn);
        let session = self
            .app
            .call(
                None,
                Method::POST,
                "/v1/auth/sessions",
                Some(json!({ "identifier": identifier, "code": code, "delivery": "TOKEN" })),
                &[],
            )
            .await
            .ok();
        (
            session["token"].as_str().unwrap().to_owned(),
            session["account"].clone(),
        )
    }

    async fn get_with_token(&self, token: &str, path: &str) -> Reply {
        self.app
            .call(
                None,
                Method::GET,
                path,
                None,
                &[("authorization", &format!("Bearer {token}"))],
            )
            .await
    }
}

/// What the append-only tables hold for an exchange.
struct History {
    /// Revisions, their contributions and the signatures on them, row by row.
    fixed: Vec<String>,
    events: Vec<String>,
}

async fn history(app: &App, exchange: &str) -> History {
    let mut fixed = Vec::new();
    for query in [
        "SELECT row_to_json(t)::text FROM revision t WHERE exchange_id = $1 ORDER BY sequence",
        "SELECT row_to_json(t)::text FROM contribution_snapshot t WHERE exchange_id = $1
         ORDER BY revision_id, position",
        "SELECT row_to_json(t)::text FROM acceptance t WHERE exchange_id = $1
         ORDER BY accepted_at, id",
    ] {
        let rows: Vec<String> = sqlx::query_scalar(query)
            .bind(id(exchange))
            .fetch_all(&app.owner)
            .await
            .unwrap();
        fixed.extend(rows);
    }
    let events = sqlx::query_scalar(
        "SELECT row_to_json(t)::text FROM exchange_event t WHERE exchange_id = $1
         ORDER BY sequence",
    )
    .bind(id(exchange))
    .fetch_all(&app.owner)
    .await
    .unwrap();
    History { fixed, events }
}

/// The types of an exchange's events with who caused each, oldest first.
async fn events(app: &App, exchange: &str) -> Vec<(String, Option<String>)> {
    sqlx::query_as(
        "SELECT type, actor_slot FROM exchange_event WHERE exchange_id = $1 ORDER BY sequence",
    )
    .bind(id(exchange))
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

fn event(kind: &str, slot: &str) -> (String, Option<String>) {
    (kind.to_owned(), Some(slot.to_owned()))
}

/// The notices queued for a person about an exchange, oldest first.
async fn notices(app: &App, user: &User, exchange: &str) -> Vec<String> {
    sqlx::query_scalar(
        "SELECT payload->>'notice' FROM outbox
         WHERE recipient_account_id = $1 AND exchange_id = $2 ORDER BY id",
    )
    .bind(user.id)
    .bind(id(exchange))
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

async fn count(app: &App, query: &'static str, account: Uuid) -> i64 {
    sqlx::query_scalar(query)
        .bind(account)
        .fetch_one(&app.owner)
        .await
        .unwrap()
}

async fn claim(app: &App, user: &User, token: &str) {
    app.post(user, "/v1/invitations/claim", json!({ "token": token }))
        .await
        .ok();
}

/// `from` proposes the fence job to `to`, who opens the link and signs
/// nothing. Returns the exchange and the open revision.
async fn negotiation(app: &App, from: &User, to: &User) -> (String, String) {
    let exchange = app.draft(from).await;
    let sent = app
        .send(from, &exchange, fence_job(Uuid::new_v4(), Uuid::new_v4()))
        .await
        .ok();
    claim(app, to, sent["invitation_token"].as_str().unwrap()).await;
    let revision = sent["exchange"]["open_revision"]["id"].as_str().unwrap();
    (exchange, revision.to_owned())
}

fn closed_as(view: &Value) -> (&str, &str, &str) {
    (
        view["state"].as_str().unwrap_or(""),
        view["closed_outcome"].as_str().unwrap_or(""),
        view["closed_reason"].as_str().unwrap_or(""),
    )
}

/// Stands in for an email provider: keeps what it was asked to send.
#[derive(Default)]
struct Provider(Mutex<Vec<Email>>);

impl EmailSender for Provider {
    fn send<'a>(&'a self, email: &'a Email) -> SendFuture<'a> {
        Box::pin(async move {
            self.0.lock().unwrap().push(email.clone());
            Ok(())
        })
    }
}

/// Delivers everything queued, and returns the addresses it went to.
async fn delivered_to(app: &App) -> Vec<String> {
    let provider = Arc::new(Provider::default());
    let delivery = Delivery {
        sender: provider.clone(),
        wording: Wording::embedded().unwrap(),
        web_origin: WEB_ORIGIN.to_owned(),
        // Everything waiting, including what earlier tests left behind.
        rules: DeliveryRules {
            batch: 10_000,
            ..DeliveryRules::default()
        },
    };
    deliver_due(
        &app.db,
        &delivery,
        OffsetDateTime::now_utc() + Duration::seconds(5),
    )
    .await
    .unwrap();
    let sent = provider.0.lock().unwrap();
    sent.iter().map(|email| email.to.clone()).collect()
}

// ---- Proof ------------------------------------------------------------------

#[tokio::test]
async fn deleting_takes_a_code_sent_for_deleting_and_nothing_less() {
    let test = start().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let still_there = || async { app.get(&ana, "/v1/me").await.ok()["email"] == ana.email };

    // A session alone is not enough, and neither is a guess.
    test.delete_with(&ana, "EMAIL", "")
        .await
        .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
    let code = test.deletion_code(&ana, "EMAIL", &ana.email).await;
    let wrong = if code == "000000" { "000001" } else { "000000" };
    test.delete_with(&ana, "EMAIL", wrong)
        .await
        .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
    assert!(still_there().await);

    // A code sent for signing in does not delete: the message it came in
    // said it was for signing in.
    let requested = app
        .call(
            None,
            Method::POST,
            "/v1/auth/codes",
            Some(json!({ "identifier": ana.email })),
            &[],
        )
        .await;
    done(&requested);
    let (sign_in_code, purpose) = test.codes.last(&ana.email);
    assert_eq!(purpose, Purpose::SignIn);
    test.delete_with(&ana, "EMAIL", &sign_in_code)
        .await
        .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
    assert!(still_there().await);

    // And a code sent for deleting does not sign anyone in.
    let code = test.deletion_code(&ana, "EMAIL", &ana.email).await;
    app.call(
        None,
        Method::POST,
        "/v1/auth/sessions",
        Some(json!({ "identifier": ana.email, "code": code, "delivery": "TOKEN" })),
        &[],
    )
    .await
    .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");

    // There is nothing to send to a phone the account lacks.
    for path in ["/v1/me/deletion/codes", "/v1/me/deletion"] {
        app.post(&ana, path, json!({ "channel": "PHONE", "code": code }))
            .await
            .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
    }
    assert!(still_there().await);

    // Nobody signed in can ask, or delete.
    for (method, path) in [
        (Method::GET, "/v1/me/deletion"),
        (Method::POST, "/v1/me/deletion/codes"),
        (Method::POST, "/v1/me/deletion"),
    ] {
        let body = (method == Method::POST).then(|| json!({ "channel": "EMAIL", "code": code }));
        app.call(None, method, path, body, &[])
            .await
            .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    }

    // The code goes to the account's own address, whatever the request
    // says: it cannot be sent anywhere else.
    let elsewhere = format!("{}@example.test", Uuid::new_v4().simple());
    let asked = app
        .post(
            &ana,
            "/v1/me/deletion/codes",
            json!({ "channel": "EMAIL", "identifier": elsewhere }),
        )
        .await;
    done(&asked);
    assert!(
        test.codes
            .0
            .lock()
            .unwrap()
            .iter()
            .all(|(to, ..)| *to != elsewhere)
    );
    let (code, purpose) = test.codes.last(&ana.email);
    assert_eq!(purpose, Purpose::DeleteAccount);

    // Used as it was meant, the code deletes the account.
    done(&test.delete_with(&ana, "EMAIL", &code).await);
    app.get(&ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

// ---- Proof, under a flood ---------------------------------------------------

/// Where a stranger who knows Ana's address works from.
const STRANGER: &str = "203.0.113.66";

/// With requests read as coming through one proxy, which names the requester
/// in `X-Forwarded-For`.
async fn start_behind_a_proxy() -> Test {
    let turn = TURN.lock().await;
    let codes = Arc::new(Codes::default());
    let app = App::start_behind(
        DATABASE,
        Rules::default(),
        codes.clone(),
        TrustedProxies::behind(HeaderName::from_static("x-forwarded-for"), 1),
    )
    .await;
    Test {
        app,
        codes,
        _turn: turn,
    }
}

impl Test {
    /// A request from `address`, signed in as `user` or as nobody.
    async fn post_from(
        &self,
        address: &str,
        user: Option<&User>,
        path: &str,
        body: Value,
    ) -> Reply {
        self.app
            .call(
                user,
                Method::POST,
                path,
                Some(body),
                &[("x-forwarded-for", address)],
            )
            .await
    }

    /// A six-digit code that was never sent to `identifier`, for guessing.
    fn never_sent(&self, identifier: &str) -> String {
        let sent = self.codes.0.lock().unwrap();
        (0..)
            .map(|n| format!("{n:06}"))
            .find(|candidate| {
                !sent
                    .iter()
                    .any(|(to, code, _)| to == identifier && code == candidate)
            })
            .unwrap()
    }
}

/// The guarantee: someone without the account's session cannot keep its
/// owner from getting a deletion code or from using one, whatever they do to
/// the identifier's sign-in and from wherever, even the owner's own address.
#[tokio::test]
async fn nobody_without_the_session_can_keep_its_owner_from_deleting() {
    let test = start_behind_a_proxy().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let to_ana = json!({ "identifier": ana.email });
    let email = json!({ "channel": "EMAIL" });

    // Ana asks for a deletion code, from the address the stranger uses.
    done(
        &test
            .post_from(STRANGER, Some(&ana), "/v1/me/deletion/codes", email.clone())
            .await,
    );
    let (kept, purpose) = test.codes.last(&ana.email);
    assert_eq!(purpose, Purpose::DeleteAccount);

    // The stranger asks for sign-in codes for her address and guesses at
    // them until its sign-in limit is used up: twenty wrong guesses for the
    // day. From then on no sign-in code is sent to it, since none could work.
    for _ in 0..4 {
        done(
            &test
                .post_from(STRANGER, None, "/v1/auth/codes", to_ana.clone())
                .await,
        );
        for _ in 0..5 {
            let guess = json!({
                "identifier": ana.email, "code": test.never_sent(&ana.email), "delivery": "TOKEN",
            });
            test.post_from(STRANGER, None, "/v1/auth/sessions", guess)
                .await
                .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
        }
    }
    let guess = json!({
        "identifier": ana.email, "code": test.never_sent(&ana.email), "delivery": "TOKEN",
    });
    test.post_from(STRANGER, None, "/v1/auth/sessions", guess)
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_GUESSES");
    test.post_from(STRANGER, None, "/v1/auth/codes", to_ana.clone())
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_GUESSES");

    // It worked against signing in: from anywhere, Ana can neither get a
    // sign-in code nor use the one she was sent.
    let (sign_in_code, purpose) = test.codes.last(&ana.email);
    assert_eq!(purpose, Purpose::SignIn);
    let own = "198.51.100.40";
    test.post_from(own, None, "/v1/auth/codes", to_ana.clone())
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_GUESSES");
    let session = json!({ "identifier": ana.email, "code": sign_in_code, "delivery": "TOKEN" });
    test.post_from(own, None, "/v1/auth/sessions", session)
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_GUESSES");

    // The deletion endpoints need her session, which the stranger lacks.
    let wrong = json!({ "channel": "EMAIL", "code": test.never_sent(&ana.email) });
    for (path, body) in [
        ("/v1/me/deletion/codes", email.clone()),
        ("/v1/me/deletion", wrong),
    ] {
        test.post_from(STRANGER, None, path, body)
            .await
            .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    }

    // But she can still be sent a deletion code, from that same address...
    done(
        &test
            .post_from(STRANGER, Some(&ana), "/v1/me/deletion/codes", email)
            .await,
    );
    assert_eq!(test.codes.last(&ana.email).1, Purpose::DeleteAccount);
    // ...and the one she asked for before all this still deletes.
    let delete = json!({ "channel": "EMAIL", "code": kept });
    done(
        &test
            .post_from(STRANGER, Some(&ana), "/v1/me/deletion", delete)
            .await,
    );
    app.get(&ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

#[tokio::test]
async fn deletion_codes_are_counted_against_the_account_and_kept_live_like_sign_in_codes() {
    let test = start_behind_a_proxy().await;
    let app = &test.app;
    let (ana, ben) = (app.user("Ana").await, app.user("Ben").await);

    // Asking again leaves the newest three live; the oldest of four is dead,
    // unless by chance it is also one of the three.
    let mut codes = Vec::new();
    for _ in 0..4 {
        codes.push(test.deletion_code(&ana, "EMAIL", &ana.email).await);
    }
    if !codes[1..].contains(&codes[0]) {
        test.delete_with(&ana, "EMAIL", &codes[0])
            .await
            .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
    }

    // Five an hour for the account, from wherever it asks.
    codes.push(test.deletion_code(&ana, "EMAIL", &ana.email).await);
    test.post_from(
        "198.51.100.41",
        Some(&ana),
        "/v1/me/deletion/codes",
        json!({ "channel": "EMAIL" }),
    )
    .await
    .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS");

    // That is the account's own count: signing in to the same address, and
    // another account's deletion, are untouched by it.
    done(
        &test
            .post_from(
                "198.51.100.42",
                None,
                "/v1/auth/codes",
                json!({ "identifier": ana.email }),
            )
            .await,
    );
    test.deletion_code(&ben, "EMAIL", &ben.email).await;

    // The oldest code still live deletes, sign-in code or no.
    done(&test.delete_with(&ana, "EMAIL", &codes[2]).await);
}

// ---- The account and its working data ---------------------------------------

#[tokio::test]
async fn the_account_ends_everywhere_and_its_identifiers_are_free_for_a_new_one() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let (ana, ben) = (&deal.ana, &deal.ben);
    let phone = format!("+1555{:07}", Uuid::new_v4().as_u128() % 10_000_000);
    sqlx::query("UPDATE account SET phone = $2, language = 'es' WHERE id = $1")
        .bind(ana.id)
        .bind(&phone)
        .execute(&app.owner)
        .await
        .unwrap();

    // Signed in on three devices: the harness's token, a second token, and
    // a browser's cookie.
    let (second_token, _) = test.sign_in(&ana.email).await;
    done(
        &app.call(
            None,
            Method::POST,
            "/v1/auth/codes",
            Some(json!({ "identifier": ana.email })),
            &[],
        )
        .await,
    );
    let cookie_session = app
        .call(
            None,
            Method::POST,
            "/v1/auth/sessions",
            Some(json!({
                "identifier": ana.email,
                "code": test.codes.last(&ana.email).0,
                "delivery": "COOKIE",
            })),
            &[("origin", WEB_ORIGIN)],
        )
        .await;
    assert_eq!(cookie_session.status, StatusCode::OK);
    let cookie = cookie_session.headers[SET_COOKIE].to_str().unwrap();
    let cookie = cookie.split(';').next().unwrap().to_owned();

    // Working data: a draft with a working copy, an idempotency key, a block,
    // a code asked for and not used, and a message waiting to be sent.
    let draft = app.draft(ana).await;
    let saved = app
        .call(
            Some(ana),
            Method::PUT,
            &format!("/v1/exchanges/{draft}/draft"),
            Some(json!({ "body": { "terms": "A fence, some day" } })),
            &[],
        )
        .await;
    done(&saved);
    let version = app.view(ana, &deal.exchange).await["version"].clone();
    app.call(
        Some(ana),
        Method::POST,
        &format!("/v1/exchanges/{}/commands", deal.exchange),
        Some(json!({
            "expected_version": version,
            "command": { "type": "CONTRIBUTION", "contribution": deal.repair, "action": "CLAIM" },
        })),
        &[("idempotency-key", "ana-claims-once")],
    )
    .await
    .ok();
    for (blocker, exchange) in [(ana, &deal.exchange), (ben, &deal.exchange)] {
        let blocked = app
            .call(
                Some(blocker),
                Method::PUT,
                &format!("/v1/exchanges/{exchange}/block"),
                None,
                &[],
            )
            .await;
        done(&blocked);
    }
    done(
        &app.call(
            None,
            Method::POST,
            "/v1/auth/codes",
            Some(json!({ "identifier": phone })),
            &[],
        )
        .await,
    );
    assert!(!notices(app, ana, &deal.exchange).await.is_empty());
    let (created_at, adult_confirmed_at): (OffsetDateTime, Option<OffsetDateTime>) =
        sqlx::query_as("SELECT created_at, adult_confirmed_at FROM account WHERE id = $1")
            .bind(ana.id)
            .fetch_one(&app.owner)
            .await
            .unwrap();

    // What she is told beforehand.
    assert_eq!(
        app.get(ana, "/v1/me/deletion").await.ok(),
        json!({ "drafts": 1, "open_proposals": 0, "agreements_in_force": 1 })
    );

    // Deleted from the browser, with a code sent to the phone.
    let code = test.deletion_code(ana, "PHONE", &phone).await;
    let body = json!({ "channel": "PHONE", "code": code });
    let cookie_only = [("cookie", cookie.as_str())];
    app.call(
        None,
        Method::POST,
        "/v1/me/deletion",
        Some(body.clone()),
        &cookie_only,
    )
    .await
    .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    let deleted = app
        .call(
            None,
            Method::POST,
            "/v1/me/deletion",
            Some(body),
            &[("cookie", cookie.as_str()), ("origin", WEB_ORIGIN)],
        )
        .await;
    done(&deleted);
    let cleared = deleted.headers[SET_COOKIE].to_str().unwrap();
    assert!(
        cleared.starts_with("exchange_session=;") && cleared.contains("Max-Age=0"),
        "{cleared}"
    );

    // Every session is over, on every device.
    app.get(ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    test.get_with_token(&second_token, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    app.call(None, Method::GET, "/v1/me", None, &cookie_only)
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    app.get(ana, &format!("/v1/exchanges/{}", deal.exchange))
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    // The row stays, with nothing on it that says who it was.
    type Row = (
        String,
        Option<String>,
        Option<String>,
        String,
        String,
        OffsetDateTime,
        Option<OffsetDateTime>,
    );
    let row: Row = sqlx::query_as(
        "SELECT status, email, phone, display_name, language, created_at, adult_confirmed_at
         FROM account WHERE id = $1",
    )
    .bind(ana.id)
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!(
        row,
        (
            "DELETED".to_owned(),
            None,
            None,
            String::new(),
            "en".to_owned(),
            created_at,
            adult_confirmed_at
        )
    );

    // Its working data is gone.
    for (what, query) in [
        (
            "sessions",
            "SELECT count(*) FROM account_session WHERE account_id = $1",
        ),
        (
            "working copies",
            "SELECT count(*) FROM exchange_draft WHERE account_id = $1",
        ),
        (
            "idempotency keys",
            "SELECT count(*) FROM idempotency_key WHERE account_id = $1",
        ),
        (
            "queued messages",
            "SELECT count(*) FROM outbox WHERE recipient_account_id = $1",
        ),
        (
            "blocks made",
            "SELECT count(*) FROM account_block WHERE blocker_account_id = $1",
        ),
    ] {
        assert_eq!(count(app, query, ana.id).await, 0, "{what}");
    }
    let codes_left: i64 =
        sqlx::query_scalar("SELECT count(*) FROM one_time_code WHERE identifier = ANY($1)")
            .bind(vec![ana.email.clone(), phone.clone()])
            .fetch_one(&app.owner)
            .await
            .unwrap();
    assert_eq!(codes_left, 0);
    // The draft was never anyone else's: nothing of hers is left on it.
    let (state, name, alias): (String, String, String) = sqlx::query_as(
        "SELECT e.state, p.display_name, p.alias
         FROM exchange e JOIN participant p ON p.exchange_id = e.id AND p.slot = 'A'
         WHERE e.id = $1",
    )
    .bind(id(&draft))
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!(
        (state.as_str(), name.as_str(), alias.as_str()),
        ("DRAFT", "", "")
    );
    assert!(events(app, &draft).await.is_empty());
    // Ben's block on her is his, and stays his to remove.
    assert_eq!(
        count(
            app,
            "SELECT count(*) FROM account_block WHERE blocked_account_id = $1",
            ana.id
        )
        .await,
        1
    );
    let listed = app.get(ben, "/v1/blocks").await.ok();
    assert_eq!(listed[0]["name"], "Ana Ruiz");

    // The same address and number sign up again as someone new, who sees
    // nothing of what the old account was in.
    let (token, account) = test.sign_in(&ana.email).await;
    assert_ne!(account["id"], json!(ana.id));
    assert_eq!(
        (&account["display_name"], &account["adult_confirmed"]),
        (&json!(""), &json!(false))
    );
    assert_eq!(account["phone"], Value::Null);
    assert_eq!(
        test.get_with_token(&token, "/v1/exchanges").await.ok(),
        json!([])
    );
    for exchange in [&deal.exchange, &draft] {
        for path in ["", "/history", "/record"] {
            test.get_with_token(&token, &format!("/v1/exchanges/{exchange}{path}"))
                .await
                .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
        }
    }
    let (_, by_phone) = test.sign_in(&phone).await;
    assert_ne!(by_phone["id"], json!(ana.id));
    assert_ne!(by_phone["id"], account["id"]);

    // The old sessions stay dead now that the address is in use again.
    app.get(ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
}

// ---- Exchanges: negotiations ------------------------------------------------

#[tokio::test]
async fn an_invitation_someone_else_bound_to_the_address_forgets_it_and_dies() {
    let test = start().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let ben = app.user("Ben").await;
    let exchange = app.draft(&ana).await;
    let sent = app
        .post(
            &ana,
            &format!("/v1/exchanges/{exchange}/revisions"),
            json!({
                "expected_version": 0,
                "terms": fence_job(Uuid::new_v4(), Uuid::new_v4()),
                "consent": consent(),
                "invitation": { "bound_to": ben.email },
            }),
        )
        .await
        .ok();
    let token = sent["invitation_token"].as_str().unwrap().to_owned();

    test.delete(&ben).await;

    let (bound, revoked): (Option<String>, bool) = sqlx::query_as(
        "SELECT bound_email, revoked_at IS NOT NULL FROM invitation WHERE exchange_id = $1",
    )
    .bind(exchange.parse::<Uuid>().unwrap())
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!((bound, revoked), (None, true));
    // A dead link like any other; Ana can issue a new one.
    app.call(
        None,
        Method::POST,
        "/v1/invitations/preview",
        Some(json!({ "token": token })),
        &[],
    )
    .await
    .refused(StatusCode::NOT_FOUND, "INVITATION_UNAVAILABLE");
    app.post(
        &ana,
        &format!("/v1/exchanges/{exchange}/invitation"),
        json!({}),
    )
    .await
    .ok();
}

#[tokio::test]
async fn an_offer_the_departing_party_sent_is_withdrawn() {
    let test = start().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let ben = app.user("Ben").await;
    let cal = app.user("Cal").await;

    // One offer Ben has opened, one that names Cal and that nobody has opened,
    // and a counteroffer of Ana's to an offer Cal sent her.
    let (opened, _) = negotiation(app, &ana, &ben).await;
    let unopened = app.draft(&ana).await;
    let sent = app
        .post(
            &ana,
            &format!("/v1/exchanges/{unopened}/revisions"),
            json!({
                "expected_version": 0,
                "terms": fence_job(Uuid::new_v4(), Uuid::new_v4()),
                "consent": consent(),
                "invitation": { "bound_to": cal.email },
            }),
        )
        .await
        .ok();
    let token = sent["invitation_token"].as_str().unwrap();
    let (countered, _) = negotiation(app, &cal, &ana).await;
    // Cal has confirmed that Ana is who he invited, so she can counter.
    app.command(&cal, &countered, json!({ "type": "CONFIRM_COUNTERPARTY" }))
        .await
        .ok();
    app.send(&ana, &countered, fence_job(Uuid::new_v4(), Uuid::new_v4()))
        .await
        .ok();
    assert_eq!(
        app.get(&ana, "/v1/me/deletion").await.ok(),
        json!({ "drafts": 0, "open_proposals": 3, "agreements_in_force": 0 })
    );

    test.leave(&ana, &[&opened, &unopened, &countered]).await;

    // Each ended as a withdrawal in her name, like any other.
    for (exchange, slot) in [(&opened, "A"), (&unopened, "A"), (&countered, "B")] {
        let recorded = events(app, exchange).await;
        assert_eq!(
            recorded[recorded.len() - 2..],
            [
                event("REVISION_WITHDRAWN", slot),
                event("EXCHANGE_CLOSED", slot)
            ],
            "{exchange}"
        );
    }
    let view = app.view(&ben, &opened).await;
    assert_eq!(closed_as(&view), ("CLOSED", "NOT_AGREED", "WITHDRAWN"));
    assert_eq!(
        closed_as(&app.view(&cal, &countered).await),
        ("CLOSED", "NOT_AGREED", "WITHDRAWN")
    );
    // The other party is told what any withdrawal tells them, and no more.
    assert_eq!(view["other_party_left"], false);
    assert_eq!(
        notices(app, &ben, &opened).await.last().unwrap(),
        "CLOSED_WITHDRAWN"
    );
    assert_eq!(
        notices(app, &cal, &countered).await.last().unwrap(),
        "CLOSED_WITHDRAWN"
    );
    // Ben still has the exchange, named as the proposal named them.
    let listed = app.get(&ben, "/v1/exchanges").await.ok();
    assert_eq!(listed[0]["other_party_name"], "Ana Ruiz");
    let record = app
        .get(&ben, &format!("/v1/exchanges/{opened}/record"))
        .await
        .ok();
    assert!(record.to_string().contains("Ana Ruiz"));

    // The link nobody opened is dead, and no longer says whom it was for.
    app.call(
        None,
        Method::POST,
        "/v1/invitations/preview",
        Some(json!({ "token": token })),
        &[],
    )
    .await
    .refused(StatusCode::NOT_FOUND, "INVITATION_UNAVAILABLE");
    app.post(&cal, "/v1/invitations/claim", json!({ "token": token }))
        .await
        .refused(StatusCode::NOT_FOUND, "INVITATION_UNAVAILABLE");
    let (revoked, bound): (bool, Option<String>) = sqlx::query_as(
        "SELECT revoked_at IS NOT NULL, bound_email FROM invitation WHERE exchange_id = $1",
    )
    .bind(id(&unopened))
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!((revoked, bound), (true, None));
}

#[tokio::test]
async fn an_offer_sent_to_the_departing_party_is_declined() {
    let test = start().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let ben = app.user("Ben").await;
    let cal = app.user("Cal").await;

    // An offer of Ana's that Ben has opened and not signed, and a
    // counteroffer of Cal's to an offer Ben sent. Each initiator has
    // confirmed who opened their link: only then can it be declined or
    // countered.
    let (offered, _) = negotiation(app, &ana, &ben).await;
    let (countered, _) = negotiation(app, &ben, &cal).await;
    for (initiator, exchange) in [(&ana, &offered), (&ben, &countered)] {
        app.command(
            initiator,
            exchange,
            json!({ "type": "CONFIRM_COUNTERPARTY" }),
        )
        .await
        .ok();
    }
    app.send(&cal, &countered, fence_job(Uuid::new_v4(), Uuid::new_v4()))
        .await
        .ok();

    test.leave(&ben, &[&offered, &countered]).await;

    for (exchange, slot, other) in [(&offered, "B", &ana), (&countered, "A", &cal)] {
        let recorded = events(app, exchange).await;
        assert_eq!(
            recorded[recorded.len() - 2..],
            [
                event("REVISION_DECLINED", slot),
                event("EXCHANGE_CLOSED", slot)
            ],
            "{exchange}"
        );
        let view = app.view(other, exchange).await;
        assert_eq!(closed_as(&view), ("CLOSED", "NOT_AGREED", "DECLINED"));
        assert_eq!(view["other_party_left"], false);
        assert_eq!(
            notices(app, other, exchange).await.last().unwrap(),
            "CLOSED_DECLINED"
        );
    }
}

#[tokio::test]
async fn someone_the_initiator_had_not_confirmed_leaves_the_exchange_as_they_go() {
    let test = start().await;
    let app = &test.app;
    let ana = app.user("Ana").await;
    let ben = app.user("Ben").await;
    let cal = app.user("Cal").await;

    // Ben has opened Ana's link and signed her offer; it waits for her to
    // confirm who he is. Cal has opened another of hers and not signed.
    let (signed, revision) = negotiation(app, &ana, &ben).await;
    app.command(&ben, &signed, accept(&revision)).await.ok();
    assert_eq!(
        app.view(&ana, &signed).await["claimant"]["display_name"],
        "Ben"
    );
    let (unsigned, _) = negotiation(app, &ana, &cal).await;

    // Neither can decline, and no command takes a signature back. Deleting
    // the account takes them out of the exchange instead, in their own name.
    test.leave(&ben, &[&signed]).await;
    test.leave(&cal, &[&unsigned]).await;

    for exchange in [&signed, &unsigned] {
        let recorded = events(app, exchange).await;
        assert_eq!(
            recorded.last().unwrap(),
            &event("COUNTERPARTY_RELEASED", "B"),
            "{exchange}"
        );

        // Ana's offer stays open, signed by her alone. Nobody is in the
        // other place: not someone to confirm, and not someone who has left
        // an exchange they are still in.
        let view = app.view(&ana, exchange).await;
        assert_eq!(
            (&view["state"], &view["counterparty"]),
            (&json!("NEGOTIATING"), &json!("UNCLAIMED"))
        );
        assert_eq!(view["open_revision"]["accepted_by"], json!(["A"]));
        assert_eq!(view["claimant"], Value::Null);
        assert_eq!(view["other_party_left"], false);
        assert_eq!(view["invitation_open"], false);
        app.command(&ana, exchange, json!({ "type": "CONFIRM_COUNTERPARTY" }))
            .await
            .refused(StatusCode::CONFLICT, "ACTION_NOT_ALLOWED");
        // She is told what she would be told of anyone leaving.
        assert_eq!(
            notices(app, &ana, exchange).await.last().unwrap(),
            "CLAIMANT_LEFT"
        );
    }
    for gone in [&ben, &cal] {
        assert_eq!(
            count(
                app,
                "SELECT count(*) FROM outbox WHERE recipient_account_id = $1",
                gone.id
            )
            .await,
            0
        );
    }

    // She is not stuck. She can invite someone else, who signs in their own
    // right: the signature Ben left is nobody's.
    let dee = app.user("Dee").await;
    let token = app
        .post(
            &ana,
            &format!("/v1/exchanges/{signed}/invitation"),
            json!({}),
        )
        .await
        .ok()["invitation_token"]
        .as_str()
        .unwrap()
        .to_owned();
    claim(app, &dee, &token).await;
    let view = app
        .command(&ana, &signed, json!({ "type": "CONFIRM_COUNTERPARTY" }))
        .await
        .ok();
    assert_eq!(view["state"], "NEGOTIATING");
    let view = app.command(&dee, &signed, accept(&revision)).await.ok();
    assert_eq!(view["state"], "ACTIVE");

    // And had she done nothing, the offer would have run out by itself.
    let later = OffsetDateTime::now_utc() + Duration::days(15);
    run_timers(&app.db, &app.rules, later).await.unwrap();
    assert_eq!(
        closed_as(&app.view(&ana, &unsigned).await),
        ("CLOSED", "NOT_AGREED", "EXPIRED")
    );
}

// ---- Exchanges: agreements in force -----------------------------------------

#[tokio::test]
async fn an_agreement_in_force_is_asked_to_close_and_closes_unresolved() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let (ana, ben) = (&deal.ana, &deal.ben);
    // Ana says the fence is repaired. Ben has not confirmed it, and owes the payment.
    app.act(ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();
    let agreement = app.view(ana, &deal.exchange).await["in_force_revision"].clone();
    assert_eq!(
        app.get(ben, "/v1/me/deletion").await.ok(),
        json!({ "drafts": 0, "open_proposals": 0, "agreements_in_force": 1 })
    );

    // Ben leaves with both outstanding. That is not refused.
    test.leave(ben, &[&deal.exchange]).await;

    // The agreement is still in force and still says what it said, with a
    // request to close in Ben's name. Nothing was waived or accepted for him.
    let view = app.view(ana, &deal.exchange).await;
    assert_eq!(view["state"], "ACTIVE");
    assert_eq!(view["in_force_revision"], agreement);
    assert_eq!(view["close_requested_by"], "B");
    assert_eq!(view["contributions"][0]["status"], "CLAIMED");
    assert_eq!(view["contributions"][1]["status"], "PENDING");
    assert_eq!(
        events(app, &deal.exchange).await.last().unwrap(),
        &event("CLOSE_REQUESTED", "B")
    );
    // Ana is told that he asked to close, and, on the exchange, that he has gone.
    assert_eq!(
        notices(app, ana, &deal.exchange).await.last().unwrap(),
        "CLOSE_REQUESTED"
    );
    assert_eq!(view["other_party_left"], true);
    let listed = app.get(ana, "/v1/exchanges").await.ok();
    assert_eq!(listed[0]["other_party_name"], "Ben Ortiz");

    // Everything the window allows is still open to her: her statement, and
    // releasing what she is owed.
    app.command(
        ana,
        &deal.exchange,
        json!({ "type": "ADD_STATEMENT", "note": "The fence was repaired on the 3rd." }),
    )
    .await
    .ok();
    app.act(ana, &deal.exchange, deal.payment, "WAIVE")
        .await
        .ok();
    // What only Ben could do stays undone, so it cannot complete.
    assert_eq!(app.view(ana, &deal.exchange).await["state"], "ACTIVE");

    // Nothing closes early; after the window, the worker closes it as it
    // stands.
    let at = |days: i64| OffsetDateTime::now_utc() + Duration::days(days);
    run_timers(&app.db, &app.rules, at(6)).await.unwrap();
    assert_eq!(app.view(ana, &deal.exchange).await["state"], "ACTIVE");
    run_timers(&app.db, &app.rules, at(8)).await.unwrap();
    let view = app.view(ana, &deal.exchange).await;
    assert_eq!(closed_as(&view), ("CLOSED", "UNRESOLVED", "CLOSE_REQUEST"));
    assert_eq!(view["contributions"][0]["status"], "CLAIMED");
    assert_eq!(view["contributions"][1]["status"], "WAIVED");
    assert_eq!(view["other_party_left"], false);
    assert_eq!(
        notices(app, ana, &deal.exchange).await.last().unwrap(),
        "CLOSED_UNRESOLVED"
    );

    // Ana keeps her record, with both names as the agreement wrote them and
    // Ben's signature on it.
    let record = app
        .get(ana, &format!("/v1/exchanges/{}/record", deal.exchange))
        .await
        .ok()
        .to_string();
    assert!(record.contains("Ben Ortiz") && record.contains("Ana Ruiz"));
    assert!(!record.contains(&ben.email));
    let signatures = count(
        app,
        "SELECT count(*) FROM acceptance WHERE account_id = $1",
        ben.id,
    )
    .await;
    assert_eq!(signatures, 1);

    // Nothing was queued for Ben, by his leaving or by the worker since, and
    // nothing is sent to the address he had.
    assert_eq!(
        count(
            app,
            "SELECT count(*) FROM outbox WHERE recipient_account_id = $1",
            ben.id
        )
        .await,
        0
    );
    let sent_to = delivered_to(app).await;
    assert!(sent_to.contains(&ana.email));
    assert!(!sent_to.contains(&ben.email));
}

#[tokio::test]
async fn an_amendment_still_waiting_is_ended_before_the_request_to_close() {
    let test = start().await;
    let app = &test.app;

    // An amendment the departing party proposed is withdrawn.
    let deal = app.active().await;
    let agreement = app.view(&deal.ana, &deal.exchange).await["in_force_revision"]["id"].clone();
    app.send(
        &deal.ben,
        &deal.exchange,
        fence_job(deal.repair, deal.payment),
    )
    .await
    .ok();
    test.leave(&deal.ben, &[&deal.exchange]).await;
    let recorded = events(app, &deal.exchange).await;
    assert_eq!(
        recorded[recorded.len() - 2..],
        [
            event("REVISION_WITHDRAWN", "B"),
            event("CLOSE_REQUESTED", "B")
        ]
    );
    let view = app.view(&deal.ana, &deal.exchange).await;
    assert_eq!(view["state"], "ACTIVE");
    assert_eq!(view["open_revision"], Value::Null);
    assert_eq!(view["in_force_revision"]["id"], agreement);
    let told = notices(app, &deal.ana, &deal.exchange).await;
    assert_eq!(
        told[told.len() - 2..],
        ["AMENDMENT_WITHDRAWN", "CLOSE_REQUESTED"]
    );

    // One proposed to them is declined: it cannot be signed for them later.
    let deal = app.active().await;
    app.send(
        &deal.ana,
        &deal.exchange,
        fence_job(deal.repair, deal.payment),
    )
    .await
    .ok();
    test.leave(&deal.ben, &[&deal.exchange]).await;
    let recorded = events(app, &deal.exchange).await;
    assert_eq!(
        recorded[recorded.len() - 2..],
        [
            event("REVISION_DECLINED", "B"),
            event("CLOSE_REQUESTED", "B")
        ]
    );
    let view = app.view(&deal.ana, &deal.exchange).await;
    assert_eq!(view["open_revision"], Value::Null);
    assert_eq!(view["close_requested_by"], "B");
}

#[tokio::test]
async fn a_request_to_close_already_pending_is_not_repeated() {
    let test = start().await;
    let app = &test.app;

    for requester in ["A", "B"] {
        let deal = app.active().await;
        let by = if requester == "A" {
            &deal.ana
        } else {
            &deal.ben
        };
        app.command(by, &deal.exchange, json!({ "type": "REQUEST_CLOSE" }))
            .await
            .ok();
        // Ana had also proposed ending by agreement. Leaving does not accept
        // it for Ben: that would release what he is owed and what he owes.
        app.command(&deal.ana, &deal.exchange, json!({ "type": "PROPOSE_END" }))
            .await
            .ok();
        let recorded = events(app, &deal.exchange).await;

        test.leave(&deal.ben, &[&deal.exchange]).await;

        assert_eq!(events(app, &deal.exchange).await, recorded, "{requester}");
        let view = app.view(&deal.ana, &deal.exchange).await;
        assert_eq!(view["close_requested_by"], requester);
        assert_eq!(view["end_proposed_by"], "A");
        assert_eq!(view["other_party_left"], true);
    }
}

#[tokio::test]
async fn a_closed_exchange_is_left_exactly_as_it_is() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let (ana, ben) = (&deal.ana, &deal.ben);
    app.act(ben, &deal.exchange, deal.repair, "CONFIRM")
        .await
        .ok();
    app.act(ana, &deal.exchange, deal.payment, "CONFIRM")
        .await
        .ok();
    let exchange = format!("/v1/exchanges/{}", deal.exchange);
    let before = (
        app.view(ana, &deal.exchange).await,
        app.get(ana, &format!("{exchange}/history")).await.ok(),
        app.get(ana, &format!("{exchange}/record")).await.ok(),
        events(app, &deal.exchange).await,
        notices(app, ana, &deal.exchange).await,
    );
    assert_eq!(closed_as(&before.0), ("CLOSED", "COMPLETED", ""));
    assert_eq!(
        app.get(ben, "/v1/me/deletion").await.ok(),
        json!({ "drafts": 0, "open_proposals": 0, "agreements_in_force": 0 })
    );

    test.leave(ben, &[&deal.exchange]).await;

    // Ana sees what she saw, is told nothing, and keeps the whole record.
    let mut record = app.get(ana, &format!("{exchange}/record")).await.ok();
    // The copy says when it was made; everything else must match.
    record["generated_at"] = before.2["generated_at"].clone();
    let after = (
        app.view(ana, &deal.exchange).await,
        app.get(ana, &format!("{exchange}/history")).await.ok(),
        record,
        events(app, &deal.exchange).await,
        notices(app, ana, &deal.exchange).await,
    );
    assert_eq!(after, before);
    assert_eq!(after.0["other_party_left"], false);
}

// ---- Repeats and races ------------------------------------------------------

#[tokio::test]
async fn a_repeated_deletion_does_nothing_more() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let ben = &deal.ben;

    let code = test.deletion_code(ben, "EMAIL", &ben.email).await;
    done(&test.delete_with(ben, "EMAIL", &code).await);
    let recorded = events(app, &deal.exchange).await;
    let account: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM account a WHERE id = $1")
            .bind(ben.id)
            .fetch_one(&app.owner)
            .await
            .unwrap();

    // The request again: the session it came with is gone.
    test.delete_with(ben, "EMAIL", &code)
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    // And the deletion itself, run again, finds nothing left to do.
    deletion::delete_account(&app.db, &app.rules, ben.id)
        .await
        .ok()
        .unwrap();

    assert_eq!(events(app, &deal.exchange).await, recorded);
    let after: String =
        sqlx::query_scalar("SELECT row_to_json(a)::text FROM account a WHERE id = $1")
            .bind(ben.id)
            .fetch_one(&app.owner)
            .await
            .unwrap();
    assert_eq!(after, account);
}

#[tokio::test]
async fn deletions_at_the_same_moment_happen_once() {
    let test = start().await;
    let app = &test.app;

    for round in 0..4 {
        let deal = app.active().await;
        let (ana, ben) = (&deal.ana, &deal.ben);
        let (offer, _) = negotiation(app, ana, ben).await;
        let phone = format!("+1555{:07}", Uuid::new_v4().as_u128() % 10_000_000);
        sqlx::query("UPDATE account SET phone = $2 WHERE id = $1")
            .bind(ben.id)
            .bind(&phone)
            .execute(&app.owner)
            .await
            .unwrap();

        // Ben confirms on two devices at once, each with a code of its own,
        // while Ana, who shares both exchanges with him, deletes hers.
        let by_email = test.deletion_code(ben, "EMAIL", &ben.email).await;
        let by_phone = test.deletion_code(ben, "PHONE", &phone).await;
        let anas = test.deletion_code(ana, "EMAIL", &ana.email).await;
        let (first, second, third) = tokio::join!(
            test.delete_with(ben, "EMAIL", &by_email),
            test.delete_with(ben, "PHONE", &by_phone),
            test.delete_with(ana, "EMAIL", &anas),
        );
        // Each of Ben's either did it, or found it done, or found the
        // session or the code gone with the account.
        for reply in [&first, &second] {
            if reply.status != StatusCode::NO_CONTENT {
                assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{}", reply.body);
            }
        }
        assert!(
            first.status == StatusCode::NO_CONTENT || second.status == StatusCode::NO_CONTENT,
            "round {round}"
        );
        done(&third);

        // Whoever went first, each exchange was ended once.
        let recorded = events(app, &deal.exchange).await;
        let requests = recorded
            .iter()
            .filter(|(kind, _)| kind == "CLOSE_REQUESTED")
            .count();
        assert_eq!(requests, 1, "round {round}: {recorded:?}");
        let recorded = events(app, &offer).await;
        let closures = recorded
            .iter()
            .filter(|(kind, _)| kind == "EXCHANGE_CLOSED")
            .count();
        assert_eq!(closures, 1, "round {round}: {recorded:?}");
        for user in [ana, ben] {
            let status: String = sqlx::query_scalar("SELECT status FROM account WHERE id = $1")
                .bind(user.id)
                .fetch_one(&app.owner)
                .await
                .unwrap();
            assert_eq!(status, "DELETED", "round {round}");
        }
    }
}

#[tokio::test]
async fn what_the_account_was_doing_as_it_was_deleted_does_not_outlive_it() {
    let test = start().await;
    let app = &test.app;

    for round in 0..6 {
        let deal = app.active().await;
        let (ana, ben) = (&deal.ana, &deal.ben);
        let code = test.deletion_code(ben, "EMAIL", &ben.email).await;
        let version = app.view(ben, &deal.exchange).await["version"].clone();
        let amendment = json!({
            "expected_version": version,
            "terms": fence_job(deal.repair, deal.payment),
            "consent": consent(),
        });
        let revisions = format!("/v1/exchanges/{}/revisions", deal.exchange);
        let block = format!("/v1/exchanges/{}/block", deal.exchange);
        let draft = format!("/v1/exchanges/{}/draft", deal.exchange);

        // On another device Ben keeps saving a working copy, one request
        // after another, until he is told he is signed out. One of those
        // requests is under way when the account goes: accepted as his while
        // the account was there, and not yet stored.
        let saving = async {
            for take in 0.. {
                let body = json!({ "body": { "terms": format!("Take {take}") } });
                let reply = app
                    .call(Some(ben), Method::PUT, &draft, Some(body), &[])
                    .await;
                if reply.status == StatusCode::UNAUTHORIZED {
                    break;
                }
                done(&reply);
            }
        };

        // At the same moment he proposes an amendment, and Ana blocks him.
        let (deleted, amended, (), blocked) = tokio::join!(
            test.delete_with(ben, "EMAIL", &code),
            app.post(ben, &revisions, amendment),
            saving,
            app.call(Some(ana), Method::PUT, &block, None, &[]),
        );
        done(&deleted);
        done(&blocked);

        // Either the amendment came first and was ended with the rest, or it
        // came after and was refused. It is never left open for Ana to sign,
        // and the request to close is there either way.
        assert!(
            amended.status == StatusCode::OK || amended.status.is_client_error(),
            "round {round}: {}",
            amended.body
        );
        let view = app.view(ana, &deal.exchange).await;
        assert_eq!(view["open_revision"], Value::Null, "round {round}");
        assert_eq!(view["close_requested_by"], "B", "round {round}");

        // Whatever was saved in time went with the account, and nothing was
        // saved after it.
        let left = count(
            app,
            "SELECT count(*) FROM exchange_draft WHERE account_id = $1",
            ben.id,
        )
        .await;
        assert_eq!(left, 0, "round {round}");
    }
}

#[tokio::test]
async fn someone_referring_to_the_account_delays_its_deletion_and_deadlocks_nobody() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let ben = &deal.ben;
    let recorded = events(app, &deal.exchange).await;
    let refer = "SELECT 1 FROM account WHERE id = $1 FOR KEY SHARE";

    // Another transaction refers to Ben's account, as storing a block
    // against him does, and stays open. The deletion does not wait on it
    // while holding his exchanges. It gives up, with nothing done at all.
    let mut other = app.owner.begin().await.unwrap();
    sqlx::query(refer)
        .bind(ben.id)
        .execute(&mut *other)
        .await
        .unwrap();
    let code = test.deletion_code(ben, "EMAIL", &ben.email).await;
    test.delete_with(ben, "EMAIL", &code)
        .await
        .refused(StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE");
    assert_eq!(app.get(ben, "/v1/me").await.ok()["email"], ben.email);
    assert_eq!(events(app, &deal.exchange).await, recorded);
    other.rollback().await.unwrap();

    // The same, where the other transaction goes on to want the exchange
    // the deletion has locked, which is what a block does next. Waiting for
    // each other here would be a deadlock; instead the deletion steps back,
    // the other finishes, and the deletion then goes through.
    let mut other = app.owner.begin().await.unwrap();
    sqlx::query(refer)
        .bind(ben.id)
        .execute(&mut *other)
        .await
        .unwrap();
    let code = test.deletion_code(ben, "EMAIL", &ben.email).await;
    let finishing = async {
        tokio::time::sleep(std::time::Duration::from_millis(60)).await;
        sqlx::query("SELECT 1 FROM exchange WHERE id = $1 FOR UPDATE")
            .bind(id(&deal.exchange))
            .execute(&mut *other)
            .await
            .unwrap();
        other.commit().await.unwrap();
    };
    let (deleted, ()) = tokio::join!(test.delete_with(ben, "EMAIL", &code), finishing);
    done(&deleted);
    let after = events(app, &deal.exchange).await;
    assert_eq!(after.len(), recorded.len() + 1);
    assert_eq!(after.last().unwrap(), &event("CLOSE_REQUESTED", "B"));
}

#[tokio::test]
async fn a_deletion_that_found_the_account_busy_leaves_the_code_for_another_try() {
    let test = start().await;
    let app = &test.app;
    let deal = app.active().await;
    let ben = &deal.ben;
    let recorded = events(app, &deal.exchange).await;
    let failed_guesses = "SELECT coalesce(sum(failed_attempts), 0)::bigint FROM one_time_code
                          WHERE identifier = (SELECT email FROM account WHERE id = $1)";

    // Another transaction refers to Ben's account and stays open, so the
    // deletion keeps finding it busy and gives up.
    let mut other = app.owner.begin().await.unwrap();
    sqlx::query("SELECT 1 FROM account WHERE id = $1 FOR KEY SHARE")
        .bind(ben.id)
        .execute(&mut *other)
        .await
        .unwrap();
    let code = test.deletion_code(ben, "EMAIL", &ben.email).await;
    test.delete_with(ben, "EMAIL", &code)
        .await
        .refused(StatusCode::SERVICE_UNAVAILABLE, "SERVICE_UNAVAILABLE");
    assert_eq!(app.get(ben, "/v1/me").await.ok()["email"], ben.email);
    assert_eq!(events(app, &deal.exchange).await, recorded);
    assert_eq!(count(app, failed_guesses, ben.id).await, 0);

    // A wrong code meanwhile is refused at once and charged, busy or not, so
    // trying again is no way around the limits on guessing.
    let wrong = if code == "000000" { "000001" } else { "000000" };
    test.delete_with(ben, "EMAIL", wrong)
        .await
        .refused(StatusCode::UNAUTHORIZED, "INVALID_CODE");
    assert_eq!(count(app, failed_guesses, ben.id).await, 1);
    other.rollback().await.unwrap();

    // Once the account is free, the same code deletes it, without a new one.
    done(&test.delete_with(ben, "EMAIL", &code).await);
    let status: String = sqlx::query_scalar("SELECT status FROM account WHERE id = $1")
        .bind(ben.id)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(status, "DELETED");
    let after = events(app, &deal.exchange).await;
    assert_eq!(after.len(), recorded.len() + 1);
    assert_eq!(after.last().unwrap(), &event("CLOSE_REQUESTED", "B"));
}
