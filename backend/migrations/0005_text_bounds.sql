-- Upper bounds on free text.
--
-- Agreement history cannot be deleted, so nothing written into it may be
-- unbounded. The service enforces the real limits (`domain::Limits`, and the
-- note limit in `domain::Rules`), which are product decisions and may change
-- without a migration. These are backstops several times larger, so that
-- nothing oversized can be stored whatever a future code path forgets.

ALTER TABLE revision
    ADD CHECK (char_length(terms) <= 100000),
    ADD CHECK (char_length(note) <= 10000),
    ADD CHECK (char_length(party_a_name) <= 1000),
    ADD CHECK (char_length(party_b_name) <= 1000);

ALTER TABLE contribution_snapshot
    ADD CHECK (char_length(description) <= 10000),
    ADD CHECK (char_length(completion_criteria) <= 10000),
    ADD CHECK (char_length(unit) <= 500);

ALTER TABLE exchange_event
    ADD CHECK (char_length(note) <= 10000);

ALTER TABLE participant
    ADD CHECK (char_length(display_name) <= 1000);

ALTER TABLE account
    ADD CHECK (char_length(display_name) <= 1000);

ALTER TABLE exchange_draft
    ADD CHECK (octet_length(body::text) <= 2000000);
