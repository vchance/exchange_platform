//! Notifications end to end: a change to an exchange queues the message for
//! the other party, and the worker's delivery sends each one once.
//!
//! Delivery acts on every queued message in the database, so the tests here
//! take turns, and each starts with an empty outbox.

mod common;

use std::collections::BTreeSet;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use axum::http::{Method, StatusCode};
use common::{App, Deal, User, accept};
use exchange_backend::auth::SendFuture;
use exchange_backend::exchanges::service::run_timers;
use exchange_backend::notifications::outbox::{Delivered, Delivery, DeliveryRules, deliver_due};
use exchange_backend::notifications::wording::Wording;
use exchange_backend::notifications::{Email, EmailSender};
use serde_json::{Value, json};
use time::{Duration, OffsetDateTime};
use tokio::sync::MutexGuard;
use uuid::Uuid;

const DATABASE: &str = "exchange_test_notifications";

static TURN: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

async fn clear_outbox(app: &App) {
    sqlx::query("DELETE FROM outbox")
        .execute(&app.db)
        .await
        .unwrap();
}

/// The guard keeps the other tests waiting until this one is done.
async fn app() -> (App, MutexGuard<'static, ()>) {
    let turn = TURN.lock().await;
    let app = App::start(DATABASE).await;
    clear_outbox(&app).await;
    (app, turn)
}

/// A deal already under way, with the messages from getting there discarded.
async fn active(app: &App) -> Deal {
    let deal = app.active().await;
    clear_outbox(app).await;
    deal
}

/// Stands in for an email provider: keeps what it was asked to send, and can
/// be told to fail or to be slow.
#[derive(Default)]
struct Provider {
    sent: Mutex<Vec<Email>>,
    attempts: AtomicUsize,
    /// How many sends fail before one succeeds.
    failures: AtomicUsize,
    delay: Option<std::time::Duration>,
    in_flight: AtomicUsize,
    /// The most sends that were ever under way at the same moment.
    most_at_once: AtomicUsize,
}

impl Provider {
    fn failing(failures: usize) -> Self {
        Self {
            failures: AtomicUsize::new(failures),
            ..Self::default()
        }
    }

    fn slow(delay: std::time::Duration) -> Self {
        Self {
            delay: Some(delay),
            ..Self::default()
        }
    }

    fn sent(&self) -> Vec<Email> {
        self.sent.lock().unwrap().clone()
    }

    fn attempts(&self) -> usize {
        self.attempts.load(Ordering::SeqCst)
    }
}

impl EmailSender for Provider {
    fn send<'a>(&'a self, email: &'a Email) -> SendFuture<'a> {
        Box::pin(async move {
            self.attempts.fetch_add(1, Ordering::SeqCst);
            let in_flight = self.in_flight.fetch_add(1, Ordering::SeqCst) + 1;
            self.most_at_once.fetch_max(in_flight, Ordering::SeqCst);
            if let Some(delay) = self.delay {
                tokio::time::sleep(delay).await;
            }
            self.in_flight.fetch_sub(1, Ordering::SeqCst);
            let failing = self
                .failures
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |left| {
                    left.checked_sub(1)
                })
                .is_ok();
            if failing {
                anyhow::bail!("the provider is down");
            }
            self.sent.lock().unwrap().push(email.clone());
            Ok(())
        })
    }
}

fn delivery(provider: &Arc<Provider>, rules: DeliveryRules) -> Delivery {
    Delivery {
        sender: provider.clone(),
        wording: Wording::embedded().unwrap(),
        web_origin: "https://app.test".to_owned(),
        rules,
    }
}

/// A moment by which everything queued so far is due.
fn soon() -> OffsetDateTime {
    OffsetDateTime::now_utc() + Duration::seconds(5)
}

/// Delivers everything due, through a provider that works.
async fn deliver(app: &App) -> (Delivered, Vec<Email>) {
    let provider = Arc::new(Provider::default());
    let delivered = deliver_due(
        &app.db,
        &delivery(&provider, DeliveryRules::default()),
        soon(),
    )
    .await
    .unwrap();
    (delivered, provider.sent())
}

/// What was queued for an exchange, oldest first, as
/// `recipient: NOTICE (event it is about)`.
async fn queued(app: &App, deal: &Deal) -> Vec<String> {
    let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
        "SELECT o.recipient_account_id, o.payload->>'notice', e.type
         FROM outbox o
         JOIN exchange_event e
           ON e.exchange_id = o.exchange_id AND e.sequence = o.event_sequence
         WHERE o.exchange_id = $1 AND o.kind = 'EMAIL'
         ORDER BY o.event_sequence, o.id",
    )
    .bind(deal.exchange.parse::<Uuid>().unwrap())
    .fetch_all(&app.owner)
    .await
    .unwrap();

    rows.into_iter()
        .map(|(recipient, notice, event)| {
            let name = if recipient == deal.ana.id {
                "Ana"
            } else if recipient == deal.ben.id {
                "Ben"
            } else {
                "someone else"
            };
            format!("{name}: {notice} ({event})")
        })
        .collect()
}

/// One queued row: attempts, last error, whether it is completed, and when
/// it is next due.
type Row = (i32, Option<String>, bool, OffsetDateTime);

async fn rows(app: &App, deal: &Deal) -> Vec<Row> {
    sqlx::query_as(
        "SELECT attempts, last_error, completed_at IS NOT NULL, available_at
         FROM outbox WHERE exchange_id = $1 ORDER BY id",
    )
    .bind(deal.exchange.parse::<Uuid>().unwrap())
    .fetch_all(&app.owner)
    .await
    .unwrap()
}

async fn only_row(app: &App, deal: &Deal) -> Row {
    let mut rows = rows(app, deal).await;
    assert_eq!(rows.len(), 1, "{rows:?}");
    rows.remove(0)
}

fn contribution(id: Uuid, action: &str, note: Option<&str>) -> Value {
    json!({ "type": "CONTRIBUTION", "contribution": id, "action": action, "note": note })
}

async fn claim(app: &App, deal: &Deal) {
    app.post(
        &deal.ben,
        "/v1/invitations/claim",
        json!({ "token": deal.invitation }),
    )
    .await
    .ok();
}

async fn display_code(app: &App, user: &User, exchange: &str) -> String {
    app.view(user, exchange).await["display_code"]
        .as_str()
        .unwrap()
        .to_owned()
}

#[tokio::test]
async fn each_party_is_told_what_the_other_did_and_never_their_own_action() {
    let (app, _turn) = app().await;

    // Ana proposes. She is not told about her own proposal, and nobody holds
    // the other slot yet.
    let deal = app.negotiating().await;
    assert_eq!(queued(&app, &deal).await, Vec::<String>::new());

    claim(&app, &deal).await;
    app.command(&deal.ben, &deal.exchange, accept(&deal.revision))
        .await
        .ok();
    app.command(
        &deal.ana,
        &deal.exchange,
        json!({ "type": "CONFIRM_COUNTERPARTY" }),
    )
    .await
    .ok();

    let note = "The gate still sticks.";
    for (who, what) in [
        (&deal.ana, contribution(deal.repair, "CLAIM", None)),
        (&deal.ben, contribution(deal.repair, "DISPUTE", Some(note))),
        (
            &deal.ana,
            contribution(deal.repair, "CLAIM", Some("Fixed.")),
        ),
        (&deal.ben, contribution(deal.repair, "CONFIRM", None)),
        (&deal.ben, contribution(deal.payment, "CLAIM", None)),
        (&deal.ana, contribution(deal.payment, "CONFIRM", None)),
    ] {
        app.command(who, &deal.exchange, what).await.ok();
    }
    assert_eq!(
        app.view(&deal.ana, &deal.exchange).await["closed_outcome"],
        "COMPLETED"
    );

    assert_eq!(
        queued(&app, &deal).await,
        [
            "Ana: INVITATION_CLAIMED_UNCONFIRMED (COUNTERPARTY_CLAIMED)",
            "Ana: ACCEPTANCE_WAITING (REVISION_ACCEPTED)",
            // Confirming Ben brought his waiting acceptance into force: two
            // events, one message.
            "Ben: AGREEMENT_IN_FORCE (AGREEMENT_IN_FORCE)",
            "Ben: DELIVERY_CLAIMED (CONTRIBUTION_CLAIMED)",
            "Ana: DISPUTE_OPENED (CONTRIBUTION_DISPUTED)",
            "Ben: DELIVERY_CLAIMED (CONTRIBUTION_CLAIMED)",
            "Ana: DELIVERY_CONFIRMED (CONTRIBUTION_CONFIRMED)",
            "Ana: DELIVERY_CLAIMED (CONTRIBUTION_CLAIMED)",
            // The last confirmation completed the exchange, and that is what
            // Ben is told, once.
            "Ben: CLOSED_COMPLETED (EXCHANGE_CLOSED)",
        ]
    );

    // Each goes to its recipient's address, links to the exchange, and says
    // nothing of what the two agreed or wrote.
    let (delivered, emails) = deliver(&app).await;
    assert_eq!(delivered.sent, 9);
    let code = display_code(&app, &deal.ana, &deal.exchange).await;
    let link = format!("https://app.test/exchanges/{}", deal.exchange);
    let to_ana = emails.iter().filter(|e| e.to == deal.ana.email).count();
    let to_ben = emails.iter().filter(|e| e.to == deal.ben.email).count();
    assert_eq!((to_ana, to_ben), (5, 4));
    for email in &emails {
        assert!(email.subject.contains(&code), "{}", email.subject);
        assert!(email.body.contains(&link), "{}", email.body);
        // The link is left out of the search: an exchange ID is random
        // letters and digits, and may spell anything.
        let text = format!("{}\n{}", email.subject, email.body)
            .replace(&link, "")
            .to_lowercase();
        for private in ["fence", "ruiz", "ortiz", "400", "payment", "gate", "fixed"] {
            assert!(!text.contains(private), "{private:?} leaked into {text}");
        }
    }
}

#[tokio::test]
async fn nothing_is_queued_for_a_slot_nobody_has_claimed() {
    let (app, _turn) = app().await;

    // Ana takes her offer back before anyone opens the link: she did it, and
    // there is no one else.
    let withdrawn = app.negotiating().await;
    app.command(
        &withdrawn.ana,
        &withdrawn.exchange,
        json!({ "type": "WITHDRAW", "revision": withdrawn.revision }),
    )
    .await
    .ok();
    assert_eq!(queued(&app, &withdrawn).await, Vec::<String>::new());

    // An offer nobody opened runs out. The timer's news is for both parties,
    // and only one of them exists.
    let unanswered = app.negotiating().await;
    let later = OffsetDateTime::now_utc() + Duration::days(15);
    run_timers(&app.db, &app.rules, later).await.unwrap();
    assert_eq!(
        queued(&app, &unanswered).await,
        ["Ana: CLOSED_EXPIRED (EXCHANGE_CLOSED)"]
    );

    let (unaddressed, strangers): (i64, i64) = sqlx::query_as(
        "SELECT count(*) FILTER (WHERE recipient_account_id IS NULL),
                count(*) FILTER (WHERE NOT EXISTS (
                    SELECT 1 FROM participant p
                    WHERE p.exchange_id = o.exchange_id
                      AND p.account_id = o.recipient_account_id))
         FROM outbox o",
    )
    .fetch_one(&app.owner)
    .await
    .unwrap();
    assert_eq!((unaddressed, strangers), (0, 0));

    // Ben, who never claimed the link, hears nothing.
    let (_, emails) = deliver(&app).await;
    assert!(emails.iter().all(|email| email.to != unanswered.ben.email));
    assert!(emails.iter().any(|email| email.to == unanswered.ana.email));
}

#[tokio::test]
async fn what_the_timers_do_is_told_to_both_parties() {
    let (app, _turn) = app().await;
    let now = OffsetDateTime::now_utc();

    let stuck = active(&app).await;
    let idle = active(&app).await;
    app.command(
        &stuck.ana,
        &stuck.exchange,
        json!({ "type": "REQUEST_CLOSE" }),
    )
    .await
    .ok();
    assert_eq!(
        queued(&app, &stuck).await,
        ["Ben: CLOSE_REQUESTED (CLOSE_REQUESTED)"]
    );

    run_timers(&app.db, &app.rules, now + Duration::days(8))
        .await
        .unwrap();
    assert_eq!(
        queued(&app, &stuck).await[1..],
        [
            "Ana: CLOSED_UNRESOLVED (EXCHANGE_CLOSED)",
            "Ben: CLOSED_UNRESOLVED (EXCHANGE_CLOSED)",
        ]
    );

    run_timers(&app.db, &app.rules, now + Duration::days(61))
        .await
        .unwrap();
    assert_eq!(
        queued(&app, &idle).await,
        [
            "Ana: INACTIVITY_PROMPTED (INACTIVITY_PROMPTED)",
            "Ben: INACTIVITY_PROMPTED (INACTIVITY_PROMPTED)",
        ]
    );
}

#[tokio::test]
async fn a_request_that_changes_nothing_queues_nothing() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;

    // Refused: the repair is Ana's to mark as delivered, not Ben's.
    app.act(&deal.ben, &deal.exchange, deal.repair, "CLAIM")
        .await
        .refused(StatusCode::FORBIDDEN, "WRONG_ACTOR");
    assert_eq!(queued(&app, &deal).await, Vec::<String>::new());

    // A retry of a request that already went through is not a second event.
    let version = app.view(&deal.ana, &deal.exchange).await["version"].clone();
    let body = json!({
        "expected_version": version,
        "command": contribution(deal.repair, "CLAIM", None),
    });
    for _ in 0..2 {
        app.call(
            Some(&deal.ana),
            Method::POST,
            &format!("/v1/exchanges/{}/commands", deal.exchange),
            Some(body.clone()),
            &[("idempotency-key", "claim-once")],
        )
        .await
        .ok();
    }
    assert_eq!(
        queued(&app, &deal).await,
        ["Ben: DELIVERY_CLAIMED (CONTRIBUTION_CLAIMED)"]
    );
}

#[tokio::test]
async fn an_account_that_cannot_be_emailed_is_not_emailed() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;

    // Queued while Ben had an address, which he then replaces with a phone
    // number before the worker gets to it.
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();
    sqlx::query("UPDATE account SET email = NULL, phone = $2 WHERE id = $1")
        .bind(deal.ben.id)
        .bind(format!("+1555{:07}", Uuid::new_v4().as_u128() % 10_000_000))
        .execute(&app.db)
        .await
        .unwrap();

    let (delivered, emails) = deliver(&app).await;
    assert_eq!((delivered.sent, delivered.dropped), (0, 1));
    assert_eq!(emails, []);
    let (attempts, reason, completed, _) = only_row(&app, &deal).await;
    assert_eq!((attempts, completed), (0, true), "closed without a try");
    assert!(reason.as_deref().unwrap().starts_with("not sent"));

    // From here on there is no address to queue anything for.
    app.act(&deal.ana, &deal.exchange, deal.repair, "RETRACT_CLAIM")
        .await
        .ok();
    assert_eq!(queued(&app, &deal).await.len(), 1);

    // Nor is a suspended account told anything.
    let other = active(&app).await;
    sqlx::query("UPDATE account SET status = 'SUSPENDED' WHERE id = $1")
        .bind(other.ben.id)
        .execute(&app.db)
        .await
        .unwrap();
    app.act(&other.ana, &other.exchange, other.repair, "CLAIM")
        .await
        .ok();
    assert_eq!(queued(&app, &other).await, Vec::<String>::new());
}

#[tokio::test]
async fn a_message_is_written_in_its_recipients_language() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;
    // Ben reads Spanish. Ana's preference is a language the product does not
    // have, so she gets the default.
    for (user, language) in [(&deal.ben, "es"), (&deal.ana, "tlh")] {
        sqlx::query("UPDATE account SET language = $2 WHERE id = $1")
            .bind(user.id)
            .bind(language)
            .execute(&app.db)
            .await
            .unwrap();
    }

    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();
    app.act(&deal.ben, &deal.exchange, deal.repair, "CONFIRM")
        .await
        .ok();

    let (_, emails) = deliver(&app).await;
    let code = display_code(&app, &deal.ana, &deal.exchange).await;
    let link = format!("https://app.test/exchanges/{}", deal.exchange);
    let [to_ben, to_ana] = &emails[..] else {
        panic!("two emails, got {emails:?}");
    };

    assert_eq!(to_ben.to, deal.ben.email);
    assert_eq!(
        to_ben.subject,
        format!("Algo se marcó como entregado ({code})")
    );
    assert!(
        to_ben
            .body
            .contains(&format!("Abre el intercambio: {link}"))
    );

    assert_eq!(to_ana.to, deal.ana.email);
    assert_eq!(to_ana.subject, format!("A delivery was confirmed ({code})"));
    assert!(to_ana.body.contains(&format!("Open the exchange: {link}")));
}

#[tokio::test]
async fn a_delivered_message_is_not_sent_again() {
    let (app, _turn) = app().await;
    let deal = app.active().await;
    assert_eq!(rows(&app, &deal).await.len(), 3);

    let provider = Arc::new(Provider::default());
    let delivery = delivery(&provider, DeliveryRules::default());
    assert_eq!(
        deliver_due(&app.db, &delivery, soon()).await.unwrap(),
        Delivered {
            sent: 3,
            ..Delivered::default()
        }
    );
    // Later passes, however late, find nothing to do.
    for at in [soon(), soon() + Duration::days(30)] {
        assert!(
            deliver_due(&app.db, &delivery, at)
                .await
                .unwrap()
                .is_empty()
        );
    }

    assert_eq!(provider.sent().len(), 3);
    for (attempts, error, completed, _) in rows(&app, &deal).await {
        assert_eq!((attempts, error, completed), (1, None, true));
    }
}

#[tokio::test]
async fn workers_draining_at_once_send_each_message_once() {
    let (app, _turn) = app().await;
    for _ in 0..4 {
        app.active().await;
    }
    let queued: Vec<i64> = sqlx::query_scalar("SELECT id FROM outbox ORDER BY id")
        .fetch_all(&app.owner)
        .await
        .unwrap();
    assert_eq!(queued.len(), 12);

    // Slow enough that every worker is mid-send while the others look for
    // work.
    let provider = Arc::new(Provider::slow(std::time::Duration::from_millis(20)));
    let delivery = delivery(&provider, DeliveryRules::default());
    let at = soon();
    let (first, second, third) = tokio::join!(
        deliver_due(&app.db, &delivery, at),
        deliver_due(&app.db, &delivery, at),
        deliver_due(&app.db, &delivery, at),
    );
    let workers = [first.unwrap(), second.unwrap(), third.unwrap()];

    let sent: Vec<i64> = provider.sent().iter().map(|e| e.reference).collect();
    assert_eq!(sent.len(), 12, "nothing was sent twice");
    assert_eq!(
        sent.iter().copied().collect::<BTreeSet<i64>>(),
        queued.into_iter().collect::<BTreeSet<i64>>(),
        "and nothing was missed"
    );
    assert_eq!(workers.iter().map(|w| w.sent).sum::<usize>(), 12);
    assert!(
        workers.iter().all(|worker| worker.sent > 0),
        "the work was shared: {workers:?}"
    );
    // A worker passes over a message another is sending instead of waiting
    // for it, so all three were sending at the same moment.
    assert_eq!(provider.most_at_once.load(Ordering::SeqCst), 3);
}

#[tokio::test]
async fn a_failing_send_is_retried_with_longer_waits_and_then_given_up_on() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();

    let provider = Arc::new(Provider::failing(usize::MAX));
    let rules = DeliveryRules {
        max_attempts: 3,
        retry_after: Duration::minutes(1),
        ..DeliveryRules::default()
    };
    let delivery = delivery(&provider, rules);
    let failed = Delivered {
        failed: 1,
        ..Delivered::default()
    };
    let start = soon();
    let minutes = |n: i64| start + Duration::minutes(n);
    let state = || async {
        let (attempts, error, completed, available_at) = only_row(&app, &deal).await;
        assert!(!completed);
        (attempts, error, available_at)
    };

    // The first try fails. The failure is recorded and the next try put off.
    assert_eq!(
        deliver_due(&app.db, &delivery, start).await.unwrap(),
        failed
    );
    let (attempts, error, available_at) = state().await;
    assert_eq!(attempts, 1);
    assert_eq!(error.as_deref(), Some("the provider is down"));
    assert!(available_at > start + Duration::seconds(59) && available_at <= minutes(1));

    // Not before its time.
    let early = start + Duration::seconds(30);
    assert!(
        deliver_due(&app.db, &delivery, early)
            .await
            .unwrap()
            .is_empty()
    );
    assert_eq!(provider.attempts(), 1);

    // The second failure waits twice as long.
    assert_eq!(
        deliver_due(&app.db, &delivery, minutes(1)).await.unwrap(),
        failed
    );
    let (attempts, _, available_at) = state().await;
    assert_eq!(attempts, 2);
    assert!(available_at > minutes(2) && available_at <= minutes(3));
    assert!(
        deliver_due(&app.db, &delivery, minutes(2))
            .await
            .unwrap()
            .is_empty()
    );

    // The third is the last.
    assert_eq!(
        deliver_due(&app.db, &delivery, minutes(3)).await.unwrap(),
        Delivered {
            failed: 1,
            given_up: 1,
            ..Delivered::default()
        }
    );
    let later = start + Duration::days(30);
    assert!(
        deliver_due(&app.db, &delivery, later)
            .await
            .unwrap()
            .is_empty()
    );

    // It stays as it was left, for someone to look at.
    let (attempts, error, _) = state().await;
    assert_eq!(attempts, 3);
    assert_eq!(error.as_deref(), Some("the provider is down"));
    assert_eq!(provider.attempts(), 3);
    assert_eq!(provider.sent(), []);
}

#[tokio::test]
async fn a_send_that_fails_and_then_works_is_delivered_once() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();

    let provider = Arc::new(Provider::failing(1));
    let delivery = delivery(&provider, DeliveryRules::default());
    let start = soon();
    assert_eq!(
        deliver_due(&app.db, &delivery, start).await.unwrap().failed,
        1
    );
    let retry = start + Duration::minutes(1);
    assert_eq!(
        deliver_due(&app.db, &delivery, retry).await.unwrap().sent,
        1
    );

    let (attempts, error, completed, _) = only_row(&app, &deal).await;
    assert_eq!((attempts, error, completed), (2, None, true));
    assert_eq!(provider.sent().len(), 1);
    assert_eq!(provider.sent()[0].to, deal.ben.email);
}

#[tokio::test]
async fn a_provider_that_does_not_answer_counts_as_a_failed_send() {
    let (app, _turn) = app().await;
    let deal = active(&app).await;
    app.act(&deal.ana, &deal.exchange, deal.repair, "CLAIM")
        .await
        .ok();

    let provider = Arc::new(Provider::slow(std::time::Duration::from_secs(5)));
    let rules = DeliveryRules {
        send_timeout: std::time::Duration::from_millis(50),
        ..DeliveryRules::default()
    };
    let delivered = deliver_due(&app.db, &delivery(&provider, rules), soon())
        .await
        .unwrap();
    assert_eq!(delivered.failed, 1);

    let (attempts, error, completed, _) = only_row(&app, &deal).await;
    assert_eq!((attempts, completed), (1, false));
    assert!(error.unwrap().starts_with("no answer within"));
    assert_eq!(provider.sent(), []);
}
