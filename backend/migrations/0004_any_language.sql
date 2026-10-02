-- Languages are not a fixed pair (DESIGN.md §4.2).
--
-- The first migration allowed only 'en' and 'es' as an account's language and
-- as the language a signer was shown the consent wording in. Which languages
-- the product supports is decided by the wording files, and adding one must
-- not need a migration, so the database now checks only that the value is
-- shaped like a language tag (`en`, `pt-BR`, `zh-Hant`).

ALTER TABLE account
    DROP CONSTRAINT account_language_check,
    ADD CONSTRAINT account_language_check
        CHECK (language ~ '^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$');

ALTER TABLE acceptance
    DROP CONSTRAINT acceptance_consent_language_check,
    ADD CONSTRAINT acceptance_consent_language_check
        CHECK (consent_language ~ '^[a-z]{2,3}(-[A-Za-z0-9]{2,8})*$');
