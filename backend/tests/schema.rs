//! Checks that the database itself enforces the record model's rules
//! (DESIGN.md §3, §10, §13.2), independent of any service code.
//!
//! Needs PostgreSQL and the two connection strings from `.env`. Every test
//! works inside a transaction that is rolled back, so nothing is left behind.

use sqlx::postgres::{PgConnection, PgPool, PgPoolOptions};
use sqlx::types::Uuid;
use sqlx::{Connection, Transaction};
use tokio::sync::OnceCell;
use yuppers_backend::db;

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

    // The initiator has confirmed who B is, without which nothing comes
    // into force.
    sqlx::query(
        "INSERT INTO participant
            (exchange_id, slot, account_id, display_name, alias, initiator_confirmed_at)
         VALUES ($1, 'A', $2, 'Ana', 'Party A', NULL), ($1, 'B', $3, 'Ben', 'Party B', now())",
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

// ---- Who holds a slot, and what a signature rests on ------------------------

/// A proposal from A, signed by sending it, with nobody yet in slot B.
struct Proposal {
    account_a: Uuid,
    exchange: Uuid,
    revision: Uuid,
}

async fn proposal(conn: &mut PgConnection) -> Proposal {
    let account_a = account(conn).await;
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
         VALUES ($1, 'A', $2, 'Ana', 'Party A'), ($1, 'B', NULL, 'Ben', 'Party B')",
    )
    .bind(exchange)
    .bind(account_a)
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
    let proposal = Proposal {
        account_a,
        exchange,
        revision,
    };
    sign(&proposal, "A", account_a)
        .execute(&mut *conn)
        .await
        .unwrap();
    proposal
}

/// `account` signing the proposal for `slot`. Whether it may is the test.
fn sign<'q>(
    proposal: &Proposal,
    slot: &'q str,
    account: Uuid,
) -> sqlx::query::Query<'q, sqlx::Postgres, sqlx::postgres::PgArguments> {
    sqlx::query(
        "INSERT INTO acceptance
            (exchange_id, revision_id, slot, account_id, content_hash, auth_method,
             authenticated_at, consent_language, consent_version)
         VALUES ($1, $2, $3, $4, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1')",
    )
    .bind(proposal.exchange)
    .bind(proposal.revision)
    .bind(slot)
    .bind(account)
}

/// Puts `account` in slot B, or takes whoever is there out of it.
async fn set_invited_party(
    conn: &mut PgConnection,
    exchange: Uuid,
    account: Option<Uuid>,
) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE participant SET account_id = $2 WHERE exchange_id = $1 AND slot = 'B'")
        .bind(exchange)
        .bind(account)
        .execute(conn)
        .await
        .map(|_| ())
}

/// slot, holding, account, whether it has ended; in that order.
async fn holdings(conn: &mut PgConnection, exchange: Uuid) -> Vec<(String, i32, Uuid, bool)> {
    sqlx::query_as(
        "SELECT slot, holding, account_id, ended_at IS NOT NULL FROM slot_holding
         WHERE exchange_id = $1 ORDER BY slot, holding",
    )
    .bind(exchange)
    .fetch_all(conn)
    .await
    .unwrap()
}

/// The holding each signature on the proposal was made under, by slot.
async fn signed_under(conn: &mut PgConnection, proposal: &Proposal) -> Vec<(String, i32, Uuid)> {
    sqlx::query_as(
        "SELECT slot, holding, account_id FROM acceptance
         WHERE revision_id = $1 ORDER BY slot, holding",
    )
    .bind(proposal.revision)
    .fetch_all(conn)
    .await
    .unwrap()
}

#[tokio::test]
async fn the_database_keeps_who_has_held_each_slot() {
    let mut tx = app().await;
    let p = proposal(&mut tx).await;
    let (stranger, ben) = (account(&mut tx).await, account(&mut tx).await);
    let a = ("A".to_owned(), 1, p.account_a, false);
    assert_eq!(
        holdings(&mut tx, p.exchange).await,
        std::slice::from_ref(&a)
    );

    // A stranger takes the invited party's place, is taken out again, and
    // the person meant takes it. Every one of them stays on record.
    set_invited_party(&mut tx, p.exchange, Some(stranger))
        .await
        .unwrap();
    set_invited_party(&mut tx, p.exchange, None).await.unwrap();
    set_invited_party(&mut tx, p.exchange, Some(ben))
        .await
        .unwrap();
    assert_eq!(
        holdings(&mut tx, p.exchange).await,
        [
            a.clone(),
            ("B".to_owned(), 1, stranger, true),
            ("B".to_owned(), 2, ben, false),
        ]
    );

    // Coming back is a new holding, not the old one resumed.
    set_invited_party(&mut tx, p.exchange, None).await.unwrap();
    set_invited_party(&mut tx, p.exchange, Some(stranger))
        .await
        .unwrap();
    assert_eq!(
        holdings(&mut tx, p.exchange).await[1..],
        [
            ("B".to_owned(), 1, stranger, true),
            ("B".to_owned(), 2, ben, true),
            ("B".to_owned(), 3, stranger, false),
        ]
    );

    // The application can read that record and cannot write it.
    for statement in [
        "INSERT INTO slot_holding (exchange_id, slot, holding, account_id)
         SELECT exchange_id, slot, 9, account_id FROM slot_holding LIMIT 1",
        "UPDATE slot_holding SET ended_at = now()",
        "UPDATE slot_holding SET ended_at = NULL",
        "DELETE FROM slot_holding",
        "TRUNCATE slot_holding CASCADE",
    ] {
        let error = refused!(tx, INSUFFICIENT_PRIVILEGE, sqlx::query(statement));
        assert!(
            error.to_string().contains("permission denied"),
            "{statement}: {error}"
        );
    }
}

#[tokio::test]
async fn the_record_of_who_held_a_slot_cannot_be_written_by_hand_whoever_asks() {
    let mut tx = owner().await.begin().await.unwrap();
    let p = proposal(&mut tx).await;
    let stranger = account(&mut tx).await;
    set_invited_party(&mut tx, p.exchange, Some(stranger))
        .await
        .unwrap();
    set_invited_party(&mut tx, p.exchange, None).await.unwrap();

    // Even the schema owner cannot reopen a holding, move its end, give it
    // to someone else, end one whose holder is still in the slot, remove
    // one, or add one the participant row never had.
    for statement in [
        "UPDATE slot_holding SET ended_at = NULL WHERE exchange_id = $1 AND slot = 'B'",
        "UPDATE slot_holding SET ended_at = now() + interval '1 day'
         WHERE exchange_id = $1 AND slot = 'B'",
        "UPDATE slot_holding SET account_id = (SELECT created_by FROM exchange WHERE id = $1)
         WHERE exchange_id = $1 AND slot = 'B'",
        "UPDATE slot_holding SET ended_at = now() WHERE exchange_id = $1 AND slot = 'A'",
        "DELETE FROM slot_holding WHERE exchange_id = $1",
        "INSERT INTO slot_holding (exchange_id, slot, holding, account_id)
         SELECT $1, 'B', 2, id FROM account ORDER BY created_at DESC LIMIT 1",
        "INSERT INTO slot_holding (exchange_id, slot, holding, account_id, ended_at)
         SELECT $1, 'A', 2, created_by, now() FROM exchange WHERE id = $1",
    ] {
        let error = refused!(
            tx,
            INSUFFICIENT_PRIVILEGE,
            sqlx::query(statement).bind(p.exchange)
        );
        assert!(
            error.to_string().contains("follows the participant row"),
            "{statement}: {error}"
        );
    }
    let error = refused!(
        tx,
        INSUFFICIENT_PRIVILEGE,
        sqlx::query("TRUNCATE slot_holding CASCADE")
    );
    assert!(
        error.to_string().contains("TRUNCATE is not allowed"),
        "{error}"
    );
}

#[tokio::test]
async fn only_an_invited_party_the_initiator_has_not_confirmed_can_be_taken_out() {
    let mut tx = app().await;
    let p = proposal(&mut tx).await;
    let (ben, carla) = (account(&mut tx).await, account(&mut tx).await);
    set_invited_party(&mut tx, p.exchange, Some(ben))
        .await
        .unwrap();

    // Nobody takes a place that someone is in.
    refused!(
        tx,
        CHECK,
        sqlx::query("UPDATE participant SET account_id = $2 WHERE exchange_id = $1 AND slot = 'B'")
            .bind(p.exchange)
            .bind(carla)
    );
    // The initiator is never taken out.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "UPDATE participant SET account_id = NULL WHERE exchange_id = $1 AND slot = 'A'"
        )
        .bind(p.exchange)
    );
    // Only someone in the place can be confirmed.
    let empty = proposal(&mut tx).await;
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "UPDATE participant SET initiator_confirmed_at = now()
             WHERE exchange_id = $1 AND slot = 'B'"
        )
        .bind(empty.exchange)
    );

    // Once confirmed, the invited party is a party for good: they cannot be
    // taken out, and the confirmation cannot be taken back to allow it.
    sqlx::query(
        "UPDATE participant SET initiator_confirmed_at = now()
         WHERE exchange_id = $1 AND slot = 'B'",
    )
    .bind(p.exchange)
    .execute(&mut *tx)
    .await
    .unwrap();
    for statement in [
        "UPDATE participant SET account_id = NULL WHERE exchange_id = $1 AND slot = 'B'",
        "UPDATE participant SET account_id = NULL, initiator_confirmed_at = NULL
         WHERE exchange_id = $1 AND slot = 'B'",
        "UPDATE participant SET initiator_confirmed_at = NULL
         WHERE exchange_id = $1 AND slot = 'B'",
    ] {
        refused!(tx, CHECK, sqlx::query(statement).bind(p.exchange));
    }
    assert_eq!(
        holdings(&mut tx, p.exchange).await[1],
        ("B".to_owned(), 1, ben, false)
    );
}

#[tokio::test]
async fn a_signature_belongs_to_whoever_held_the_slot_when_it_was_made() {
    let mut tx = app().await;
    let p = proposal(&mut tx).await;
    let (stranger, ben) = (account(&mut tx).await, account(&mut tx).await);

    // Nobody is in the invited party's place, so nobody can sign for it.
    refused!(tx, FOREIGN_KEY, sign(&p, "B", stranger));

    // A stranger takes the place and signs.
    set_invited_party(&mut tx, p.exchange, Some(stranger))
        .await
        .unwrap();
    refused!(tx, FOREIGN_KEY, sign(&p, "B", ben));
    sign(&p, "B", stranger).execute(&mut *tx).await.unwrap();
    refused!(tx, UNIQUE, sign(&p, "B", stranger));

    // The stranger is taken out. Their signature stays where it was, and
    // they can add no other: not as themselves, and not by naming the
    // holding they once had.
    set_invited_party(&mut tx, p.exchange, None).await.unwrap();
    refused!(tx, FOREIGN_KEY, sign(&p, "B", stranger));
    set_invited_party(&mut tx, p.exchange, Some(ben))
        .await
        .unwrap();
    refused!(tx, FOREIGN_KEY, sign(&p, "B", stranger));
    refused!(
        tx,
        FOREIGN_KEY,
        sqlx::query(
            "INSERT INTO acceptance
                (exchange_id, revision_id, slot, holding, account_id, content_hash, auth_method,
                 authenticated_at, consent_language, consent_version)
             VALUES ($1, $2, 'B', 1, $3, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1')"
        )
        .bind(p.exchange)
        .bind(p.revision)
        .bind(stranger)
    );

    // The person now in the place signs the same revision in their own
    // right, whatever holding the insert names.
    sqlx::query(
        "INSERT INTO acceptance
            (exchange_id, revision_id, slot, holding, account_id, content_hash, auth_method,
             authenticated_at, consent_language, consent_version)
         VALUES ($1, $2, 'B', 1, $3, sha256('r1'), 'EMAIL_OTP', now(), 'en', '1')",
    )
    .bind(p.exchange)
    .bind(p.revision)
    .bind(ben)
    .execute(&mut *tx)
    .await
    .unwrap();
    assert_eq!(
        signed_under(&mut tx, &p).await,
        [
            ("A".to_owned(), 1, p.account_a),
            ("B".to_owned(), 1, stranger),
            ("B".to_owned(), 2, ben),
        ]
    );
}

#[tokio::test]
async fn a_revision_comes_into_force_only_on_the_signatures_of_those_who_hold_the_slots() {
    let mut tx = app().await;
    let p = proposal(&mut tx).await;
    let (stranger, ben) = (account(&mut tx).await, account(&mut tx).await);
    let in_force = "UPDATE exchange SET state = 'ACTIVE', open_revision_id = NULL,
                    in_force_revision_id = $2 WHERE id = $1";
    let confirm = "UPDATE participant SET initiator_confirmed_at = now()
                   WHERE exchange_id = $1 AND slot = 'B'";
    macro_rules! not_in_force {
        ($why:expr) => {
            let error = refused!(
                tx,
                CHECK,
                sqlx::query(in_force).bind(p.exchange).bind(p.revision)
            );
            assert!(
                error.to_string().contains("comes into force only"),
                "{}: {error}",
                $why
            );
        };
    }

    not_in_force!("only the initiator has signed");

    // A stranger takes the place and signs. The initiator has not confirmed
    // them, so it binds nobody.
    set_invited_party(&mut tx, p.exchange, Some(stranger))
        .await
        .unwrap();
    sign(&p, "B", stranger).execute(&mut *tx).await.unwrap();
    not_in_force!("the signer is not confirmed");

    // The stranger is taken out and the person meant comes in and is
    // confirmed. Two signatures are on the revision, one for each slot, and
    // one of them is nobody's any more.
    set_invited_party(&mut tx, p.exchange, None).await.unwrap();
    set_invited_party(&mut tx, p.exchange, Some(ben))
        .await
        .unwrap();
    sqlx::query(confirm)
        .bind(p.exchange)
        .execute(&mut *tx)
        .await
        .unwrap();
    not_in_force!("the only signature for the invited party is a removed claimant's");

    // The person in the place signs, and it comes into force.
    sign(&p, "B", ben).execute(&mut *tx).await.unwrap();
    sqlx::query(in_force)
        .bind(p.exchange)
        .bind(p.revision)
        .execute(&mut *tx)
        .await
        .unwrap();
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
async fn oversized_text_cannot_be_stored_whatever_the_service_allows() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;

    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO revision (exchange_id, sequence, author_slot, terms, expires_at,
                                   content_hash, party_a_name, party_b_name)
             VALUES ($1, 2, 'A', repeat('x', 100001), now() + interval '14 days',
                     sha256('r2'), 'Ana', 'Ben')"
        )
        .bind(a.exchange)
    );
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO exchange_event (exchange_id, sequence, type, actor_kind, note)
             VALUES ($1, 1, 'STATEMENT_ADDED', 'SYSTEM', repeat('x', 10001))"
        )
        .bind(a.exchange)
    );
    refused!(
        tx,
        CHECK,
        sqlx::query("UPDATE account SET display_name = repeat('x', 1001) WHERE id = $1")
            .bind(a.account_a)
    );
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

#[tokio::test]
async fn a_reminder_is_recorded_once_and_then_neither_changed_nor_removed() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;
    let b = agreement(&mut tx).await;
    let record = |exchange: Uuid, contribution: Uuid, kind: &'static str, due: &'static str| {
        sqlx::query(
            "INSERT INTO contribution_reminder (exchange_id, contribution_id, kind, due_date)
             VALUES ($1, $2, $3, $4::date)",
        )
        .bind(exchange)
        .bind(contribution)
        .bind(kind)
        .bind(due)
    };

    record(a.exchange, a.item, "DUE_SOON", "2026-11-01")
        .execute(&mut *tx)
        .await
        .unwrap();
    // The same reminder about the same date, again.
    refused!(
        tx,
        UNIQUE,
        record(a.exchange, a.item, "DUE_SOON", "2026-11-01")
    );
    // The other kind, and the same kind about a date the contribution was
    // moved to, are different reminders.
    for (kind, due) in [("OVERDUE", "2026-11-01"), ("DUE_SOON", "2026-11-20")] {
        record(a.exchange, a.item, kind, due)
            .execute(&mut *tx)
            .await
            .unwrap();
    }

    refused!(tx, CHECK, record(a.exchange, a.item, "LATE", "2026-11-01"));
    // Another exchange's contribution.
    refused!(
        tx,
        FOREIGN_KEY,
        record(a.exchange, b.item, "OVERDUE", "2026-11-01")
    );

    // The service can only add: a row changed or removed would be a
    // reminder that could go out again.
    for statement in [
        "UPDATE contribution_reminder SET due_date = due_date + 1",
        "DELETE FROM contribution_reminder",
        "TRUNCATE contribution_reminder",
    ] {
        let error = refused!(tx, INSUFFICIENT_PRIVILEGE, sqlx::query(statement));
        assert!(
            error.to_string().contains("permission denied"),
            "{statement}: {error}"
        );
    }
}

#[tokio::test]
async fn sign_in_limits_are_counted_by_keyed_hash_and_the_service_can_forget_them() {
    let mut tx = app().await;
    let subject = Uuid::new_v4().as_bytes().repeat(2);
    let count = |scope: &'static str, subject: Vec<u8>| {
        sqlx::query(
            "INSERT INTO sign_in_limit (scope, subject, window_start, count)
             VALUES ($1, $2, date_trunc('hour', now()), 1)",
        )
        .bind(scope)
        .bind(subject)
    };

    count("code-requests-by-address", subject.clone())
        .execute(&mut *tx)
        .await
        .unwrap();
    // One row per thing counted and window.
    refused!(
        tx,
        UNIQUE,
        count("code-requests-by-address", subject.clone())
    );
    // Only the kinds of count the service keeps, and only a hash: an address
    // or identifier in the clear does not fit.
    refused!(tx, CHECK, count("guesses", subject.clone()));
    refused!(
        tx,
        CHECK,
        count("failed-guesses-by-identifier", b"ana@example.com".to_vec())
    );
    // A one-time code says what it was sent for.
    refused!(
        tx,
        CHECK,
        sqlx::query(
            "INSERT INTO one_time_code (identifier, purpose, code_hash, expires_at)
             VALUES ('ana@example.com', 'anything', $1, now())",
        )
        .bind(subject.clone())
    );

    // The service adds to counts and removes old ones.
    sqlx::query("UPDATE sign_in_limit SET count = count + 1 WHERE subject = $1")
        .bind(subject.clone())
        .execute(&mut *tx)
        .await
        .unwrap();
    sqlx::query("DELETE FROM sign_in_limit WHERE subject = $1")
        .bind(subject)
        .execute(&mut *tx)
        .await
        .unwrap();
}

#[tokio::test]
async fn a_wallet_pass_and_its_devices_hold_only_what_the_service_needs() {
    let mut tx = app().await;
    let a = agreement(&mut tx).await;
    let pass = |platform: &'static str, serial: &'static str, token: Option<Vec<u8>>| {
        sqlx::query(
            "INSERT INTO wallet_pass (account_id, exchange_id, platform, external_id, auth_token_hash)
             VALUES ($1, $2, $3, $4, $5)",
        )
        .bind(a.account_b)
        .bind(a.exchange)
        .bind(platform)
        .bind(serial)
        .bind(token)
    };
    let hash = Uuid::new_v4().as_bytes().repeat(2);

    pass("APPLE", "0123abcd", Some(hash.clone()))
        .execute(&mut *tx)
        .await
        .unwrap();
    // One pass per person, exchange and platform, and serials never repeat.
    refused!(tx, UNIQUE, pass("APPLE", "4567ef", Some(hash.clone())));
    pass("GOOGLE", "89ab", None)
        .execute(&mut *tx)
        .await
        .unwrap();
    // Only a hash, only for Apple, and only serials both platforms accept.
    refused!(tx, CHECK, pass("APPLE", "x1", Some(b"the-token".to_vec())));
    refused!(tx, CHECK, pass("GOOGLE", "x2", Some(hash.clone())));
    refused!(tx, CHECK, pass("APPLE", "has/slash", None));
    refused!(tx, CHECK, pass("PAPER", "x3", None));

    let id: Uuid = sqlx::query_scalar("SELECT id FROM wallet_pass WHERE external_id = '0123abcd'")
        .fetch_one(&mut *tx)
        .await
        .unwrap();
    let register = |device: String, token: &'static str| {
        sqlx::query(
            "INSERT INTO wallet_device_registration (wallet_pass_id, device_library_id, push_token)
             VALUES ($1, $2, $3)",
        )
        .bind(id)
        .bind(device)
        .bind(token)
    };
    register("device-1".to_owned(), "00ff")
        .execute(&mut *tx)
        .await
        .unwrap();
    refused!(tx, UNIQUE, register("device-1".to_owned(), "00ff"));
    refused!(tx, CHECK, register("d".repeat(129), "00ff"));
    refused!(tx, CHECK, register("device-2".to_owned(), ""));

    // The service marks, updates and revokes passes, and forgets devices;
    // it never deletes a pass.
    for statement in [
        "UPDATE wallet_pass SET update_status = 'PENDING', mark_seq = mark_seq + 1",
        "UPDATE wallet_pass SET voided_at = now()",
        "UPDATE wallet_device_registration SET push_token = 'abcd'",
        "DELETE FROM wallet_device_registration",
    ] {
        sqlx::query(statement).execute(&mut *tx).await.unwrap();
    }
    refused!(
        tx,
        INSUFFICIENT_PRIVILEGE,
        sqlx::query("DELETE FROM wallet_pass")
    );
    refused!(
        tx,
        CHECK,
        sqlx::query("UPDATE wallet_pass SET update_status = 'SENT'")
    );
}
