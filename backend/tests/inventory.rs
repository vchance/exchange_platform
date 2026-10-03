//! The inventory a restore is checked against: every grant the migrations
//! give the application role and every trigger they create, each with the
//! migration it comes with (`scripts/restore-inventory.txt`, read by
//! `scripts/restore.sh`; docs/operations.md, "Restoring").
//!
//! A backup holds the migrations of the release that made it, so a restored
//! database must hold exactly the lines of the migrations it has applied,
//! and nothing more. This test applies the migrations one at a time to a
//! database of its own and checks the inventory after each, so the file
//! cannot fall behind a migration or name the wrong one. On a difference it
//! prints the file as it should be.

use std::collections::BTreeMap;

use sqlx::migrate::Migrator;
use sqlx::postgres::{PgPool, PgPoolOptions};
use yuppers_backend::db;

/// The query restore.sh runs, with the role's name in place.
const QUERY: &str = include_str!("../../scripts/restore-inventory.sql");
/// What the migrations give: "VERSION LINE", one per line.
const EXPECTED: &str = include_str!("../../scripts/restore-inventory.txt");

fn env(name: &str) -> String {
    dotenvy::dotenv().ok();
    std::env::var(name).unwrap_or_else(|_| panic!("{name} must be set; see .env.example"))
}

/// The application role's name, from its connection string.
fn app_role() -> String {
    let url = env("DATABASE_URL");
    let rest = url.split_once("://").map_or(url.as_str(), |(_, rest)| rest);
    rest.split([':', '@'])
        .next()
        .expect("DATABASE_URL names a user")
        .to_owned()
}

/// The lines of the inventory a database holds now.
pub async fn inventory(db: &PgPool) -> Vec<String> {
    let query = QUERY.replace(":'app_role'", &format!("'{}'", app_role()));
    sqlx::query_scalar(sqlx::AssertSqlSafe(query))
        .fetch_all(db)
        .await
        .expect("the inventory query runs")
}

/// The lines the file says a database holds once `version` is applied.
fn expected(version: i64) -> Vec<String> {
    let mut lines: Vec<String> = EXPECTED
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .filter_map(|line| {
            let (from, rest) = line.split_once(' ').expect("VERSION LINE");
            let from: i64 = from.parse().expect("a migration version");
            (from <= version).then(|| rest.to_owned())
        })
        .collect();
    lines.sort();
    lines
}

/// A short tag for this checkout, as the other tests' databases carry.
fn checkout_tag() -> String {
    let hash = env!("CARGO_MANIFEST_DIR")
        .bytes()
        .fold(0x811c_9dc5_u32, |hash, byte| {
            (hash ^ u32::from(byte)).wrapping_mul(0x0100_0193)
        });
    format!("{hash:08x}")
}

#[tokio::test]
async fn the_inventory_names_every_grant_and_trigger_with_the_migration_it_comes_with() {
    let owner_url = env("MIGRATION_DATABASE_URL");
    let name = format!("yuppers_inventory_test_{}", checkout_tag());
    let admin = PgPoolOptions::new()
        .max_connections(1)
        .connect(&owner_url)
        .await
        .unwrap();
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .expect("the schema owner can drop its test database");
    sqlx::query(sqlx::AssertSqlSafe(format!("CREATE DATABASE {name}")))
        .execute(&admin)
        .await
        .expect("the schema owner can create databases");
    let (base, _) = owner_url.rsplit_once('/').unwrap();
    let db = PgPoolOptions::new()
        .max_connections(1)
        .connect(&format!("{base}/{name}"))
        .await
        .unwrap();

    // When each line first appears, to print the file as it should be.
    let mut first_seen: BTreeMap<String, i64> = BTreeMap::new();
    let mut wrong = Vec::new();
    let migrator: &Migrator = &db::MIGRATOR;
    for migration in migrator.iter() {
        migrator
            .run_to(migration.version, &db)
            .await
            .expect("each migration applies");
        let held = inventory(&db).await;
        for line in &held {
            first_seen.entry(line.clone()).or_insert(migration.version);
        }
        if held != expected(migration.version) {
            wrong.push(migration.version);
        }
    }
    db.close().await;
    sqlx::query(sqlx::AssertSqlSafe(format!(
        "DROP DATABASE IF EXISTS {name} WITH (FORCE)"
    )))
    .execute(&admin)
    .await
    .unwrap();

    if !wrong.is_empty() {
        let mut file = String::from(HEADER);
        for (line, version) in &first_seen {
            file.push_str(&format!("{version:04} {line}\n"));
        }
        panic!(
            "scripts/restore-inventory.txt is wrong after migrations {wrong:?}. As it should \
             be:\n{file}"
        );
    }
}

/// The comment at the top of the file.
const HEADER: &str = "\
# What each migration gives the application role, and the triggers it
# creates: the inventory scripts/restore.sh checks a restored database
# against. One line each, \"VERSION LINE\": the line holds from that
# migration on. LINE is as scripts/restore-inventory.sql prints it.
# backend/tests/inventory.rs checks this file after every migration, and
# prints it as it should be when it is not.
";

#[test]
fn the_file_reads_and_is_in_order() {
    let lines: Vec<&str> = EXPECTED
        .lines()
        .filter(|line| !line.trim().is_empty() && !line.starts_with('#'))
        .collect();
    assert!(!lines.is_empty());
    assert!(
        EXPECTED.starts_with(HEADER),
        "the header is as the test writes it"
    );
    let mut sorted = lines.clone();
    sorted.sort_by(|a, b| {
        a.split_once(' ')
            .unwrap()
            .1
            .cmp(b.split_once(' ').unwrap().1)
    });
    assert_eq!(lines, sorted, "sorted by line, as the test prints it");
}
