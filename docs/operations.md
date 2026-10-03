# Operations

How to run the service: the first deployment, what to check and watch, backups and the restore drill, rotating the secret, and what to do when the worker stops. Nothing here assumes a particular host. A deployment is a managed PostgreSQL database, a container platform that can run one image three ways, a reverse proxy or load balancer that terminates TLS, and an SMTP account; for codes to phone numbers, an SMS provider's account, and for push notifications, an Expo project ("Text messages and push notifications", below). `.env.example` documents every setting with its default; README, "Deploying", says what the image is.

## The processes

| Process | Runs | Listens on |
|---|---|---|
| `migrate` | once per release, before the others start, then exits | nothing |
| `api` | always; as many copies as the load needs | `BIND_ADDR` (the image sets `0.0.0.0:8080`), and `METRICS_ADDR` if set |
| `worker` | always; one copy | `METRICS_ADDR` if set, nothing else |

All three come from the same image: the `api` is its default command, the other two are `/usr/local/bin/worker` and `/usr/local/bin/migrate`. The image has no shell, so health checks are HTTP requests made by the platform, and backups run from somewhere else (below).

## First deployment

1. **The database.** Create a PostgreSQL 17 database and two roles: the schema owner (here `exchange`), which owns the database and runs migrations, and the application role `exchange_app`, which may log in and nothing more. Each gets a long random password of its own.

   ```sql
   CREATE ROLE exchange LOGIN PASSWORD '...';
   CREATE ROLE exchange_app LOGIN PASSWORD '...';
   CREATE DATABASE yuppers OWNER exchange;
   ```

   On a managed service the owner may be the role the service gives you; what matters is that the API and worker never connect as it. `exchange_app` must exist before the first migration, which grants to it, and must have exactly that name: the migrations name it, and they are never edited once applied. (The roles predate the name Yuppers; the database's name is free.) Require TLS to the database if the service offers it (`?sslmode=require` on both connection strings).

2. **Secrets.** In the platform's secret store, never in the image or the repository:
   - `DATABASE_URL`: `exchange_app`'s connection string, for the api and the worker.
   - `MIGRATION_DATABASE_URL`: the owner's, for `migrate` only.
   - `APP_SECRET`: `openssl rand -hex 32`.
   - `SMTP_PASSWORD`, and `SMTP_USERNAME` if the provider treats it as secret.
   - When they are switched on: `SMS_AUTH_TOKEN` for the api, and `EXPO_ACCESS_TOKEN` for the worker if the Expo project has push security on.
   - Once Wallet passes are wanted: `APPLE_PASS_KEY` and `GOOGLE_WALLET_SERVICE_ACCOUNT` ([docs/wallet.md](wallet.md)).

3. **Settings** (plain environment):
   - `WEB_ORIGIN`: the public HTTPS origin, such as `https://app.example.com`, without a trailing slash. Cookie sessions are honored only from it, emails link into it, and because it is HTTPS every response carries HSTS.
   - `CODE_DELIVERY=smtp`, `NOTIFICATION_DELIVERY=smtp`, and `SMTP_HOST`, `SMTP_PORT`, `SMTP_TLS` (`tls` for port 465, `starttls` for 587), `SMTP_FROM`. The sending domain needs the provider's SPF and DKIM records, or codes land in spam.
   - `TRUSTED_PROXY_HEADER`: the header the proxy in front of the API sets to the client's address, and `TRUSTED_PROXIES` if more than one proxy appends to `X-Forwarded-For`. Without it every signature records the proxy's address (`DESIGN.md` §8) and the per-address sign-in limits count every person as one. Name a header only if the proxy always sets it and clients cannot reach the API around the proxy; otherwise a client can choose its own address.
   - The `SIGN_IN_*` limits only if the placeholders in `.env.example` do not suit (README, "Deploying").
   - `SMS_DELIVERY` and `PUSH_DELIVERY` stay unset (off) until their accounts exist; "Text messages and push notifications" says how to switch each on.
   - `LOG_FORMAT=json` if a log collector reads the output; `RUST_LOG` stays `info`.
   - `METRICS_ADDR=0.0.0.0:9100` on the api and the worker, if something will scrape them (below).
   - The Wallet settings, on the api and the worker, once the accounts exist, with `WALLET_DELIVERY=live` ([docs/wallet.md](wallet.md)). Until then, none of them.

4. **Migrate.** Run the image with `/usr/local/bin/migrate` and `MIGRATION_DATABASE_URL`. It exits 0 with `migrations applied`. Safe to run again; every release runs it before the new api and worker start.

5. **The worker.** One copy, `/usr/local/bin/worker`, with `DATABASE_URL`, `WEB_ORIGIN`, the delivery and SMTP settings, and `PUSH_DELIVERY` (with `EXPO_ACCESS_TOKEN`) once push is on. It logs `worker started`. Give it a stop grace period of at least 35 seconds: on `SIGTERM` it finishes the message it is sending (at most 30 seconds) and exits 0.

6. **The api.** The image's default command, with `DATABASE_URL`, `APP_SECRET`, `WEB_ORIGIN`, `CODE_DELIVERY`, the SMTP settings and `TRUSTED_PROXY_HEADER`, and once they are on, the `SMS_*` settings and the same `PUSH_DELIVERY` as the worker. It serves the web app itself from `/srv/web`. Point the platform's health check at `/readyz` (below). Any number of copies; each holds up to 10 database connections, so copies × 10 plus the worker's 10 must stay under the database's connection limit.

7. **TLS at the proxy.** The proxy or load balancer terminates HTTPS for `WEB_ORIGIN`'s host and forwards plain HTTP to port 8080, adding the header named in `TRUSTED_PROXY_HEADER`. Redirect HTTP to HTTPS there. Do not route the metrics port through it.

8. **Check it.** `https://<origin>/healthz` and `/readyz` answer 204; the home page loads; sign in with a real address and the code arrives; an exchange between two test accounts sends both their notification emails within a few seconds. Each response carries an `X-Request-Id`.

## Text messages and push notifications

Both are built and off: with `SMS_DELIVERY` and `PUSH_DELIVERY` unset nothing changes from an email-only deployment. Each is switched on by settings alone, once its account exists. README, "Notifications" and "Signing in", says what each does.

**Codes by text message.**

1. Open an account with Twilio (or write another `SmsSender` for another provider: `backend/src/notifications/sms.rs`). Buy a number able to send SMS in the countries served, or create a Messaging Service; for US numbers, register the brand and campaign (A2P 10DLC) or verify a toll-free number, without which carriers block or filter the messages. Restrict the account's geographic permissions to those countries: for a US launch, the United States only (add Canada if it is served). `SMS_ALLOWED_COUNTRY_CODES` (default `+1`) is the service's own check, by country calling code, and `+1` also covers Canada and some twenty Caribbean and Pacific countries, whose messages cost more; the provider's permissions are what keep those out, and a message the provider refuses for them uses up no place under the caps.
2. On the api: `SMS_DELIVERY=twilio`, `SMS_ACCOUNT_SID`, `SMS_AUTH_TOKEN` (a secret), `SMS_FROM` (`+1...` or `MG...`), `SMS_MAX_PER_HOUR` if 50 an hour is not right, `SMS_MAX_PER_PREFIX_PER_HOUR` if 10 an hour to one area code is not, and `SMS_ALLOWED_COUNTRY_CODES` if countries other than `+1` are served. The api logs at start how many it may send an hour.
3. Check: ask for a code for a phone you hold; it arrives in one message, in the account's or the browser's language, and `yuppers_sms_codes_this_hour{result="sent"}` counts it.

With SMS off, a code for a phone number is refused as unavailable, as before; sign-in and deletion screens still offer phone numbers. Costs to watch are in "What to watch".

**Push notifications.**

1. The Expo project, the APNs key and the Firebase credentials, as docs/mobile-release.md, "Once the accounts exist", step 6, says; and a build of the app made after them.
2. On the api and the worker: `PUSH_DELIVERY=expo`. On the worker, `EXPO_ACCESS_TOKEN` if push security is on for the project. The api then tells the apps to offer notifications; the worker sends them, reads Expo's receipts about fifteen minutes later, and removes devices Expo says are gone.
3. Check: on a device, turn notifications on, have the other party act, and see `yuppers_push_deliveries_total{result="sent"}` go up and the notification arrive.

`PUSH_DELIVERY=log` on both is for development: the worker writes each notification to its log. Turning push off again closes whatever is queued for push unsent, and the apps stop offering it; the devices stay registered, harmlessly, until their sessions end.

## Health checks

| Path | Answers | Use it for |
|---|---|---|
| `GET /healthz` | 204 while the process runs | liveness: restart the copy if it fails |
| `GET /readyz` | 204 when the database answers, 503 when not | readiness: send traffic only when it passes |

The api starts without a database and answers `/readyz` with 503 until it can reach one, so a database outage takes copies out of rotation rather than restarting them in a loop. The worker has no health path; with `METRICS_ADDR` set, its `/metrics` answers while it runs, and `yuppers_worker_last_pass_timestamp_seconds` says when it last went round its jobs (every 5 seconds).

## Logs

Every process writes to standard output, one line per event. `LOG_FORMAT=text` (the default) is for reading; `LOG_FORMAT=json` writes one JSON object per line: `timestamp`, `level`, `target`, `message`, the event's own fields, and for anything logged while handling a request, `span` with that request's `method`, `path` and `request_id`.

Each request writes one line when it is answered, `request completed`, with:

| Field | What it is |
|---|---|
| `method`, `path` | The method and the path, never the query string |
| `status` | The status code returned |
| `latency_ms` | Time to answer, in milliseconds to the microsecond |
| `request_id` | `X-Request-Id` from the request if it is 1 to 64 letters, digits, `-`, `_` or `.`; otherwise a new UUID. Returned in the response's `X-Request-Id`, and on every other line logged for that request |

A proxy that sets its own request ID ties its logs to the service's that way.

**Never logged:** request or response bodies, query strings, headers other than the request ID (so no `Authorization`, no cookie), session or invitation tokens, one-time codes, email addresses, phone numbers. A database error is logged by its SQLSTATE, constraint and table, never the server's message, which can quote a value. An email that could not be sent is logged by its outbox ID with the SMTP reply code only; the same goes into `outbox.last_error`. A push notification likewise, with the HTTP status and Expo's error code, and a text message that could not be sent with the HTTP status and Twilio's error code: never the provider's message, which quotes the token or the number. `backend/tests/telemetry.rs` signs a person in at every log level and checks that the output holds none of their address, phone number, codes, token or cookie; `backend/tests/smtp.rs` does the same for a refused recipient, and `backend/tests/sms.rs` for a refused text message.

The one exception is the development deliveries, `CODE_DELIVERY=log`, `NOTIFICATION_DELIVERY=log`, `SMS_DELIVERY=log` and `PUSH_DELIVERY=log`, which write each code, email, text message and push notification to the log because that is their job. Even they write a phone number masked (`+1••••••••67`) and a push token by its first characters only. A deployment has to choose `CODE_DELIVERY` and `NOTIFICATION_DELIVERY`, so it never gets the log by default; check that both say `smtp`, and that `SMS_DELIVERY` and `PUSH_DELIVERY` are unset, `twilio` and `expo`.

Useful lines besides requests: `api listening`, `worker started`, `worker shutting down`, `notifications delivered` and `push notifications delivered` (counts per pass), `notification not sent; will retry`, `push notification not sent; will retry`, `notification given up on` and `push notification given up on` (with the outbox ID), `push receipts read` and `push receipts could not be read`, `devices of ended sessions removed`, `push tickets past their receipts removed`, `push notifications are off (PUSH_DELIVERY)` at the worker's start, `one-time code could not be delivered`, `a code was not sent by SMS: an hourly cap is reached` (with `whose`: the whole service, or numbers beginning alike), `timers ran`, `reminders queued`, `issuing Wallet passes` (at start, naming the platforms), `wallet passes updated`, `wallet pass not updated; will retry` and `wallet pass update given up on` (with the pass's ID), `database error`, `readiness check failed`.

## Metrics

Off unless `METRICS_ADDR` is set. Then the process serves `GET /metrics` in the Prometheus text format on that address, a listener of its own: it is never on the API's port, so publishing the API cannot publish the metrics by accident. Keep the metrics port on the private network, reachable only by the scraper. The api and the worker each have their own; a scraper collects both.

From the api:

| Metric | Type | Labels |
|---|---|---|
| `yuppers_http_requests_total` | counter | `route` (the route's template, such as `/v1/exchanges/{id}`, or `unmatched` for the web app's pages and unknown paths), `method`, `status` (`2xx`, `4xx`, ...) |
| `yuppers_http_request_duration_seconds` | histogram, buckets from 1 ms to 10 s | the same |
| `yuppers_sms_codes_this_hour` | gauge, only with `SMS_DELIVERY` on | `result`: `sent` (taken by the provider, each a message paid for), `refused` (not sent, for any of the reasons below), `failed` (the provider did not take it; not in `sent`, and it uses up no place under the caps). For the whole service in the current UTC hour, read from the database at each scrape. |
| `yuppers_sms_codes_refused_this_hour` | gauge, the same | `reason`: `hourly_cap` (`SMS_MAX_PER_HOUR`), `prefix_cap` (`SMS_MAX_PER_PREFIX_PER_HOUR`, numbers beginning alike), `country` (the number's country is not in `SMS_ALLOWED_COUNTRY_CODES`) |
| `yuppers_sms_codes_hourly_cap` | gauge, the same | `SMS_MAX_PER_HOUR` |

From the worker:

| Metric | Type | Labels |
|---|---|---|
| `yuppers_outbox_deliveries_total` | counter | `result`: `sent`, `failed` (every failed try), `given_up` (the last try failed), `dropped` (closed unsent: recipient gone, or a reminder no longer true). Emails. |
| `yuppers_push_deliveries_total` | counter | The same results for push notifications, one per person and notice; `sent` once the push service took it for at least one of their devices, `dropped` also when no device is left or push is off |
| `yuppers_push_devices_removed_total` | counter | devices removed because the push service said their token is no longer registered |
| `yuppers_push_receipt_checks_total` | counter | `result`: `ok`, `error`: requests for the receipts of earlier notifications |
| `yuppers_worker_runs_total` | counter | `job`: `timers`, `reminders`; `result`: `ok`, `error` |
| `yuppers_worker_timer_changes_total` | counter | expiries, lapsed close requests, inactivity prompts and closures |
| `yuppers_worker_reminders_queued_total` | counter | |
| `yuppers_worker_last_pass_timestamp_seconds` | gauge | when the last pass over all jobs ended |

From both, read from the database at each scrape (so they are right however many processes send, and the api still shows them while the worker is down):

| Metric | Type | |
|---|---|---|
| `yuppers_outbox_messages` | gauge | `state`: `pending` (waiting, or between retries), `given_up`; emails and push notifications together |
| `yuppers_outbox_oldest_pending_age_seconds` | gauge | how long the oldest pending message has waited since it was queued; 0 when none |
| `yuppers_database_up` | gauge | 0 when that read failed |
| `yuppers_db_pool_max`, `yuppers_db_pool_size`, `yuppers_db_pool_in_use` | gauge | this process's connection pool: its limit, connections open, connections busy |

## What to watch

Starting points; tune them once there is real traffic.

- **Outbox age.** `yuppers_outbox_oldest_pending_age_seconds` above 10 minutes. A message normally goes within one 5-second pass; a failure waits 1, 2, 4 ... minutes, up to an hour, so a single retrying message can legitimately be older, but a rising age with a growing `pending` count means mail is not going out. Check the worker is running, then its `notification not sent` lines for the SMTP reply code.
- **Given up.** `yuppers_outbox_messages{state="given_up"}` above 0. After 8 failed tries a message is left for someone to look at (below).
- **Error rate.** `5xx` responses above 1% of `yuppers_http_requests_total` over 5 minutes, or any sustained run of them; then the api's `database error` lines.
- **Latency.** The 95th percentile of `yuppers_http_request_duration_seconds` above 500 ms for a route. The load check (README) measured under 20 ms on a laptop.
- **Pool.** `yuppers_db_pool_in_use` at `yuppers_db_pool_max` for minutes: requests are queuing for connections.
- **Worker alive.** `time() - yuppers_worker_last_pass_timestamp_seconds` above 60, or its scrape failing.
- **Readiness** failing on every copy: the database is unreachable.
- **Text messages, which cost money.** `yuppers_sms_codes_this_hour{result="sent"}` against the cap, and its daily sum against the budget: at the default cap of 50 an hour the service can send at most 1,200 a day, about $10 to $20 a day at US prices in 2026 (the provider's per-message price plus carrier fees; check the provider's price list, and international numbers cost several times more). Any `refused` means people asking for a code by phone were turned away; `yuppers_sms_codes_refused_this_hour` says why. `hourly_cap`: either real demand above the cap, which is the cue to raise `SMS_MAX_PER_HOUR`, or someone sending codes to numbers that are not theirs (SMS pumping), which the provider's fraud tools and its geographic permissions (allow only the countries you serve) are for. `prefix_cap` from one or two area codes at a time is more likely the latter; spread over many, demand, and the cue to raise `SMS_MAX_PER_PREFIX_PER_HOUR`. `country`: people abroad trying to sign in by phone, or someone trying numbers the service will not text; neither costs anything. `failed` above a few in an hour: the provider is refusing; its error code is in the api's log.
- **Push.** `yuppers_push_deliveries_total{result="given_up"}` growing, or `yuppers_push_receipt_checks_total{result="error"}` most of the time: Expo is refusing or unreachable; the error code is in the worker's log. A jump in `yuppers_push_devices_removed_total` after a release can mean the app's project or credentials changed and every token stopped working.
- **Wallet passes**, once on: `wallet pass update given up on` in the worker's log, or rows in `wallet_pass` with `update_status = 'FAILED'` (`last_error` says why; APNs refusing the certificate means it expired or was revoked). And the pass type certificate's expiry date, which the api warns about at start 30 days ahead and refuses to start past.
- **Refusals** are not errors: `429` is a limit working (too many codes asked for, too many wrong guesses), and `4xx` in general is a person or a client being told no. Watch them for sudden jumps, not as failures.

## When the worker is down

Requests keep working: people can sign in, sign and record deliveries. What stops:

- **Notification emails and push notifications** queue in the outbox; nothing is lost. One-time codes, by email or text message, are sent by the api itself, so sign-in is unaffected.
- **Timers**: unanswered revisions do not expire, close requests do not lapse into closing as unresolved, idle exchanges are not prompted or closed.
- **Reminders** of contributions due soon or overdue are not sent.
- **Wallet passes** are not updated; each catches up with the latest face when the worker is back.
- **Purges**: network addresses and user agents older than 90 days (`DESIGN.md` §14) and old sign-in counts are not removed, so a long outage keeps personal data past its retention period.

To recover:

1. Look at its last log lines and exit code. A worker that will not start says why (a missing setting, wording that cannot be read, a metrics address in use); one that cannot reach the database keeps running and logs `timers failed` with `database error: pool timed out` or similar.
2. Restart it. On its first pass it applies every timer whose time has passed, queues the reminders still due, purges what is past retention, then drains the outbox at up to 100 messages per pass (each pass stops taking new messages after 20 seconds, so a slow SMTP server cannot hold up the timers) until `pending` is back near 0.
3. One worker is the normal setup. Delivering is safe from several at once (each message is locked while it is sent), so a second copy started during a handover does no harm.
4. If messages were given up on while SMTP was failing, send them again once it works. They hold no personal data, only which notice and which exchange:

   ```sql
   -- As the owner. Shows what failed and why (the SMTP reply code only).
   SELECT id, exchange_id, attempts, last_error, created_at FROM outbox
   WHERE completed_at IS NULL AND attempts >= 8 ORDER BY id;

   -- Gives them another round of tries.
   UPDATE outbox SET attempts = 0, available_at = now()
   WHERE completed_at IS NULL AND attempts >= 8;
   ```

   A reminder that is no longer true is dropped when it is retried, not sent.

## Backups

`scripts/backup.sh` writes the whole database to one file in `pg_dump`'s custom format: schema, data, and the grants to `exchange_app`. It runs against the live database without stopping anything; the file is one consistent snapshot.

```sh
MIGRATION_DATABASE_URL=postgres://exchange:...@db.internal:5432/yuppers \
  scripts/backup.sh /backups/yuppers-$(date -u +%Y%m%d).dump
```

- With no file named it writes `yuppers-<UTC time>.dump` in the current directory. It refuses to replace a file that is already there, unless given `--force`, and refuses a directory even then. It writes the dump under a fresh name beside the target first, created with `mktemp` so that a link someone left in the directory is never followed, readable by the running user alone, and moves it into place only once `pg_restore` can read it.
- It needs `pg_dump` and `pg_restore` of the server's major version or newer (PostgreSQL 17); the image does not contain them. Run it from a small scheduled job in the same network, for example a `postgres:17` container with the repository's `scripts/` mounted, or set `PG_BIN` to where the tools are.
- A connection string with a password in it is visible to other users of the same machine while the command runs. On a shared machine leave the password out of the URL and put it in `PGPASSWORD` or a `.pgpass` file.
- Managed databases take their own snapshots and point-in-time recovery; keep those on. These files are the copy that does not depend on the provider, that can be restored anywhere, and that the drill below proves.
- **What the file holds**: every account's email address and phone number, every agreement, signature and the network addresses recorded with signatures. Encrypt it at rest, keep it where access is as narrow as the database's, and never in the repository (`.gitignore` refuses `*.dump`).
- **How long to keep them**: the service forgets network metadata after 90 days and deleted accounts' contact details at once (`DESIGN.md` §14); a backup keeps whatever it held when it was taken. Keep backups no longer than the retention the privacy policy states, and see "Restoring" for what a restore brings back.

## Restoring

`scripts/restore.sh` restores a backup into a database, in one transaction: it completes or changes nothing.

```sh
# As the owner, on the server to restore to. The application role must exist
# there first, with a password of its own; roles are not in a backup.
psql "$ADMIN_URL" -c "CREATE ROLE exchange_app LOGIN PASSWORD '...'"   # if it does not exist
psql "$ADMIN_URL" -c "CREATE DATABASE yuppers_restored OWNER exchange"

scripts/restore.sh -d postgres://exchange:...@db.internal:5432/yuppers_restored yuppers.dump
```

- `APP_ROLE` names the application role if it is not `exchange_app`. It reaches the server only as a value psql quotes (`:'app_role'`), never pasted into a query.
- It refuses a database that already holds tables. `--overwrite` replaces every object the backup holds instead, but leaves alone anything it does not, so a new, empty database is the safe target; point `DATABASE_URL` and `MIGRATION_DATABASE_URL` at it when it is ready.
- **Grants**: the backup carries the grants the migrations gave `exchange_app`, and the restore applies them as they were. Afterwards `exchange_app` holds exactly those: `SELECT` and `INSERT` on the five append-only tables (`revision`, `revision_attachment`, `contribution_snapshot`, `acceptance`, `exchange_event`) and `contribution_reminder`; `SELECT`, `INSERT`, `UPDATE` on the current-state tables; `SELECT` only on `slot_holding`; `SELECT`, `INSERT`, `UPDATE`, `DELETE` on working data (drafts, blocks, idempotency keys, the outbox, network metadata, codes, sessions, sign-in counts, devices registered for push and their push tickets). `backend/tests/schema.rs` asserts these. Do not restore with `--no-acl` or as another application role: the service would then have no rights, or the wrong ones.
- **Owner**: every object belongs to the role that ran the restore, whatever the owner was called where the backup was made, so a backup moves between servers whose owner roles have different names.
- **Triggers**: the append-only triggers come back with their tables and refuse `UPDATE`, `DELETE` and `TRUNCATE` for every role again. The restore loads rows before creating triggers, so loading history does not trip them and the stamps the database writes (slot holdings) are restored as stored, not recomputed. The script checks afterwards that all five append-only triggers exist and that `exchange_app` cannot change those tables, and fails if not.
- **Migrations**: the backup includes `_sqlx_migrations`, so `migrate` against the restored database applies only what is newer than the backup. Restore with the release that made the backup or a newer one, never an older one.
- **What a restore brings back**: everything as it was at the backup. Accounts deleted since then return with their contact details, sessions revoked since then are live again, and network metadata purged since then is back until the worker's next pass purges it again. After restoring a production backup, deletions made since the backup must be applied again. That list does not exist anywhere but in the lost database; how to handle that must be decided before real use (below).

## The restore drill

A backup is only known to work once it has been restored. Do this on a schedule, monthly at least, and after any change to the schema or the database's settings, on a scratch database that is not the one the service uses:

1. Take a backup with `scripts/backup.sh`, or pick last night's.
2. Create an empty database and restore into it with `scripts/restore.sh`. Note how long it took: that, plus starting the processes, is the time to recover.
3. `scripts/check-restore.sh SOURCE_URL RESTORED_URL` compares the two: migrations, every privilege of `exchange_app`, every trigger, each table's row count and a digest of its rows. Against the live database the counts and digests differ by what has been written since the backup; against a database restored from the same file they must match exactly.
4. `scripts/check-restored-record.sh RESTORED_URL RESTORED_APP_URL` starts an api on the copy, reads the oldest agreement in force through the API, and checks that its stored terms still reproduce the hash that was signed. It writes a session into the copy and deletes it again when it finishes, whether it passed or not; still, never point it at the database the service uses. The session's token is never on a command line: psql reads it on standard input, and curl from a header file only the running user can read.
5. Drop the scratch database and the backup copy you made for it.

CI runs the same steps on every change (the `Backup and restore` job): it fills a database through the API with the load check, backs it up, checks that `backup.sh` will not replace that file without `--force`, that `restore.sh` refuses the non-empty source, restores into a new database, compares them with `check-restore.sh`, restores again with `--overwrite` and compares again, runs `backend/tests/schema.rs` against the copy, and reads a signed agreement back from it.

## Rotating `APP_SECRET`

`APP_SECRET` keys the hashes of one-time codes and the hashes that sign-in limits are counted under, and the tokens of Wallet passes. It does not touch sessions, invitation links, signatures or content hashes, and the worker does not use it.

To rotate it, set the new value and restart every api copy together (a rolling restart works, but while old and new copies both run, a code sent by one is refused by the other). What it invalidates:

- **Codes in flight**: every sign-in, deletion and new-identifier code already sent stops working. People ask for a new one; nobody is signed out.
- **Sign-in limit counts**: the counts for the current hour and day start again from zero, since they are kept under the old secret's hashes. The old rows are removed by the worker within two days.
- **Wallet passes**: links to download an Apple pass that are in flight (ten minutes at most) stop working. Passes already on devices keep working, since only their tokens' hashes are checked; one handed out again after the rotation gets a new token, and other devices' copies of that pass then stop updating until it is added again.

Rotate it if it may have leaked: anyone with it and a copy of the database could test guesses at codes offline, and could tell which identifiers a count belongs to.

## Upgrading

1. Take a backup.
2. Run `migrate` from the new image.
3. Roll out the worker and the api from the new image.

Between steps 2 and 3 the old processes run against the new schema for a few minutes. A release whose migration the old processes cannot live with says so in its notes, and then the old processes are stopped before step 2.

## Decisions still open

- How long backups are kept, and where, given the retention in `DESIGN.md` §14.
- What happens to deletions made between a backup and a restore of it. Today they would have to be redone by hand from a record nobody keeps.
- The alert thresholds above are placeholders until there is real traffic.
- **Push through Expo, or straight to Apple and Google.** `DESIGN.md` §13.1 decides that app push goes directly from the backend to Apple's and Google's services, with no third party. This build sends through Expo's push service instead, which holds the APNs key and FCM credentials, sees each token and the generic text, and needs nothing from Apple or Google on the service. Going direct means an APNs adapter (HTTP/2, a signed JWT per hour) and an FCM v1 adapter (OAuth with a service account), and the app registering device tokens (`getDevicePushTokenAsync`) instead of Expo tokens; the `PushSender` interface and the `device.service` column are where they would go.
- **Email and push both.** Someone with the app and an email address gets both for each notice, one per channel, as §12 reads. Sending the email only when the push was not delivered or not opened within some time would halve that, at the cost of the email's detail; the outbox could hold an email back for that time.
- **The SMS caps**, 50 an hour and 10 an hour per area code, are placeholders like the sign-in limits.
- **`+1` is more than the US.** `SMS_ALLOWED_COUNTRY_CODES=+1` takes every number of the North American Numbering Plan. Refusing the Caribbean and Pacific area codes in the service as well would need a list of area codes kept up to date; for now the provider's geographic permissions do it.
