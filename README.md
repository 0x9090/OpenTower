# OpenTower

OpenTower brings the classic SimTower experience to modern Windows, macOS,
and Linux computers. Build upward and underground, attract tenants, manage
elevator traffic, and grow a one-star building into a five-star tower.

OpenTower is a work in progress. Save files and game balance may change while
the remaining original mechanics are completed.

## What you need

OpenTower does not distribute the original game's artwork or sounds. Keep a
copy of your original Windows `SimTower.exe`; OpenTower reads the resources it
needs from that file.

## Install and start playing

1. Download the package for your operating system from the project's
   [Releases page](https://github.com/0x9090/OpenTower/releases).
2. Extract the package and launch `OpenTower.app` on macOS, `OpenTower.exe`
   on Windows, or `OpenTower` on Linux. The desktop game does not require a
   terminal or command-prompt window.
3. The first time OpenTower starts, select your original `SimTower.exe` when
   prompted.

You can alternatively place `SIMTOWER.EXE` beside the OpenTower application.
On Linux, sound playback currently requires either `paplay` or `aplay`.

## Building your first tower

1. Open the Structure tools and build empty floor space.
2. Paint a lobby onto the first floor.
3. Add stairs or an elevator, then place offices, shops, restaurants, hotels,
   condominiums, and services on the empty floors.
4. Keep elevator waits short and ensure every occupied tenant has a route back
   to the ground-floor lobby.

Tenants take time to move in. Occupied tenants pay rent, attract visitors, and
increase the tower population. New facilities unlock as the tower earns stars.

For tall towers, build sky lobbies on floors 15, 30, 45, 60, 75, and 90. A
continuous sky lobby must physically connect an express elevator with a local
elevator before people can transfer between them. Express elevators stop at
basement levels, the ground-floor lobby, and connected sky lobbies.

## Controls

- Left click: use the selected tool
- Left click and drag: paint floors or lobby slices
- Shift + left click: fill the available floor, lobby, or tenant space
- Right click empty sky or ground, or press Escape: return to the selection
  cursor
- Right click an elevator: open its controls
- Drag an elevator's arrow: extend its shaft
- Mouse wheel: scroll vertically
- Shift + mouse wheel: scroll horizontally
- Arrow keys or WASD: pan the view
- `~`: pause
- `1`: 1x speed
- `2`: 2x speed
- `3`: 3x speed
- `4`: 5x speed
- `5`: 10x speed
- Space: pause or resume

The Menu button contains New, Save, Load, automatic report, and Quit options.
The speaker button on the top bar mutes or restores game audio. Saved towers
are stored in the `SimTower` folder in your user home directory.

## Sky lobbies and elevator transfers

People may change elevators once during a trip. A sky lobby therefore needs to
be a single uninterrupted painted run touching both elevator shafts. Merely
building empty floor between the shafts does not connect them. Local elevators
serve the nearby floors; express elevators provide the long-distance trip from
the ground lobby to the appropriate sky lobby.

## Build from source

If a packaged build is not available for your platform, install the current
Rust toolchain and run:

```sh
cargo run -p simtower-desktop --no-default-features
```

The app will ask for your original `SimTower.exe`. Contributors working from a
checkout with locally extracted resources can use the default Cargo features:

```sh
cargo run -p simtower-desktop
```

Please report gameplay problems and platform-specific issues through the
project's [GitHub issue tracker](https://github.com/0x9090/OpenTower/issues).
