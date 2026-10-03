#!/bin/sh
# Restores a backup made by scripts/backup.sh (docs/operations.md,
# "Restoring").
#
#   scripts/restore.sh [--overwrite] [-d DATABASE_URL] FILE
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
# The restore is one transaction: it either completes or leaves the database
# as it was. Set PG_BIN to the directory holding pg_restore and psql if those
# on the PATH are older than the server.

set -eu

usage() {
    echo "usage: $0 [--overwrite] [-d DATABASE_URL] FILE" >&2
    exit 2
}

url="${MIGRATION_DATABASE_URL:-}"
overwrite=no
file=
while [ $# -gt 0 ]; do
    case "$1" in
        --overwrite) overwrite=yes ;;
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

# The query goes in on standard input, where psql substitutes its variables
# (it does not in --command): the role's name reaches the server only as
# :'app_role', quoted by psql, never pasted into the text.
sql() {
    printf '%s\n' "$1" | "${bin}psql" --no-psqlrc --quiet --tuples-only --no-align \
        --set ON_ERROR_STOP=1 --set app_role="$app_role" --dbname="$url" --file=-
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

# What must hold afterwards, checked rather than assumed.
triggers=$(sql "SELECT count(*) FROM pg_trigger t JOIN pg_class c ON c.oid = t.tgrelid
                WHERE NOT t.tgisinternal AND t.tgenabled <> 'D'
                  AND t.tgname IN ('revision_append_only', 'revision_attachment_append_only',
                                   'contribution_snapshot_append_only', 'acceptance_append_only',
                                   'exchange_event_append_only')")
if [ "$triggers" != "5" ]; then
    echo "$0: restored, but only $triggers of the 5 append-only triggers are in place" >&2
    exit 1
fi
writable=$(sql "SELECT count(*) FROM (VALUES ('revision'), ('revision_attachment'),
                    ('contribution_snapshot'), ('acceptance'), ('exchange_event')) AS t(name)
                WHERE has_table_privilege(:'app_role', t.name, 'UPDATE')
                   OR has_table_privilege(:'app_role', t.name, 'DELETE')
                   OR has_table_privilege(:'app_role', t.name, 'TRUNCATE')
                   OR NOT has_table_privilege(:'app_role', t.name, 'INSERT')")
if [ "$writable" != "0" ]; then
    echo "$0: restored, but $app_role's rights on the append-only tables are not as the" >&2
    echo "migrations set them" >&2
    exit 1
fi

migrations=$(sql "SELECT count(*) || ' migrations, the latest ' || max(version)
                  FROM _sqlx_migrations WHERE success")
exchanges=$(sql "SELECT count(*) FROM exchange")
events=$(sql "SELECT count(*) FROM exchange_event")
echo "restored $file: $migrations; $exchanges exchanges, $events events"
