-- Sign-in limits (DESIGN.md §8, §18 item 4).
--
-- A new code no longer ends the live ones, so a code must say what it was
-- sent for: the codes kept live, checked and spent together are those of one
-- identifier and one purpose. Codes already stored were sent before this
-- column existed and are taken as sign-in codes; a deletion code in flight
-- then simply stops working, which is the safe way round.
ALTER TABLE one_time_code
    ADD COLUMN purpose text NOT NULL DEFAULT 'sign-in'
        CHECK (purpose IN ('sign-in', 'delete-account'));

DROP INDEX one_time_code_identifier_idx;
-- Finds the live codes for an identifier and purpose, and counts recent requests.
CREATE INDEX one_time_code_identifier_idx
    ON one_time_code (identifier, purpose, created_at DESC);

-- How many code requests and failed guesses each requester, identifier or
-- account has made in the current window. One row per thing counted and
-- window; the service adds to it and reads it, holding the row locked while
-- it decides, so that concurrent requests cannot both slip under a limit.
CREATE TABLE sign_in_limit (
    -- What is counted, and for whom.
    scope        text NOT NULL CHECK (scope IN (
                     'code-requests-by-address',
                     'failed-guesses-by-address',
                     'failed-guesses-by-identifier',
                     'code-requests-by-account',
                     'failed-guesses-by-account')),
    -- HMAC-SHA-256 under the service's secret of the network address,
    -- identifier or account counted. Neither an address nor an email address
    -- or phone number is stored here in the clear.
    subject      bytea NOT NULL CHECK (octet_length(subject) = 32),
    -- The start of the hour or the UTC day being counted.
    window_start timestamptz NOT NULL,
    count        integer NOT NULL DEFAULT 0 CHECK (count >= 0),
    PRIMARY KEY (scope, subject, window_start)
);

-- For the worker, which removes windows long past.
CREATE INDEX sign_in_limit_window_idx ON sign_in_limit (window_start);

GRANT SELECT, INSERT, UPDATE, DELETE ON sign_in_limit TO exchange_app;
