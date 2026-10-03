-- Indexes the load check found missing (README, "Load check"). Each serves a
-- query that otherwise reads the whole exchange table, so its cost grew with
-- every exchange ever made.

-- How many exchanges an account has made in the last day, counted on every
-- creation under the per-account limit.
CREATE INDEX exchange_created_by_idx ON exchange (created_by, created_at);

-- The worker's timers, on every pass. Only an exchange under negotiation has
-- an open revision, and only one that has been prompted for inactivity has a
-- prompt time, so both indexes stay as small as what the timers look for.
-- Without the first, finding expired revisions read every revision ever sent
-- once it was past its expiry, whether or not anything still waited on it.
CREATE INDEX exchange_open_revision_idx ON exchange (open_revision_id)
    WHERE open_revision_id IS NOT NULL;
CREATE INDEX exchange_inactivity_prompted_idx ON exchange (inactivity_prompted_at)
    WHERE inactivity_prompted_at IS NOT NULL;
