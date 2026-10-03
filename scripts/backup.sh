#!/bin/sh
# Backs up the database to one file in pg_dump's custom format
# (docs/operations.md, "Backups").
#
#   scripts/backup.sh [-d DATABASE_URL] [FILE]
#
# Connects as the schema owner: -d, or else MIGRATION_DATABASE_URL. Writes
# FILE, by default exchange-<UTC time>.dump in the current directory, and
# prints its name. The file holds the schema, the data and the grants to the
# application role; it does not hold roles or their passwords, which belong
# to the server, not the database.
#
# The dump is one consistent snapshot, taken without stopping the api or the
# worker. pg_dump must be the server's major version or newer; set PG_BIN to
# the directory that holds it if the one on the PATH is older.
#
# The file holds everything the service stores: names, email addresses,
# phone numbers and every agreement. Keep it encrypted, somewhere with access
# as narrow as the database's own.

set -eu

usage() {
    echo "usage: $0 [-d DATABASE_URL] [FILE]" >&2
    exit 2
}

url="${MIGRATION_DATABASE_URL:-}"
while getopts "d:h" option; do
    case "$option" in
        d) url="$OPTARG" ;;
        *) usage ;;
    esac
done
shift $((OPTIND - 1))
[ $# -le 1 ] || usage
if [ -z "$url" ]; then
    echo "$0: no database: give -d DATABASE_URL or set MIGRATION_DATABASE_URL" >&2
    exit 2
fi

bin="${PG_BIN:+$PG_BIN/}"
file="${1:-exchange-$(date -u +%Y%m%dT%H%M%SZ).dump}"
partial="$file.partial"

# Written beside the final name and moved into place only once whole and
# readable, so a file with the final name is never a broken backup.
trap 'rm -f "$partial"' EXIT INT TERM
umask 077
"${bin}pg_dump" --format=custom --compress=6 --file="$partial" --dbname="$url"
"${bin}pg_restore" --list "$partial" >/dev/null
mv "$partial" "$file"
trap - EXIT INT TERM

echo "$file"
