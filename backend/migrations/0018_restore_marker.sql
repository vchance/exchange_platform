-- A restored database waits for its deletion log to be replayed
-- (docs/operations.md, "Restoring").
--
-- A backup restored brings back every account deleted since it was taken.
-- Replaying the newest deletion log deletes them again, and until it has run
-- the api and the worker must not start on the copy: people who deleted
-- their accounts could sign in again and be written to. Reminding whoever
-- restores was not enough, so `scripts/restore.sh` now writes a mark into
-- the restored database, the api and the worker refuse to start while it
-- stands, and `replay-deletions` clears it once every account in the log is
-- deleted. `restore.sh --no-replay-needed` is the override for an emergency,
-- and is recorded here too.
--
-- The table is a history, never changed: the latest entry says where the
-- database stands. Only the schema owner writes it, as `restore.sh` does.
-- The service's role reads it, and can add the one entry that says the
-- replay is done, only through `restore_replay_done()` below, which first
-- checks that every account in the deletion log is deleted.
--
-- `restore.sh` creates this table itself when it restores a backup made
-- before this migration, with the same definition, so that the mark is in
-- place before `migrate` runs; hence IF NOT EXISTS and OR REPLACE here.

CREATE TABLE IF NOT EXISTS restore_marker (
    id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    event   text NOT NULL CHECK (event IN (
                -- Restored; the deletion log is still to be replayed.
                'REPLAY_PENDING',
                -- Restored with --no-replay-needed: whoever restored said
                -- that no replay was needed.
                'REPLAY_NOT_NEEDED',
                -- The deletion log was replayed.
                'REPLAYED')),
    at      timestamptz NOT NULL DEFAULT now(),
    -- The role that connected to write it.
    by_role text NOT NULL DEFAULT session_user,
    note    text CHECK (char_length(note) <= 1000)
);

CREATE OR REPLACE TRIGGER restore_marker_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON restore_marker
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

GRANT SELECT ON restore_marker TO exchange_app;

-- Whether the database is waiting for its deletion log to be replayed.
CREATE OR REPLACE FUNCTION restore_replay_pending() RETURNS boolean
    LANGUAGE sql STABLE SET search_path = public, pg_temp AS
$$
    SELECT coalesce(
        (SELECT event = 'REPLAY_PENDING' FROM restore_marker ORDER BY id DESC LIMIT 1),
        false);
$$;

-- Records that the deletion log has been replayed, if the database was
-- waiting for that, and says whether it was. Refused while any account the
-- deletion log names is not deleted: whatever the replay did, the log in
-- this database at least must hold.
CREATE OR REPLACE FUNCTION restore_replay_done() RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
DECLARE
    live bigint;
BEGIN
    -- One at a time, so two replays finishing together write one entry.
    LOCK TABLE restore_marker IN SHARE ROW EXCLUSIVE MODE;
    IF NOT restore_replay_pending() THEN
        RETURN false;
    END IF;
    SELECT count(*) INTO live
    FROM deletion_log l JOIN account a ON a.id = l.account_id
    WHERE a.status <> 'DELETED';
    IF live > 0 THEN
        RAISE EXCEPTION '% accounts in the deletion log are not deleted; replay again', live
            USING ERRCODE = 'check_violation';
    END IF;
    INSERT INTO restore_marker (event, note) VALUES ('REPLAYED', 'replay-deletions');
    RETURN true;
END;
$$;

REVOKE ALL ON FUNCTION restore_replay_done() FROM PUBLIC;
GRANT EXECUTE ON FUNCTION restore_replay_done() TO exchange_app;
