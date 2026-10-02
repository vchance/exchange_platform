# Migrations

SQL files named `<version>_<description>.sql`, embedded into the binaries at build time and applied by `cargo run --bin migrate`.

- Migrations run as the schema owner (`MIGRATION_DATABASE_URL`). The `api` and `worker` processes connect as the restricted application role `exchange_app` and never run them.
- The role `exchange_app` must exist before migrating; migrations grant to it and fail if it is missing.
- Every new table needs its own explicit `GRANT` to `exchange_app`. Nothing is granted by default, so a table is unreachable by the service until a migration says what the service may do with it.
- A migration that has been applied anywhere is never edited. Add a new one.

## 0001_record_model

The record model of `DESIGN.md` §10 with the rules of §13.2.

**Append-only tables:** `revision`, `revision_attachment`, `contribution_snapshot`, `acceptance`, `exchange_event`. Two independent protections:

- the application role is granted only `SELECT` and `INSERT` on them;
- a trigger refuses `UPDATE`, `DELETE` and `TRUNCATE` for every role, the schema owner included.

Erasure and retention deletes (`DESIGN.md` §14) are still open with counsel. Until that is decided nothing can remove history; the migration that implements it adds the one audited path allowed to.

**Other rules the database enforces:**

- An acceptance must carry the content hash of the revision it signs, and the signer must be the account holding that slot.
- Every reference stays inside one exchange (composite foreign keys on `exchange_id`).
- Event sequence numbers are unique per exchange.
- At most one claimable invitation per exchange.
- Amount, currency and settlement mode appear on money contributions only, in the exchange's currency.
- A contribution can wait only on another contribution in the same revision, never on itself. Longer cycles are the service's job to reject.
- An exchange's state, closed outcome and revision pointers must agree.

`backend/tests/schema.rs` checks each of these against a real database.

**What the service still owns:** computing content hashes, validating timezones, generating display codes, rejecting dependency cycles, checking invitation expiry, and every state transition.
