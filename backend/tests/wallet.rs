//! Wallet passes through the API (DESIGN.md §11): who may have one, Apple's
//! pass web service from a device's side, the Google save link, updates
//! reaching devices through the worker, and revocation when an account is
//! deleted. With throwaway credentials made by the test run
//! (`src/wallet/testkit.rs`); nothing reaches Apple or Google: the worker's
//! senders here record what they were given.

mod common;
#[path = "../src/wallet/testkit.rs"]
mod testkit;

use std::collections::HashMap;
use std::io::{Cursor, Read};
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use common::{App, Deal, User};
use ring::signature::{RSA_PKCS1_2048_8192_SHA256, UnparsedPublicKey};
use serde_json::{Value, json};
use time::OffsetDateTime;
use uuid::Uuid;
use yuppers_backend::domain::Rules;
use yuppers_backend::wallet::apple::push::{PassPush, PushFuture, PushOutcome};
use yuppers_backend::wallet::config::WalletConfig;
use yuppers_backend::wallet::delivery::{UpdateRules, WalletDelivery, deliver_due};
use yuppers_backend::wallet::google::objects::{PatchFuture, PatchOutcome, WalletObjects};
use yuppers_backend::wallet::{Wallet, store};

const DB: &str = "yuppers_test_wallet";
const SERVICE: &str = "/v1/wallet/apple";

/// A wallet with the platforms whose settings `keep` names (all, without).
fn wallet(keep: Option<&[&str]>) -> Arc<Wallet> {
    let settings: HashMap<&str, String> = testkit::settings()
        .into_iter()
        .filter(|(name, _)| {
            keep.is_none_or(|keep| {
                *name == "WALLET_DELIVERY" || keep.iter().any(|k| name.starts_with(k))
            })
        })
        .collect();
    let config = WalletConfig::from_lookup(&|name| settings.get(name).cloned()).unwrap();
    Arc::new(
        Wallet::new(
            &config,
            "https://app.test",
            Some(b"test-secret-test-secret-test-secret"),
        )
        .unwrap(),
    )
}

async fn app() -> App {
    App::start_with_wallet(DB, wallet(None)).await
}

async fn wallet_post(app: &App, user: &User, exchange: &str, platform: &str) -> common::Reply {
    app.call(
        Some(user),
        Method::POST,
        &format!("/v1/exchanges/{exchange}/wallet/{platform}"),
        None,
        &[],
    )
    .await
}

/// The files of a `.pkpass`.
fn unzip(bytes: &[u8]) -> HashMap<String, Vec<u8>> {
    let mut archive = zip::ZipArchive::new(Cursor::new(bytes)).expect("a zip archive");
    (0..archive.len())
        .map(|index| {
            let mut file = archive.by_index(index).unwrap();
            let mut contents = Vec::new();
            file.read_to_end(&mut contents).unwrap();
            (file.name().to_owned(), contents)
        })
        .collect()
}

fn pass_json(reply: &common::Reply) -> Value {
    assert_eq!(reply.status, StatusCode::OK, "{}", reply.body);
    assert_eq!(
        reply.headers["content-type"],
        "application/vnd.apple.pkpass"
    );
    serde_json::from_slice(&unzip(&reply.bytes)["pass.json"]).unwrap()
}

/// What the worker's senders were asked to do.
#[derive(Default)]
struct Recorder {
    pushes: Mutex<Vec<String>>,
    patches: Mutex<Vec<(String, Value)>>,
}

impl PassPush for Recorder {
    fn push<'a>(&'a self, push_token: &'a str) -> PushFuture<'a> {
        Box::pin(async move {
            self.pushes.lock().unwrap().push(push_token.to_owned());
            Ok(PushOutcome::Sent)
        })
    }
}

impl WalletObjects for Recorder {
    fn patch<'a>(&'a self, object_id: &'a str, object: &'a Value) -> PatchFuture<'a> {
        Box::pin(async move {
            self.patches
                .lock()
                .unwrap()
                .push((object_id.to_owned(), object.clone()));
            Ok(PatchOutcome::Updated)
        })
    }
}

/// Held for the whole of each test that runs the worker. The worker takes
/// every pass that is due in the database, which the tests share; one test's
/// run would otherwise send another test's updates to its own recorder.
static WORKER: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// Runs the worker's Wallet pass once, with `recorder` as both platforms.
async fn run_worker(app: &App, recorder: &Arc<Recorder>) {
    let delivery = WalletDelivery {
        wallet: wallet(None),
        apple: Some(recorder.clone()),
        google: Some(recorder.clone()),
        rules: UpdateRules::default(),
    };
    // The lease of an earlier run in this test may still hold a pass; the
    // run looks from far enough ahead that it has run out.
    let later = OffsetDateTime::now_utc() + time::Duration::minutes(5);
    deliver_due(&app.db, &Rules::default(), &delivery, later)
        .await
        .unwrap();
}

async fn pass_status(app: &App, serial: &str) -> (String, bool) {
    sqlx::query_as(
        "SELECT update_status, voided_at IS NOT NULL FROM wallet_pass WHERE external_id = $1",
    )
    .bind(serial)
    .fetch_one(&app.owner)
    .await
    .unwrap()
}

/// A request from a device to the pass web service.
async fn device(
    app: &App,
    method: Method,
    path: &str,
    token: Option<&str>,
    body: Option<Value>,
    headers: &[(&'static str, &str)],
) -> common::Reply {
    let authorization = token.map(|token| format!("ApplePass {token}"));
    let mut all: Vec<(&'static str, &str)> = headers.to_vec();
    if let Some(authorization) = &authorization {
        all.push(("authorization", authorization));
    }
    app.call(None, method, &format!("{SERVICE}{path}"), body, &all)
        .await
}

async fn platforms(app: &App) -> Value {
    let reply = app
        .call(None, Method::GET, "/v1/meta", None, &[])
        .await
        .ok();
    reply["wallet_platforms"].clone()
}

#[tokio::test]
async fn meta_names_only_the_platforms_that_are_configured() {
    let off = App::start(DB).await;
    assert_eq!(platforms(&off).await, json!([]));
    let google = App::start_with_wallet(DB, wallet(Some(&["GOOGLE_"]))).await;
    assert_eq!(platforms(&google).await, json!(["GOOGLE"]));
    let apple = App::start_with_wallet(DB, wallet(Some(&["APPLE_"]))).await;
    assert_eq!(platforms(&apple).await, json!(["APPLE"]));
    assert_eq!(platforms(&app().await).await, json!(["APPLE", "GOOGLE"]));
}

#[tokio::test]
async fn a_platform_that_is_not_configured_says_so() {
    let off = App::start(DB).await;
    let deal = off.active().await;
    for platform in ["apple", "google", "apple/link"] {
        wallet_post(&off, &deal.ben, &deal.exchange, platform)
            .await
            .refused(StatusCode::NOT_FOUND, "WALLET_UNAVAILABLE");
    }
    let apple_only = App::start_with_wallet(DB, wallet(Some(&["APPLE_"]))).await;
    wallet_post(&apple_only, &deal.ben, &deal.exchange, "google")
        .await
        .refused(StatusCode::NOT_FOUND, "WALLET_UNAVAILABLE");
    assert_eq!(
        wallet_post(&apple_only, &deal.ben, &deal.exchange, "apple")
            .await
            .status,
        StatusCode::OK
    );
    // Nor does the pass web service answer.
    let reply = device(
        &off,
        Method::GET,
        &format!("/v1/passes/{}/x", testkit::PASS_TYPE_ID),
        Some("x"),
        None,
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn only_a_party_gets_a_pass_and_only_once_something_is_agreed() {
    let app = app().await;
    let deal = app.active().await;
    let stranger = app.user("Sam").await;
    for platform in ["apple", "google", "apple/link"] {
        wallet_post(&app, &stranger, &deal.exchange, platform)
            .await
            .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
        let reply = app
            .call(
                None,
                Method::POST,
                &format!("/v1/exchanges/{}/wallet/{platform}", deal.exchange),
                None,
                &[],
            )
            .await;
        reply.refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
        wallet_post(&app, &deal.ben, "not-an-id", platform)
            .await
            .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
    }
    // Nobody tried has a pass row: refusing comes before issuing.
    let rows: i64 = sqlx::query_scalar("SELECT count(*) FROM wallet_pass WHERE account_id = $1")
        .bind(stranger.id)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(rows, 0);

    let negotiating = app.negotiating().await;
    wallet_post(&app, &negotiating.ana, &negotiating.exchange, "apple")
        .await
        .refused(StatusCode::CONFLICT, "ACTION_NOT_ALLOWED");

    for user in [&deal.ana, &deal.ben] {
        let json = pass_json(&wallet_post(&app, user, &deal.exchange, "apple").await);
        assert_eq!(json["passTypeIdentifier"], testkit::PASS_TYPE_ID);
        assert_eq!(json["webServiceURL"], "https://app.test/v1/wallet/apple");
        assert_eq!(
            json["generic"]["backFields"][0]["value"],
            format!("https://app.test/exchanges/{}", deal.exchange)
        );
        // Nothing from the agreement is on it (`fence_job` in common).
        let text = json.to_string();
        for secret in ["Ana", "Ben", "Ruiz", "Ortiz", "fence", "40000", "400.00"] {
            assert!(!text.contains(secret), "{secret} is on the pass: {text}");
        }
    }
}

#[tokio::test]
async fn handing_out_a_pass_is_limited_per_hour() {
    let app = app().await;
    let deal = app.active().await;
    for _ in 0..10 {
        assert_eq!(
            wallet_post(&app, &deal.ben, &deal.exchange, "google")
                .await
                .status,
            StatusCode::OK
        );
    }
    wallet_post(&app, &deal.ben, &deal.exchange, "google")
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS");
    // Another platform, and another person, count apart.
    assert_eq!(
        wallet_post(&app, &deal.ben, &deal.exchange, "apple")
            .await
            .status,
        StatusCode::OK
    );
    assert_eq!(
        wallet_post(&app, &deal.ana, &deal.exchange, "google")
            .await
            .status,
        StatusCode::OK
    );
}

/// Ben's Apple pass for the deal: its serial number and token.
async fn ben_apple_pass(app: &App, deal: &Deal) -> (String, String) {
    let json = pass_json(&wallet_post(app, &deal.ben, &deal.exchange, "apple").await);
    (
        json["serialNumber"].as_str().unwrap().to_owned(),
        json["authenticationToken"].as_str().unwrap().to_owned(),
    )
}

#[tokio::test]
async fn the_pass_web_service_from_a_device() {
    let _worker = WORKER.lock().await;
    let app = app().await;
    let deal = app.active().await;
    let (serial, token) = ben_apple_pass(&app, &deal).await;
    let pass_type = testkit::PASS_TYPE_ID;
    let device_id = "a1b2c3d4e5f6";
    let registration = format!("/v1/devices/{device_id}/registrations/{pass_type}/{serial}");
    let push = json!({ "pushToken": "00ff00ff" });

    // Registering: the pass's token, and only it.
    for wrong in [None, Some("not-the-token"), Some("")] {
        let reply = device(
            &app,
            Method::POST,
            &registration,
            wrong,
            Some(push.clone()),
            &[],
        )
        .await;
        assert_eq!(reply.status, StatusCode::UNAUTHORIZED, "{wrong:?}");
    }
    let other_type = format!("/v1/devices/{device_id}/registrations/pass.other.type/{serial}");
    let reply = device(
        &app,
        Method::POST,
        &other_type,
        Some(&token),
        Some(push.clone()),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = device(
        &app,
        Method::POST,
        &registration,
        Some(&token),
        Some(push.clone()),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    let reply = device(
        &app,
        Method::POST,
        &registration,
        Some(&token),
        Some(push.clone()),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK, "registered already");
    let reply = device(
        &app,
        Method::POST,
        &registration,
        Some(&token),
        Some(json!({ "pushToken": "not hex" })),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::BAD_REQUEST);

    // Which passes changed: all of them the first time.
    let list = format!("/v1/devices/{device_id}/registrations/{pass_type}");
    let reply = device(&app, Method::GET, &list, None, None, &[]).await;
    assert_eq!(reply.status, StatusCode::OK);
    assert_eq!(reply.body["serialNumbers"], json!([serial]));
    let tag = reply.body["lastUpdated"].as_str().unwrap().to_owned();
    let reply = device(
        &app,
        Method::GET,
        "/v1/devices/unknown/registrations/x",
        None,
        None,
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    // The latest pass, and 304 while it has not changed.
    let latest = format!("/v1/passes/{pass_type}/{serial}");
    let reply = device(&app, Method::GET, &latest, Some(&token), None, &[]).await;
    let first = pass_json(&reply);
    assert_eq!(first["generic"]["primaryFields"][0]["value"], "In force");
    let modified = reply.headers["last-modified"].to_str().unwrap().to_owned();
    let reply = device(
        &app,
        Method::GET,
        &latest,
        Some(&token),
        None,
        &[("if-modified-since", &modified)],
    )
    .await;
    assert_eq!(reply.status, StatusCode::NOT_MODIFIED);
    let reply = device(&app, Method::GET, &latest, Some("wrong"), None, &[]).await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);

    // Ana marks the repair she owes Ben as delivered. The pass is marked in
    // the same transaction, and the worker pushes to Ben's device.
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();
    assert_eq!(
        pass_status(&app, &serial).await,
        ("PENDING".to_owned(), false)
    );
    let recorder = Arc::new(Recorder::default());
    run_worker(&app, &recorder).await;
    assert_eq!(*recorder.pushes.lock().unwrap(), ["00ff00ff"]);
    assert_eq!(
        pass_status(&app, &serial).await,
        ("CURRENT".to_owned(), false)
    );

    // The device asks what changed since its tag, and fetches it.
    let reply = device(
        &app,
        Method::GET,
        &format!("{list}?passesUpdatedSince={tag}"),
        None,
        None,
        &[],
    )
    .await;
    assert_eq!(reply.body["serialNumbers"], json!([serial]));
    let reply = device(
        &app,
        Method::GET,
        &latest,
        Some(&token),
        None,
        &[("if-modified-since", &modified)],
    )
    .await;
    let updated = pass_json(&reply);
    assert_eq!(
        updated["generic"]["primaryFields"][0]["value"],
        "Waiting for you"
    );
    assert_ne!(reply.headers["last-modified"].to_str().unwrap(), modified);

    // A change that leaves the face as it was pushes nothing.
    app.command(
        &deal.ben,
        &deal.exchange,
        json!({ "type": "REQUEST_CLOSE", "note": null }),
    )
    .await
    .ok();
    app.command(
        &deal.ben,
        &deal.exchange,
        json!({ "type": "RETRACT_CLOSE" }),
    )
    .await
    .ok();
    run_worker(&app, &recorder).await;
    assert_eq!(recorder.pushes.lock().unwrap().len(), 1);

    // Unregistering: the device stops hearing about it.
    let reply = device(&app, Method::DELETE, &registration, Some(&token), None, &[]).await;
    assert_eq!(reply.status, StatusCode::OK);
    let reply = device(&app, Method::GET, &list, None, None, &[]).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);

    // The device's log is taken and answered.
    let reply = device(
        &app,
        Method::POST,
        "/v1/log",
        None,
        Some(json!({ "logs": ["a line"] })),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::OK);
}

#[tokio::test]
async fn a_download_link_gives_the_pass_without_a_session_for_a_while() {
    let app = app().await;
    let deal = app.active().await;
    let link = wallet_post(&app, &deal.ben, &deal.exchange, "apple/link")
        .await
        .ok();
    let url = link["url"].as_str().unwrap();
    assert!(link["expires_at"].is_string());
    let path = url
        .strip_prefix("https://app.test")
        .expect("on the web origin");
    assert!(path.starts_with("/v1/wallet/apple/pass?token="));
    let json = pass_json(&app.call(None, Method::GET, path, None, &[]).await);
    assert_eq!(json["passTypeIdentifier"], testkit::PASS_TYPE_ID);

    // One character of the token changed.
    let at = path.find("token=").unwrap() + 10;
    let changed = if &path[at..=at] == "A" { "B" } else { "A" };
    let forged = format!("{}{changed}{}", &path[..at], &path[at + 1..]);
    let reply = app.call(None, Method::GET, &forged, None, &[]).await;
    assert_eq!(reply.status, StatusCode::NOT_FOUND);
}

/// The claims of a JWT whose signature holds under the test service
/// account's key.
fn verified_claims(jwt: &str) -> Value {
    let (signed, signature) = jwt.rsplit_once('.').unwrap();
    UnparsedPublicKey::new(
        &RSA_PKCS1_2048_8192_SHA256,
        &testkit::CREDENTIALS.google_public_der,
    )
    .verify(
        signed.as_bytes(),
        &URL_SAFE_NO_PAD.decode(signature).unwrap(),
    )
    .expect("signed by the service account");
    let claims = signed.split_once('.').unwrap().1;
    serde_json::from_slice(&URL_SAFE_NO_PAD.decode(claims).unwrap()).unwrap()
}

#[tokio::test]
async fn the_google_link_is_signed_and_carries_this_partys_object() {
    let _worker = WORKER.lock().await;
    let app = app().await;
    let deal = app.active().await;
    let link = |user| {
        let (app, deal) = (&app, &deal);
        async move {
            let reply = wallet_post(app, user, &deal.exchange, "google").await.ok();
            let url = reply["url"].as_str().unwrap().to_owned();
            let jwt = url
                .strip_prefix("https://pay.google.com/gp/v/save/")
                .expect("a save link")
                .to_owned();
            verified_claims(&jwt)
        }
    };
    let ben = link(&deal.ben).await;
    assert_eq!(ben["iss"], testkit::CLIENT_EMAIL);
    assert_eq!(
        (ben["aud"].as_str(), ben["typ"].as_str()),
        (Some("google"), Some("savetowallet"))
    );
    assert_eq!(ben["origins"], json!(["https://app.test"]));
    let object = &ben["payload"]["genericObjects"][0];
    let id = object["id"].as_str().unwrap();
    assert!(id.starts_with(&format!("{}.", testkit::ISSUER_ID)));
    assert_eq!(object["classId"], ben["payload"]["genericClasses"][0]["id"]);
    assert_eq!(object["state"], "ACTIVE");
    assert_eq!(object["header"]["defaultValue"]["value"], "In force");
    assert_eq!(
        object["linksModuleData"]["uris"][0]["uri"],
        format!("https://app.test/exchanges/{}", deal.exchange)
    );
    let text = ben.to_string();
    for secret in ["Ana", "Ben", "Ruiz", "fence", "40000"] {
        assert!(!text.contains(secret), "{secret} is in the link: {text}");
    }

    // The same object every time for Ben; another one for Ana.
    let again = link(&deal.ben).await;
    assert_eq!(again["payload"]["genericObjects"][0]["id"], id);
    let ana = link(&deal.ana).await;
    assert_ne!(ana["payload"]["genericObjects"][0]["id"], id);

    // A change reaches the object through the worker.
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();
    let recorder = Arc::new(Recorder::default());
    run_worker(&app, &recorder).await;
    let patches = recorder.patches.lock().unwrap();
    let (_, patched) = patches
        .iter()
        .find(|(object, _)| object == id)
        .expect("Ben's object patched");
    assert_eq!(
        patched["header"]["defaultValue"]["value"],
        "Waiting for you"
    );
}

#[tokio::test]
async fn deleting_an_account_revokes_its_passes() {
    let _worker = WORKER.lock().await;
    let app = app().await;
    let deal = app.active().await;
    let (serial, token) = ben_apple_pass(&app, &deal).await;
    let google = wallet_post(&app, &deal.ben, &deal.exchange, "google")
        .await
        .ok();
    let object_id = {
        let jwt = google["url"]
            .as_str()
            .unwrap()
            .rsplit('/')
            .next()
            .unwrap()
            .to_owned();
        verified_claims(&jwt)["payload"]["genericObjects"][0]["id"]
            .as_str()
            .unwrap()
            .to_owned()
    };
    let pass_type = testkit::PASS_TYPE_ID;
    let registration = format!("/v1/devices/dev-ben/registrations/{pass_type}/{serial}");
    let reply = device(
        &app,
        Method::POST,
        &registration,
        Some(&token),
        Some(json!({ "pushToken": "beef" })),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::CREATED);
    let (ana_serial, _) = {
        let json = pass_json(&wallet_post(&app, &deal.ana, &deal.exchange, "apple").await);
        (json["serialNumber"].as_str().unwrap().to_owned(), ())
    };

    yuppers_backend::deletion::delete_account(&app.db, &app.rules, deal.ben.id)
        .await
        .unwrap();
    assert_eq!(
        pass_status(&app, &serial).await,
        ("PENDING".to_owned(), true)
    );
    // Ana's pass is marked too, by the request to close made in Ben's name;
    // it is hers and stays live.
    assert_eq!(
        pass_status(&app, &ana_serial).await,
        ("PENDING".to_owned(), false)
    );

    let recorder = Arc::new(Recorder::default());
    run_worker(&app, &recorder).await;
    // Ben's device was told once, then forgotten.
    assert!(recorder.pushes.lock().unwrap().contains(&"beef".to_owned()));
    let devices: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM wallet_device_registration r JOIN wallet_pass p ON p.id = r.wallet_pass_id
         WHERE p.account_id = $1",
    )
    .bind(deal.ben.id)
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!(devices, 0);
    // Google's object is made inactive and stripped.
    let patches = recorder.patches.lock().unwrap().clone();
    let (_, object) = patches
        .iter()
        .find(|(id, _)| *id == object_id)
        .expect("Ben's Google object patched");
    assert_eq!(object["state"], "INACTIVE");
    assert_eq!(object["linksModuleData"]["uris"], json!([]));
    assert!(!object.to_string().contains(&deal.exchange));

    // What the web service now gives for the pass is the void face: no
    // reference, no link, nothing of the exchange.
    let reply = device(
        &app,
        Method::GET,
        &format!("/v1/passes/{pass_type}/{serial}"),
        Some(&token),
        None,
        &[],
    )
    .await;
    let json = pass_json(&reply);
    assert_eq!(json["voided"], true);
    let text = json.to_string();
    assert!(
        !text.contains(&deal.exchange) && !text.contains("exchanges/"),
        "{text}"
    );
    assert_eq!(json["generic"]["auxiliaryFields"], json!([]));
    // A void pass takes no device, and its devices hear nothing more.
    let reply = device(
        &app,
        Method::POST,
        &registration,
        Some(&token),
        Some(json!({ "pushToken": "beef" })),
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::UNAUTHORIZED);
    let reply = device(
        &app,
        Method::GET,
        "/v1/devices/dev-ben/registrations/x",
        None,
        None,
        &[],
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT);
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({ "type": "ADD_STATEMENT", "note": "Noted." }),
    )
    .await
    .ok();
    assert_eq!(
        pass_status(&app, &serial).await,
        ("CURRENT".to_owned(), true)
    );
}

#[tokio::test]
async fn the_store_marks_and_revokes_passes_in_the_callers_transaction() {
    let app = app().await;
    let deal = app.active().await;
    let (serial, _) = ben_apple_pass(&app, &deal).await;
    let exchange: Uuid = deal.exchange.parse().unwrap();
    let mut tx = app.db.begin().await.unwrap();
    store::mark_exchange_changed(&mut tx, exchange)
        .await
        .unwrap();
    tx.rollback().await.unwrap();
    assert_eq!(pass_status(&app, &serial).await.0, "CURRENT");
    let mut tx = app.db.begin().await.unwrap();
    assert_eq!(
        store::revoke_for_account(&mut tx, deal.ben.id)
            .await
            .unwrap(),
        1
    );
    tx.rollback().await.unwrap();
    assert_eq!(
        pass_status(&app, &serial).await,
        ("CURRENT".to_owned(), false)
    );
}
