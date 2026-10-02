//! The worker's timers. They act on every exchange in the database at a
//! given moment, so this runs as one test in a database of its own, where no
//! other test's exchanges can be swept up.

mod common;

use common::App;
use exchange_backend::exchanges::service::run_timers;
use serde_json::json;
use time::{Duration, OffsetDateTime};
use uuid::Uuid;

async fn events(app: &App, exchange: &str) -> Vec<(String, String)> {
    sqlx::query_as(
        "SELECT type, actor_kind FROM exchange_event WHERE exchange_id = $1 ORDER BY sequence",
    )
    .bind(exchange.parse::<Uuid>().unwrap())
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

#[tokio::test]
async fn timers_expire_offers_lapse_close_requests_and_close_idle_exchanges() {
    let app = App::start("exchange_test_timers").await;
    let now = OffsetDateTime::now_utc();
    let at = |days: i64| now + Duration::days(days);

    let unanswered = app.negotiating().await;
    let stuck = app.active().await;
    let idle = app.active().await;
    app.command(
        &stuck.ana,
        &stuck.exchange,
        json!({ "type": "REQUEST_CLOSE" }),
    )
    .await
    .ok();

    // Nothing is due yet.
    assert_eq!(run_timers(&app.db, &app.rules, at(6)).await.unwrap(), 0);

    // A week on, the close request has had its window.
    assert_eq!(run_timers(&app.db, &app.rules, at(8)).await.unwrap(), 1);
    let view = app.view(&stuck.ben, &stuck.exchange).await;
    assert_eq!(
        (&view["closed_outcome"], &view["closed_reason"]),
        (&json!("UNRESOLVED"), &json!("CLOSE_REQUEST"))
    );
    assert_eq!(app.view(&idle.ana, &idle.exchange).await["state"], "ACTIVE");

    // Two weeks on, the offer nobody answered runs out.
    assert_eq!(run_timers(&app.db, &app.rules, at(15)).await.unwrap(), 1);
    let view = app.view(&unanswered.ana, &unanswered.exchange).await;
    assert_eq!(
        (&view["closed_outcome"], &view["closed_reason"]),
        (&json!("NOT_AGREED"), &json!("EXPIRED"))
    );
    assert_eq!(
        events(&app, &unanswered.exchange).await.last().unwrap(),
        &("EXCHANGE_CLOSED".to_owned(), "SYSTEM".to_owned())
    );

    // Two months of silence prompts both parties, once.
    assert_eq!(run_timers(&app.db, &app.rules, at(61)).await.unwrap(), 1);
    assert_eq!(run_timers(&app.db, &app.rules, at(62)).await.unwrap(), 0);
    assert_eq!(app.view(&idle.ana, &idle.exchange).await["state"], "ACTIVE");
    assert_eq!(
        events(&app, &idle.exchange).await.last().unwrap().0,
        "INACTIVITY_PROMPTED"
    );

    // A month after the prompt, with still nothing, it closes.
    assert_eq!(run_timers(&app.db, &app.rules, at(90)).await.unwrap(), 0);
    assert_eq!(run_timers(&app.db, &app.rules, at(92)).await.unwrap(), 1);
    let view = app.view(&idle.ben, &idle.exchange).await;
    assert_eq!(
        (&view["closed_outcome"], &view["closed_reason"]),
        (&json!("UNRESOLVED"), &json!("INACTIVE"))
    );
    // Each contribution keeps the status it had.
    assert_eq!(view["contributions"][0]["status"], "PENDING");
}
