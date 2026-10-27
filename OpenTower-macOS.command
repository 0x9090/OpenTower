#!/bin/sh

# Project-relative shortcut to the latest compiled macOS release.
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
app_bundle="$project_dir/target/release/OpenTower.app"
binary="$project_dir/target/release/simtower"

if [ -d "$app_bundle" ]; then
    open -n "$app_bundle" --args "$@"
    exit 0
fi

if [ ! -x "$binary" ]; then
    printf '%s\n' \
        "OpenTower has not been compiled yet." \
        "Run ./OpenTower.sh once to create the latest release build." >&2
    exit 1
fi

# Detach the GUI process so launching this shortcut does not leave a terminal
# attached to the game window.
cd "$project_dir"
nohup "$binary" "$@" >/dev/null 2>&1 &
