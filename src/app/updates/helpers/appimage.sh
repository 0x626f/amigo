#!/bin/sh
parent=$1
target=$2
staged=$3
backup=$4
cache=$5
count=0
while kill -0 "$parent" 2>/dev/null; do
    count=$((count + 1))
    [ "$count" -lt 120 ] || exit 1
    sleep 1
done
old=0
if [ -f "$target" ]; then
    mv -- "$target" "$backup" || exit 1
    old=1
fi
if mv -- "$staged" "$target"; then
    chmod 755 "$target"
    [ "$old" -eq 0 ] || rm -f -- "$backup"
    rm -rf -- "$cache"
    unset APPIMAGE APPDIR OWD
    exec "$target"
else
    [ "$old" -eq 0 ] || mv -- "$backup" "$target"
    export AMIGO_UPDATE_ERROR='The update could not replace the AppImage. The previous version was restored.'
    exec "$target"
fi
