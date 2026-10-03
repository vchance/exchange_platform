-- Staff review of abuse reports (DESIGN.md §9, §18 item 5).
--
-- Who may review is a row in `staff_member`, which only the schema owner can
-- write: the `staff` command, run with MIGRATION_DATABASE_URL. The service
-- can read it and nothing else, so no request to the API can make anyone a
-- reviewer.
--
-- A report is resolved once: who resolved it, when, the outcome and a short
-- note are set together and never changed afterwards. Everything a reviewer
-- does, reading a report included, is a row in `review_event`, which nothing
-- can change or remove. A later look at the same matter (lifting a
-- suspension, showing hidden content again) is a new event there, not an
-- edit of the report.

------------------------------------------------------------------------------
-- Reviewers
------------------------------------------------------------------------------

CREATE TABLE staff_member (
    account_id uuid PRIMARY KEY REFERENCES account,
    granted_at timestamptz NOT NULL DEFAULT now()
);

-- The suspended accounts, which a reviewer lists to lift a suspension.
CREATE INDEX account_suspended_idx ON account (id) WHERE status = 'SUSPENDED';

------------------------------------------------------------------------------
-- How a report was resolved
------------------------------------------------------------------------------

ALTER TABLE report
    ADD COLUMN resolved_by     uuid REFERENCES account,
    ADD COLUMN outcome         text CHECK (outcome IN (
                                   'DISMISSED',
                                   'CONTENT_HIDDEN',
                                   'ACCOUNT_SUSPENDED',
                                   'CONTENT_HIDDEN_AND_ACCOUNT_SUSPENDED')),
    ADD COLUMN resolution_note text CHECK (char_length(resolution_note) <= 1000),
    -- A resolution is whole: a reviewer, an outcome, and the time. A report
    -- resolved before reviewers existed has a time and nothing else.
    ADD CONSTRAINT report_resolution_whole CHECK (
        (resolved_by IS NULL) = (outcome IS NULL)
        AND (outcome IS NULL OR resolved_at IS NOT NULL)
        AND (resolution_note IS NULL OR outcome IS NOT NULL)),
    -- The status says the same as the outcome, in the words of 0001.
    ADD CONSTRAINT report_outcome_status CHECK (
        outcome IS NULL
        OR (outcome = 'DISMISSED') = (status = 'DISMISSED'));

-- What was reported cannot change, and a resolution is set once. A report
-- can be updated only from open to resolved.
CREATE FUNCTION report_resolved_once() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    IF OLD.resolved_at IS NOT NULL
       OR NEW.reporter_account_id IS DISTINCT FROM OLD.reporter_account_id
       OR NEW.subject_exchange_id IS DISTINCT FROM OLD.subject_exchange_id
       OR NEW.subject_account_id IS DISTINCT FROM OLD.subject_account_id
       OR NEW.reason IS DISTINCT FROM OLD.reason
       OR NEW.details IS DISTINCT FROM OLD.details
       OR NEW.created_at IS DISTINCT FROM OLD.created_at THEN
        RAISE EXCEPTION 'a report is resolved once, and what was reported never changes'
            USING ERRCODE = 'insufficient_privilege';
    END IF;
    RETURN NEW;
END;
$$;

CREATE TRIGGER report_resolved_once
    BEFORE UPDATE ON report
    FOR EACH ROW EXECUTE FUNCTION report_resolved_once();

CREATE TRIGGER report_never_removed
    BEFORE DELETE OR TRUNCATE ON report
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

-- A reviewer opening a report sees the other reports about the same exchange.
CREATE INDEX report_exchange_idx ON report (subject_exchange_id);

------------------------------------------------------------------------------
-- Content hidden by review
------------------------------------------------------------------------------

-- What the parties wrote in an exchange, hidden from one account by a
-- reviewer. The record itself is untouched; the service shows that account
-- a placeholder in place of the text. Removing the row shows it again, and
-- `review_event` keeps both.
CREATE TABLE hidden_content (
    exchange_id uuid NOT NULL REFERENCES exchange,
    account_id  uuid NOT NULL REFERENCES account,
    report_id   uuid NOT NULL REFERENCES report,
    hidden_at   timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (exchange_id, account_id)
);

------------------------------------------------------------------------------
-- The audit history of review
------------------------------------------------------------------------------

CREATE TABLE review_event (
    id               bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    -- The reviewer. Nobody for what the owner did from the command line.
    staff_account_id uuid REFERENCES account,
    action           text NOT NULL CHECK (action IN (
                         'REPORT_VIEWED',
                         'REPORT_DISMISSED',
                         'CONTENT_HIDDEN',
                         'CONTENT_RESTORED',
                         'ACCOUNT_SUSPENDED',
                         'SUSPENSION_LIFTED',
                         'STAFF_GRANTED',
                         'STAFF_REVOKED')),
    report_id        uuid REFERENCES report,
    exchange_id      uuid REFERENCES exchange,
    -- The account acted on: suspended, reinstated, hidden from, or made a
    -- reviewer.
    account_id       uuid REFERENCES account,
    note             text CHECK (char_length(note) <= 1000),
    occurred_at      timestamptz NOT NULL DEFAULT now(),

    CHECK ((staff_account_id IS NULL) = (action IN ('STAFF_GRANTED', 'STAFF_REVOKED')))
);

CREATE INDEX review_event_report_idx ON review_event (report_id);
CREATE INDEX review_event_exchange_idx ON review_event (exchange_id);
CREATE INDEX review_event_account_idx ON review_event (account_id);
-- Counting what one reviewer did in the last hour, for the rate limits.
CREATE INDEX review_event_staff_idx ON review_event (staff_account_id, occurred_at);

CREATE TRIGGER review_event_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON review_event
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

------------------------------------------------------------------------------
-- Application role
------------------------------------------------------------------------------

-- Who is a reviewer is read, never written, by the service.
GRANT SELECT ON staff_member TO exchange_app;
-- The audit history is read and added to.
GRANT SELECT, INSERT ON review_event TO exchange_app;
-- Hidden content is working state: hidden, and shown again.
GRANT SELECT, INSERT, DELETE ON hidden_content TO exchange_app;
