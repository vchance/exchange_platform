#!/bin/sh
# Restores a backup made by scripts/backup.sh (docs/operations.md,
# "Restoring").
#
#   scripts/restore.sh [--overwrite] [--no-replay-needed] [-d DATABASE_URL] FILE
#
# Connects as the schema owner: -d, or else MIGRATION_DATABASE_URL. The
# database must exist, and should be a new, empty one owned by that role:
#
#   CREATE DATABASE yuppers_restored OWNER exchange;
#
# It refuses a database that already holds tables, unless given
# --overwrite, which drops and recreates every object the backup holds.
# Objects the backup does not hold are left alone, so restoring an older
# backup over a newer schema leaves a mixture; a new database is the safe
# way.
#
# The application role (APP_ROLE, default exchange_app) must already exist on
# the server, because roles are not part of a backup; create it as
# docker/postgres-init.sql does, with a password of its own. The backup's
# grants are restored as they were: the application role ends with exactly
# what the migrations gave it, and the append-only triggers come back with
# the tables. Every object belongs to the role that restores it, whatever its
# name was where the backup was made.
#
# A restore brings back accounts deleted since the backup. Replay the
# deletion log afterwards with scripts/replay-deletions.sh. Until it has
# run, the restored database is marked as waiting for it (restore_marker,
# migration 0018), and the api and the worker refuse to start on it;
# replay-deletions clears the mark once every account in the log is deleted.
# --no-replay-needed marks it as needing no replay instead, for an emergency
# where whoever restores knows that no account was deleted since the backup;
# the mark records that it was used, and when.
#
# Afterwards it checks that the application role holds exactly the grants,
# and the database exactly the triggers, that the migrations the backup
# holds give (scripts/restore-inventory.txt, printed by
# scripts/restore-inventory.sql), and fails if not.
#
# The restore is one transaction: it either completes or leaves the database
# as it was. Set PG_BIN to the directory holding pg_restore and psql if those
# on the PATH are older than the server.

set -eu

usage() {
    echo "usage: $0 [--overwrite] [--no-replay-needed] [-d DATABASE_URL] FILE" >&2
    exit 2
}

url="${MIGRATION_DATABASE_URL:-}"
overwrite=no
replay=needed
file=
while [ $# -gt 0 ]; do
    case "$1" in
        --overwrite) overwrite=yes ;;
        --no-replay-needed) replay=not-needed ;;
        -d)
            [ $# -ge 2 ] || usage
            url="$2"
            shift
            ;;
        -h | --help) usage ;;
        -*) usage ;;
        *)
            [ -z "$file" ] || usage
            file="$1"
            ;;
    esac
    shift
done
[ -n "$file" ] || usage
if [ -z "$url" ]; then
    echo "$0: no database: give -d DATABASE_URL or set MIGRATION_DATABASE_URL" >&2
    exit 2
fi
if [ ! -r "$file" ]; then
    echo "$0: cannot read $file" >&2
    exit 2
fi

bin="${PG_BIN:+$PG_BIN/}"
app_role="${APP_ROLE:-exchange_app}"
here=$(dirname -- "$0")
inventory="$here/restore-inventory.txt"
if [ ! -r "$inventory" ] || [ ! -r "$here/restore-inventory.sql" ]; then
    echo "$0: cannot read $inventory or restore-inventory.sql beside it" >&2
    exit 2
fi
backup_name=$(basename -- "$file")

# The query goes in on standard input, where psql substitutes its variables
# (it does not in --command): the role's name and the backup's reach the
# server only as :'app_role' and :'backup_name', quoted by psql, never pasted
# into the text.
sql() {
    printf '%s\n' "$1" | "${bin}psql" --no-psqlrc --quiet --tuples-only --no-align \
        --set ON_ERROR_STOP=1 --set app_role="$app_role" --set backup_name="$backup_name" \
        --dbname="$url" --file=-
}

# Readable as a backup before anything is touched.
"${bin}pg_restore" --list "$file" >/dev/null

role=$(sql "SELECT count(*) FROM pg_roles WHERE rolname = :'app_role'")
if [ "$role" != "1" ]; then
    echo "$0: the role $app_role does not exist on this server. Create it first" >&2
    echo "(docker/postgres-init.sql), or set APP_ROLE to its name." >&2
    exit 1
fi

tables=$(sql "SELECT count(*) FROM pg_class c JOIN pg_namespace n ON n.oid = c.relnamespace
              WHERE c.relkind IN ('r', 'p', 'v', 'm', 'S')
                AND n.nspname NOT IN ('pg_catalog', 'information_schema')
                AND n.nspname NOT LIKE 'pg_toast%' AND n.nspname NOT LIKE 'pg_temp%'")
clean=
if [ "$tables" != "0" ]; then
    if [ "$overwrite" != "yes" ]; then
        echo "$0: the database already holds $tables tables, sequences or views; refusing to" >&2
        echo "restore over them. Restore into a new database, or pass --overwrite to replace" >&2
        echo "every object the backup holds." >&2
        exit 1
    fi
    clean="--clean --if-exists"
fi

# --no-owner: objects belong to whoever restores them, so a backup from a
# server where the owner had another name restores all the same. Grants are
# kept (no --no-acl): they are what limits the application role.
# shellcheck disable=SC2086 # $clean is two words or none.
"${bin}pg_restore" --no-owner --single-transaction --exit-on-error $clean \
    --dbname="$url" "$file"

# The mark first, so that whatever fails below, the api and the worker do
# not start on this copy before someone has looked. A backup made before
# migration 0018 has no table for it: it is made here as 0018 makes it, and
# 0018 then finds it there.
created_marker=no
if [ "$(sql "SELECT to_regclass('public.restore_marker') IS NULL")" = "t" ]; then
    created_marker=yes
    sql "CREATE TABLE restore_marker (
             id      bigint GENERATED ALWAYS AS IDENTITY PRIMARY KEY,
             event   text NOT NULL CHECK (event IN (
                         'REPLAY_PENDING', 'REPLAY_NOT_NEEDED', 'REPLAYED')),
             at      timestamptz NOT NULL DEFAULT now(),
             by_role text NOT NULL DEFAULT session_user,
             note    text CHECK (char_length(note) <= 1000));
         CREATE TRIGGER restore_marker_append_only
             BEFORE UPDATE OR DELETE OR TRUNCATE ON restore_marker
             FOR EACH STATEMENT EXECUTE FUNCTION forbid_change();
         GRANT SELECT ON restore_marker TO :\"app_role\";" >/dev/null
fi
if [ "$replay" = "needed" ]; then
    sql "INSERT INTO restore_marker (event, note)
         VALUES ('REPLAY_PENDING', 'restore.sh ' || :'backup_name')" >/dev/null
else
    sql "INSERT INTO restore_marker (event, note)
         VALUES ('REPLAY_NOT_NEEDED', 'restore.sh --no-replay-needed ' || :'backup_name')" >/dev/null
fi

# What must hold afterwards, checked rather than assumed: every grant the
# application role holds, and every trigger, exactly as the migrations the
# backup holds give them. Nothing missing (an append-only trigger, say), and
# nothing more (a grant someone added by hand to the database backed up).
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
trap 'exit 130' INT TERM
applied=$(sql "SELECT coalesce(max(version), 0) FROM _sqlx_migrations WHERE success")
awk -v applied="$applied" '!/^#/ && NF { if ($1 + 0 <= applied + 0) { sub(/^[^ ]+ /, ""); print } }' \
    "$inventory" | LC_ALL=C sort >"$work/expected"
sql "$(cat "$here/restore-inventory.sql")" | LC_ALL=C sort >"$work/held"
if [ "$created_marker" = "yes" ]; then
    # Made above, not by the backup's migrations.
    grep -v ' restore_marker ' "$work/held" >"$work/held.without" || true
    mv "$work/held.without" "$work/held"
fi
if ! diff -u "$work/expected" "$work/held" >"$work/difference"; then
    cat "$work/difference" >&2
    echo "$0: restored, but $app_role's grants or the triggers are not what the $applied" >&2
    echo "migrations of the backup give (above: - missing, + not expected). Do not use" >&2
    echo "this copy until that is understood." >&2
    exit 1
fi

migrations=$(sql "SELECT count(*) || ' migrations, the latest ' || max(version)
                  FROM _sqlx_migrations WHERE success")
exchanges=$(sql "SELECT count(*) FROM exchange")
events=$(sql "SELECT count(*) FROM exchange_event")
held=$(wc -l <"$work/held" | tr -d ' ')
echo "restored $file: $migrations; $exchanges exchanges, $events events; grants and triggers as expected ($held)"
if [ "$replay" = "needed" ]; then
    # Accounts deleted since the backup are live again in the copy until the
    # deletion log is replayed (docs/operations.md, "Restoring").
    echo "next: run migrate, then scripts/replay-deletions.sh with the newest deletion log." >&2
    echo "Until it has run, the api and the worker refuse to start on this database." >&2
else
    echo "marked as needing no replay (--no-replay-needed, recorded in restore_marker):" >&2
    echo "accounts deleted since the backup stay live here. Run migrate next." >&2
fi
