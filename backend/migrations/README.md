# Migrations

SQL files named `<version>_<description>.sql`, embedded into the binaries at build time and applied by `cargo run --bin migrate`.

- Migrations run as the schema owner (`MIGRATION_DATABASE_URL`). The `api` and `worker` processes connect as the restricted application role `exchange_app` and never run them.
- The role `exchange_app` must exist before migrating; migrations grant to it and fail if it is missing.
- Every new table needs its own explicit `GRANT` to `exchange_app`. Nothing is granted by default, so a table is unreachable by the service until a migration says what the service may do with it.
- A migration that has been applied anywhere is never edited. Add a new one.
- So the role names stay as the migrations wrote them, from before the product was called Yuppers: `exchange_app` for the service, and by convention `exchange` for the owner. A role belongs to the whole PostgreSQL cluster, so a migration cannot rename it either: every other database on the cluster still has to apply `0001` and grant to `exchange_app`. Likewise the `exchange` table and the other names in the schema are the technical model, not the brand.

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

## 0002_auth

One-time codes and sessions (`DESIGN.md` §8). A code is stored as a keyed hash (HMAC under `APP_SECRET`), because six digits are too few to protect with a plain hash; a session token is 256 random bits and stored as a SHA-256 hash. `backend/tests/auth.rs` exercises both through the HTTP endpoints.

## 0003_exchange_projection

What the exchange API needed beyond the first model. Each revision now records both party names as written, since they are part of what is signed. The exchange row gains the state the rules track between commands: a pending end proposal or close request, the inactivity prompt, and last activity. Indexes support the worker's timers.

`backend/tests/exchanges.rs` and `backend/tests/timers.rs` drive the API against databases of their own, created fresh on each run, because agreement history cannot be deleted and so cannot be cleaned up.

## 0004_any_language

The first migration allowed only `en` and `es` as an account's language and as a signer's consent language. Which languages exist is decided by the wording files (`DESIGN.md` §4.2), and adding one must not need a migration, so the database now checks only that the value is shaped like a language tag.

## 0005_text_bounds

Upper bounds on free text, as backstops. Agreement history cannot be deleted, so nothing written into it may be unbounded. The service enforces the real, tighter limits (`domain::Limits`), which can change without a migration.

## 0006_slot_holdings

Someone who opened an invitation that named nobody can be removed, or can leave, before the initiator confirms them (`DESIGN.md` §8). The invited party's slot is then free for someone else, so `participant.account_id` is no longer set once and for all, and a signature can no longer be tied to it: the signature is permanent and the row is not.

**`slot_holding`** has one row for each time an account held a slot: who, from when, and until when if it ended. A signature (`acceptance.holding`) now belongs to a holding.

- The database writes the table itself, from changes to `participant.account_id`. The application role can read it and nothing else; no role can change a holding except to end it once, by emptying the slot.
- Only the invited party's slot can be emptied, and only while the initiator has not confirmed them. A slot is emptied before someone else takes it. A confirmation is never taken back.
- A signature is stamped by the database with the holding open in its slot when it is inserted, and must come from that holding's account. That is the old rule, "the signer is the account holding that slot", made to hold for the past as well.
- One signature per revision, slot and holding: whoever takes a slot after someone was removed from it signs the same revision in their own right.
- A revision comes into force only when both slots' current holders have signed it and the invited party is confirmed. A signature made under a holding that has ended counts for nothing, whoever tries to rely on it.

Nothing is deleted: a removed person's claim event, their signature and their holding all stay, under their account.

**Rejected.** Dropping the foreign key and checking the signer in a trigger alone would have left no record of who held the slot when. Letting several participant rows share a slot would have changed what every other table's reference to a slot means. A "void" flag on the signature would have meant updating an append-only table. Starting a fresh exchange for the next claimant would have changed the exchange ID, which is part of what the initiator signed.

`backend/tests/schema.rs` checks these against the database alone, and `backend/tests/claimant.rs` through the API.

## 0007_reminders

`contribution_reminder`: what each contribution has already been reminded about (`DESIGN.md` §12). A "due soon" or "overdue" reminder is not caused by an event, so the outbox's key of one message per event per person cannot stop it repeating; a row here, written in the same transaction that queues the emails, is what does. The key is the contribution, the kind of reminder and the due date it was about, so an amendment that moves the date makes the new date something not yet reminded about, and one that leaves it alone brings no second reminder.

- The application role may read and add rows and nothing else: a row that could be changed or removed is a reminder that could go out twice.
- It is not agreement history, so no trigger protects it from the schema owner. It does hold a date taken from the agreement, so the erasure and retention path, when it exists, has to remove these rows with the exchange they belong to.
- A reminder writes nothing to `exchange` or `exchange_event`: the version, the history and the last activity stay as they were.

The index on `exchange (timezone)` for active exchanges is for the worker, which reads the date in each timezone that has an active exchange and then looks through that timezone's exchanges.

`backend/tests/schema.rs` checks the key and the grants; `backend/tests/reminders.rs` drives the worker's pass against a database of its own.

## 0008_sign_in_limits

Sign-in hardening (`DESIGN.md` §8, §18 item 4). A new code no longer ends the live ones, so `one_time_code` gains `purpose`: the codes kept live, checked and spent together are one identifier's for one purpose. Codes stored before it are taken as sign-in codes, so a deletion code in flight during the upgrade stops working, which is the safe way round.

**`sign_in_limit`** counts code requests per requester's address or per account, and failed guesses per identifier or per account, one row per thing counted and fixed window (an hour, or a UTC day). The service locks the row while it decides, so concurrent requests cannot both slip under a limit. Whom a row counts is a keyed hash under `APP_SECRET`, never an address or identifier in the clear, and the worker removes windows more than two days old; so deleting an account need not touch the table. The application role may read, add, change and remove rows. `backend/tests/schema.rs` checks the constraints and grants; `backend/tests/auth.rs` and `backend/tests/deletion.rs` the limits through the API. The scope `failed-guesses-by-address` is allowed by the constraint but no longer written: wrong guesses stopped being counted by address (README, "Signing in"), and its old rows go with the worker's purge.

## 0009_load_indexes

Three indexes on `exchange`, for queries the load check (README, "Load check") found reading the whole table, each run often enough that its cost would have grown with every exchange ever made:

- `(created_by, created_at)`: counting an account's exchanges in the last day, on every creation, under the per-account limit.
- `(open_revision_id)` where there is one: the worker's search for expired revisions, on every pass. Without it the search read every revision whose expiry had passed, which in time is nearly every revision ever sent, accepted or not.
- `(inactivity_prompted_at)` where it is set: the worker's search for idle exchanges whose prompt has gone unanswered, on every pass.

The last two are partial, so they hold only the exchanges the worker is looking for. Like every migration this one runs in a transaction, so the indexes are built without `CONCURRENTLY` and writes to `exchange` wait while they build. Against the load check's 4,500 exchanges the whole migration took a tenth of a second.

## 0010_wallet

What Wallet passes need beyond the `wallet_pass` table of 0001 (`DESIGN.md` §11, [docs/wallet.md](../../docs/wallet.md)).

- **`wallet_pass`** gains the SHA-256 of an Apple pass's authentication token (Apple's only: a check refuses one on a Google pass, and anything but 32 bytes), the face's Last-Modified time and the hashes of the face it belongs to and of the face last delivered, and the update queue's columns: a mark counter, when to try next, the worker's lease, the attempts and the last error. The pass row is its own queue entry, marked in the transaction of every change to its exchange. Its serial (`external_id`) must fit both Apple's and Google's rules. The application role keeps the grants of 0001: it reads, adds and updates passes, and never deletes one; a revoked pass is voided, not removed.
- **`wallet_device_registration`**: the devices that asked Apple's pass web service for a pass's updates, with their push tokens, which the service adds, updates and removes as devices and Apple say. Working data, granted in full.

`backend/tests/schema.rs` checks the constraints and the grants; `backend/tests/wallet.rs` everything through the API.

## Outside the database

**What the service still owns:** computing content hashes, validating timezones, generating display codes, rejecting dependency cycles, checking invitation expiry, and every state transition.
