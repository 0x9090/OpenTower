#!/bin/sh

# Project-relative shortcut to the latest compiled Linux release.
set -eu

project_dir=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
binary="$project_dir/target/release/simtower"

if [ ! -x "$binary" ]; then
    printf '%s\n' \
        "OpenTower has not been compiled yet." \
        "Run ./OpenTower.sh once to create the latest release build." >&2
    exit 1
fi

cd "$project_dir"
# Detach the GUI process so launching this shortcut does not keep a terminal
# window attached to the game.
nohup "$binary" "$@" >/dev/null 2>&1 &
