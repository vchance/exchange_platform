//! A signer's network address is recorded with the signature (DESIGN.md §8,
//! §13.4): the connection's peer, unless the deployment names a proxy header
//! to believe. Every request the test helpers make arrives from
//! `common::PEER`.
//!
//! The tests take turns: one purges every address recorded in the database,
//! which would race the others' checks of what was recorded.

mod common;

use std::sync::Arc;

use axum::http::{HeaderName, Method};
use common::{App, Deal, PEER, accept};
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use yuppers_backend::auth::LogSender;
use yuppers_backend::domain::Rules;
use yuppers_backend::exchanges::service::purge_network_metadata;
use yuppers_backend::http::TrustedProxies;

const DATABASE: &str = "yuppers_test_client_address";
const SPOOFED: &str = "203.0.113.7, 10.0.0.2";

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// The addresses recorded with an exchange's signatures, oldest first.
async fn recorded(app: &App, exchange: &str) -> Vec<Option<String>> {
    sqlx::query_scalar(
        "SELECT host(m.ip_address) FROM acceptance_network_metadata m
         JOIN acceptance a ON a.id = m.acceptance_id
         WHERE a.exchange_id = $1
         ORDER BY a.accepted_at, a.id",
    )
    .bind(exchange.parse::<Uuid>().unwrap())
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

/// Ben claims the link and is confirmed, then accepts with a forwarding
/// header on his request, as a client could add one itself.
async fn accept_with_header(app: &App, deal: &Deal) {
    app.post(
        &deal.ben,
        "/v1/invitations/claim",
        json!({ "token": deal.invitation }),
    )
    .await
    .ok();
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({ "type": "CONFIRM_COUNTERPARTY" }),
    )
    .await
    .ok();
    let version = app.view(&deal.ben, &deal.exchange).await["version"].clone();
    app.call(
        Some(&deal.ben),
        Method::POST,
        &format!("/v1/exchanges/{}/commands", deal.exchange),
        Some(json!({ "expected_version": version, "command": accept(&deal.revision) })),
        &[("x-forwarded-for", SPOOFED)],
    )
    .await
    .ok();
}

#[tokio::test]
async fn by_default_the_peer_is_recorded_and_a_forwarding_header_is_ignored() {
    let _turn = TURN.lock().await;
    let app = App::start(DATABASE).await;

    // Ana signs by sending, Ben by accepting.
    let deal = app.active().await;
    let peer = Some(PEER.ip().to_string());
    assert_eq!(
        recorded(&app, &deal.exchange).await,
        [peer.clone(), peer.clone()]
    );

    // A header nobody said to trust changes nothing.
    let deal = app.negotiating().await;
    accept_with_header(&app, &deal).await;
    assert_eq!(recorded(&app, &deal.exchange).await, [peer.clone(), peer]);
}

#[tokio::test]
async fn behind_trusted_proxies_the_address_comes_from_the_header() {
    let _turn = TURN.lock().await;
    let app = App::start_behind(
        DATABASE,
        Rules::default(),
        Arc::new(LogSender),
        TrustedProxies::behind(HeaderName::from_static("x-forwarded-for"), 2),
    )
    .await;

    let deal = app.negotiating().await;
    accept_with_header(&app, &deal).await;
    assert_eq!(
        recorded(&app, &deal.exchange).await,
        [
            // Ana's request carried no header: the peer is all there is.
            Some(PEER.ip().to_string()),
            // Two proxies: the second entry from the end is the client.
            Some("203.0.113.7".to_owned()),
        ]
    );
}

#[tokio::test]
async fn the_address_is_forgotten_after_the_retention_period_and_the_signature_kept() {
    let _turn = TURN.lock().await;
    let app = App::start(DATABASE).await;
    let deal = app.active().await;
    let exchange = deal.exchange.parse::<Uuid>().unwrap();
    assert_eq!(recorded(&app, &deal.exchange).await.len(), 2);

    // Nothing is old enough yet.
    let now = OffsetDateTime::now_utc();
    assert_eq!(
        purge_network_metadata(&app.db, &app.rules, now)
            .await
            .unwrap(),
        0
    );
    assert_eq!(recorded(&app, &deal.exchange).await.len(), 2);

    // A day past the retention period, both records go, with whatever other
    // tests in this database have signed; the signatures stay.
    let later = now + app.rules.network_metadata_retention + Duration::days(1);
    let removed = purge_network_metadata(&app.db, &app.rules, later)
        .await
        .unwrap();
    assert!(removed >= 2, "{removed}");
    assert_eq!(
        recorded(&app, &deal.exchange).await,
        Vec::<Option<String>>::new()
    );
    let signatures: i64 =
        sqlx::query_scalar("SELECT count(*) FROM acceptance WHERE exchange_id = $1")
            .bind(exchange)
            .fetch_one(&app.owner)
            .await
            .unwrap();
    assert_eq!(signatures, 2);
}
