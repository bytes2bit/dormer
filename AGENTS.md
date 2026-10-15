# Working on dormer

## Project context

dormer is a small native Linux/Wayland program that paints a living wallpaper:
a familiar place under the real sky, informed by local time, weather, the week,
and activity on the user's machine. It produces both PNG stills for a wallpaper
daemon and an animated background layer for each monitor. Live mode accepts
mouse input by default; `[live] interactive = false` restores click-through.
Four views (rooftops, street, city, coast) are inhabited by a living scene: objects
with state that gestures change, and people, animals and vehicles who respond.

The active implementation is the Rust 2021 binary in `src/`, versioned through
`Cargo.toml`. It uses a custom CPU renderer and direct Wayland client protocols;
there is no GPU engine, web frontend, or application framework. `prototype/`
contains the original Python implementation (Pillow, optional GTK live layer).
Use it as historical/reference material; implement current features in Rust
unless the task explicitly concerns the prototype.

Start with `README.md` for the visual vocabulary and user behavior,
`config.example.toml` for settings, and `src/views/mod.rs` for the extension
contract. `docs/views.jpg` and `docs/weather.jpg` are visual references.

The product's character matters: a stable, quietly animated place, with details
that mean something. An agent session lights a lamp; containers light blue
windows or equivalent objects; CPU load produces smoke/steam; audio produces
music; low battery makes the lamp dim and red. Preserve these meanings across
views. Keep explanations and user-facing prose plain and concrete, consistent
with the README and `dormer explain`.

## Repository map

| File | Responsibility |
| --- | --- |
| `src/main.rs` | Hand-written CLI parser and commands; `Moment`, overrides, `build`, persisted draw `State`, systemd unit templates, bench/soak tools. |
| `src/sources.rs` | Config and XDG paths; weather/location/theme discovery; Linux machine signals; subprocesses; wallpaper backends; Hyprland queries. |
| `src/tm.rs` | System-local calendar time via libc, date parsing, realtime and monotonic clocks. |
| `src/sky.rs` | Low-precision sun/moon/planet ephemerides, sky projection, star catalogue, meteor showers, weather codes/presets. |
| `src/scene.rs` | Derives one `Scene` from `Inputs`: colors, illumination, weather effects, daily/weekly/seasonal behavior, machine signals and view knobs. |
| `src/color.rs` | Floating-point RGB, palettes, interpolation and easing helpers. |
| `src/gfx.rs` | Software rasterizer, antialiasing, blending, masks/depth, bloom, grain, damage rectangles and PNG writing. |
| `src/paint.rs` | `Cache` and still-frame painting; shared sky/weather, star field, lighting helpers; dispatches to a view's stage. |
| `src/props.rs` | `Layout` transforms and reusable drawing functions generic over `Surface`. |
| `src/town.rs` | Seeded old-town geometry, windows and household schedules, layout fingerprint. |
| `src/rng.rs` | Python-compatible Mersenne Twister (`PyRng`), lightweight animation RNG (`Fast`), deterministic hashes. |
| `src/views/mod.rs` | `View`/`Stage` contracts, registry `ALL`, animation `Anchors`, common machine-signal descriptions and lamp colors. |
| `src/views/kit.rs` | Shared building tables, skyline masks, window schedules, painting and terrain helpers. |
| `src/views/globe.rs` | Hand-drawn Earth geography and globe rendering for the moon view; flat chart export. |
| `src/live.rs` | `World` simulation, particles and anchor effects, moving drawing, damage coalescing. |
| `src/interaction.rs` | Pointer coordinates (logical surface to design units), preview-point parsing and the release velocity of a throw. |
| `src/story/` | The living scene, no pixels or Wayland: `model` (objects, state, limits, validation), `events` (bounded bus, sim clock), `physics` (gestures, bodies, basins, springs), `actors` (routes, reservations, walking), `director` (sequences, escalation, cleanup), `behaviors` (perch, occupant, animal, `repair`/`visit`/`meeting`/`round` templates), `draw` (people, animals, vehicles, props), `replay` (replay files, headless `Session`), `mod` (`StoryWorld`). |
| `src/views/episodes/` | Each developed view's scene as data: `rooftops`, `street`, `city`, `coast` define objects, routes, surfaces, stories and behaviors from the view's own geometry. |
| `src/pointer.rs` | Seat/pointer focus and cursor resources, including the software cursor fallback. |
| `src/buffer.rs` | Pixel-exact stale/damage restoration and visibility/resume decisions. |
| `src/wl.rs` | Per-monitor Wayland surfaces, shared-memory double buffers, scaling, crossfades, visibility/power pacing, signals and file watches; headless `one_frame`. |
| `src/settings/mod.rs` | `RuntimeSettings`: the validated, immutable snapshot of `config.toml` (`Source::{Auto,Fixed}` inputs, activity rates, per-view knobs); `runtime()`/`reload()` swap whole snapshots. |
| `src/settings/profile.rs` | Laying the saved profile over a `Moment` in system-local time (`apply_except` honours inputs a panel draft hands back to reality). |
| `src/settings/file.rs` | `ConfigFile::save`: `toml_edit` round-trip, conflict detection, backup, atomic write, reconciling the persisted `view` file with rollback. |
| `src/dev.rs`, `src/dev/` | Settings panel shell (terminal, event loop, saving, preview via `dev.json`) and its `model`, `input`, `layout`, `render`. |
| `src/words.rs` | `dormer explain`: shared sky descriptions plus each view's words. |

The registered view IDs are `rooftops` (`rooftops.rs`), `street` (`street.rs`),
`city` (`downtown.rs`), `coast` (`coast.rs`) and `moon` (`moonbase.rs`). The
former `vale` view is retired: `views::RETIRED` maps a saved `vale` choice to
`rooftops` with a diagnostic, and `--view vale` is refused. CLI IDs do not always match filenames. All use `Sky::Home` except
`moon`, which uses `Sky::Void` and paints its own heavens and has no living scene.

## Rendering and extension contracts

The main flow is:

```text
sources + CLI/dev overrides -> Moment -> build / scene::scene -> Scene
View::stage(seed, width, height) -> paint::Cache (layout, stage, stars)
Scene + Cache -> paint::render -> Canvas + Anchors -> Canvas::finish -> pixels
live::World -> Draw<Frame> over the still pixels -> Wayland damage/commit
```

A `View` supplies its ID, description, horizon, sky type, stage constructor,
optional dev knobs/defaults, and explanation text. Its `Stage` owns geometry
and masks for a screen size, determines window lights, and paints:

- `heavens`: additions behind the clouds; the entire sky for `Sky::Void`.
- `ground`: the foreground, far to near. Leave moving objects out when
  `Scene.live` is true so they are not baked into the animated background.
- `alive`: the view's moving objects; `lively` requests the faster frame rate
  when needed.
- `indoors` / `indoors_alive`: objects on the viewer's side of the glass,
  drawn after weather. The live hook's `falling` flag tells it to restore
  foreground objects that rain or snow has just overpainted.
- the living scene, all optional: `story` (the `Definition`: objects, routes,
  surfaces, stories, behaviors), `ground_state` / `indoors_state` (the same
  painting as `ground` / `indoors`, but reading a `Snapshot` of object state:
  whatever is missing, displaced or moving is left out of the base),
  `lights_state`, `story_depth`, `story_alive` (objects that are moving or
  carried), `story_actor` (draw one of the scene's actors in the view's own
  style) and `story_tone` (the light people stand in). `ground` simply calls
  `ground_state` with `Snapshot::intact()`, so a view with no scene is unchanged.

The still pass paints shared sky/stars/sun/moon, `heavens`, clouds, still sky
visitors, `ground`, still weather, `indoors`, then bloom. `Sky::Void` bypasses
the shared atmospheric sky and weather. The live world also suppresses
atmospheric visitors for void scenes.

Use `Anchors` for reusable effects (smoke, lamp breathing, TV flicker, beacons,
twinkling bulbs, music, perches and fireflies) rather than duplicating their
simulation in each view. Anchor positions are screen coordinates; the live
simulation converts relevant positions to design coordinates.

The living scene reaches the picture only through state. A gesture or story
changes an object; `StoryWorld` reports the design-space rectangles whose base
picture changed; the live layer repaints just those from `ground_state` and
draws moving things over them. Never repaint the whole base per frame, never
bake a moving object into the base, and keep each object's `bounds` large
enough for every form it can take (that rectangle is what is repainted).
`docs/engine.md` is the guide, with a compiled example in
`src/story/example.rs`.

To add a view, implement `View` and `Stage` in `src/views/`, declare the module,
and register it in `ALL` (including its array length). Registry-based discovery
handles view selection, dev-panel entries and soak coverage. Update README
view descriptions, the CLI help examples and config comments as appropriate.
Read `coast.rs` for hand-drawn terrain, `downtown.rs`/`kit.rs` for buildings,
`street.rs` for indoor layering, and `moonbase.rs` for a custom sky.

## Constraints to preserve

- **Stable geometry:** `PyRng` matches Python's random sequence. The order of
  calls in `Town::build`/skyline generation is part of the existing wallpaper's
  identity. Add later details with independent random streams; do not casually
  reorder calls or replace this RNG. The seed-12 fingerprint test guards this.
- **Coordinate systems:** general views use a 1600x1000 design; the old town
  retains 5120x3200 reference coordinates. Use `Layout.dx/dy/d`, `cx/cy`, and
  `sy` for their respective spaces. The ground scales uniformly with width
  and sits on the bottom edge; different aspect ratios change sky space or
  crop the design. `Layout.moved/scaled` transform individual pieces.
- **Still/live consistency:** share object drawing through `Draw<S>` and
  `Surface` where practical. Still composition uses floating-point `Canvas`;
  animation uses packed pixels in `Frame`. Keep depth masks aligned with
  geometry and set/reset `Frame.behind` deliberately.
- **Damage correctness:** moving shapes must report touched rectangles. Each
  buffer restores previous damage from the still image before drawing again;
  missing damage causes trails, oversized damage wastes CPU. Preserve
  `live::coalesce` behavior instead of bounding all scattered effects together.
  `Frame.full_damage` skips per-shape bookkeeping only when the caller reports
  full-buffer damage itself. Never suppress damage without that guarantee.
- **Low idle cost:** live still frames normally rebuild every 20 seconds;
  animation pacing adapts to activity and precipitation. Preserve frame
  callbacks, buffer ownership, hidden-screen suppression and resume behavior.
  Hyprland coverage detection includes tiled/fullscreen windows and nearly
  complete floating-window coverage. Generic compositors rely on callbacks.
- **Clock meanings:** timestamps and CLI `--at` use the system time zone;
  animation pacing uses monotonic time. Latitude/longitude do not set the
  process time zone. Use a fixed `TZ` when comparing rendered moments.
- **Scope and style:** match the module-local conventions and explanatory
  comments. Keep changes focused; avoid reformatting whole files or adding
  dependencies for capabilities already provided by this renderer.

## Configuration, state and external integration

Paths honor XDG variables, with the usual home-directory defaults:

| Location | Contents |
| --- | --- |
| `$XDG_CONFIG_HOME/dormer/config.toml` (`~/.config/...`) | Optional TOML settings. Integration settings (`sources::config()`: place, backend, weather, house, power, dev, seed/size/top colours) are read once per process; the editable ones (`settings::runtime()`) are reloaded by the live layer when the file changes. |
| `$XDG_STATE_HOME/dormer/view` (`~/.local/state/...`) | Persistent `dormer view` choice; takes precedence over `[look].view`. |
| `$XDG_CACHE_HOME/dormer/` (`~/.cache/...`) | `state.json`, alternating `frame-a/b.png`, draw `lock`, weather cache, `pretend.json`, `dev.json`, `live.pid`. |
| `$XDG_CONFIG_HOME/systemd/user/` | Three units written by `dormer install`: still service/timer and live service. |

Config settings cover place, look, wallpaper, weather, house, live FPS, power
hold file, dev terminal, activity rates, fixed scene inputs (`[scene]`,
`[scene.simulated_machine]`) and per-view knobs (`[views.NAME]`). Missing
settings fall back to defaults. Invalid *values* make `settings::RuntimeSettings`
refuse the whole file: at start dormer says so on stderr and uses the built-in
defaults; a running live layer keeps its previous snapshot. See
`sources::config` and `settings` for actual behavior and keep
`config.example.toml` in sync when adding settings. Precedence, lowest first:
real inputs, saved profile, dev draft (`dev.json` `set` and `auto`), CLI flags.

Pointer events are native Wayland seat events on exposed wallpaper surfaces;
there is no global mouse polling or keyboard capture. `Screen` owns its
interaction state. Convert logical pointer coordinates through the actual
logical/buffer dimensions and `Layout.back`; do not assume integer scale.
Gestures reach the scene as press/drag/release with a measured velocity; at
most 16 are queued per screen between ticks, and the scene is bounded (four
incidents, 24 actors, 128 bodies, 64 objects, 256 events, 256 instructions a
tick, three escalations then a protected cleanup). Paused, covered or sleeping
outputs ignore new input and do not step their scene: no catch-up on resume.
Maintain immediate stale-output rebuild on wake independently of the global
scene age, and preserve crossfades for visible view changes.

The panel edits a `Draft` (saved snapshot vs edited copy) in `dev/model.rs`;
every key and mouse event becomes an `Action`, and `Model::dispatch` returns
`Effect`s (publish, save, reload, exit) that `dev.rs` performs. Layout and hit
testing share one `Layout`; a dragged slider is captured by its control id and
original cells until release, cancelled by resize or focus loss. Parameter
writes to `dev.json` are batched at most 30 times a second. On Hyprland its own
terminal floats and fades (opacity and blur only, never resized) during
adjustment; `--here` preserves the host terminal's opacity. The still-painter
worker sleeps on a bounded channel and the event loop has no idle timer other
than the 20-second reality refresh. `tests/dev_panel.py` drives all of this
through a real PTY.

Weather `auto` tries Noctalia's cached reading/location, then Open-Meteo via
`curl` if it has a location and no usable reading. Explicit place coordinates
override the location. Open-Meteo refreshes at most every 20 minutes and accepts
cached readings up to four hours old. With no reading, a fair sky is assumed;
with no location, Greenwich is used. `source = "none"` disables weather fetches.
Even `draw --out`, `explain` and explicit `--weather` first gather a real moment,
so flags alone do not isolate network access or local runtime state.

Machine signals read `/proc` and `/sys`: named process sessions, Docker cgroup
scopes, normalized load average, discharging battery capacity, ALSA playback
status. They do not require Docker or audio CLI tools. Noctalia, `swww`,
`hyprpaper` and configured shell commands are still-wallpaper backends. The
README identifies the latter three as implemented but untested integrations.

`dev.json` contains a producer PID and `set` overrides, active only while that
process is alive; `tour` uses the same channel. `hold on/off` sends signals to
the live PID; a configured hold file is a separate pause mechanism. Plain
`dormer` defaults to `draw`, which sets the desktop wallpaper. `view NAME`,
`pretend`, `tour`, `dev`, `install` and `uninstall` also change runtime state or
the desktop: use explicit file exports for routine verification.

## Build and verification

Standard commands, from the repository root:

```sh
cargo build --locked
cargo test --locked
cargo build --release --locked
```

Add `--offline` when dependencies are already cached. Use the release binary
for rendering/performance checks. There is no checked-in CI, separate
Rust integration-test suite, or repository-specific rustfmt configuration.
The optional `python3 tests/dev_panel.py` check drives a real PTY using isolated
offline state and a harmless `true` wallpaper command. It covers fragmented
input, dragging, first adjustment, preview, reset, idle wakeups and cleanup.

`cargo test` includes `story::acceptance`: every incident of every view run
repeatedly, by day and night, in rain and snow, at several proportions and under
interference, asserting the world returns to exactly what the view authored.
`cargo test --release -- --ignored scene_costs --nocapture` prints the scene's
per-frame cost. `soak` also drives random stories and gestures through every
developed view's scene and asserts its limits.

Unit tests cover Python RNG parity, the seed-12 town fingerprint, household
schedules, Tangier astronomy, sky-projection round trips and damage merging.
For behavior changes, add focused tests in the relevant module. For visual
changes, inspect exported PNGs as well: compilation and numerical tests do not
establish visual quality. Check daylight/night, precipitation and contrasting
aspect ratios, and both still and animated paths when affected.

For isolated headless previews, create a temporary config (choose a fresh
directory if this path is already in use):

```sh
mkdir -p /tmp/dormer-preview/config/dormer
cat > /tmp/dormer-preview/config/dormer/config.toml <<'EOF'
[place]
latitude = 35.7758
longitude = -5.7909
name = "Tangier"
[weather]
source = "none"
[house]
lamp = []
containers = false
EOF
export XDG_CONFIG_HOME=/tmp/dormer-preview/config
export XDG_CACHE_HOME=/tmp/dormer-preview/cache
export XDG_STATE_HOME=/tmp/dormer-preview/state
export TZ=Africa/Casablanca

target/release/dormer draw --out /tmp/dormer-preview/street.png \
  --view street --size 1280x800 --at '2026-10-13 21:00' \
  --weather rain --mode dark --agents 2 --containers 3

target/release/dormer sheet /tmp/dormer-preview/sheet.png --size 640x400 \
  '2026-10-13 07:10,clear,rooftops' '2026-10-13 21:00,rain,street' \
  '2026-10-13 21:00,clear,city' '2026-10-13 18:00,storm,coast' \
  '2026-10-13 21:00,clear,moon' '2026-10-13 08:00,snow,coast'

target/release/dormer live --frame /tmp/dormer-preview/live.png \
  --view street --size 640x400 --at '2026-10-13 21:00' \
  --weather rain --mode dark --seconds 2
```

`live --frame` simulates/draws animation without connecting to Wayland. These
exports do not validate compositor lifecycle, scaling, occlusion or suspend;
those require an actual Wayland session. The temporary XDG settings isolate
config/cache/state; machine readings remain real unless overridden.

Additional tools: `bench --n N` times stage setup, painting and finishing;
`soak --n N --from SEED` exercises randomized still/live rendering across all
views and six screen sizes (default 2000 moments; caches every view/size and
can be expensive); `town --seed 12` prints geometry fingerprints; `globe --out
PATH` exports flat Earth geography. `sheet` accepts view knob assignments after
the view ID. `DORMER_TIME=1` prints still-render stage timings. `DORMER_PROBE=1`
reports live timings every five seconds **and keeps animation running when
covered**, so it changes the behavior being measured.

## Working-tree context

Always inspect `git status` and the existing diff before editing; preserve
unrelated work.

Verification notes for the living-scene work are in
`docs/living-scenes-verification.md`, including which checks could not be run
without a native Wayland session. Interactive Wayland/systemd integration (real
pointer events, scaling, occlusion, suspend) was not exercised by automated
checks; treat any change touching `wl.rs`, `pointer.rs` or `buffer.rs` as
needing a real session.
