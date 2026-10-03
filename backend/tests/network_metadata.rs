//! A signer's network address is kept for a limited time, and the signature
//! for good (DESIGN.md §14).
//!
//! In a test binary of its own, so in a database of its own: the purge
//! removes every old enough address in the database, and in one shared with
//! other tests it would remove theirs while they are still checking them.

mod common;

use common::App;
use exchange_backend::exchanges::service::purge_network_metadata;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

const DATABASE: &str = "exchange_test_network_metadata";

/// The addresses recorded with an exchange's signatures, oldest first.
async fn recorded(app: &App, exchange: Uuid) -> Vec<Option<String>> {
    sqlx::query_scalar(
        "SELECT host(m.ip_address) FROM acceptance_network_metadata m
         JOIN acceptance a ON a.id = m.acceptance_id
         WHERE a.exchange_id = $1
         ORDER BY a.accepted_at, a.id",
    )
    .bind(exchange)
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_address_is_forgotten_after_the_retention_period_and_the_signature_kept() {
    let app = App::start(DATABASE).await;
    let deal = app.active().await;
    let exchange = deal.exchange.parse::<Uuid>().unwrap();
    assert_eq!(recorded(&app, exchange).await.len(), 2);

    // Nothing is old enough yet.
    let now = OffsetDateTime::now_utc();
    assert_eq!(
        purge_network_metadata(&app.db, &app.rules, now)
            .await
            .unwrap(),
        0
    );
    assert_eq!(recorded(&app, exchange).await.len(), 2);

    // A day past the retention period, both records go; the signatures stay.
    let later = now + app.rules.network_metadata_retention + Duration::days(1);
    let removed = purge_network_metadata(&app.db, &app.rules, later)
        .await
        .unwrap();
    assert!(removed >= 2, "{removed}");
    assert_eq!(recorded(&app, exchange).await, Vec::<Option<String>>::new());
    let signatures: i64 =
        sqlx::query_scalar("SELECT count(*) FROM acceptance WHERE exchange_id = $1")
            .bind(exchange)
            .fetch_one(&app.owner)
            .await
            .unwrap();
    assert_eq!(signatures, 2);
}
