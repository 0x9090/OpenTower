#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
binary_path="${1:-${repository_root}/target/debug/simtower}"
bundle_path="${2:-${repository_root}/target/debug/OpenTower.app}"

if [[ ! -x "${binary_path}" ]]; then
    echo "OpenTower executable not found: ${binary_path}" >&2
    exit 1
fi

mkdir -p "${bundle_path}/Contents/MacOS"
install -m 755 "${binary_path}" "${bundle_path}/Contents/MacOS/OpenTower"
install -m 644 "${repository_root}/packaging/macos/Info.plist" "${bundle_path}/Contents/Info.plist"

echo "Created ${bundle_path}"
