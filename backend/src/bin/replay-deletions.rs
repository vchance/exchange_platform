//! Applies a deletion log to a restored database (docs/operations.md,
//! "Restoring"; `scripts/replay-deletions.sh` runs it).
//!
//!     replay-deletions FILE
//!
//! Connects with `DATABASE_URL`, as the application role, so that it can do
//! nothing a person deleting their own account could not. Each account in
//! the log is deleted again through the service's own deletion
//! (`yuppers_backend::deletion::replay`); one already deleted, or one the
//! database never held, is reported and skipped, so replaying a file twice
//! changes nothing the second time. One suspended in the copy has its
//! suspension lifted, recorded in the review history as the owner's, and is
//! deleted in the same transaction. It prints a line for each account and a
//! count at the end, and exits with status 1 if any account is left
//! undeleted (a failure), 2 if the file cannot be read or is damaged, in
//! which case nothing is changed.

use std::process::ExitCode;

use anyhow::Context;
use yuppers_backend::domain::Rules;
use yuppers_backend::{db, deletion_log, telemetry};

#[tokio::main]
async fn main() -> anyhow::Result<ExitCode> {
    telemetry::init()?;
    dotenvy::dotenv().ok();

    let mut args = std::env::args().skip(1);
    let (Some(file), None) = (args.next(), args.next()) else {
        eprintln!("usage: replay-deletions FILE");
        return Ok(ExitCode::from(2));
    };
    let text = match std::fs::read_to_string(&file) {
        Ok(text) => text,
        Err(error) => {
            eprintln!("replay-deletions: cannot read {file}: {error}");
            return Ok(ExitCode::from(2));
        }
    };
    let entries = match deletion_log::parse(&text) {
        Ok(entries) => entries,
        Err(bad) => {
            eprintln!("replay-deletions: {file}, {bad}; nothing was changed");
            return Ok(ExitCode::from(2));
        }
    };

    let url = std::env::var("DATABASE_URL").context("set DATABASE_URL")?;
    let pool = db::pool(&url)?;
    println!("replaying {} deletions from {file}", entries.len());
    let summary = deletion_log::replay(&pool, &Rules::default(), &entries, |line| {
        println!("{line}")
    })
    .await;
    println!("{summary}");
    if summary.complete() {
        Ok(ExitCode::SUCCESS)
    } else {
        eprintln!(
            "replay-deletions: not every account in the log is deleted here; see above \
             (docs/operations.md, \"Restoring\")"
        );
        Ok(ExitCode::from(1))
    }
}
