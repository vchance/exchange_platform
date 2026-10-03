//! Replaying the deletion log onto a restored database (docs/operations.md,
//! "Restoring"; the `replay-deletions` binary and
//! `scripts/replay-deletions.sh`).
//!
//! A backup restored brings back every account deleted since it was taken.
//! The log of deletions (migration 0015), exported to a file beside each
//! backup by `scripts/backup.sh` or from the live database by
//! `scripts/export-deletions.sh`, says which. Each one is applied again
//! through `deletion::replay`, the code a person's own deletion runs, so
//! that every rule runs again rather than a copy of them in SQL.
//!
//! The file is text, one deletion per line: the account's ID and the time of
//! the deletion in RFC 3339, separated by white space. Blank lines and lines
//! starting with `#` are ignored. Nothing else is in it.

use std::fmt;

use sqlx::PgPool;
use time::OffsetDateTime;
use time::format_description::well_known::Rfc3339;
use uuid::Uuid;

use crate::deletion::{self, Replayed};
use crate::domain::Rules;

/// One line of the log.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Entry {
    pub account: Uuid,
    pub deleted_at: OffsetDateTime,
}

/// A line that is not a deletion. The whole file is refused for it, before
/// anything is changed: a damaged log is not half replayed.
#[derive(Debug, PartialEq, Eq)]
pub struct BadLine {
    pub number: usize,
    pub reason: &'static str,
}

impl fmt::Display for BadLine {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.number, self.reason)
    }
}

impl std::error::Error for BadLine {}

/// Reads a log file's text.
pub fn parse(text: &str) -> Result<Vec<Entry>, BadLine> {
    let mut entries = Vec::new();
    for (index, line) in text.lines().enumerate() {
        let number = index + 1;
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let bad = |reason| BadLine { number, reason };
        let mut words = line.split_whitespace();
        let (Some(account), Some(deleted_at), None) = (words.next(), words.next(), words.next())
        else {
            return Err(bad("expected an account ID and a time"));
        };
        let account = Uuid::parse_str(account).map_err(|_| bad("not an account ID"))?;
        let deleted_at =
            OffsetDateTime::parse(deleted_at, &Rfc3339).map_err(|_| bad("not an RFC 3339 time"))?;
        entries.push(Entry {
            account,
            deleted_at,
        });
    }
    Ok(entries)
}

/// What a replay did, counted.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct Summary {
    /// Live in the restored database, and deleted again; those that were
    /// suspended there included.
    pub deleted: usize,
    /// Of those deleted again, the ones suspended in the restored database,
    /// whose suspension was lifted first (`deletion::replay`). Listed, so
    /// that whoever restores can tell the reviewers.
    pub lifted: Vec<Uuid>,
    pub already_deleted: usize,
    pub not_here: usize,
    /// Could not be deleted this time (the database refused or was busy).
    /// Replaying again tries them again.
    pub failed: Vec<Uuid>,
}

impl Summary {
    /// Every account in the log is deleted, or was never here.
    pub fn complete(&self) -> bool {
        self.failed.is_empty()
    }
}

impl fmt::Display for Summary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} deleted again ({} of them suspended here, their suspension lifted), {} already \
             deleted, {} not in this database, {} failed",
            self.deleted,
            self.lifted.len(),
            self.already_deleted,
            self.not_here,
            self.failed.len()
        )
    }
}

/// Applies each deletion in turn, in the order of the log, and reports each
/// one to `report` as it goes. A failure is reported and counted, and the
/// rest still run: every step can be repeated, so the remedy for a failure
/// is to replay the same file again.
pub async fn replay(
    db: &PgPool,
    rules: &Rules,
    entries: &[Entry],
    mut report: impl FnMut(&str),
) -> Summary {
    let mut summary = Summary::default();
    for entry in entries {
        let Entry {
            account,
            deleted_at,
        } = *entry;
        let when = deleted_at.format(&Rfc3339).unwrap_or_default();
        match deletion::replay(db, rules, account, deleted_at).await {
            Ok(Replayed::Deleted) => {
                summary.deleted += 1;
                report(&format!("{account}: deleted again (deleted {when})"));
            }
            Ok(Replayed::AlreadyDeleted) => {
                summary.already_deleted += 1;
                report(&format!("{account}: already deleted"));
            }
            Ok(Replayed::NotHere) => {
                summary.not_here += 1;
                report(&format!("{account}: not in this database"));
            }
            Ok(Replayed::SuspensionLiftedAndDeleted) => {
                summary.deleted += 1;
                summary.lifted.push(account);
                report(&format!(
                    "{account}: deleted again (deleted {when}); it was suspended here, and the \
                     suspension was lifted first, in the review history as the owner's"
                ));
            }
            Err(error) => {
                summary.failed.push(account);
                report(&format!(
                    "{account}: FAILED ({:?}); replay again to retry",
                    error.code
                ));
            }
        }
    }
    summary
}

#[cfg(test)]
mod tests {
    use time::macros::datetime;

    use super::*;

    const ANA: &str = "0b6f6a43-5f4e-4c64-9d4c-0f3f0c1f7a01";
    const BEN: &str = "6a8c2a5e-2f9b-4a8e-b0a7-8f1e2d3c4b02";

    #[test]
    fn a_log_is_an_id_and_a_time_per_line_with_comments_and_blank_lines_ignored() {
        let text = format!(
            "# Yuppers deletion log\n\n{ANA}\t2026-10-03T09:15:00.123456Z\n  {BEN}  \
             2026-10-03T10:00:00+02:00  \n# the end\n"
        );
        assert_eq!(
            parse(&text),
            Ok(vec![
                Entry {
                    account: ANA.parse().unwrap(),
                    deleted_at: datetime!(2026-10-03 09:15:00.123456 UTC),
                },
                Entry {
                    account: BEN.parse().unwrap(),
                    deleted_at: datetime!(2026-10-03 08:00:00 UTC),
                },
            ])
        );
    }

    #[test]
    fn an_empty_log_replays_nothing() {
        assert_eq!(parse(""), Ok(vec![]));
        assert_eq!(parse("# Yuppers deletion log\n"), Ok(vec![]));
    }

    #[test]
    fn a_damaged_line_refuses_the_whole_file_and_says_where() {
        let good = format!("{ANA} 2026-10-03T09:15:00Z\n");
        for (bad, reason) in [
            (format!("{ANA}\n"), "expected an account ID and a time"),
            (
                format!("{ANA} 2026-10-03T09:15:00Z extra\n"),
                "expected an account ID and a time",
            ),
            (
                "not-an-id 2026-10-03T09:15:00Z\n".to_owned(),
                "not an account ID",
            ),
            (format!("{ANA} 2026-10-03\n"), "not an RFC 3339 time"),
            (format!("{ANA} yesterday\n"), "not an RFC 3339 time"),
            // Half a line, as a file cut short would end.
            (
                format!("{}\n", &ANA[..20]),
                "expected an account ID and a time",
            ),
        ] {
            assert_eq!(
                parse(&format!("{good}{bad}")),
                Err(BadLine { number: 2, reason }),
                "{bad}"
            );
        }
    }

    #[test]
    fn a_summary_says_what_was_done_and_whether_anything_is_left() {
        let mut summary = Summary {
            deleted: 2,
            already_deleted: 1,
            not_here: 3,
            ..Summary::default()
        };
        assert!(summary.complete());
        assert_eq!(
            summary.to_string(),
            "2 deleted again (0 of them suspended here, their suspension lifted), 1 already \
             deleted, 3 not in this database, 0 failed"
        );
        // A suspension lifted to delete the account leaves nothing undone.
        summary.lifted.push(ANA.parse().unwrap());
        assert!(summary.complete());
        assert!(
            summary
                .to_string()
                .starts_with("2 deleted again (1 of them suspended here")
        );
        summary.failed.push(BEN.parse().unwrap());
        assert!(!summary.complete());
    }
}
