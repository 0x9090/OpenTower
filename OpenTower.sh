#!/usr/bin/env sh

# OpenTower source-tree launcher for macOS and Linux.
# Builds the current checkout, stops an older development instance, and starts
# the freshly built game with the original SimTower application icon.

set -eu

repository_root=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
cd "$repository_root"

if ! command -v cargo >/dev/null 2>&1; then
    echo "OpenTower requires the Rust toolchain (cargo) to build from source." >&2
    exit 1
fi

platform=$(uname -s)
case "$platform" in
    Darwin)
        pkill -x OpenTower >/dev/null 2>&1 || true
        ;;
    Linux)
        pkill -x simtower >/dev/null 2>&1 || true
        ;;
    *)
        echo "OpenTower.sh supports macOS and Linux; use OpenTower.cmd on Windows." >&2
        exit 1
        ;;
esac

echo "Building OpenTower..."
cargo build --release --package simtower-desktop

case "$platform" in
    Darwin)
        app_bundle="$repository_root/target/release/OpenTower.app"
        "$repository_root/scripts/create-macos-app.sh" \
            "$repository_root/target/release/simtower" \
            "$app_bundle"
        open -n "$app_bundle" --args "$@"
        ;;
    Linux)
        exec "$repository_root/target/release/simtower" "$@"
        ;;
esac
