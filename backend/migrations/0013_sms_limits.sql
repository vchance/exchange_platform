-- Text messages are also counted per number prefix (the country code and
-- the three digits after it: the area code for +1) under the deployment's
-- cap (SMS_MAX_PER_PREFIX_PER_HOUR), so that a few requesters cannot use the
-- whole service's hourly cap. Refusals are counted by reason for the
-- metrics: the prefix's cap, and a number of a country the service does not
-- take (SMS_ALLOWED_COUNTRY_CODES). As before, the subject is a keyed hash,
-- here of the prefix or of a fixed value, never a number.
ALTER TABLE sign_in_limit DROP CONSTRAINT sign_in_limit_scope_check;
ALTER TABLE sign_in_limit ADD CONSTRAINT sign_in_limit_scope_check CHECK (scope IN (
    'code-requests-by-address',
    'failed-guesses-by-address',
    'failed-guesses-by-identifier',
    'code-requests-by-account',
    'failed-guesses-by-account',
    'sms-sent',
    'sms-refused',
    'sms-failed',
    'sms-sent-by-prefix',
    'sms-refused-prefix',
    'sms-refused-country'));
