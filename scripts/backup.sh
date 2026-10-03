#!/bin/sh
# Backs up the database to one file in pg_dump's custom format
# (docs/operations.md, "Backups").
#
#   scripts/backup.sh [--force] [-d DATABASE_URL] [FILE]
#
# Connects as the schema owner: -d, or else MIGRATION_DATABASE_URL. Writes
# FILE, by default yuppers-<UTC time>.dump in the current directory, and
# prints its name. It refuses to replace a file that is already there unless
# given --force. The file holds the schema, the data and the grants to the
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
    echo "usage: $0 [--force] [-d DATABASE_URL] [FILE]" >&2
    exit 2
}

url="${MIGRATION_DATABASE_URL:-}"
force=no
file=
while [ $# -gt 0 ]; do
    case "$1" in
        --force) force=yes ;;
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
if [ -z "$url" ]; then
    echo "$0: no database: give -d DATABASE_URL or set MIGRATION_DATABASE_URL" >&2
    exit 2
fi

bin="${PG_BIN:+$PG_BIN/}"
file="${file:-yuppers-$(date -u +%Y%m%dT%H%M%SZ).dump}"

# An existing backup is never replaced by accident. A directory with that
# name, or a link to one, is refused even with --force, since the backup
# would land inside it; with --force a link to a file is replaced itself,
# never the file it points to.
if [ -d "$file" ]; then
    echo "$0: $file is a directory; give the name of the file to write" >&2
    exit 1
fi
if { [ -e "$file" ] || [ -L "$file" ]; } && [ "$force" != "yes" ]; then
    echo "$0: $file already exists; refusing to overwrite it. Give another name, or" >&2
    echo "pass --force to replace it." >&2
    exit 1
fi

# Written beside the final name, under a name nobody could have prepared:
# mktemp creates a new file, readable by this user alone, and never follows
# a link that was there first. It is moved into place only once whole and
# readable, so a file with the final name is never a broken backup.
umask 077
partial=$(mktemp "$(dirname -- "$file")/.$(basename -- "$file").partial.XXXXXX")
trap 'rm -f "$partial"' EXIT
trap 'exit 130' INT TERM
"${bin}pg_dump" --format=custom --compress=6 --file="$partial" --dbname="$url"
"${bin}pg_restore" --list "$partial" >/dev/null
if [ "$force" = "yes" ]; then
    mv -f -- "$partial" "$file"
else
    # A hard link fails if the name was taken meanwhile, where mv would
    # replace it.
    if ! ln -- "$partial" "$file"; then
        echo "$0: could not put the backup in place as $file; anything there is left as it was" >&2
        exit 1
    fi
    rm -f -- "$partial"
fi
trap - EXIT INT TERM

echo "$file"
