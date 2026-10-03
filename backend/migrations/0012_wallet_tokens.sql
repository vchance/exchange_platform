-- Wallet passes, after review (DESIGN.md §11, docs/wallet.md): authentication
-- tokens that are random and can be rotated, download links that work once,
-- a bound on device registrations, a void face that reaches every device,
-- and the daily look at faces that change with the date.

-- An Apple pass's authentication tokens, by their SHA-256. A token is random
-- and made each time the pass is handed out; the service keeps the latest
-- few valid, so that the copies already on a person's other devices keep
-- updating, and forgets older ones. The token itself is never stored.
CREATE TABLE wallet_auth_token (
    token_hash     bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    wallet_pass_id uuid NOT NULL REFERENCES wallet_pass,
    created_at     timestamptz NOT NULL DEFAULT clock_timestamp()
);

CREATE INDEX wallet_auth_token_pass_idx ON wallet_auth_token (wallet_pass_id, created_at);

-- The one token each Apple pass had until now, derived from APP_SECRET, stays
-- valid as the first of its tokens, so passes already on devices keep working
-- until it is rotated out.
INSERT INTO wallet_auth_token (token_hash, wallet_pass_id, created_at)
SELECT auth_token_hash, id, created_at FROM wallet_pass WHERE auth_token_hash IS NOT NULL;

ALTER TABLE wallet_pass
    DROP COLUMN auth_token_hash,
    -- The day, in the exchange's timezone, the face last delivered was drawn
    -- for. "Due soon" and "Overdue" follow the date, so the worker marks a
    -- pass again once its exchange's day has moved past this one.
    ADD COLUMN face_date date,
    -- New device registrations in the current hour (the limit is the
    -- service's).
    ADD COLUMN registration_window_started_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN registrations_in_window integer NOT NULL DEFAULT 0
        CHECK (registrations_in_window >= 0);

-- A link that downloads an Apple pass without a session: by the SHA-256 of
-- its random token, good until it expires and only once.
CREATE TABLE wallet_download_link (
    token_hash     bytea PRIMARY KEY CHECK (octet_length(token_hash) = 32),
    wallet_pass_id uuid NOT NULL REFERENCES wallet_pass,
    expires_at     timestamptz NOT NULL,
    used_at        timestamptz,
    created_at     timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX wallet_download_link_expires_idx ON wallet_download_link (expires_at);

-- When a device was told, in the list of its changed passes, that a voided
-- pass changed, after which it fetches the void face. The registration is
-- kept until then, or for a bounded time after the voiding, and removed after.
ALTER TABLE wallet_device_registration
    ADD COLUMN void_listed_at timestamptz;

-- Working data, like the registrations.
GRANT SELECT, INSERT, UPDATE, DELETE ON wallet_auth_token, wallet_download_link TO exchange_app;
