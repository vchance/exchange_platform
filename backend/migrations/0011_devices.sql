-- Devices that receive push notifications (DESIGN.md §12, §13), the push
-- service's tickets awaiting their receipts, and a global count of the text
-- messages that carry one-time codes.

-- A device the app has registered for push, under the session it was signed
-- in with. Signing out of that session removes the device; so does deleting
-- the account, and the push service saying the token is no longer valid. A
-- device whose session has ended is not sent to, and the worker removes it.
CREATE TABLE device (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id  uuid NOT NULL REFERENCES account,
    session_id  uuid NOT NULL REFERENCES account_session ON DELETE CASCADE,
    -- Which push service the token belongs to. Only Expo's for now; tokens
    -- for Apple's and Google's own services would be another value here.
    service     text NOT NULL CHECK (service IN ('EXPO')),
    token       text NOT NULL CHECK (char_length(token) BETWEEN 1 AND 256),
    platform    text NOT NULL CHECK (platform IN ('ios', 'android')),
    -- The app's version and language when it registered, as the app said.
    -- The text sent follows the account's language, like every email.
    app_version text NOT NULL CHECK (char_length(app_version) BETWEEN 1 AND 32),
    language    text NOT NULL CHECK (language ~ '^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$'),
    created_at  timestamptz NOT NULL DEFAULT now(),
    updated_at  timestamptz NOT NULL DEFAULT now(),
    -- One token is one device, whoever signed in on it last.
    UNIQUE (service, token)
);

CREATE INDEX device_account_idx ON device (account_id);
CREATE INDEX device_session_idx ON device (session_id);

-- A message the push service accepted, waiting for the receipt that says
-- whether it reached the device. A receipt that says the token is no longer
-- valid removes the device. Rows go once their receipt is read, or after a
-- day, when the service no longer keeps it.
CREATE TABLE push_ticket (
    id         text PRIMARY KEY CHECK (char_length(id) BETWEEN 1 AND 128),
    device_id  uuid NOT NULL REFERENCES device ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX push_ticket_created_idx ON push_ticket (created_at);
CREATE INDEX push_ticket_device_idx ON push_ticket (device_id);

-- Text messages cost money per message, so the codes sent by SMS are also
-- counted across the whole service per hour, under the deployment's cap
-- (SMS_MAX_PER_HOUR): those handed to the provider, those refused because
-- the cap was reached, and those the provider did not take. The subject is
-- still a keyed hash, of a fixed value: the count is nobody's.
ALTER TABLE sign_in_limit DROP CONSTRAINT sign_in_limit_scope_check;
ALTER TABLE sign_in_limit ADD CONSTRAINT sign_in_limit_scope_check CHECK (scope IN (
    'code-requests-by-address',
    'failed-guesses-by-address',
    'failed-guesses-by-identifier',
    'code-requests-by-account',
    'failed-guesses-by-account',
    'sms-sent',
    'sms-refused',
    'sms-failed'));

GRANT SELECT, INSERT, UPDATE, DELETE ON device, push_ticket TO exchange_app;
