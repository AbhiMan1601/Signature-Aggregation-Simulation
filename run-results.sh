#!/bin/sh
set -eu

BIN="${COMPARE_TOPOLOGIES_BIN:-/usr/local/bin/compare_topologies}"
if [ ! -x "$BIN" ]; then
    if [ -x "./target/release/compare_topologies" ]; then
        BIN="./target/release/compare_topologies"
    elif [ -x "./target/debug/compare_topologies" ]; then
        BIN="./target/debug/compare_topologies"
    fi
fi

if [ "$#" -gt 0 ]; then
    exec "$BIN" "$@"
fi

VALIDATORS="${VALIDATORS:-128000,256000,512000,1000000,2097152,2500000}"
FORMAT="${FORMAT:-table}"
CSV="${CSV:-false}"

echo "Running standalone SSF/3SF aggregation results"
echo "validators=${VALIDATORS}"
echo "format=${FORMAT}"

if [ "$FORMAT" = "csv" ] || [ "$CSV" = "1" ] || [ "$CSV" = "true" ]; then
    exec "$BIN" --validators "$VALIDATORS" --csv
fi

exec "$BIN" --validators "$VALIDATORS"
