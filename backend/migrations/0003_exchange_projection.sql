-- What the exchange API needs beyond the first record model.

-- Each party's name as written in a revision. The names are part of what is
-- signed, so they belong to the immutable revision, not to the participant
-- row, which follows the latest revision.
ALTER TABLE revision
    ADD COLUMN party_a_name text NOT NULL DEFAULT '',
    ADD COLUMN party_b_name text NOT NULL DEFAULT '';
ALTER TABLE revision
    ALTER COLUMN party_a_name DROP DEFAULT,
    ALTER COLUMN party_b_name DROP DEFAULT;

-- Current-state projection of things the rules track between commands. Each
-- is also recoverable from the event history.
ALTER TABLE exchange
    -- Who has proposed ending by agreement, if anyone.
    ADD COLUMN end_proposed_by        text CHECK (end_proposed_by IN ('A', 'B')),
    -- A pending request to close without agreement.
    ADD COLUMN close_requested_by     text CHECK (close_requested_by IN ('A', 'B')),
    ADD COLUMN close_requested_at     timestamptz,
    ADD COLUMN inactivity_prompted_at timestamptz,
    -- When a party last did anything.
    ADD COLUMN last_activity_at       timestamptz NOT NULL DEFAULT now(),
    ADD CHECK ((close_requested_by IS NULL) = (close_requested_at IS NULL));

-- The worker looks for exchanges whose timers have run out.
CREATE INDEX exchange_close_requested_idx ON exchange (close_requested_at)
    WHERE close_requested_at IS NOT NULL;
CREATE INDEX exchange_inactivity_idx ON exchange (last_activity_at) WHERE state = 'ACTIVE';
CREATE INDEX revision_expiry_idx ON revision (expires_at);
