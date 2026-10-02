-- Record model (DESIGN.md §10), with the database enforcing the invariants of
-- §3 and the rules of §13.2.
--
-- Run as the schema owner. The application role `exchange_app` must already
-- exist; the grants at the end are what the api and worker processes may do.
--
-- Conventions:
--   * Identifiers exposed outside the service are random UUIDs.
--   * Enumerations are text with CHECK constraints, spelled as in the design.
--   * Hashes are raw SHA-256 digests (32 bytes).
--   * Instants are timestamptz in UTC; due dates are plain dates read in the
--     exchange's timezone; money is whole minor units plus an ISO 4217 code.
--   * Slot 'A' is the initiator, slot 'B' the invited counterparty.
--   * Rows that belong to one exchange carry exchange_id, and composite foreign
--     keys keep every reference inside that exchange.

------------------------------------------------------------------------------
-- Accounts
------------------------------------------------------------------------------

CREATE TABLE account (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Only verified identifiers are stored here. A deleted account keeps its
    -- row, so the other party's record stays intact, but loses its identifiers.
    email              text UNIQUE CHECK (email = lower(email)),
    phone              text UNIQUE CHECK (phone ~ '^\+[1-9][0-9]{6,14}$'),
    display_name       text NOT NULL,
    language           text NOT NULL DEFAULT 'en' CHECK (language IN ('en', 'es')),
    status             text NOT NULL DEFAULT 'ACTIVE'
                       CHECK (status IN ('ACTIVE', 'SUSPENDED', 'DELETED')),
    -- Set when the holder confirms they are 18 or over; required before signing.
    adult_confirmed_at timestamptz,
    created_at         timestamptz NOT NULL DEFAULT now(),
    CHECK (status = 'DELETED' OR num_nonnulls(email, phone) >= 1)
);

------------------------------------------------------------------------------
-- Exchanges and participants
------------------------------------------------------------------------------

CREATE TABLE exchange (
    id                   uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- For humans to tell exchanges apart. Never accepted as a credential.
    display_code         text NOT NULL UNIQUE,
    state                text NOT NULL DEFAULT 'DRAFT'
                         CHECK (state IN ('DRAFT', 'NEGOTIATING', 'ACTIVE', 'CLOSED')),
    closed_outcome       text CHECK (closed_outcome IN
                             ('NOT_AGREED', 'COMPLETED', 'ENDED_BY_AGREEMENT', 'UNRESOLVED')),
    closed_reason        text,
    closed_at            timestamptz,
    -- The one revision awaiting acceptance, if any. A single pointer is what
    -- guarantees "one open revision at a time"; revision rows never change.
    open_revision_id     uuid,
    -- The revision both parties accepted and that is currently binding.
    in_force_revision_id uuid,
    risk_tier            smallint NOT NULL DEFAULT 0 CHECK (risk_tier IN (0, 1, 2)),
    timezone             text NOT NULL,
    currency             text NOT NULL DEFAULT 'USD' CHECK (currency ~ '^[A-Z]{3}$'),
    -- Optimistic concurrency: every mutating transaction locks this row,
    -- checks the version the client sent, and increments it.
    version              bigint NOT NULL DEFAULT 0,
    -- Sequence number of the latest event; the next event takes this plus one.
    last_event_seq       bigint NOT NULL DEFAULT 0,
    created_by           uuid NOT NULL REFERENCES account,
    created_at           timestamptz NOT NULL DEFAULT now(),
    updated_at           timestamptz NOT NULL DEFAULT now(),

    UNIQUE (id, currency),
    CHECK ((state = 'CLOSED') = (closed_outcome IS NOT NULL)),
    CHECK ((state = 'CLOSED') = (closed_at IS NOT NULL)),
    CHECK (state <> 'DRAFT' OR (open_revision_id IS NULL AND in_force_revision_id IS NULL)),
    CHECK (state <> 'ACTIVE' OR in_force_revision_id IS NOT NULL),
    CHECK (state <> 'CLOSED' OR open_revision_id IS NULL),
    CHECK (in_force_revision_id IS NULL OR state IN ('ACTIVE', 'CLOSED')),
    -- An exchange closes as NOT_AGREED exactly when nothing was ever in force.
    CHECK (closed_outcome IS NULL
           OR (closed_outcome = 'NOT_AGREED') = (in_force_revision_id IS NULL))
);

CREATE TABLE participant (
    exchange_id            uuid NOT NULL REFERENCES exchange,
    slot                   text NOT NULL CHECK (slot IN ('A', 'B')),
    -- Empty until the slot is claimed; set once and never reassigned.
    account_id             uuid REFERENCES account,
    display_name           text NOT NULL,
    -- Shown on passes and lock screens in place of the display name.
    alias                  text NOT NULL,
    -- When the initiator confirmed who claimed this slot. An acceptance takes
    -- effect only once the counterparty is confirmed (or was pre-bound).
    initiator_confirmed_at timestamptz,
    -- When this party asked for the closed exchange to be removed.
    removal_requested_at   timestamptz,

    PRIMARY KEY (exchange_id, slot),
    UNIQUE (exchange_id, account_id),
    UNIQUE (exchange_id, slot, account_id)
);

CREATE INDEX participant_account_idx ON participant (account_id);

-- Working copy of terms not yet sent: the initiator's first proposal, or
-- either party's counteroffer or amendment in progress. Nothing here is
-- binding or visible to the other party; sending creates a revision.
CREATE TABLE exchange_draft (
    exchange_id uuid NOT NULL REFERENCES exchange,
    account_id  uuid NOT NULL REFERENCES account,
    body        jsonb NOT NULL,
    updated_at  timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (exchange_id, account_id)
);

CREATE TABLE invitation (
    id          uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id uuid NOT NULL REFERENCES exchange,
    -- The token itself is never stored.
    token_hash  bytea NOT NULL UNIQUE CHECK (octet_length(token_hash) = 32),
    -- Pre-binding: when set, only this verified identifier may claim.
    bound_email text CHECK (bound_email = lower(bound_email)),
    bound_phone text,
    expires_at  timestamptz NOT NULL,
    claimed_by  uuid REFERENCES account,
    claimed_at  timestamptz,
    revoked_at  timestamptz,
    created_at  timestamptz NOT NULL DEFAULT now(),

    CHECK (num_nonnulls(bound_email, bound_phone) <= 1),
    CHECK ((claimed_by IS NULL) = (claimed_at IS NULL))
);

-- At most one invitation per exchange can still be claimed. Expiry is checked
-- by the service; an expired invitation is revoked before another is issued.
CREATE UNIQUE INDEX invitation_claimable_key ON invitation (exchange_id)
    WHERE claimed_by IS NULL AND revoked_at IS NULL;

------------------------------------------------------------------------------
-- Attachments
------------------------------------------------------------------------------

CREATE TABLE attachment (
    id           uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id  uuid NOT NULL REFERENCES exchange,
    content_hash bytea NOT NULL CHECK (octet_length(content_hash) = 32),
    storage_key  text NOT NULL,
    media_type   text NOT NULL,
    size_bytes   bigint NOT NULL CHECK (size_bytes > 0),
    scan_status  text NOT NULL DEFAULT 'PENDING'
                 CHECK (scan_status IN ('PENDING', 'CLEAN', 'REJECTED')),
    uploaded_by  uuid NOT NULL REFERENCES account,
    created_at   timestamptz NOT NULL DEFAULT now(),

    UNIQUE (exchange_id, id)
);

------------------------------------------------------------------------------
-- Revisions (append-only)
------------------------------------------------------------------------------

CREATE TABLE revision (
    id                 uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id        uuid NOT NULL REFERENCES exchange,
    sequence           integer NOT NULL CHECK (sequence > 0),
    parent_revision_id uuid,
    author_slot        text NOT NULL,
    note               text,
    terms              text NOT NULL DEFAULT '',
    expires_at         timestamptz NOT NULL,
    -- SHA-256 over the canonical JSON of this revision, computed by the service.
    content_hash       bytea NOT NULL CHECK (octet_length(content_hash) = 32),
    created_at         timestamptz NOT NULL DEFAULT now(),

    UNIQUE (exchange_id, sequence),
    UNIQUE (exchange_id, id),
    UNIQUE (exchange_id, id, content_hash),
    FOREIGN KEY (exchange_id, parent_revision_id) REFERENCES revision (exchange_id, id),
    FOREIGN KEY (exchange_id, author_slot) REFERENCES participant (exchange_id, slot)
);

ALTER TABLE exchange
    ADD FOREIGN KEY (id, open_revision_id) REFERENCES revision (exchange_id, id),
    ADD FOREIGN KEY (id, in_force_revision_id) REFERENCES revision (exchange_id, id);

CREATE TABLE revision_attachment (
    exchange_id   uuid NOT NULL,
    revision_id   uuid NOT NULL,
    attachment_id uuid NOT NULL,

    PRIMARY KEY (revision_id, attachment_id),
    FOREIGN KEY (exchange_id, revision_id) REFERENCES revision (exchange_id, id),
    FOREIGN KEY (exchange_id, attachment_id) REFERENCES attachment (exchange_id, id)
);

------------------------------------------------------------------------------
-- Contributions
------------------------------------------------------------------------------

-- The stable identity of a contribution for the life of the exchange, and the
-- projection of its current status. The status is derived from events and can
-- be rebuilt from them; it is meaningful for contributions in the in-force
-- revision.
CREATE TABLE contribution (
    id             uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id    uuid NOT NULL REFERENCES exchange,
    status         text NOT NULL DEFAULT 'PENDING'
                   CHECK (status IN
                       ('PENDING', 'CLAIMED', 'DISPUTED', 'ACCEPTED', 'WAIVED', 'REMOVED')),
    -- The event that produced the current status.
    last_event_seq bigint,

    UNIQUE (exchange_id, id)
);

-- A contribution's terms as written in one revision (append-only).
CREATE TABLE contribution_snapshot (
    exchange_id               uuid NOT NULL,
    revision_id               uuid NOT NULL,
    contribution_id           uuid NOT NULL,
    position                  integer NOT NULL CHECK (position >= 0),
    from_slot                 text NOT NULL CHECK (from_slot IN ('A', 'B')),
    to_slot                   text NOT NULL CHECK (to_slot IN ('A', 'B')),
    type                      text NOT NULL
                              CHECK (type IN ('ITEM', 'SERVICE', 'TASK', 'MONEY', 'OTHER')),
    description               text NOT NULL,
    quantity                  numeric CHECK (quantity > 0),
    unit                      text,
    -- Due on a calendar date, when the agreement is accepted, or when another
    -- contribution in the same revision is accepted.
    due_kind                  text NOT NULL
                              CHECK (due_kind IN ('DATE', 'ON_AGREEMENT', 'AFTER_CONTRIBUTION')),
    due_date                  date,
    due_after_contribution_id uuid,
    completion_criteria       text,
    required                  boolean NOT NULL DEFAULT true,
    -- Money only.
    amount_minor              bigint,
    currency                  text,
    settlement_mode           text CHECK (settlement_mode IN ('OFF_PLATFORM', 'PROCESSOR')),

    PRIMARY KEY (revision_id, contribution_id),
    UNIQUE (revision_id, position),
    FOREIGN KEY (exchange_id, revision_id) REFERENCES revision (exchange_id, id),
    FOREIGN KEY (exchange_id, contribution_id) REFERENCES contribution (exchange_id, id),
    -- One currency per exchange.
    FOREIGN KEY (exchange_id, currency) REFERENCES exchange (id, currency),
    -- Cycles longer than one step are rejected by the service at revision
    -- creation. Deferred so a revision's snapshots can be inserted in any order.
    FOREIGN KEY (revision_id, due_after_contribution_id)
        REFERENCES contribution_snapshot (revision_id, contribution_id)
        DEFERRABLE INITIALLY DEFERRED,

    CHECK (from_slot <> to_slot),
    CHECK ((due_kind = 'DATE') = (due_date IS NOT NULL)),
    CHECK ((due_kind = 'AFTER_CONTRIBUTION') = (due_after_contribution_id IS NOT NULL)),
    CHECK (due_after_contribution_id <> contribution_id),
    CHECK (CASE WHEN type = 'MONEY'
                THEN amount_minor > 0 AND currency IS NOT NULL AND settlement_mode IS NOT NULL
                ELSE num_nonnulls(amount_minor, currency, settlement_mode) = 0
           END)
);

------------------------------------------------------------------------------
-- Acceptances (append-only): one signature on one exact revision
------------------------------------------------------------------------------

CREATE TABLE acceptance (
    id               uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    exchange_id      uuid NOT NULL,
    revision_id      uuid NOT NULL,
    slot             text NOT NULL,
    account_id       uuid NOT NULL,
    -- The hash the signer accepted; must be the revision's own.
    content_hash     bytea NOT NULL,
    auth_method      text NOT NULL CHECK (auth_method IN ('EMAIL_OTP', 'PHONE_OTP')),
    authenticated_at timestamptz NOT NULL,
    consent_language text NOT NULL CHECK (consent_language IN ('en', 'es')),
    consent_version  text NOT NULL,
    accepted_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (revision_id, slot),
    FOREIGN KEY (exchange_id, revision_id, content_hash)
        REFERENCES revision (exchange_id, id, content_hash),
    -- The signer is the account bound to that slot.
    FOREIGN KEY (exchange_id, slot, account_id)
        REFERENCES participant (exchange_id, slot, account_id)
);

-- Kept apart from the acceptance because it is purged after 90 days while the
-- acceptance itself is permanent.
CREATE TABLE acceptance_network_metadata (
    acceptance_id uuid PRIMARY KEY REFERENCES acceptance,
    ip_address    inet,
    user_agent    text,
    recorded_at   timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX acceptance_network_metadata_recorded_idx
    ON acceptance_network_metadata (recorded_at);

------------------------------------------------------------------------------
-- Exchange events (append-only): the history every state is derived from
------------------------------------------------------------------------------

CREATE TABLE exchange_event (
    exchange_id            uuid NOT NULL REFERENCES exchange,
    sequence               bigint NOT NULL CHECK (sequence > 0),
    -- The service's event enum is the authority on which types exist.
    type                   text NOT NULL CHECK (type ~ '^[A-Z][A-Z0-9_]*$'),
    actor_kind             text NOT NULL CHECK (actor_kind IN ('PARTICIPANT', 'SYSTEM', 'STAFF')),
    actor_slot             text,
    contribution_id        uuid,
    -- The revision in force, or the revision the event is about.
    revision_id            uuid,
    note                   text,
    evidence_attachment_id uuid,
    -- Details specific to the event type.
    data                   jsonb NOT NULL DEFAULT '{}',
    occurred_at            timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (exchange_id, sequence),
    FOREIGN KEY (exchange_id, actor_slot) REFERENCES participant (exchange_id, slot),
    FOREIGN KEY (exchange_id, contribution_id) REFERENCES contribution (exchange_id, id),
    FOREIGN KEY (exchange_id, revision_id) REFERENCES revision (exchange_id, id),
    FOREIGN KEY (exchange_id, evidence_attachment_id) REFERENCES attachment (exchange_id, id),
    CHECK ((actor_kind = 'PARTICIPANT') = (actor_slot IS NOT NULL))
);

ALTER TABLE contribution
    ADD FOREIGN KEY (exchange_id, last_event_seq)
        REFERENCES exchange_event (exchange_id, sequence);

------------------------------------------------------------------------------
-- Wallet passes
------------------------------------------------------------------------------

CREATE TABLE wallet_pass (
    id              uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    account_id      uuid NOT NULL REFERENCES account,
    exchange_id     uuid NOT NULL REFERENCES exchange,
    platform        text NOT NULL CHECK (platform IN ('APPLE', 'GOOGLE')),
    -- Apple serial number or Google object ID.
    external_id     text NOT NULL,
    -- Version of the pass face last rendered; a late or repeated update with a
    -- lower version is dropped.
    display_version bigint NOT NULL DEFAULT 0,
    update_status   text NOT NULL DEFAULT 'CURRENT'
                    CHECK (update_status IN ('CURRENT', 'PENDING', 'FAILED')),
    voided_at       timestamptz,
    created_at      timestamptz NOT NULL DEFAULT now(),
    updated_at      timestamptz NOT NULL DEFAULT now(),

    UNIQUE (account_id, exchange_id, platform),
    UNIQUE (platform, external_id)
);

CREATE INDEX wallet_pass_exchange_idx ON wallet_pass (exchange_id);

------------------------------------------------------------------------------
-- Abuse reports and blocks
------------------------------------------------------------------------------

CREATE TABLE report (
    id                  uuid PRIMARY KEY DEFAULT gen_random_uuid(),
    -- Empty for a report made from the unauthenticated proposal view.
    reporter_account_id uuid REFERENCES account,
    subject_exchange_id uuid REFERENCES exchange,
    subject_account_id  uuid REFERENCES account,
    reason              text NOT NULL,
    details             text,
    status              text NOT NULL DEFAULT 'OPEN'
                        CHECK (status IN ('OPEN', 'ACTIONED', 'DISMISSED')),
    created_at          timestamptz NOT NULL DEFAULT now(),
    resolved_at         timestamptz,

    CHECK (num_nonnulls(subject_exchange_id, subject_account_id) >= 1),
    CHECK ((status = 'OPEN') = (resolved_at IS NULL))
);

-- The review queue, oldest first.
CREATE INDEX report_open_idx ON report (created_at) WHERE status = 'OPEN';

CREATE TABLE account_block (
    blocker_account_id uuid NOT NULL REFERENCES account,
    blocked_account_id uuid NOT NULL REFERENCES account,
    created_at         timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (blocker_account_id, blocked_account_id),
    CHECK (blocker_account_id <> blocked_account_id)
);

------------------------------------------------------------------------------
-- Request idempotency and the transactional outbox
------------------------------------------------------------------------------

CREATE TABLE idempotency_key (
    account_id      uuid NOT NULL REFERENCES account,
    key             text NOT NULL,
    -- Detects a key reused for a different request.
    request_hash    bytea NOT NULL CHECK (octet_length(request_hash) = 32),
    -- Empty while the first request is still in progress.
    response_status smallint,
    response_body   jsonb,
    created_at      timestamptz NOT NULL DEFAULT now(),

    PRIMARY KEY (account_id, key)
);

CREATE INDEX idempotency_key_created_idx ON idempotency_key (created_at);

-- Written in the same transaction as the event that caused it; drained by the
-- worker with FOR UPDATE SKIP LOCKED.
CREATE TABLE outbox (
    id                   bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
    kind                 text NOT NULL CHECK (kind IN ('EMAIL', 'PUSH', 'PASS_UPDATE')),
    recipient_account_id uuid REFERENCES account,
    exchange_id          uuid,
    event_sequence       bigint,
    payload              jsonb NOT NULL,
    -- Not before this time; moved forward on each failed attempt.
    available_at         timestamptz NOT NULL DEFAULT now(),
    attempts             integer NOT NULL DEFAULT 0,
    last_error           text,
    completed_at         timestamptz,
    created_at           timestamptz NOT NULL DEFAULT now(),

    FOREIGN KEY (exchange_id, event_sequence) REFERENCES exchange_event (exchange_id, sequence),
    -- One event produces one message per person per channel.
    UNIQUE (exchange_id, event_sequence, kind, recipient_account_id)
);

CREATE INDEX outbox_pending_idx ON outbox (available_at) WHERE completed_at IS NULL;

------------------------------------------------------------------------------
-- Append-only enforcement
------------------------------------------------------------------------------

-- History is corrected by later events, never edited. These triggers hold for
-- every role, the schema owner included. Erasure and retention deletes
-- (DESIGN.md §14) are still open with counsel; when decided, a later
-- migration adds the one audited path that may bypass this.
CREATE FUNCTION forbid_change() RETURNS trigger
    LANGUAGE plpgsql AS
$$
BEGIN
    RAISE EXCEPTION '% is append-only: % is not allowed', TG_TABLE_NAME, TG_OP
        USING ERRCODE = 'insufficient_privilege';
END;
$$;

CREATE TRIGGER revision_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON revision
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

CREATE TRIGGER revision_attachment_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON revision_attachment
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

CREATE TRIGGER contribution_snapshot_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON contribution_snapshot
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

CREATE TRIGGER acceptance_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON acceptance
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

CREATE TRIGGER exchange_event_append_only
    BEFORE UPDATE OR DELETE OR TRUNCATE ON exchange_event
    FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();

------------------------------------------------------------------------------
-- Application role
------------------------------------------------------------------------------

-- Append-only tables: read and add, nothing else.
GRANT SELECT, INSERT ON
    revision, revision_attachment, contribution_snapshot, acceptance, exchange_event
    TO exchange_app;

-- Current-state tables: read, add and update. Deleting accounts and exchanges
-- waits for the erasure and retention path.
GRANT SELECT, INSERT, UPDATE ON
    account, exchange, participant, invitation, attachment, contribution, wallet_pass, report
    TO exchange_app;

-- Working data the service may also remove.
GRANT SELECT, INSERT, UPDATE, DELETE ON
    exchange_draft, account_block, idempotency_key, outbox, acceptance_network_metadata
    TO exchange_app;
