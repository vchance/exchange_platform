-- Reminders that something is due soon or overdue (DESIGN.md §12).
--
-- A reminder is not caused by an event: it becomes true because time passed.
-- The outbox's key, one message per event per person, therefore cannot stop
-- the worker sending the same reminder on every pass. What stops it is this
-- table: what each contribution has already been reminded about.

CREATE TABLE contribution_reminder (
    exchange_id     uuid NOT NULL,
    contribution_id uuid NOT NULL,
    -- 'DUE_SOON' is told to the party who owes the contribution; 'OVERDUE'
    -- to both parties.
    kind            text NOT NULL CHECK (kind IN ('DUE_SOON', 'OVERDUE')),
    -- The due date the reminder was about, as the agreement in force gave it
    -- at the time. It is part of the key, so an amendment that moves the date
    -- makes the new date something not yet reminded about, and one that
    -- leaves the date alone brings no second reminder.
    due_date        date NOT NULL,
    reminded_at     timestamptz NOT NULL DEFAULT now(),

    -- Each kind once per contribution per due date. Led by the exchange,
    -- because the worker reads everything one exchange has been reminded of.
    PRIMARY KEY (exchange_id, contribution_id, kind, due_date),
    FOREIGN KEY (exchange_id, contribution_id) REFERENCES contribution (exchange_id, id)
);

-- The worker reads today's date in each timezone that has an active exchange,
-- then looks through that timezone's active exchanges.
CREATE INDEX exchange_active_timezone_idx ON exchange (timezone) WHERE state = 'ACTIVE';

-- Read and add, nothing else: the worker only ever records that a reminder
-- was sent, and a row that could be changed or removed is a reminder that
-- could go out twice. This is not agreement history, so no trigger guards it
-- against the schema owner; the erasure and retention path (DESIGN.md §14)
-- will need to remove these rows along with the exchange they belong to.
GRANT SELECT, INSERT ON contribution_reminder TO exchange_app;
