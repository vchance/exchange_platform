-- Who has held each slot, and when (DESIGN.md §8, decision of 2 October 2026).
--
-- Someone who claimed an invitation that named nobody can be removed before
-- the initiator confirms them: by the initiator ("not who I invited"), or by
-- leaving. The invited party's slot is then free for someone else, so the
-- account in a slot is no longer set once and for all.
--
-- A signature is permanent and was tied, by a foreign key, to the account in
-- the participant row. That row can now be emptied, so the tie moves to
-- something that never changes: the holding. A holding is one account's time
-- in one slot. The database writes this table itself, from the one column
-- that says who holds a slot now, and the application can only read it.
--
-- What the database guarantees as a result:
--   * A signature is stamped, by the database, with the holding that was open
--     in its slot when it was made, and must come from that holding's account.
--   * A holding only ever ends, and only for an invited party the initiator
--     has not confirmed. One that ended never resumes: whoever comes back
--     gets a new holding, and nothing signed under the old one comes with it.
--   * A revision comes into force only when both slots' current holders have
--     signed it and the invited party is confirmed. A signature made under a
--     holding that ended can never count toward that.
--   * Everything a removed person did stays: the claim event, the signature
--     and the holding name their account, with when it began and ended.
--
-- The functions below run with the schema owner's rights and a fixed search
-- path, so the application role needs no right to write `slot_holding`, and
-- a temporary table of the same name cannot stand in for it.

CREATE TABLE slot_holding (
    exchange_id uuid NOT NULL,
    slot        text NOT NULL,
    -- 1 for the first account to hold the slot, counting up.
    holding     integer NOT NULL CHECK (holding > 0),
    account_id  uuid NOT NULL REFERENCES account,
    began_at    timestamptz NOT NULL DEFAULT now(),
    -- Set once, when the holder was removed or left.
    ended_at    timestamptz,

    PRIMARY KEY (exchange_id, slot, holding),
    -- What a signature points at: the holding together with whose it was.
    UNIQUE (exchange_id, slot, holding, account_id),
    FOREIGN KEY (exchange_id, slot) REFERENCES participant (exchange_id, slot)
);

-- A slot has one holder at a time.
CREATE UNIQUE INDEX slot_holding_current_key ON slot_holding (exchange_id, slot)
    WHERE ended_at IS NULL;

CREATE INDEX slot_holding_account_idx ON slot_holding (account_id);

-- Every slot filled so far has had exactly one holder.
INSERT INTO slot_holding (exchange_id, slot, holding, account_id, began_at)
SELECT p.exchange_id, p.slot, 1, p.account_id,
       coalesce(
           (SELECT max(i.claimed_at) FROM invitation i
            WHERE i.exchange_id = p.exchange_id AND i.claimed_by = p.account_id),
           e.created_at)
FROM participant p
JOIN exchange e ON e.id = p.exchange_id
WHERE p.account_id IS NOT NULL;

------------------------------------------------------------------------------
-- Holdings follow the participant row
------------------------------------------------------------------------------

COMMENT ON COLUMN participant.account_id IS
    'Who holds the slot now. Empty until claimed, and again if an unconfirmed claimant is removed or leaves; slot_holding keeps everyone who has held it.';

-- Only someone in the slot can be confirmed.
ALTER TABLE participant
    ADD CHECK (initiator_confirmed_at IS NULL OR account_id IS NOT NULL);

CREATE FUNCTION participant_holding() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
BEGIN
    IF TG_OP = 'UPDATE' THEN
        IF OLD.initiator_confirmed_at IS NOT NULL AND NEW.initiator_confirmed_at IS NULL THEN
            RAISE EXCEPTION 'a confirmation is never taken back'
                USING ERRCODE = 'check_violation';
        END IF;
        IF NEW.account_id IS NOT DISTINCT FROM OLD.account_id THEN
            RETURN NULL;
        END IF;
        IF OLD.account_id IS NOT NULL THEN
            IF NEW.account_id IS NOT NULL THEN
                RAISE EXCEPTION 'a slot is emptied before another account takes it'
                    USING ERRCODE = 'check_violation';
            END IF;
            IF OLD.slot <> 'B' OR OLD.initiator_confirmed_at IS NOT NULL THEN
                RAISE EXCEPTION 'only an invited party the initiator has not confirmed can be removed'
                    USING ERRCODE = 'check_violation';
            END IF;
            UPDATE slot_holding SET ended_at = now()
            WHERE exchange_id = OLD.exchange_id AND slot = OLD.slot AND ended_at IS NULL;
            RETURN NULL;
        END IF;
    END IF;

    IF NEW.account_id IS NOT NULL THEN
        INSERT INTO slot_holding (exchange_id, slot, holding, account_id)
        SELECT NEW.exchange_id, NEW.slot, coalesce(max(holding), 0) + 1, NEW.account_id
        FROM slot_holding
        WHERE exchange_id = NEW.exchange_id AND slot = NEW.slot;
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER participant_holding
    AFTER INSERT OR UPDATE OF account_id, initiator_confirmed_at ON participant
    FOR EACH ROW EXECUTE FUNCTION participant_holding();

-- The record of who held a slot is as permanent as the rest of the history,
-- and says only what the participant row said. A holding begins, open, for
-- the account that row names; it ends, once, when the row no longer names
-- anyone; and nothing else about it can change. Like the append-only
-- triggers, this holds for every role.
CREATE FUNCTION slot_holding_follows_participant() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
DECLARE
    holder uuid;
BEGIN
    IF TG_OP <> 'DELETE' THEN
        SELECT p.account_id INTO holder FROM participant p
        WHERE p.exchange_id = NEW.exchange_id AND p.slot = NEW.slot;

        IF TG_OP = 'INSERT' THEN
            IF NEW.ended_at IS NULL AND holder = NEW.account_id THEN
                RETURN NEW;
            END IF;
        ELSIF OLD.ended_at IS NULL AND NEW.ended_at IS NOT NULL AND holder IS NULL
              AND (NEW.exchange_id, NEW.slot, NEW.holding, NEW.account_id, NEW.began_at)
                  = (OLD.exchange_id, OLD.slot, OLD.holding, OLD.account_id, OLD.began_at) THEN
            RETURN NEW;
        END IF;
    END IF;
    RAISE EXCEPTION 'slot_holding follows the participant row: % is not allowed here', TG_OP
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

CREATE TRIGGER slot_holding_follows_participant
    BEFORE INSERT OR UPDATE OR DELETE ON slot_holding
    FOR EACH ROW EXECUTE FUNCTION slot_holding_follows_participant();

CREATE TRIGGER slot_holding_no_truncate
    BEFORE TRUNCATE ON slot_holding
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

------------------------------------------------------------------------------
-- A signature belongs to the holding it was made under
------------------------------------------------------------------------------

-- Existing signatures were all made by a slot's first and only holder. Adding
-- a column with a default rewrites no row, so the append-only trigger is not
-- involved.
ALTER TABLE acceptance ADD COLUMN holding integer NOT NULL DEFAULT 1;
ALTER TABLE acceptance ALTER COLUMN holding DROP DEFAULT;

ALTER TABLE acceptance
    -- Was: the signer is the account in the participant row.
    DROP CONSTRAINT acceptance_exchange_id_slot_account_id_fkey,
    -- Was: one signature per slot. Someone who takes a slot after another
    -- was removed from it signs the same revision in their own right.
    DROP CONSTRAINT acceptance_revision_id_slot_key,
    ADD UNIQUE (revision_id, slot, holding),
    -- The signer is the account whose holding it is.
    ADD FOREIGN KEY (exchange_id, slot, holding, account_id)
        REFERENCES slot_holding (exchange_id, slot, holding, account_id);

-- The holding is not the signer's to name. Whatever the insert says, the
-- signature gets the holding open in its slot at that moment; the foreign
-- key above then refuses it unless the signer is that holding's account.
CREATE FUNCTION acceptance_holding() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
BEGIN
    SELECT h.holding INTO NEW.holding
    FROM slot_holding h
    WHERE h.exchange_id = NEW.exchange_id AND h.slot = NEW.slot AND h.ended_at IS NULL;
    IF NOT FOUND THEN
        RAISE EXCEPTION 'nobody holds slot % of exchange %', NEW.slot, NEW.exchange_id
            USING ERRCODE = 'foreign_key_violation';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER acceptance_holding
    BEFORE INSERT ON acceptance
    FOR EACH ROW EXECUTE FUNCTION acceptance_holding();

------------------------------------------------------------------------------
-- What it takes for a revision to come into force
------------------------------------------------------------------------------

-- Both slots' current holders have signed it, and the invited party is
-- confirmed. Signatures made under a holding that has ended do not count,
-- whoever made them and whenever.
--
-- Checked after the row is written, like the foreign keys, and after them:
-- a revision that is not this exchange's is still refused as that.
CREATE FUNCTION exchange_in_force_signed() RETURNS trigger
    LANGUAGE plpgsql SECURITY DEFINER SET search_path = public, pg_temp AS
$$
BEGIN
    IF NEW.in_force_revision_id IS NULL
       OR (TG_OP = 'UPDATE'
           AND NEW.in_force_revision_id IS NOT DISTINCT FROM OLD.in_force_revision_id) THEN
        RETURN NULL;
    END IF;

    IF (SELECT count(*)
        FROM acceptance a
        JOIN slot_holding h
          ON h.exchange_id = a.exchange_id AND h.slot = a.slot AND h.holding = a.holding
        WHERE a.exchange_id = NEW.id AND a.revision_id = NEW.in_force_revision_id
          AND h.ended_at IS NULL) < 2 THEN
        RAISE EXCEPTION 'a revision comes into force only when both current parties have signed it'
            USING ERRCODE = 'check_violation';
    END IF;

    IF NOT EXISTS (SELECT 1 FROM participant p
                   WHERE p.exchange_id = NEW.id AND p.slot = 'B'
                     AND p.initiator_confirmed_at IS NOT NULL) THEN
        RAISE EXCEPTION 'a revision comes into force only once the invited party is confirmed'
            USING ERRCODE = 'check_violation';
    END IF;
    RETURN NULL;
END;
$$;

CREATE TRIGGER exchange_in_force_signed
    AFTER INSERT OR UPDATE OF in_force_revision_id ON exchange
    FOR EACH ROW EXECUTE FUNCTION exchange_in_force_signed();

------------------------------------------------------------------------------
-- Application role
------------------------------------------------------------------------------

-- Read only: the triggers above are what write it.
GRANT SELECT ON slot_holding TO exchange_app;
