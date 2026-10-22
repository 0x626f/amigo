#!/bin/sh
set -eu
source_app=$1
target=$2
backup=$3
parent_dir=$(dirname "$target")
mkdir -p "$parent_dir"
old=0
if [ -d "$target" ]; then
    mv "$target" "$backup"
    old=1
fi
if /usr/bin/ditto "$source_app" "$target"; then
    [ "$old" -eq 0 ] || rm -rf "$backup"
else
    rm -rf "$target"
    [ "$old" -eq 0 ] || mv "$backup" "$target"
    exit 1
fi
