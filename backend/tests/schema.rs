//! Checks that the database itself enforces the record model's rules
//! (DESIGN.md §3, §10, §13.2), independent of any service code.
//!
//! Needs PostgreSQL and the two connection strings from `.env`. Every test
//! works inside a transaction that is rolled back, so nothing is left behind.

use exchange_backend::db;
use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions};
use sqlx::types::Uuid;
use sqlx::{Connection, Transaction};
use tokio::sync::OnceCell;

const FOREIGN_KEY: &str = "23503";
const UNIQUE: &str = "23505";
const CHECK: &str = "23514";
const INSUFFICIENT_PRIVILEGE: &str = "42501";

const APPEND_ONLY: [&str; 5] = [
    "revision",
    "revision_attachment",
    "contribution_snapshot",
    "acceptance",
    "exchange_event",
];

fn env(name: &str) -> String {
    dotenvy::dotenv().ok();
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set; see .env.example"))
}

async fn connect(url_var: &str) -> PgPool {
    PgPoolOptions::new()
        .max_connections(2)
        .connect(&env(url_var))
        .await
        .unwrap_or_else(|error| panic!("cannot connect using {url_var}: {error}"))
}

/// Connects as the schema owner, applying migrations once per test run.
async fn owner() -> PgPool {
    static MIGRATED: OnceCell<()> = OnceCell::const_new();
    let pool = connect("MIGRATION_DATABASE_URL").await;
    MIGRATED
        .get_or_init(|| async { db::MIGRATOR.run(&pool).await.expect("migrations apply") })
        .await;
    pool
}

/// Opens a transaction as the application role, the way the api and worker
/// processes connect.
async fn app() -> Transaction<'static, sqlx::Postgres> {
    owner().await;
    connect("DATABASE_URL").await.begin().await.unwrap()
}

fn sqlstate(error: &sqlx::Error) -> String {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .map(|code| code.into_owned())
        .unwrap_or_else(|| panic!("not a database error: {error}"))
}

/// Runs a statement that the database must refuse, inside a savepoint so the
/// surrounding transaction stays usable, and checks the SQLSTATE.
macro_rules! refused {
    ($conn:expr, $code:expr, $query:expr) => {{
        let mut savepoint = $conn.begin().await.unwrap();
        let error = $query
            .execute(&mut *savepoint)
            .await
            .expect_err("the database should refuse this statement");
        savepoint.rollback().await.unwrap();
        assert_eq!(sqlstate(&error), $code, "{error}");
        error
    }};
}

/// An active exchange: two accounts, one accepted revision, an item owed by
/// A and a payment owed by B once the item is accepted.
struct Agreement {
    account_a: Uuid,
    account_b: Uuid,
    exchange: Uuid,
    revision: Uuid,
    item: Uuid,
    payment: Uuid,
}

async fn account(conn: &mut PgConnection) -> Uuid {
    sqlx::query_scalar(
        "INSERT INTO account (email, display_name, adult_confirmed_at)
         VALUES (gen_random_uuid() || '@example.test', 'Test', now())
         RETURNING id",
    )
    .fetch_one(conn)
    .await
    .unwrap()
}

async fn contribution(conn: &mut PgConnection, exchange: Uuid) -> Uuid {
    sqlx::query_scalar("INSERT INTO contribution (exchange_id) VALUES ($1) RETURNING id")
        .bind(exchange)
        .fetch_one(conn)
        .await
        .unwrap()
}

async fn agreement(conn: &mut PgConnection) -> Agreement {
    let account_a = account(conn).await;
    let account_b = account(conn).await;

    let exchange: Uuid = sqlx::query_scalar(
        "INSERT INTO exchange (display_code, timezone, created_by)
         VALUES (substr(gen_random_uuid()::text, 1, 8), 'America/New_York', $1)
         RETURNING id",
    )
    .bind(account_a)
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO participant (exchange_id, slot, account_id, display_name, alias)
         VALUES ($1, 'A', $2, 'Ana', 'Party A'), ($1, 'B', $3, 'Ben', 'Party B')",
    )
    .bind(exchange)
    .bind(account_a)
    .bind(account_b)
    .execute(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO invitation (exchange_id, token_hash, expires_at, claimed_by, claimed_at)
         VALUES ($1, sha256(gen_random_uuid()::text::bytea), now() + interval '14 days', $2, now())",
    )
    .bind(exchange)
    .bind(account_b)
    .execute(&mut *conn)
    .await
    .unwrap();

    let revision: Uuid = sqlx::query_scalar(
        "INSERT INTO revision (exchange_id, sequence, author_slot, terms, expires_at, content_hash,
                               party_a_name, party_b_name)
         VALUES ($1, 1, 'A', 'Fix the fence for $500.', now() + interval '14 days', sha256('r1'),
                 'Ana', 'Ben')
         RETURNING id",
    )
    .bind(exchange)
    .fetch_one(&mut *conn)
    .await
    .unwrap();

    sqlx::query("UPDATE exchange SET state = 'NEGOTIATING', open_revision_id = $2 WHERE id = $1")
        .bind(exchange)
        .bind(revision)
        .execute(&mut *conn)
        .await
        .unwrap();

    let item = contribution(conn, exchange).await;
    let payment = contribution(conn, exchange).await;

    // The payment is inserted first although it depends on the item: the
    // dependency is only checked once the whole revision is in place.
    sqlx::query(
        "INSERT INTO contribution_snapshot
            (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
             description, due_kind, due_after_contribution_id, amount_minor, currency,
             settlement_mode)
         VALUES ($1, $2, $3, 1, 'B', 'A', 'MONEY', 'Payment on completion',
                 'AFTER_CONTRIBUTION', $4, 50000, 'USD', 'OFF_PLATFORM')",
    )
    .bind(exchange)
    .bind(revision)
    .bind(payment)
    .bind(item)
    .execute(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO contribution_snapshot
            (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
             description, due_kind, due_date)
         VALUES ($1, $2, $3, 0, 'A', 'B', 'SERVICE', 'Repair the fence', 'DATE', '2026-11-01')",
    )
    .bind(exchange)
    .bind(revision)
    .bind(item)
    .execute(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO acceptance
            (exchange_id, revision_id, slot, account_id, content_hash, auth_method,
             authenticated_at, consent_language, consent_version)
         VALUES ($1, $2, 'A', $3, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1'),
                ($1, $2, 'B', $4, sha256('r1'), 'PHONE_OTP', now(), 'es', '1')",
    )
    .bind(exchange)
    .bind(revision)
    .bind(account_a)
    .bind(account_b)
    .execute(&mut *conn)
    .await
    .unwrap();

    sqlx::query(
        "UPDATE exchange
         SET state = 'ACTIVE', open_revision_id = NULL, in_force_revision_id = $2,
             version = version + 1
         WHERE id = $1",
    )
    .bind(exchange)
    .bind(revision)
    .execute(&mut *conn)
    .await
    .unwrap();

    Agreement {
        account_a,
        account_b,
        exchange,
        revision,
        item,
        payment,
    }
}

#[tokio::test]
async fn the_application_role_can_record_an_agreement_and_its_fulfillment() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    sqlx::query(
        "INSERT INTO exchange_event
            (exchange_id, sequence, type, actor_kind, actor_slot, contribution_id, revision_id)
         VALUES ($1, 1, 'CONTRIBUTION_CLAIMED', 'PARTICIPANT', 'A', $2, $3)",
    )
    .bind(a.exchange)
    .bind(a.item)
    .bind(a.revision)
    .execute(&mut *tx)
    .await
    .unwrap();

    sqlx::query("UPDATE contribution SET status = 'CLAIMED', last_event_seq = 1 WHERE id = $1")
        .bind(a.item)
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query("UPDATE exchange SET last_event_seq = 1, version = version + 1 WHERE id = $1")
        .bind(a.exchange)
        .execute(&mut *tx)
        .await
        .unwrap();

    sqlx::query(
        "INSERT INTO outbox (kind, recipient_account_id, exchange_id, event_sequence, payload)
         VALUES ('EMAIL', $2, $1, 1, '{}')",
    )
    .bind(a.exchange)
    .bind(a.account_b)
    .execute(&mut *tx)
    .await
    .unwrap();

    sqlx::query(
        "INSERT INTO idempotency_key (account_id, key, request_hash) VALUES ($1, 'k1', sha256('q'))",
    )
    .bind(a.account_a)
    .execute(&mut *tx)
    .await
    .unwrap();

    // Run the checks that are otherwise deferred to commit.
    sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *tx)
        .await
        .unwrap();

    let status: String = sqlx::query_scalar("SELECT status FROM contribution WHERE id = $1")
        .bind(a.payment)
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    assert_eq!(status, "PENDING");
}

#[tokio::test]
async fn the_application_role_cannot_rewrite_history() {
    let mut tx = app().await;

    for table in APPEND_ONLY {
        for statement in [
            format!("UPDATE {table} SET exchange_id = exchange_id"),
            format!("DELETE FROM {table}"),
            format!("TRUNCATE {table} CASCADE"),
        ] {
            let error = refused!(
                tx,
                INSUFFICIENT_PRIVILEGE,
                sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
            );
            assert!(
                error.to_string().contains("permission denied"),
                "{statement}: {error}"
            );
        }
    }
}

#[tokio::test]
async fn the_schema_owner_cannot_rewrite_history_either() {
    let mut tx = owner().await.begin().await.unwrap();

    for table in APPEND_ONLY {
        for statement in [
            format!("UPDATE {table} SET exchange_id = exchange_id"),
            format!("DELETE FROM {table}"),
        ] {
            let error = refused!(
                tx,
                INSUFFICIENT_PRIVILEGE,
                sqlx::query(sqlx::AssertSqlSafe(statement.clone()))
            );
            assert!(
                error.to_string().contains("append-only"),
                "{statement}: {error}"
            );
        }
    }

    // TRUNCATE is checked on a table nothing else references, so the trigger
    // is what answers and not a foreign-key rule.
    let error = refused!(
        tx,
        INSUFFICIENT_PRIVILEGE,
        sqlx::query("TRUNCATE revision_attachment")
    );
    assert!(error.to_string().contains("append-only"), "{error}");
}

#[tokio::test]
async fn an_acceptance_must_carry_the_hash_of_the_revision_it_signs() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    let second: Uuid = sqlx::query_scalar(
        "INSERT INTO revision (exchange_id, sequence, parent_revision_id, author_slot,
                               expires_at, content_hash, party_a_name, party_b_name)
         VALUES ($1, 2, $2, 'B', now() + interval '14 days', sha256('r2'), 'Ana', 'Ben')
         RETURNING id",
    )
    .bind(a.exchange)
    .bind(a.revision)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query(
            "INSERT INTO acceptance
                (exchange_id, revision_id, slot, account_id, content_hash, auth_method,
                 authenticated_at, consent_language, consent_version)
             VALUES ($1, $2, 'B', $3, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1')"
        )
        .bind(a.exchange)
        .bind(second)
        .bind(a.account_b)
    );
}

#[tokio::test]
async fn only_the_account_holding_a_slot_can_sign_for_it() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    let second: Uuid = sqlx::query_scalar(
        "INSERT INTO revision (exchange_id, sequence, author_slot, expires_at, content_hash,
                               party_a_name, party_b_name)
         VALUES ($1, 2, 'A', now() + interval '14 days', sha256('r2'), 'Ana', 'Ben')
         RETURNING id",
    )
    .bind(a.exchange)
    .fetch_one(&mut *tx)
    .await
    .unwrap();

    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query(
            "INSERT INTO acceptance
                (exchange_id, revision_id, slot, account_id, content_hash, auth_method,
                 authenticated_at, consent_language, consent_version)
             VALUES ($1, $2, 'A', $3, sha256('r2'), 'EMAIL_OTP', now(), 'en', '1')"
        )
        .bind(a.exchange)
        .bind(second)
        .bind(a.account_b)
    );
}

#[tokio::test]
async fn each_party_signs_a_revision_once() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    refused!(
        tx,
        UNIQUE,
        sqlx::query(
            "INSERT INTO acceptance
                (exchange_id, revision_id, slot, account_id, content_hash, auth_method,
                 authenticated_at, consent_language, consent_version)
             VALUES ($1, $2, 'A', $3, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1')"
        )
        .bind(a.exchange)
        .bind(a.revision)
        .bind(a.account_a)
    );
}

#[tokio::test]
async fn money_fields_belong_to_money_contributions_only() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;
    let extra = contribution(&mut tx, a.exchange).await;

    // An item with an amount.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO contribution_snapshot
                (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
                 description, due_kind, amount_minor, currency, settlement_mode)
             VALUES ($1, $2, $3, 2, 'A', 'B', 'ITEM', 'A ladder', 'ON_AGREEMENT',
                     1000, 'USD', 'OFF_PLATFORM')"
        )
        .bind(a.exchange)
        .bind(a.revision)
        .bind(extra)
    );

    // Money without an amount.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO contribution_snapshot
                (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
                 description, due_kind)
             VALUES ($1, $2, $3, 2, 'B', 'A', 'MONEY', 'Deposit', 'ON_AGREEMENT')"
        )
        .bind(a.exchange)
        .bind(a.revision)
        .bind(extra)
    );

    // Money in a currency other than the exchange's.
    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query(
            "INSERT INTO contribution_snapshot
                (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
                 description, due_kind, amount_minor, currency, settlement_mode)
             VALUES ($1, $2, $3, 2, 'B', 'A', 'MONEY', 'Deposit', 'ON_AGREEMENT',
                     1000, 'CAD', 'OFF_PLATFORM')"
        )
        .bind(a.exchange)
        .bind(a.revision)
        .bind(extra)
    );
}

#[tokio::test]
async fn a_contribution_can_only_wait_on_another_in_the_same_revision() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;
    let extra = contribution(&mut tx, a.exchange).await;

    // On itself.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO contribution_snapshot
                (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
                 description, due_kind, due_after_contribution_id)
             VALUES ($1, $2, $3, 2, 'A', 'B', 'TASK', 'Clean up', 'AFTER_CONTRIBUTION', $3)"
        )
        .bind(a.exchange)
        .bind(a.revision)
        .bind(extra)
    );

    // On a contribution that is not part of this revision. The insert is
    // allowed; the check runs when the revision is complete.
    let other = contribution(&mut tx, a.exchange).await;
    let mut savepoint = tx.begin().await.unwrap();
    sqlx::query(
        "INSERT INTO contribution_snapshot
            (exchange_id, revision_id, contribution_id, position, from_slot, to_slot, type,
             description, due_kind, due_after_contribution_id)
         VALUES ($1, $2, $3, 2, 'A', 'B', 'TASK', 'Clean up', 'AFTER_CONTRIBUTION', $4)",
    )
    .bind(a.exchange)
    .bind(a.revision)
    .bind(extra)
    .bind(other)
    .execute(&mut *savepoint)
    .await
    .unwrap();
    let error = sqlx::query("SET CONSTRAINTS ALL IMMEDIATE")
        .execute(&mut *savepoint)
        .await
        .expect_err("the dangling dependency should be refused");
    assert_eq!(sqlstate(&error), FOREIGN_KEY, "{error}");
    savepoint.rollback().await.unwrap();
}

#[tokio::test]
async fn an_exchange_has_one_claimable_invitation_at_a_time() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    // The agreement's own invitation was claimed, so a new one may be issued.
    sqlx::query(
        "INSERT INTO invitation (exchange_id, token_hash, expires_at)
         VALUES ($1, sha256(gen_random_uuid()::text::bytea), now() + interval '14 days')",
    )
    .bind(a.exchange)
    .execute(&mut *tx)
    .await
    .unwrap();

    refused!(
        tx,
        UNIQUE,
        sqlx::query(
            "INSERT INTO invitation (exchange_id, token_hash, expires_at)
             VALUES ($1, sha256(gen_random_uuid()::text::bytea), now() + interval '14 days')"
        )
        .bind(a.exchange)
    );
}

#[tokio::test]
async fn event_sequence_numbers_never_repeat_within_an_exchange() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    let insert = "INSERT INTO exchange_event (exchange_id, sequence, type, actor_kind)
                  VALUES ($1, 1, 'REMINDER_SENT', 'SYSTEM')";
    sqlx::query(insert)
        .bind(a.exchange)
        .execute(&mut *tx)
        .await
        .unwrap();
    refused!(tx, UNIQUE, sqlx::query(insert).bind(a.exchange));
}

#[tokio::test]
async fn any_well_formed_language_tag_is_stored_and_nothing_else() {
    let mut tx = app().await;
    let account = account(&mut tx).await;

    for tag in ["en", "es", "fr", "pt-BR", "zh-Hant", "fil"] {
        sqlx::query("UPDATE account SET language = $2 WHERE id = $1")
            .bind(account)
            .bind(tag)
            .execute(&mut *tx)
            .await
            .unwrap_or_else(|error| panic!("{tag}: {error}"));
    }
    for junk in ["", "EN", "english", "e", "en_US"] {
        refused!(
            tx,
            CHECK,
            sqlx::query("UPDATE account SET language = $2 WHERE id = $1")
                .bind(account)
                .bind(junk)
        );
    }
}

#[tokio::test]
async fn references_stay_inside_one_exchange() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;
    let b = agreement(&mut tx).await;

    // An event in one exchange about another exchange's contribution.
    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query(
            "INSERT INTO exchange_event
                (exchange_id, sequence, type, actor_kind, actor_slot, contribution_id)
             VALUES ($1, 1, 'CONTRIBUTION_CLAIMED', 'PARTICIPANT', 'A', $2)"
        )
        .bind(a.exchange)
        .bind(b.item)
    );

    // An exchange pointing at another exchange's revision.
    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query("UPDATE exchange SET in_force_revision_id = $2 WHERE id = $1")
            .bind(a.exchange)
            .bind(b.revision)
    );
}

#[tokio::test]
async fn exchange_state_and_outcome_must_agree() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    // Closed without an outcome.
    refused!(
        tx,
        CHECK,
        sqlx::query("UPDATE exchange SET state = 'CLOSED', closed_at = now() WHERE id = $1")
            .bind(a.exchange)
    );

    // Active with nothing in force.
    refused!(
        tx,
        CHECK,
        sqlx::query("UPDATE exchange SET in_force_revision_id = NULL WHERE id = $1")
            .bind(a.exchange)
    );

    // "Not agreed" although an agreement was in force.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "UPDATE exchange
             SET state = 'CLOSED', closed_outcome = 'NOT_AGREED', closed_at = now()
             WHERE id = $1"
        )
        .bind(a.exchange)
    );

    // A proper close.
    sqlx::query(
        "UPDATE exchange
         SET state = 'CLOSED', closed_outcome = 'COMPLETED', closed_at = now()
         WHERE id = $1",
    )
    .bind(a.exchange)
    .execute(&mut *tx)
    .await
    .unwrap();
}
