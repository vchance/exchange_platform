#!/bin/sh
# Compares a restored database with the one it was backed up from
# (docs/operations.md, "The restore drill").
#
#   scripts/check-restore.sh SOURCE_URL RESTORED_URL
#
# Both as the schema owner. Checks, and prints a difference for anything
# that does not match:
#
#   - the migrations applied;
#   - every privilege the application role (APP_ROLE, default exchange_app)
#     holds on a table, sequence, schema or function;
#   - every trigger, the append-only ones among them, and whether it is
#     enabled;
#   - for every table, its row count and a digest of all its rows.
#
# Meant for a source nobody is writing to while the check runs, such as a
# copy restored from the same backup, or a staging database; against a live
# database, the row counts and digests will differ by whatever was written
# since the backup. Set PG_BIN to the directory holding psql if needed.

set -eu

if [ $# -ne 2 ]; then
    echo "usage: $0 SOURCE_URL RESTORED_URL" >&2
    exit 2
fi
source_url="$1"
restored_url="$2"
bin="${PG_BIN:+$PG_BIN/}"
app_role="${APP_ROLE:-exchange_app}"

work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT INT TERM

# The same settings on both sides, so rows print the same way.
settings="SET TimeZone = 'UTC'; SET DateStyle = 'ISO, YMD'; SET IntervalStyle = 'iso_8601';
          SET extra_float_digits = 3; SET bytea_output = 'hex';"

describe() {
    "${bin}psql" --no-psqlrc --quiet --tuples-only --no-align --set ON_ERROR_STOP=1 \
        --dbname="$1" <<SQL
$settings
SELECT 'migration ' || version || ' ' || description || ' ' || success
FROM _sqlx_migrations;

SELECT 'grant ' || kind || ' ' || name || ' ' || privilege FROM (
    SELECT 'relation' AS kind, c.relname AS name, a.privilege_type AS privilege
    FROM pg_class c
    JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(c.relacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = '$app_role'
    UNION ALL
    SELECT 'column', c.relname || '.' || att.attname, a.privilege_type
    FROM pg_attribute att
    JOIN pg_class c ON c.oid = att.attrelid
    JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(att.attacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = '$app_role'
    UNION ALL
    SELECT 'function', p.proname, a.privilege_type
    FROM pg_proc p
    JOIN pg_namespace n ON n.oid = p.pronamespace AND n.nspname = 'public'
    CROSS JOIN LATERAL aclexplode(p.proacl) a
    JOIN pg_roles r ON r.oid = a.grantee AND r.rolname = '$app_role'
    UNION ALL
    SELECT 'schema', 'public', privilege
    FROM (VALUES ('USAGE'), ('CREATE')) AS p(privilege)
    WHERE has_schema_privilege('$app_role', 'public', privilege)
) AS grants;

SELECT 'trigger ' || c.relname || ' ' || t.tgname || ' enabled=' || t.tgenabled::text || ' '
       || pg_get_triggerdef(t.oid)
FROM pg_trigger t
JOIN pg_class c ON c.oid = t.tgrelid
JOIN pg_namespace n ON n.oid = c.relnamespace AND n.nspname = 'public'
WHERE NOT t.tgisinternal;

SELECT 'table ' || table_name || ' rows=' || x.count || ' digest=' || x.digest
FROM information_schema.tables
CROSS JOIN LATERAL (
    SELECT (xpath('/row/count/text()', doc))[1]::text AS count,
           (xpath('/row/digest/text()', doc))[1]::text AS digest
    FROM query_to_xml(format(
        'SELECT count(*) AS count,
                md5(coalesce(string_agg(t::text, E''\\n'' ORDER BY t::text COLLATE "C"), '''')) AS digest
         FROM %I.%I AS t', table_schema, table_name), false, true, '') AS doc
) AS x
WHERE table_schema = 'public' AND table_type = 'BASE TABLE';
SQL
}

describe "$source_url" >"$work/source.raw"
LC_ALL=C sort "$work/source.raw" >"$work/source"
describe "$restored_url" >"$work/restored.raw"
LC_ALL=C sort "$work/restored.raw" >"$work/restored"

for kind in migration grant trigger table; do
    count=$(grep -c "^$kind " "$work/source" || true)
    echo "$kind: $count in the source"
done
for name in revision_append_only revision_attachment_append_only \
    contribution_snapshot_append_only acceptance_append_only exchange_event_append_only; do
    if ! grep -q "^trigger [a-z_]* $name enabled=O " "$work/restored"; then
        echo "$0: the restored database lacks the enabled trigger $name" >&2
        exit 1
    fi
done
if ! grep -q "^table revision rows=[1-9]" "$work/source"; then
    echo "$0: the source holds no revision, so the check proves little" >&2
    exit 1
fi

if diff -u "$work/source" "$work/restored"; then
    echo "the restored database matches the source"
else
    echo "$0: the restored database differs from the source (above)" >&2
    exit 1
fi
