#!/bin/sh
# Reads one signed agreement back out of a restored copy through the API, and
# checks that what is stored still reproduces the hash that was signed
# (docs/operations.md, "The restore drill").
#
#   scripts/check-restored-record.sh RESTORED_OWNER_URL RESTORED_APP_URL [SOURCE_URL]
#
# Picks the oldest exchange with an agreement in force, gives its initiator a
# session in the copy, starts an api on the copy (API_BIN, default
# backend/target/debug/api, on 127.0.0.1:PORT, default 8095) and fetches the
# exchange's record. It passes when the record shows the revision's content
# hash, the same as in SOURCE_URL if given, and the api did not report that
# stored terms no longer reproduce the signed hash.
#
# It writes a session into the database it checks: run it against a copy,
# never against the database the service uses.

set -eu

if [ $# -lt 2 ] || [ $# -gt 3 ]; then
    echo "usage: $0 RESTORED_OWNER_URL RESTORED_APP_URL [SOURCE_URL]" >&2
    exit 2
fi
restored_url="$1"
restored_app_url="$2"
source_url="${3:-$1}"
bin="${PG_BIN:+$PG_BIN/}"
api_bin="${API_BIN:-backend/target/debug/api}"
port="${PORT:-8095}"

sql() {
    "${bin}psql" --no-psqlrc --quiet --tuples-only --no-align --field-separator=' ' \
        --set ON_ERROR_STOP=1 --dbname="$1" --command="$2"
}

found=$(sql "$restored_url" "SELECT id, in_force_revision_id, created_by FROM exchange
                             WHERE in_force_revision_id IS NOT NULL
                             ORDER BY created_at, id LIMIT 1")
if [ -z "$found" ]; then
    echo "$0: the copy holds no agreement in force to read back" >&2
    exit 1
fi
# shellcheck disable=SC2086 # three words, split on purpose.
set -- $found
exchange="$1"
revision="$2"
account="$3"
signed=$(sql "$source_url" "SELECT encode(content_hash, 'hex') FROM revision WHERE id = '$revision'")

work=$(mktemp -d)
token="restore-check-$(od -An -N16 -tx1 /dev/urandom | tr -d ' \n')"
sql "$restored_url" "INSERT INTO account_session
                         (account_id, token_hash, auth_method, authenticated_at, expires_at)
                     VALUES ('$account', sha256(convert_to('$token', 'UTF8')), 'EMAIL_OTP',
                             now(), now() + interval '10 minutes')" >/dev/null

DATABASE_URL="$restored_app_url" BIND_ADDR="127.0.0.1:$port" WEB_ORIGIN="http://127.0.0.1:$port" \
    APP_SECRET="restore-check-only-$token" CODE_DELIVERY=log NO_COLOR=1 \
    METRICS_ADDR='' WEB_DIR='' "$api_bin" >"$work/api.log" 2>&1 &
api=$!
trap 'kill "$api" 2>/dev/null || true; rm -rf "$work"' EXIT INT TERM

ready=no
for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20 21 22 23 24 25 26 27 28 29 30; do
    if curl --silent --fail --output /dev/null "http://127.0.0.1:$port/readyz"; then
        ready=yes
        break
    fi
    sleep 1
done
if [ "$ready" != "yes" ]; then
    cat "$work/api.log" >&2
    echo "$0: the api on the copy did not become ready" >&2
    exit 1
fi

curl --silent --fail --header "Authorization: Bearer $token" \
    "http://127.0.0.1:$port/v1/exchanges/$exchange/record" >"$work/record.json"

if ! grep -q "\"content_hash\":\"$signed\"" "$work/record.json"; then
    echo "$0: the record of exchange $exchange does not show revision $revision's hash $signed" >&2
    exit 1
fi
if grep -q "no longer reproduce the signed hash" "$work/api.log"; then
    grep "no longer reproduce" "$work/api.log" >&2
    exit 1
fi
echo "exchange $exchange: revision $revision read back with its signed hash $signed"
