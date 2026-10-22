#!/bin/sh
# Arguments are passed separately; no paths are evaluated as shell code.
parent=$1
executable=$2
cache=$3
count=0
while kill -0 "$parent" 2>/dev/null; do
    count=$((count + 1))
    [ "$count" -lt 120 ] || exit 1
    sleep 1
done
rm -rf -- "$cache"
exec "$executable"
