-- One-time codes and sessions (DESIGN.md §8).
--
-- Neither a code nor a session token is ever stored: only a keyed hash of the
-- code and a hash of the token.

-- A code sent to an email address or phone number to prove control of it.
CREATE TABLE one_time_code (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Normalized: a lower-case email address or an E.164 phone number.
    identifier      text NOT NULL,
    -- HMAC-SHA-256 of the identifier and code under the service's secret. A
    -- six-digit code is too small to protect with an unkeyed hash.
    code_hash       bytea NOT NULL CHECK (octet_length(code_hash) = 32),
    expires_at      timestamptz NOT NULL,
    failed_attempts smallint NOT NULL DEFAULT 0,
    consumed_at     timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now()
);

-- Finds the newest code for an identifier, and counts recent requests.
CREATE INDEX one_time_code_identifier_idx ON one_time_code (identifier, created_at DESC);

CREATE TABLE account_session (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id       uuid NOT NULL REFERENCES account,
    -- SHA-256 of the session token. The token is 256 random bits, so an
    -- unkeyed hash is enough.
    token_hash       bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    -- How and when the holder last proved control of an identifier. An
    -- acceptance records both, and a higher risk tier needs a recent one.
    auth_method      text NOT NULL CHECK (auth_method IN ('EMAIL_OTP', 'PHONE_OTP')),
    authenticated_at timestamptz NOT NULL,
    expires_at       timestamptz NOT NULL,
    revoked_at       timestamptz,
    created_at       timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX account_session_account_idx ON account_session (account_id);

GRANT SELECT, INSERT, UPDATE, DELETE ON one_time_code, account_session TO exchange_app;
