#!/usr/bin/env bash
set -euo pipefail

repository_root="$(cd "$(dirname "$0")/.." && pwd)"
binary_path="${1:-${repository_root}/target/debug/simtower}"
bundle_path="${2:-${repository_root}/target/debug/OpenTower.app}"
icon_source="${repository_root}/packaging/OpenTower.svg"
fallback_icon_source="${repository_root}/assets/original/icons/1.bmp"

if [[ ! -x "${binary_path}" ]]; then
    echo "OpenTower executable not found: ${binary_path}" >&2
    exit 1
fi

mkdir -p "${bundle_path}/Contents/MacOS" "${bundle_path}/Contents/Resources"
install -m 755 "${binary_path}" "${bundle_path}/Contents/MacOS/OpenTower"
install -m 644 "${repository_root}/packaging/macos/Info.plist" "${bundle_path}/Contents/Info.plist"

if command -v sips >/dev/null 2>&1; then
    icon_output="${bundle_path}/Contents/Resources/OpenTower.icns"
    icon_work_dir="$(mktemp -d)"
    if command -v qlmanage >/dev/null 2>&1 \
        && [[ -f "${icon_source}" ]] \
        && qlmanage -t -s 1024 -o "${icon_work_dir}" "${icon_source}" >/dev/null 2>&1 \
        && [[ -f "${icon_work_dir}/OpenTower.svg.png" ]]; then
        icon_png="${icon_work_dir}/OpenTower.svg.png"
        sips -z 512 512 "${icon_png}" >/dev/null
        sips -s format icns "${icon_png}" --out "${icon_output}" >/dev/null
    elif [[ -f "${fallback_icon_source}" ]]; then
        sips -s format icns "${fallback_icon_source}" --out "${icon_output}" >/dev/null
    fi
    rm -rf "${icon_work_dir}"
fi

echo "Created ${bundle_path}"
