# OpenTower

OpenTower is an evidence-driven Rust reimplementation of the 1995 Windows
release of SimTower. The long-term target is one codebase for Windows, macOS,
and Linux.

This repository contains the portable Rust rewrite: a tower grid with
construction, funds, placement validation, simulation, and a native desktop UI
using the original authorized pixel art and sounds. It is not yet a complete
game.

## Run

```sh
cargo run -p simtower-desktop
```

- Click a category in the compact tool palette to open its construction submenu
- Start with Structure → Floor, then click or drag horizontally to create empty
  floor space one slice at a time
- Select Lobby or a tenant/business and paint it onto existing floor; Lobby can
  also be dragged one slice at a time, while wider facilities require empty
  floor under their complete footprint
- Grey submenu items document facilities that are visible but not yet ported
- `1`–`5`: quick-select the five currently implemented construction tools
- Inspect and bulldoze are selectable directly from the palette
- Arrow keys or `WASD`: pan
- Mouse wheel: move between floors
- Space: pause
- Hold Tab: fast-forward

The palette and top status strip follow the original game's compact layout,
while remaining inside a single application window. The tower view uses the
original 8-pixel horizontal construction slice and 36-pixel floor pitch. The
mouse wheel scrolls vertically, while holding either Shift key changes the
wheel to horizontal tower scrolling on macOS, Windows, and Linux. The build
menus expose the executable-backed artwork for the complete placeable facility
set, including hotel rooms, shops, transports, services, entertainment, and
multi-floor tenants. The original sky tile is drawn at native scale and
anchored to floor zero. Original
bitmap `849` supplies the 360-pixel-deep soil gradient, tiled horizontally but
never stacked by floor. The app starts maximized in a normal decorated window,
and the buildable site spans 256 columns. Empty floor is an independent
structural layer, rendered as the original dark buildable bay and repeating
12-pixel slab, with bitmap `1069` split into emergency-stair overhangs at both
ends. Above the first floor, every new structural slice requires a floor slice
directly below it, matching the original game's zero-overhang rule. Lobby construction first displays the recovered scaffolding art, then
resolves to the dedicated 36-pixel-tall lobby background from raw resource
`2536`. The red `OPEN` awning halves from bitmap `1001` hang beyond the complete
floor run rather than covering its interior. Lobby placement uses its segment sound `7001` together with
the general construction effect `7000`.

The desktop layer uses Macroquad, while all game state and rules live in
`simtower-core`. Original graphics are decoded to RGBA by the Rust format crate
and rendered with nearest-neighbor filtering; original RIFF/WAVE resources are
played through the platform sound player. macOS and Windows use built-in system
players; Linux currently requires `paplay` or `aplay`. This separation keeps the
simulation deterministic and makes the same core usable on all three desktop
platforms.

Facility resources are registered by name instead of array position. The
verified mappings include lobby background `2536`, lobby entrance awning
`1001`, structural strip `5000`, office `1448`, condominium `1576`, restaurant
`1384`, security `1896`, and housekeeping `1960`. Multi-floor facilities are
assembled vertically from their original per-floor bitmap strips.

To inspect an original expanded Windows executable without Ghidra or Wine:

```sh
cargo run -p simtower-inspect -- analysis/input/SIMTOWER.EXE
```

The parser is pure Rust and bounds-checks every NE resource entry before
exposing its bytes.

To reproduce the authorized assets from the expanded executable:

```sh
cargo run -p simtower-inspect -- \
  analysis/input/SIMTOWER.EXE extract assets/original
```

This exports 250 DIB resources and 11 custom raw sprite resources as portable
BMP files, plus 58 embedded WaveMix resources as standard WAV files.

## Reverse engineering

See [REVERSE_ENGINEERING.md](REVERSE_ENGINEERING.md) for binary hashes,
verified findings, asset mappings, and the compatibility roadmap. Original
binaries, disc images, decompiler installations/databases, and local tooling
are deliberately excluded from this repository.

## Manual releases

The **Build release binaries** workflow is manual-only. From the repository's
Actions tab, choose the workflow, select **Run workflow**, and provide the
release tag and title. It builds packaged Windows, macOS, and Linux binaries
and attaches them to a GitHub Release. The workflow has no push, tag,
pull-request, or scheduled trigger.
