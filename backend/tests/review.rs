//! Staff review of abuse reports (DESIGN.md §9), against a real database:
//! who may review and how they are named, the queue, opening a report,
//! every action and the audit history it leaves, hidden content as each
//! party sees it, suspension, the alert to reviewers and the metric.

mod common;

use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use common::{App, Deal, User, accept, fence_job};
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};
use uuid::Uuid;
use yuppers_backend::auth::{CodeMessage, CodeSender, SendFuture};
use yuppers_backend::domain::Rules;
use yuppers_backend::metrics::{self, Text};
use yuppers_backend::notifications::outbox::{Delivery, DeliveryRules, deliver_due};
use yuppers_backend::notifications::wording::Wording;
use yuppers_backend::notifications::{Email, EmailSender};
use yuppers_backend::review;

const DB: &str = "yuppers_review_test";

async fn app() -> App {
    App::start(DB).await
}

/// Makes `user` a reviewer, the way the owner does: as the schema owner.
async fn make_staff(app: &App, user: &User) {
    assert!(
        review::grant(&app.owner, &user.email).await.unwrap(),
        "not a reviewer before"
    );
}

/// A reviewer, signed in just now.
async fn reviewer(app: &App) -> User {
    let staff = app.user("Rita Reviewer").await;
    make_staff(app, &staff).await;
    staff
}

/// Ben reports the agreement he has with Ana, and with it Ana. Returns the
/// report's ID.
async fn ben_reports(app: &App, deal: &Deal) -> Uuid {
    let reply = app
        .post(
            &deal.ben,
            &format!("/v1/exchanges/{}/reports", deal.exchange),
            json!({ "reason": "HARASSMENT", "details": "She sent threats in the notes." }),
        )
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    sqlx::query_scalar(
        "SELECT id FROM report WHERE reporter_account_id = $1 AND subject_exchange_id = $2::uuid
         ORDER BY created_at DESC LIMIT 1",
    )
    .bind(deal.ben.id)
    .bind(&deal.exchange)
    .fetch_one(&app.owner)
    .await
    .unwrap()
}

async fn resolve(
    app: &App,
    staff: &User,
    report: Uuid,
    outcome: &str,
    note: Option<&str>,
) -> common::Reply {
    app.post(
        staff,
        &format!("/v1/staff/reports/{report}/resolution"),
        json!({ "outcome": outcome, "note": note }),
    )
    .await
}

/// The audit history about one report, as (action, staff, account, note).
async fn audit(
    app: &App,
    report: Uuid,
) -> Vec<(String, Option<Uuid>, Option<Uuid>, Option<String>)> {
    sqlx::query_as(
        "SELECT action, staff_account_id, account_id, note FROM review_event
         WHERE report_id = $1 ORDER BY id",
    )
    .bind(report)
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

// ---- Who reviews ------------------------------------------------------------

#[tokio::test]
async fn reviewers_are_named_only_by_the_owner_from_the_command_line() {
    let app = app().await;
    let rita = app.user("Rita").await;

    // The service's own role can read the list and nothing more.
    let refused = sqlx::query("INSERT INTO staff_member (account_id) VALUES ($1)")
        .bind(rita.id)
        .execute(&app.db)
        .await
        .unwrap_err();
    assert_eq!(
        refused.as_database_error().unwrap().code().as_deref(),
        Some("42501")
    );

    // The command itself, run as the owner would run it.
    let staff = |args: &[&str]| {
        let output = std::process::Command::new(env!("CARGO_BIN_EXE_staff"))
            .args(args)
            .env("MIGRATION_DATABASE_URL", &app.owner_url)
            .output()
            .unwrap();
        (
            output.status.success(),
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
        )
    };
    let (ok, out, _) = staff(&["grant", &rita.email.to_uppercase()]);
    assert!(ok);
    assert!(out.contains("now reviews reports"), "{out}");
    let (ok, out, _) = staff(&["grant", &rita.email]);
    assert!(ok);
    assert!(out.contains("already reviews reports"), "{out}");
    let (ok, out, _) = staff(&["list"]);
    assert!(ok);
    assert!(
        out.contains(&rita.email) && out.contains(&rita.id.to_string()),
        "{out}"
    );
    assert!(review::is_staff(&app.db, rita.id).await.unwrap());

    let (ok, _, err) = staff(&["grant", "nobody-at-all@example.test"]);
    assert!(!ok);
    assert!(err.contains("no account"), "{err}");
    let (ok, _, err) = staff(&["promote", &rita.email]);
    assert!(!ok);
    assert!(err.contains("usage"), "{err}");

    // By account ID too.
    let (ok, out, _) = staff(&["revoke", &rita.id.to_string()]);
    assert!(ok, "{out}");
    assert!(out.contains("no longer reviews reports"), "{out}");
    assert!(!review::is_staff(&app.db, rita.id).await.unwrap());

    // Both are in the review history, as the owner's.
    let history: Vec<(String, Option<Uuid>)> = sqlx::query_as(
        "SELECT action, staff_account_id FROM review_event WHERE account_id = $1 ORDER BY id",
    )
    .bind(rita.id)
    .fetch_all(&app.owner)
    .await
    .unwrap();
    assert_eq!(
        history,
        [
            ("STAFF_GRANTED".to_owned(), None),
            ("STAFF_REVOKED".to_owned(), None)
        ]
    );

    // A suspended account cannot be made a reviewer.
    let sam = app.user("Sam").await;
    sqlx::query("UPDATE account SET status = 'SUSPENDED' WHERE id = $1")
        .bind(sam.id)
        .execute(&app.owner)
        .await
        .unwrap();
    assert!(review::grant(&app.owner, &sam.email).await.is_err());
}

#[tokio::test]
async fn every_staff_path_is_not_found_to_anyone_but_a_reviewer() {
    let app = app().await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;
    let stranger = app.user("Stranger").await;
    let some = Uuid::new_v4();

    let paths: Vec<(Method, String, Option<Value>)> = vec![
        (Method::GET, "/v1/staff/reports".into(), None),
        (Method::GET, format!("/v1/staff/reports/{report}"), None),
        (
            Method::POST,
            format!("/v1/staff/reports/{report}/resolution"),
            Some(json!({ "outcome": "DISMISSED" })),
        ),
        // A body that would not even read is still just "not found".
        (
            Method::POST,
            format!("/v1/staff/reports/{report}/resolution"),
            Some(json!({ "outcome": "BANISHED" })),
        ),
        (Method::GET, "/v1/staff/suspensions".into(), None),
        (
            Method::POST,
            format!("/v1/staff/suspensions/{some}/lift"),
            Some(json!({ "note": "x" })),
        ),
        (Method::GET, "/v1/staff/hidden".into(), None),
        (
            Method::POST,
            "/v1/staff/hidden/restore".into(),
            Some(json!({ "exchange_id": some, "account_id": some, "note": "x" })),
        ),
    ];
    for user in [None, Some(&stranger), Some(&deal.ben), Some(&deal.ana)] {
        for (method, path, body) in &paths {
            let reply = app
                .call(user, method.clone(), path, body.clone(), &[])
                .await;
            assert_eq!(
                (reply.status, reply.code()),
                (StatusCode::NOT_FOUND, "NOT_FOUND"),
                "{method} {path}"
            );
        }
    }
    // Nothing was looked at or done.
    assert!(audit(&app, report).await.is_empty());
}

#[tokio::test]
async fn a_reviewer_must_have_signed_in_within_the_last_twelve_hours() {
    let app = app().await;
    let staff = reviewer(&app).await;
    sqlx::query(
        "UPDATE account_session SET authenticated_at = now() - interval '13 hours'
         WHERE account_id = $1",
    )
    .bind(staff.id)
    .execute(&app.owner)
    .await
    .unwrap();

    for path in [
        "/v1/staff/reports",
        "/v1/staff/suspensions",
        "/v1/staff/hidden",
    ] {
        app.get(&staff, path)
            .await
            .refused(StatusCode::UNAUTHORIZED, "SESSION_TOO_OLD");
    }
    // The session itself still works everywhere else.
    assert_eq!(app.get(&staff, "/v1/me").await.status, StatusCode::OK);

    sqlx::query(
        "UPDATE account_session SET authenticated_at = now() - interval '11 hours'
         WHERE account_id = $1",
    )
    .bind(staff.id)
    .execute(&app.owner)
    .await
    .unwrap();
    assert_eq!(
        app.get(&staff, "/v1/staff/reports").await.status,
        StatusCode::OK
    );
}

// ---- The queue and a report -------------------------------------------------

#[tokio::test]
async fn the_queue_lists_open_reports_oldest_first_with_their_age() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let fresh = ben_reports(&app, &deal).await;

    // One filed through the invitation link without signing in, two days ago,
    // as could be done before reporting needed an account: the column allows
    // it, so the queue and the screen still show such a report.
    let old: Uuid = sqlx::query_scalar(
        "INSERT INTO report (subject_exchange_id, subject_account_id, reason, created_at)
         VALUES ($1::uuid, $2, 'SCAM', now() - interval '2 days') RETURNING id",
    )
    .bind(&deal.exchange)
    .bind(deal.ana.id)
    .fetch_one(&app.owner)
    .await
    .unwrap();

    let queue = app.get(&staff, "/v1/staff/reports").await.ok();
    assert_eq!(queue["review_within_hours"], 24);
    let reports = queue["reports"].as_array().unwrap();
    let at = |id: Uuid| {
        reports
            .iter()
            .position(|report| report["id"] == id.to_string())
            .unwrap_or_else(|| panic!("{id} is not queued"))
    };
    assert!(at(old) < at(fresh), "oldest first");

    let old = &reports[at(old)];
    assert_eq!(old["reason"], "SCAM");
    assert_eq!(old["reporter_account_id"], Value::Null, "through the link");
    assert_eq!(old["overdue"], true);
    assert!(old["age_seconds"].as_i64().unwrap() >= 2 * 24 * 3600);

    let fresh = &reports[at(fresh)];
    assert_eq!(fresh["reason"], "HARASSMENT");
    assert_eq!(fresh["details"], "She sent threats in the notes.");
    assert_eq!(fresh["reporter_account_id"], deal.ben.id.to_string());
    assert_eq!(fresh["subject_account_id"], deal.ana.id.to_string());
    assert_eq!(fresh["exchange_id"], deal.exchange);
    assert!(fresh["display_code"].as_str().unwrap().len() >= 4);
    assert_eq!(fresh["overdue"], false);
    assert!(fresh["age_seconds"].as_i64().unwrap() < 600);
}

#[tokio::test]
async fn opening_a_report_shows_the_exchange_and_is_recorded() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({
            "type": "CONTRIBUTION", "contribution": deal.repair, "action": "CLAIM",
            "note": "I know where you live.",
        }),
    )
    .await
    .ok();
    let report = ben_reports(&app, &deal).await;

    let detail = app
        .get(&staff, &format!("/v1/staff/reports/{report}"))
        .await
        .ok();
    assert_eq!(detail["report"]["id"], report.to_string());
    assert_eq!(detail["subject"]["id"], deal.ana.id.to_string());
    assert_eq!(detail["subject"]["status"], "ACTIVE");
    assert_eq!(detail["subject"]["party"], "A");
    assert_eq!(detail["subject"]["name"], "Ana Ruiz");
    assert_eq!(detail["reporter"]["party"], "B");
    assert_eq!(detail["content_hidden"], false);
    // The record as a party's copy holds it, with nothing hidden.
    let record = &detail["record"];
    assert_eq!(record["complete"], true);
    assert_eq!(record["parties"]["A"], "Ana Ruiz");
    assert_eq!(
        record["revisions"][0]["signed"]["terms"],
        "Repair the back fence."
    );
    let notes: Vec<&str> = record["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|event| event["note"].as_str())
        .collect();
    assert!(notes.contains(&"I know where you live."), "{notes:?}");

    // The view is in the audit history, and in what the reviewer is shown.
    assert_eq!(
        audit(&app, report).await,
        [(
            "REPORT_VIEWED".to_owned(),
            Some(staff.id),
            Some(deal.ana.id),
            None
        )]
    );
    // Opened again, it shows the earlier view.
    let again = app
        .get(&staff, &format!("/v1/staff/reports/{report}"))
        .await
        .ok();
    let actions: Vec<&str> = again["history"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry["action"].as_str().unwrap())
        .collect();
    assert_eq!(actions, ["REPORT_VIEWED", "REPORT_VIEWED"]);

    // No such report.
    app.get(&staff, &format!("/v1/staff/reports/{}", Uuid::new_v4()))
        .await
        .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
    app.get(&staff, "/v1/staff/reports/not-an-id")
        .await
        .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
}

#[tokio::test]
async fn opening_reports_is_limited_per_reviewer_per_hour() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;
    sqlx::query(
        "INSERT INTO review_event (staff_account_id, action, report_id)
         SELECT $1, 'REPORT_VIEWED', $2 FROM generate_series(1, $3)",
    )
    .bind(staff.id)
    .bind(report)
    .bind(review::VIEWS_PER_HOUR as i32)
    .execute(&app.owner)
    .await
    .unwrap();
    app.get(&staff, &format!("/v1/staff/reports/{report}"))
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS");
    // Acting is counted apart.
    assert_eq!(
        resolve(&app, &staff, report, "DISMISSED", None)
            .await
            .status,
        StatusCode::NO_CONTENT
    );
}

// ---- Dismissing --------------------------------------------------------------

#[tokio::test]
async fn a_dismissed_report_is_resolved_once_and_shows_nothing_more() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;

    let reply = resolve(
        &app,
        &staff,
        report,
        "DISMISSED",
        Some("  A misunderstanding.  "),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    let (status, by, outcome, note, resolved): (String, Uuid, String, String, bool) =
        sqlx::query_as(
            "SELECT status, resolved_by, outcome, resolution_note, resolved_at IS NOT NULL
             FROM report WHERE id = $1",
        )
        .bind(report)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(
        (
            status.as_str(),
            by,
            outcome.as_str(),
            note.as_str(),
            resolved
        ),
        (
            "DISMISSED",
            staff.id,
            "DISMISSED",
            "A misunderstanding.",
            true
        )
    );
    assert_eq!(
        audit(&app, report).await,
        [(
            "REPORT_DISMISSED".to_owned(),
            Some(staff.id),
            Some(deal.ana.id),
            Some("A misunderstanding.".to_owned())
        )]
    );

    // Gone from the queue, closed to reading, and not resolved again.
    let queue = app.get(&staff, "/v1/staff/reports").await.ok();
    assert!(
        !queue["reports"]
            .as_array()
            .unwrap()
            .iter()
            .any(|queued| queued["id"] == report.to_string())
    );
    app.get(&staff, &format!("/v1/staff/reports/{report}"))
        .await
        .refused(StatusCode::CONFLICT, "REPORT_RESOLVED");
    resolve(
        &app,
        &staff,
        report,
        "ACCOUNT_SUSPENDED",
        Some("On second thoughts"),
    )
    .await
    .refused(StatusCode::CONFLICT, "REPORT_RESOLVED");

    // Not even the owner can rewrite a resolution, or remove a report.
    let rewrite = sqlx::query("UPDATE report SET resolution_note = 'changed' WHERE id = $1")
        .bind(report)
        .execute(&app.owner)
        .await;
    assert!(rewrite.is_err());
    assert!(
        sqlx::query("DELETE FROM report WHERE id = $1")
            .bind(report)
            .execute(&app.owner)
            .await
            .is_err()
    );
    assert!(
        sqlx::query("DELETE FROM review_event WHERE report_id = $1")
            .bind(report)
            .execute(&app.owner)
            .await
            .is_err()
    );

    // Ben, who reported it, can report again: the first one is closed.
    assert_ne!(ben_reports(&app, &deal).await, report);
}

#[tokio::test]
async fn every_outcome_but_dismissing_needs_a_note() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;
    for outcome in [
        "CONTENT_HIDDEN",
        "ACCOUNT_SUSPENDED",
        "CONTENT_HIDDEN_AND_ACCOUNT_SUSPENDED",
    ] {
        resolve(&app, &staff, report, outcome, None)
            .await
            .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
        resolve(&app, &staff, report, outcome, Some("   "))
            .await
            .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
    }
    resolve(&app, &staff, report, "DISMISSED", Some(&"x".repeat(1001)))
        .await
        .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
    // Nothing was done.
    assert!(audit(&app, report).await.is_empty());
    let status: String = sqlx::query_scalar("SELECT status FROM report WHERE id = $1")
        .bind(report)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(status, "OPEN");
}

#[tokio::test]
async fn acting_is_limited_per_reviewer_per_hour() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;
    sqlx::query(
        "INSERT INTO review_event (staff_account_id, action, account_id, note)
         SELECT $1, 'SUSPENSION_LIFTED', $1, 'filler' FROM generate_series(1, $2)",
    )
    .bind(staff.id)
    .bind(review::ACTIONS_PER_HOUR as i32)
    .execute(&app.owner)
    .await
    .unwrap();
    resolve(&app, &staff, report, "DISMISSED", None)
        .await
        .refused(StatusCode::TOO_MANY_REQUESTS, "TOO_MANY_REQUESTS");
    // Reading is counted apart.
    assert_eq!(
        app.get(&staff, &format!("/v1/staff/reports/{report}"))
            .await
            .status,
        StatusCode::OK
    );
}

// ---- Hiding content -----------------------------------------------------------

#[tokio::test]
async fn hidden_content_is_hidden_from_the_person_reported_and_only_them() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({
            "type": "CONTRIBUTION", "contribution": deal.repair, "action": "CLAIM",
            "note": "Done. Meet me at 12 Elm Street for the money.",
        }),
    )
    .await
    .ok();
    // Ana proposes an amendment, which Ben has not answered.
    let (repair, payment) = (deal.repair, deal.payment);
    let mut amended = fence_job(repair, payment);
    amended["terms"] = json!("Repair the back fence by Friday.");
    let amendment = app.send(&deal.ana, &deal.exchange, amended).await.ok();
    let amendment = amendment["exchange"]["open_revision"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let report = ben_reports(&app, &deal).await;

    let reply = resolve(
        &app,
        &staff,
        report,
        "CONTENT_HIDDEN",
        Some("Ben's address must not stay in front of her."),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    assert_eq!(
        audit(&app, report).await,
        [(
            "CONTENT_HIDDEN".to_owned(),
            Some(staff.id),
            Some(deal.ana.id),
            Some("Ben's address must not stay in front of her.".to_owned())
        )]
    );
    let status: String = sqlx::query_scalar("SELECT status FROM report WHERE id = $1")
        .bind(report)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(status, "ACTIONED");

    // Ana, who was reported, sees none of what was written.
    let hidden = "Hidden by review";
    let view = app.view(&deal.ana, &deal.exchange).await;
    assert_eq!(view["content_hidden"], true);
    assert_eq!(view["draft"], Value::Null);
    for revision in ["in_force_revision", "open_revision"] {
        let terms = &view[revision]["terms"];
        assert_eq!(terms["terms"], hidden, "{revision}");
        for contribution in terms["contributions"].as_array().unwrap() {
            assert_eq!(contribution["description"], hidden);
        }
        // Names and amounts stay, so the yup can be told apart and ended.
        assert_eq!(terms["party_a_name"], "Ana Ruiz");
        assert_eq!(terms["contributions"][1]["amount_minor"], 40000);
    }
    let history = app
        .get(
            &deal.ana,
            &format!("/v1/exchanges/{}/history", deal.exchange),
        )
        .await
        .ok();
    assert_eq!(history["content_hidden"], true);
    let text = history.to_string();
    assert!(!text.contains("Elm Street"), "{text}");
    let record = app
        .get(
            &deal.ana,
            &format!("/v1/exchanges/{}/record", deal.exchange),
        )
        .await
        .ok();
    assert_eq!(record["content_hidden"], true);
    let text = record.to_string();
    for written in [
        "Elm Street",
        "Repair the back fence",
        "Payment on completion",
    ] {
        assert!(!text.contains(written), "{written} in {text}");
    }
    assert!(text.contains(hidden));

    // She can no longer sign or send terms, and can still end it.
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({ "type": "WITHDRAW", "revision": amendment }),
    )
    .await
    .ok();
    app.send(&deal.ana, &deal.exchange, fence_job(repair, payment))
        .await
        .refused(StatusCode::CONFLICT, "CONTENT_HIDDEN");
    app.act(&deal.ana, &deal.exchange, repair, "RETRACT_CLAIM")
        .await
        .ok();

    // Ben, who reported, sees everything as before.
    let view = app.view(&deal.ben, &deal.exchange).await;
    assert_eq!(view["content_hidden"], false);
    assert_eq!(
        view["in_force_revision"]["terms"]["terms"],
        "Repair the back fence."
    );
    let history = app
        .get(
            &deal.ben,
            &format!("/v1/exchanges/{}/history", deal.exchange),
        )
        .await
        .ok();
    assert_eq!(history["content_hidden"], false);
    assert!(history.to_string().contains("Elm Street"));
    let record = app
        .get(
            &deal.ben,
            &format!("/v1/exchanges/{}/record", deal.exchange),
        )
        .await
        .ok();
    assert_eq!(record.get("content_hidden"), None);

    // In Ana's own language.
    sqlx::query("UPDATE account SET language = 'es' WHERE id = $1")
        .bind(deal.ana.id)
        .execute(&app.owner)
        .await
        .unwrap();
    let view = app.view(&deal.ana, &deal.exchange).await;
    assert_eq!(
        view["in_force_revision"]["terms"]["terms"],
        "Oculto por revisión"
    );

    // Listed for review, and shown again with a note.
    let listed = app.get(&staff, "/v1/staff/hidden").await.ok();
    assert!(listed.as_array().unwrap().iter().any(|entry| {
        entry["exchange_id"] == deal.exchange && entry["account_id"] == deal.ana.id.to_string()
    }));
    let body = json!({ "exchange_id": deal.exchange, "account_id": deal.ana.id, "note": "" });
    app.post(&staff, "/v1/staff/hidden/restore", body)
        .await
        .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
    let body = json!({
        "exchange_id": deal.exchange, "account_id": deal.ana.id, "note": "Mistaken."
    });
    let reply = app
        .post(&staff, "/v1/staff/hidden/restore", body.clone())
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    app.post(&staff, "/v1/staff/hidden/restore", body)
        .await
        .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
    let view = app.view(&deal.ana, &deal.exchange).await;
    assert_eq!(view["content_hidden"], false);
    assert_eq!(
        view["in_force_revision"]["terms"]["terms"],
        "Repair the back fence."
    );
    // A later look is a new event; the resolution stands as it was.
    let actions: Vec<String> = audit(&app, report)
        .await
        .into_iter()
        .map(|(action, ..)| action)
        .collect();
    assert_eq!(actions, ["CONTENT_HIDDEN", "CONTENT_RESTORED"]);
    let outcome: String = sqlx::query_scalar("SELECT outcome FROM report WHERE id = $1")
        .bind(report)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(outcome, "CONTENT_HIDDEN");
}

#[tokio::test]
async fn the_person_reported_cannot_sign_what_is_hidden_from_them() {
    let app = app().await;
    let staff = reviewer(&app).await;
    // Ana proposes; Ben joins and reports her before signing. Then Ben's
    // counterproposal waits for Ana, whose view is hidden.
    let deal = app.negotiating().await;
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
    let counter = app
        .send(
            &deal.ben,
            &deal.exchange,
            fence_job(deal.repair, deal.payment),
        )
        .await
        .ok();
    let counter = counter["exchange"]["open_revision"]["id"]
        .as_str()
        .unwrap()
        .to_owned();
    let report = ben_reports(&app, &deal).await;
    let reply = resolve(&app, &staff, report, "CONTENT_HIDDEN", Some("Hide it.")).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    app.command(&deal.ana, &deal.exchange, accept(&counter))
        .await
        .refused(StatusCode::CONFLICT, "CONTENT_HIDDEN");
    // Declining needs no reading, and ends it.
    let view = app
        .command(
            &deal.ana,
            &deal.exchange,
            json!({ "type": "DECLINE", "revision": counter }),
        )
        .await
        .ok();
    assert_eq!(view["state"], "CLOSED");
}

// ---- Suspending ----------------------------------------------------------------

/// Keeps the codes the service "sent".
#[derive(Default)]
struct Codes(Mutex<Vec<(String, String)>>);

impl CodeSender for Codes {
    fn send<'a>(&'a self, message: CodeMessage<'a>) -> SendFuture<'a> {
        Box::pin(async move {
            self.0
                .lock()
                .unwrap()
                .push((message.to.as_str().to_owned(), message.code.to_owned()));
            Ok(())
        })
    }
}

#[tokio::test]
async fn a_suspended_account_is_signed_out_and_kept_out_until_lifted() {
    let codes = Arc::new(Codes::default());
    let app = App::start_sending(DB, Rules::default(), codes.clone()).await;
    let staff = reviewer(&app).await;
    let deal = app.active().await;
    let report = ben_reports(&app, &deal).await;

    let reply = resolve(&app, &staff, report, "ACCOUNT_SUSPENDED", Some("Threats.")).await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);

    // Signed out everywhere.
    app.get(&deal.ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");
    let live: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM account_session WHERE account_id = $1 AND revoked_at IS NULL",
    )
    .bind(deal.ana.id)
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!(live, 0);
    // And kept out.
    let sign_in = |email: String| {
        let app = &app;
        let codes = codes.clone();
        async move {
            let reply = app
                .call(
                    None,
                    Method::POST,
                    "/v1/auth/codes",
                    Some(json!({ "identifier": email })),
                    &[],
                )
                .await;
            assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
            let code = codes
                .0
                .lock()
                .unwrap()
                .iter()
                .rev()
                .find(|(to, _)| *to == email)
                .map(|(_, code)| code.clone())
                .unwrap();
            app.call(
                None,
                Method::POST,
                "/v1/auth/sessions",
                Some(json!({ "identifier": email, "code": code, "delivery": "TOKEN" })),
                &[],
            )
            .await
        }
    };
    sign_in(deal.ana.email.clone())
        .await
        .refused(StatusCode::FORBIDDEN, "ACCOUNT_SUSPENDED");

    // Ben can still end the agreement.
    app.command(
        &deal.ben,
        &deal.exchange,
        json!({ "type": "REQUEST_CLOSE", "note": null }),
    )
    .await
    .ok();

    // Listed, with the note; lifted with a note of its own.
    let listed = app.get(&staff, "/v1/staff/suspensions").await.ok();
    let entry = listed
        .as_array()
        .unwrap()
        .iter()
        .find(|entry| entry["account_id"] == deal.ana.id.to_string())
        .expect("listed")
        .clone();
    assert_eq!(entry["note"], "Threats.");
    assert_eq!(entry["report_id"], report.to_string());
    let lift = format!("/v1/staff/suspensions/{}/lift", deal.ana.id);
    app.post(&staff, &lift, json!({ "note": " " }))
        .await
        .refused(StatusCode::UNPROCESSABLE_ENTITY, "INVALID_REQUEST");
    let reply = app
        .post(&staff, &lift, json!({ "note": "Appeal upheld." }))
        .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    app.post(&staff, &lift, json!({ "note": "Again." }))
        .await
        .refused(StatusCode::NOT_FOUND, "NOT_FOUND");

    // She can sign in again; the old session stays ended.
    assert_eq!(sign_in(deal.ana.email.clone()).await.status, StatusCode::OK);
    app.get(&deal.ana, "/v1/me")
        .await
        .refused(StatusCode::UNAUTHORIZED, "UNAUTHENTICATED");

    let actions: Vec<(String, Option<String>)> = audit(&app, report)
        .await
        .into_iter()
        .map(|(action, _, _, note)| (action, note))
        .collect();
    assert_eq!(
        actions,
        [
            ("ACCOUNT_SUSPENDED".to_owned(), Some("Threats.".to_owned())),
            (
                "SUSPENSION_LIFTED".to_owned(),
                Some("Appeal upheld.".to_owned())
            ),
        ]
    );
}

#[tokio::test]
async fn a_reviewer_cannot_suspend_themselves_and_both_outcomes_do_both() {
    let app = app().await;
    let staff = reviewer(&app).await;
    let other = reviewer(&app).await;
    let ben = app.user("Ben").await;
    // Ben reports the reviewer.
    let deal = app.active_between(staff.clone(), ben).await;
    let report = ben_reports(&app, &deal).await;

    resolve(&app, &staff, report, "ACCOUNT_SUSPENDED", Some("Not me."))
        .await
        .refused(StatusCode::CONFLICT, "ACTION_NOT_ALLOWED");
    assert!(audit(&app, report).await.is_empty(), "nothing was done");

    let reply = resolve(
        &app,
        &other,
        report,
        "CONTENT_HIDDEN_AND_ACCOUNT_SUSPENDED",
        Some("Both."),
    )
    .await;
    assert_eq!(reply.status, StatusCode::NO_CONTENT, "{}", reply.body);
    let actions: Vec<String> = audit(&app, report)
        .await
        .into_iter()
        .map(|(action, ..)| action)
        .collect();
    assert_eq!(actions, ["CONTENT_HIDDEN", "ACCOUNT_SUSPENDED"]);
    let status: String = sqlx::query_scalar("SELECT status FROM account WHERE id = $1")
        .bind(staff.id)
        .fetch_one(&app.owner)
        .await
        .unwrap();
    assert_eq!(status, "SUSPENDED");
    // A suspended reviewer is signed out, so is no reviewer any more.
    app.get(&staff, "/v1/staff/reports")
        .await
        .refused(StatusCode::NOT_FOUND, "NOT_FOUND");
}

// ---- The alert and the metric --------------------------------------------------

#[derive(Default)]
struct Mailbox(Mutex<Vec<Email>>);

impl EmailSender for Mailbox {
    fn send<'a>(&'a self, email: &'a Email) -> SendFuture<'a> {
        Box::pin(async move {
            self.0.lock().unwrap().push(email.clone());
            Ok(())
        })
    }
}

#[tokio::test]
async fn a_new_report_tells_every_reviewer_without_saying_what_it_is() {
    let app = app().await;
    let staff = reviewer(&app).await;
    // A reviewer with only a phone number cannot be emailed.
    let phone_only = app.user("Pat").await;
    make_staff(&app, &phone_only).await;
    sqlx::query("UPDATE account SET email = NULL, phone = '+15555550123' WHERE id = $1")
        .bind(phone_only.id)
        .execute(&app.owner)
        .await
        .unwrap();

    let queued = |account: Uuid| {
        let owner = app.owner.clone();
        async move {
            let count: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM outbox
                 WHERE recipient_account_id = $1 AND payload ->> 'staff' = 'REPORT_RECEIVED'",
            )
            .bind(account)
            .fetch_one(&owner)
            .await
            .unwrap();
            count
        }
    };

    let deal = app.active().await;
    ben_reports(&app, &deal).await;
    assert_eq!(queued(staff.id).await, 1);
    assert_eq!(queued(phone_only.id).await, 0);
    assert_eq!(queued(deal.ana.id).await, 0, "not to a party");

    // A second report before the first alert went is not a second alert.
    // (Other tests here file reports too, and every reviewer hears of
    // theirs, which this holds for as well.)
    let other = app.active().await;
    ben_reports(&app, &other).await;
    assert_eq!(queued(staff.id).await, 1);

    let mailbox = Arc::new(Mailbox::default());
    let delivery = Delivery {
        sender: mailbox.clone(),
        wording: Wording::embedded().unwrap(),
        web_origin: "https://app.test".to_owned(),
        rules: DeliveryRules::default(),
    };
    loop {
        let delivered = deliver_due(
            &app.db,
            &delivery,
            OffsetDateTime::now_utc() + Duration::seconds(5),
        )
        .await
        .unwrap();
        if delivered.is_empty() {
            break;
        }
    }
    let sent = mailbox.0.lock().unwrap().clone();
    let alerts: Vec<&Email> = sent
        .iter()
        .filter(|email| email.to == staff.email)
        .collect();
    assert!(!alerts.is_empty());
    for alert in alerts {
        assert_eq!(alert.subject, "A report is waiting for review");
        assert!(
            alert.body.contains("https://app.test/staff"),
            "{}",
            alert.body
        );
        for secret in ["threats", "HARASSMENT", &deal.exchange, "Ana", "Ben"] {
            assert!(!alert.body.contains(secret), "{secret} in {}", alert.body);
            assert!(!alert.subject.contains(secret));
        }
    }

    // A former reviewer is told of no more.
    review::revoke(&app.owner, &staff.email).await.unwrap();
    let before = queued(staff.id).await;
    let third = app.active().await;
    ben_reports(&app, &third).await;
    assert_eq!(queued(staff.id).await, before);
}

#[tokio::test]
async fn the_metrics_say_how_many_reports_wait_and_for_how_long() {
    let app = app().await;
    let deal = app.active().await;
    sqlx::query(
        "INSERT INTO report (subject_exchange_id, subject_account_id, reason, created_at)
         VALUES ($1::uuid, $2, 'SCAM', now() - interval '30 days')",
    )
    .bind(&deal.exchange)
    .bind(deal.ana.id)
    .execute(&app.owner)
    .await
    .unwrap();
    let mut text = Text::new();
    metrics::render_reports(&mut text, &app.db).await;
    let text = text.finish();
    let value = |name: &str| -> f64 {
        text.lines()
            .find_map(|line| line.strip_prefix(&format!("{name} ")))
            .unwrap_or_else(|| panic!("{name} in {text}"))
            .parse()
            .unwrap()
    };
    assert!(value("yuppers_reports_open") >= 1.0);
    assert!(value("yuppers_reports_oldest_open_age_seconds") >= 30.0 * 24.0 * 3600.0);
    assert!(text.contains("# TYPE yuppers_reports_open gauge"), "{text}");
}
