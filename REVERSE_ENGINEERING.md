# SimTower reverse-engineering workspace

## Source binaries

- `SimTower ISO/extracted/STOWER.EXE`
  - SHA-256: `33010bbda7ab2600a3e2d00723cdd8b41093a62860a9fd2dadeb3c2fa7732c44`
  - 32-bit Intel PE installer/bootstrap executable, not the game
- `analysis/input/SIMTOWER.EXE`
  - SHA-256: `2825a3c53f77945c63b6d72e26faa7dde5ddd56c31ca668e67a12576d7feca96`
  - Expanded from `SIMTOWER.EX_` with libmspack
  - 16-bit Windows New Executable using protected-mode x86

The original binary and game data are research inputs. The new implementation
must be original Rust source; do not copy decompiler output directly into the
port.

## Ghidra-MCP setup

The workspace contains Ghidra 12.1.4 and the upstream `ghidra-mcp` checkout.
OpenJDK 21 is required to compile the extension.

```sh
tools/extract-original.sh
tools/setup-ghidra-mcp.sh
tools/build-ghidra-natives.sh # Apple Silicon only
tools/start-ghidra-mcp.sh --file "$PWD/analysis/input/SIMTOWER.EXE"
```

The headless analysis API listens only on `127.0.0.1:8089`. The workspace
`.mcp.json` connects MCP clients to that API through the Python bridge.

## Porting approach

1. Record externally observable behavior, file formats, resource identifiers,
   imports, and subsystem boundaries in this document.
2. Name and type functions/data in the Ghidra project with evidence-backed
   comments.
3. Express behavior as black-box tests and small specifications.
4. Implement those specifications independently in portable Rust.
5. Keep platform integration behind narrow interfaces and test the simulation
   core without a renderer.

This is a long-running compatibility project. A playable cross-platform port
requires staged milestones rather than a single mechanical decompilation.

## Verified findings

- Ghidra 12.1.4 identifies the game as `x86:LE:16:Protected Mode`.
- Auto-analysis currently identifies 1,216 functions across 80 executable code
  segments, two writable data segments, and 500 resource segments.
- The independent Rust NE parser finds 82 loadable segments and 499 resource
  records. Those records include 250 standard Windows bitmaps, 51 dialogs,
  cursors/icons/menu/accelerator resources, and 233 entries spread across 11
  game-specific resource type ordinals. The Ghidra count includes one
  additional synthetic resource block.
- The analyzed program is preserved at
  `analysis/ghidra/SIMTOWER-analyzed-arm64.gzf`. The older
  `SIMTOWER-analyzed.gzf` predates the native decompiler build.
- The binary exports named window procedures and dialog filters including
  `MAINWNDPROC`, `MAPWNDPROC`, `INFOWNDPROC`, `ELVDLOGMAIN`,
  `TENANTINFODLOGFILTER`, and `PEPLEINFODLOGFILTER`. These names support a
  simulation/UI boundary rather than a direct translation of Win16 messages.
- `MAINWNDPROC` dispatches at least 22 Windows messages through a jump table;
  `MAPWNDPROC` dispatches at least 13. Platform events belong in the desktop
  adapter and should not leak into the simulation crate.
- Construction prices recovered from the original strings include Lobby
  `$5,000`, Office `$40,000`, Condo `$80,000`, Restaurant `$200,000`, and
  Security `$100,000`. The current Rust core uses those verified prices.
- A recovered placement message says lobbies are restricted to the first floor
  and every 15 floors. The core has a regression test for that rule.
- All 250 `BITMAP` resources are uncompressed Windows DIBs. The current set is
  8-bit indexed color with a `BITMAPINFOHEADER`, a BGRA palette, DWORD-aligned
  bottom-up rows, and resource-alignment padding. `simtower-formats` now decodes
  1/4/8/24/32-bit uncompressed DIBs with bounds checks.
- Custom resource type ordinal `32522` contains 58 complete RIFF/WAVE files.
  Every recovered sound is mono 8-bit PCM at approximately 5.5 or 11.1 kHz.
  Resource alignment adds trailing bytes, so the extractor trims each file to
  the RIFF-declared length.
- The game imports `WAVMIX16`. Cross-references isolate its wrapper subsystem in
  segment `11d0`. Import ordinals and call shapes identify `WaveMixInit`,
  `WaveMixActivate`, `WaveMixOpenWave`, `WaveMixOpenChannel`, `WaveMixPlay`,
  `WaveMixFlushChannel`, `WaveMixFreeWave`, `WaveMixCloseSession`, and
  `WaveMixPump`. The open call passes the resource flag and an integer resource
  ID, directly connecting custom type `32522` to playback. API semantics were
  cross-checked against the [WaveMix 1.80 documentation](https://www.compuphase.com/wavemix.htm).
- The desktop shell currently uses raw resource `2536` (first-floor lobby
  background), bitmap resources `1001` (lobby entrance awning), `5000`
  (repeating 12-pixel structural strip), `1448`
  (office), `1576` (condo), `1384` (restaurant), and `1960` (security), plus
  background `352`, tool palette `300`, pointer tools `604`, and construction
  scaffolding `3624`. The facility groups use the executable's observed
  `1000 + type * 64` layout: restaurant is type 6, office type 7, condo type 9,
  and security type 15. Bitmap `5000` is loaded by a dedicated routine that
  tiles the structural strip horizontally below 24-pixel room art; the recovered
  coordinate math uses a 36-pixel floor pitch. The port models that strip and
  its empty 24-pixel buildable bay as a separate floor layer: floor is painted
  first, then lobby and tenant/business art is painted over it. Horizontal
  placement divides screen coordinates by 8, producing the original 8×36
  slice-to-floor perspective. Bitmap `352` is a 200×288 sky tile with a
  264-pixel horizon; bitmap `849` supplies the separate, non-repeating 32×360
  soil depth texture. The original floor-placement rule permits no upper-floor
  overhang: every new above-ground floor cell must have a cell directly below.
  This matches both the manual's “additional floors cannot exceed the width of
  the floor below” rule and the executable string `Cannot place items wider
  than floor below!`. Bitmap `1069` contains the mirrored 24-pixel emergency
  stair halves used outside the ends of a contiguous structural floor. Raw
  lobby resource `2536` decodes as 8×36 cells into a 992×36 sheet; its first
  328-pixel chunk contains the repeating 256-pixel lobby body and the 56-pixel
  facade at x=272. The two 56-pixel halves of awning `1001` are drawn beyond
  the structural floor's outer edges. Some
  animation-frame boundaries remain provisional until the corresponding
  per-facility render routines are named and typed.
- The desktop facility registry now covers all 24 placeable types. Corrected
  structural widths include office 9 cells, condominium 16, restaurant 24,
  shop 12, security 16, and housekeeping 15. Stairs, escalators, cinema, party
  hall, metro, recycling, and cathedral are composed from their original
  vertically stacked resource strips. Security uses bitmap `1896`; bitmap
  `1960` is housekeeping rather than security.
- The food sheets group four operating states for every restaurant and fast
  food style: open/empty, open/some customers, open/full, and closed. In the
  extracted Windows resources the even-numbered bitmap of each style contains
  empty and some, while the following odd-numbered bitmap contains full and
  closed. Rendering now selects those source frames from actual visitors and
  the documented business hours. Full begins at the corresponding strong
  patronage band: 35 customers for fast food and 70 for restaurants.
- Sound resource `7000` is the general construction effect used when placing a
  facility. The recovered variable-segment builder additionally invokes `7001`,
  which the port now plays for lobby construction. Resource `7002` is the short
  insufficient-funds cue and `7003` is the demolition crash; the port plays
  them for rejected purchases and successful bulldozer actions respectively.
  Resource `10013` is the original cash-register payment cue. Positive tenant
  income events queue this cue at half-second intervals, matching the original
  settlement run without overlapping every payer in a single frame.
  Sounds `1384/1385`, `1448`, and `1576/1577` belong to the
  restaurant, office, and condominium ambience families respectively; they
  are not construction sounds. Sounds `5000` through `5005` are dispatched at
  fixed simulation-clock events, so `5000` must not be paired with lobby
  construction. No dedicated security ambience has been established.
- The binary exports the elevator procedures `ELVDLOGMAIN`, `ELVPOPUP`, and
  `ELVINFODLOGFILTER`, and embeds the status strings `Elevator is far away`,
  `Elevator is very far away`, `No more cars in this shaft!`, and the elevator
  car purchase prices. The original manual and surviving screenshots establish
  the control dialog's WD/WE schedules, six daily time periods, Local/Express
  modes, per-car home floors, Show On/Off controls, and a floor/car activity
  grid. Its default Waiting Car Response is five floors and Standard Floor
  Departure is zero seconds. Standard cars hold 17 people. The independent
  Rust simulation now models calls, direction-aware multi-car dispatch,
  configurable dwell/home floors, capacity, passenger boarding/unloading,
  business visits and return trips. A person progresses from calm to concerned
  to angry while queued; the desktop renderer tints angry sprites red and uses
  original people and elevator-car resources. Routing prefers a connected
  escalator (up to five floors) or stairway (up to three floors), otherwise it
  chooses the closest usable elevator. Express shafts stop on floor 1, every
  basement, and sky-lobby floors divisible by 15. A trip may change elevator
  shafts once, and only at a lobby.
- Stair resources `2408` and `2472` form matching upper/lower sheets with seven
  64-pixel occupancy states; escalator resources `2728` and `2792` follow the
  same seven-state pattern. The empty state is frame zero and the remaining
  frames progressively add riders, so the renderer now substitutes a populated
  transport frame and suppresses the separate walking sprite while a simulated
  person is on it. These overlays are drawn after lobbies and tenants, matching
  the original rule that stairs may be built over occupied floor space.
- Standard elevator car sheet `1065` has four 32-pixel load states followed by
  the top and bottom machinery/arrow states. Express sheet `1067` has five
  48-pixel load states followed by corresponding endpoint machinery. The
  endpoint arrows are rendered as shaft extension handles; dragging one adds
  every supported shaft segment crossed. Contemporary instructions corroborate
  that the original Finger tool drags the small arrow beside the whirling gears
  to extend a shaft. Service sheet `1066` has load states but no endpoint pair,
  so its 32-pixel shaft uses the standard machinery caps.
- Bitmap `1068` is the repeating 16×36 dark elevator-shaft background. It does
  not contain floor-number frames: original screenshots show the changing
  floor designations rendered dynamically in muted gray over the shaft and
  underneath the car artwork. Basement labels use the same `B1`, `B2`, …
  formatting as the elevator configuration grid.
- Elevator construction uses three distinct charges recovered from the money
  and tuning tables. A new standard/service/express shaft costs
  `$200,000`/`$100,000`/`$400,000` and includes its first car. Extending a
  shaft from an endpoint has no second shaft charge; it pays only for any new
  structural floor cells. Additional cars cost `$80,000`/`$50,000`/`$150,000`
  respectively.
- The original build unlock table is now part of `FacilityKind` metadata and is
  enforced by the desktop build tools. One star exposes floor, lobby/skylobby,
  stairs, standard elevator, office, condominium, and fast food. Two stars add
  service elevator, single hotel room, security, and housekeeping. Three stars
  add escalator, express elevator, restaurant, retail shop, cinema, party hall,
  twin room, suite, medical, recycling, parking, and ramp. Four stars add the
  metro station; five stars add the cathedral. This table is consistent across
  the contemporary Macintosh FAQ/README transcription and later reference
  tables. Locked items remain visible in the menu with their required rating.
- People sheet `1512` is 480×24 and divides into thirty adjacent two-frame,
  8-pixel-wide character pairs. Walking now alternates within the selected pair
  rather than treating all sixty cells as unrelated static people. Exterior
  pedestrians use the ground line as their foot datum and switch to the indoor
  24-pixel room datum only after stepping onto a constructed floor cell.
- Surviving Windows screenshots show no separate generic roof zone or roof
  construction tool. The topmost tenant artwork supplies its own upper border,
  while bare space retains the exposed structural/floor edge. Bitmap `1002` is
  the transparent 36×36 construction crane anchored to the highest floor; it
  is an indicator above that edge, not a buildable roof layer. Special
  top-floor structures such as the cathedral remain facilities.
- The first-floor lobby's custom resources are a vertical family rather than
  animation alternates: raw resources `2536`, `2537`, and `2538` are the
  ground, second, and third stories of the hidden super lobby. Contemporary
  instructions agree that Control builds two stories and Control+Shift builds
  three, only for the tower's first lobby, with construction cost multiplied
  per story. The lower-left fresh-tower lobby attempt raises the initial fund
  from $2,000,000 to $4,000,000.
- Hidden-event resource tracing identifies bitmap `904` as Santa's sleigh and
  bitmap `10003` as the ancient buried treasure. The treasure routine is gated
  to grades two through four and chooses grade-dependent payout table entries;
  the port uses the documented $200,000/$300,000/$500,000 progression and a
  single randomized site between B3 and B8. The extracted Windows resource set contains no witch
  bitmap, so the documented October 31 flyby uses a small palette-matched
  renderer while December 25 uses the original Santa art. `OPENTOWER_DATE` can
  supply an `MM-DD` date for deterministic cross-platform verification without
  changing the host clock.
- The contemporary CheatCodes FAQ is now a behavioral specification for the
  economy and rating engine. Cumulative ratings require 300 people for two
  stars, 1,000 plus security for three, 5,000 plus parking, two occupied
  suites, medical, and recycling for four, and 10,000 plus the metro for five.
  Public businesses settle from completed daily visits rather than the much
  smaller set of agents that happen to be visible inside at the instant of
  settlement. The documented steady-state patronage is 35 weekday/48 weekend
  customers for fast food, 25/30 for shops, and 35 for restaurants. Offices
  and shops pay quarterly; hotels and food tenants pay daily; tenant revenue
  does not begin before move-in and the first completed visit. Standard office
  rent is $10,000 per quarter, shop rent is $15,000 per quarter, and a condo
  sale pays $150,000 once. Lobby maintenance is free through two stars, then
  $300 per segment at three stars and $1,000 at four or five. Elevator
  maintenance is charged once per shaft and once per installed car ($10,000
  local/service or $20,000 express), never once per vertical shaft segment.
  The desktop ledger separates operating income, maintenance, construction,
  and other income, snapshots population by tenant type, and automatically
  opens an original-style report at each quarter boundary.
- Original construction caps are enforced in the core: 24 elevator shafts and
  eight cars per shaft; 64 combined stairs/escalators; ten each of security
  and medical; sixteen combined cinemas/party halls; 512 combined fast food,
  restaurant, and shop tenants; 512 parking spaces; one metro; one cathedral;
  and a single vertical column of parking ramps. Businesses also observe their
  documented hours, so closed destinations do not generate new visitors.
- Tenant move-in and visible crowding now depend on real access. A tenant stays
  vacant until an unbroken horizontal floor path and a valid transport route
  connect it to a first-floor lobby. People are still created outside the
  building, and populated tenant art is selected only after an agent completes
  that trip. Continuous stair columns can span three flights and escalator
  columns five; their routing cost rises with every flight, causing agents to
  prefer a usable elevator for progressively longer climbs.
- The bundled distribution lacked an Apple Silicon decompiler executable.
  `tools/build-ghidra-natives.sh` builds it from Ghidra's included C++ source;
  verified C-like output is now available for the segmented 16-bit target.
  Disassembly, call/xref data, named exports, resource inspection, and
  black-box behavior remain necessary because types and indirect jump tables
  still require manual recovery.

## Milestones

1. **Foundation (in progress):** deterministic core, desktop shell, grid,
   construction, clock, and analysis archive.
2. **Resources and saves (in progress):** the NE table parser, DIB decoder,
   RIFF/WAVE parser, inventory CLI, reproducible extractor, authorized asset
   set, and first native renderer/audio integration are complete. Sprite-frame
   semantics and the `.TDT` save format remain.
3. **Vertical transport:** recover shaft/car schedules, queues, satisfaction,
   and route-finding; add elevators and stairs to the core.
4. **Tenants and economy:** offices, condos, hotels, restaurants, population,
   rent/sales, ratings, and time-based demand.
5. **Events and parity:** fire, bomb threats, VIP evaluation, metro, cathedral,
   cinema, UI dialogs, sound, and save compatibility.
6. **Release:** package and test native builds for Windows, macOS, and Linux.
