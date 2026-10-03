-- Wallet passes (DESIGN.md §11): what Apple's pass web service and the
-- worker that keeps passes up to date need beyond the `wallet_pass` table of
-- 0001. One row is one party's pass for one exchange on one platform; its
-- `external_id` is the Apple serial number, or the last part of the Google
-- object ID, random and never the exchange's ID.

ALTER TABLE wallet_pass
    -- SHA-256 of the Apple pass's authentication token, which a device sends
    -- back to the web service. The token itself is never stored.
    ADD COLUMN auth_token_hash bytea CHECK (octet_length(auth_token_hash) = 32),
    -- When the face last changed, given to Apple as Last-Modified: whole
    -- seconds, and later for every change, so that two changes in one second
    -- are still told apart.
    ADD COLUMN changed_at timestamptz NOT NULL DEFAULT date_trunc('second', now()),
    -- SHA-256 of the face `changed_at` belongs to, and of the face last
    -- delivered to the platform (pushed to Apple's devices, or patched into
    -- the Google object). A change that leaves the face as it was sends
    -- nothing.
    ADD COLUMN face_hash bytea CHECK (octet_length(face_hash) = 32),
    ADD COLUMN delivered_hash bytea CHECK (octet_length(delivered_hash) = 32),
    -- Counts the changes that may alter the face. The worker notes it when it
    -- takes the pass and leaves the pass PENDING if it moved meanwhile.
    ADD COLUMN mark_seq bigint NOT NULL DEFAULT 0,
    -- Delivery, as for the outbox: not before `available_at`, moved forward
    -- after each failure; `claimed_until` is the worker's lease while it
    -- sends, which keeps the row unlocked for the change that marks it again.
    ADD COLUMN available_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN claimed_until timestamptz,
    ADD COLUMN attempts integer NOT NULL DEFAULT 0 CHECK (attempts >= 0),
    ADD COLUMN last_error text CHECK (char_length(last_error) <= 500),
    -- How often the pass was handed out in the current hour (the limit is the
    -- service's).
    ADD COLUMN issue_window_started_at timestamptz NOT NULL DEFAULT now(),
    ADD COLUMN issues_in_window integer NOT NULL DEFAULT 0 CHECK (issues_in_window >= 0),
    ADD CHECK (platform = 'APPLE' OR auth_token_hash IS NULL),
    -- Fits Apple's serial number and Google's object ID rules alike.
    ADD CHECK (external_id ~ '^[A-Za-z0-9_.-]{1,64}$');

-- The worker's queue.
CREATE INDEX wallet_pass_pending_idx ON wallet_pass (available_at)
    WHERE update_status = 'PENDING';
-- Revoking an account's passes when it is deleted.
CREATE INDEX wallet_pass_account_idx ON wallet_pass (account_id);

-- The devices that asked Apple's pass web service for updates to a pass.
-- The push token is the device's for this pass type; the worker sends it a
-- push when the pass changes, and removes the row when Apple says the token
-- is no longer valid, or after it has told the device that the pass is void.
CREATE TABLE wallet_device_registration (
    wallet_pass_id    uuid NOT NULL REFERENCES wallet_pass,
    device_library_id text NOT NULL CHECK (char_length(device_library_id) BETWEEN 1 AND 128),
    push_token        text NOT NULL CHECK (char_length(push_token) BETWEEN 1 AND 256),
    created_at        timestamptz NOT NULL DEFAULT now(),
    updated_at        timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (wallet_pass_id, device_library_id)
);

-- A device asks which of its passes changed.
CREATE INDEX wallet_device_registration_device_idx
    ON wallet_device_registration (device_library_id);

-- Registrations are working data: added, updated and removed as devices and
-- Apple say. `wallet_pass` keeps the grants of 0001: read, add, update.
GRANT SELECT, INSERT, UPDATE, DELETE ON wallet_device_registration TO exchange_app;
