-- Replaying the deletion log lifts a suspension (docs/operations.md,
-- "Replaying deletions"; `deletion::replay`).
--
-- A suspended account cannot delete itself, so an account that the deletion
-- log names and that a restored copy holds suspended was reinstated after
-- the backup, and then deleted by its holder. Replaying the log does the
-- same again in one transaction: it lifts the suspension and deletes the
-- account. The lifting belongs in the review history like any other, but
-- no reviewer did it: the owner did, by running `replay-deletions` during a
-- restore.
--
-- The history already has an actor for that. What the owner does from the
-- command line (naming and removing reviewers) is recorded with no reviewer
-- (0014), and the staff screen shows such an entry as the owner's. A lifting
-- by the owner is recorded the same way, and must say why in its note, which
-- a reviewer's lifting does too. Every other action of a reviewer still
-- needs the reviewer, and naming or removing one still needs none.

ALTER TABLE review_event DROP CONSTRAINT review_event_check;

ALTER TABLE review_event ADD CONSTRAINT review_event_actor CHECK (
    CASE
        WHEN action IN ('STAFF_GRANTED', 'STAFF_REVOKED') THEN staff_account_id IS NULL
        WHEN action = 'SUSPENSION_LIFTED' THEN staff_account_id IS NOT NULL OR note IS NOT NULL
        ELSE staff_account_id IS NOT NULL
    END
);
