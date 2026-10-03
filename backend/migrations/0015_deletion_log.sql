-- The deletion log (docs/operations.md, "Restoring"): which accounts were
-- deleted, and when, so that deletions made after a backup can be applied
-- again to a database restored from it. An account ID and a time, nothing
-- else: no address, no name, nothing that says who the person was.
--
-- A row is written in the transaction that deletes the account
-- (`deletion::attempt`), so the log and the deletions never disagree.
-- `scripts/backup.sh` exports the log beside each backup, and
-- `scripts/replay-deletions.sh` applies it to a restored copy.
CREATE TABLE deletion_log (
    account_id uuid PRIMARY KEY REFERENCES account,
    deleted_at timestamptz NOT NULL DEFAULT now()
);

-- Accounts deleted before the log existed: the time they were deleted was
-- not kept, so they carry the time this migration ran, which is after it.
-- Replaying only needs to know that they are deleted.
INSERT INTO deletion_log (account_id, deleted_at)
SELECT id, now() FROM account WHERE status = 'DELETED';

-- The service adds to the log and reads it; it never changes or removes a
-- row.
GRANT SELECT, INSERT ON deletion_log TO exchange_app;
