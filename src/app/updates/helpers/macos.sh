#!/bin/sh
parent=$1
source_app=$2
target=$3
backup=$4
cache=$5
installer=$6
count=0
while kill -0 "$parent" 2>/dev/null; do
    count=$((count + 1))
    [ "$count" -lt 120 ] || exit 1
    sleep 1
done
parent_dir=$(dirname "$target")
if [ -d "$parent_dir" ] && [ -w "$parent_dir" ]; then
    /bin/sh "$installer" "$source_app" "$target" "$backup"
    result=$?
else
    /usr/bin/osascript - "$installer" "$source_app" "$target" "$backup" <<'APPLESCRIPT'
on run argv
    set command to "/bin/sh"
    repeat with argument in argv
        set command to command & " " & quoted form of (contents of argument)
    end repeat
    do shell script command with administrator privileges
end run
APPLESCRIPT
    result=$?
fi
if [ "$result" -eq 0 ]; then
    /usr/bin/open -a "$target"
    rm -rf "$cache"
else
    /usr/bin/osascript -e 'display alert "Amigo Update" message "The update did not complete. Your previous application was preserved. Retry from Settings > Updates."'
    [ ! -d "$target" ] || /usr/bin/open -a "$target"
fi
