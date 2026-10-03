-- What the service's own role can write about deletions and review is
-- narrowed to what the service does (docs/operations.md, "Reviewing
-- reports" and "Replaying deletions").
--
-- 1. The deletion log. A line in it makes a replay delete the account, so a
--    line for a live account would be a deletion waiting for the next
--    restore. The log now takes only an account that is already deleted,
--    which the deletion writes in the same transaction, just before.
--
-- 2. The review history's entries with no reviewer read as the owner's:
--    naming and removing reviewers from the command line, and a suspension
--    lifted by replaying the deletion log. Only the schema owner may now
--    write one. The replay runs as the service's role, so its lifting goes
--    through `replay_lift_suspension`, which runs as the owner, lifts only a
--    suspension the deletion log's time can follow, and only for an account
--    deleted before the transaction ends.
--
-- 3. How often a reviewer lists the queue, the suspended accounts and the
--    hidden content, counted per hour, so that a stolen session cannot read
--    them over and over (`crate::review`).

------------------------------------------------------------------------------
-- The deletion log takes deleted accounts only
------------------------------------------------------------------------------

CREATE FUNCTION deletion_log_account_deleted() RETURNS trigger
    LANGUAGE plpgsql SET search_path = public, pg_temp AS
$$
BEGIN
    IF NOT EXISTS (SELECT 1 FROM account WHERE id = NEW.account_id AND status = 'DELETED') THEN
        RAISE EXCEPTION 'only a deleted account is added to the deletion log'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER deletion_log_account_deleted
    BEFORE INSERT ON deletion_log
    FOR EACH ROW EXECUTE FUNCTION deletion_log_account_deleted();

------------------------------------------------------------------------------
-- Only the owner writes an entry with no reviewer
------------------------------------------------------------------------------

CREATE FUNCTION review_event_owner_only() RETURNS trigger
    LANGUAGE plpgsql SET search_path = public, pg_temp AS
$$
BEGIN
    -- The owner of the table, whoever restored it (restore.sh restores
    -- every object as the role that runs it), or a role that is a member of
    -- it. Inside `replay_lift_suspension` the current user is the owner.
    IF NEW.staff_account_id IS NULL AND NOT pg_has_role(
           current_user,
           (SELECT relowner FROM pg_class WHERE oid = TG_RELID),
           'MEMBER') THEN
        RAISE EXCEPTION 'only the schema owner writes a review entry with no reviewer'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER review_event_owner_only
    BEFORE INSERT ON review_event
    FOR EACH ROW EXECUTE FUNCTION review_event_owner_only();

-- A suspension lifted by the owner is lifted to replay a deletion, so the
-- account must be deleted, and in the deletion log, by the time the
-- transaction that lifted it commits.
CREATE FUNCTION review_event_owner_lift_deletes() RETURNS trigger
    LANGUAGE plpgsql SET search_path = public, pg_temp AS
$$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM deletion_log l JOIN account a ON a.id = l.account_id
        WHERE l.account_id = NEW.account_id AND a.status = 'DELETED') THEN
        RAISE EXCEPTION 'a suspension lifted with no reviewer is lifted only to replay a deletion'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    RETURN NULL;
END;
$$;

CREATE CONSTRAINT TRIGGER review_event_owner_lift_deletes
    AFTER INSERT ON review_event
    DEFERRABLE INITIALLY DEFERRED
    FOR EACH ROW
    WHEN (NEW.staff_account_id IS NULL AND NEW.action = 'SUSPENSION_LIFTED')
    EXECUTE FUNCTION review_event_owner_lift_deletes();

-- Lifts the suspension of an account that a replayed deletion log says was
-- deleted at `deleted_at`, and records it with no reviewer, as the owner's,
-- with `note`. Returns whether the account was suspended. Refused when the
-- log's time is before the account was created or before it was last
-- suspended: such a line cannot be a deletion of this account as it stands
-- here (`crate::deletion::replay` reports it and does nothing).
CREATE FUNCTION replay_lift_suspension(target uuid, deleted_at timestamptz, note text)
    RETURNS boolean
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
DECLARE
    created timestamptz;
    suspended_at timestamptz;
    suspended_for uuid;
BEGIN
    SELECT a.created_at INTO created FROM account a
    WHERE a.id = target AND a.status = 'SUSPENDED'
    FOR NO KEY UPDATE;
    IF NOT FOUND THEN
        RETURN false;
    END IF;
    SELECT e.occurred_at, e.report_id INTO suspended_at, suspended_for
    FROM review_event e
    WHERE e.account_id = target AND e.action = 'ACCOUNT_SUSPENDED'
    ORDER BY e.id DESC
    LIMIT 1;
    IF deleted_at IS NULL OR deleted_at < created
       OR (suspended_at IS NOT NULL AND deleted_at < suspended_at) THEN
        RAISE EXCEPTION 'the deletion log''s time is before the account was created or last suspended'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    IF note IS NULL OR btrim(note) = '' THEN
        RAISE EXCEPTION 'a suspension lifted by the owner says why'
            USING ERRCODE = 'check_violation';
    END IF;
    UPDATE account SET status = 'ACTIVE' WHERE id = target;
    INSERT INTO review_event (staff_account_id, action, report_id, account_id, note)
    VALUES (NULL, 'SUSPENSION_LIFTED', suspended_for, target, note);
    RETURN true;
END;
$$;

REVOKE ALL ON FUNCTION replay_lift_suspension(uuid, timestamptz, text) FROM PUBLIC;
GRANT EXECUTE ON FUNCTION replay_lift_suspension(uuid, timestamptz, text) TO exchange_app;

------------------------------------------------------------------------------
-- Reviewers' lists, counted
------------------------------------------------------------------------------

-- How many lists one reviewer has read in each hour. One row per reviewer
-- and hour, added to under its row lock, so that requests at the same time
-- cannot both slip under the limit. Rows a day old are removed as new ones
-- are written.
CREATE TABLE staff_list_limit (
    staff_account_id uuid NOT NULL REFERENCES account,
    window_start     timestamptz NOT NULL,
    count            integer NOT NULL DEFAULT 0 CHECK (count >= 0),
    PRIMARY KEY (staff_account_id, window_start)
);

GRANT SELECT, INSERT, UPDATE, DELETE ON staff_list_limit TO exchange_app;
