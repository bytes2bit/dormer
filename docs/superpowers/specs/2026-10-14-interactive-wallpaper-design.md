# Interactive wallpaper and engine efficiency

## Intent and scope

Make dormer's existing six views respond to mouse movement and clicks in a way
that feels like touching the place, while preserving the familiar composition,
real sky, weather, machine signals and quiet background behavior. The user has
delegated the choice of reactions to the implementer. Optimize measured or
demonstrably redundant engine work without reducing image resolution, particle
density, antialiasing, bloom, grain or the richness of the scenes.

The pre-existing work was committed as `62b9529` before this work began. This
change targets the Rust implementation, not the Python prototype.

## Approach

Use native Wayland pointer events on the exposed background surface, with
small per-view targets and a bounded, per-monitor interaction simulation.
Reuse `Layout`, `Draw<Frame>`, depth masks and damage restoration. Keep the
existing view geometry and rendering world as the visual authority.

Alternatives considered:

- Global cursor polling would make a click-through surface react to movement
  but would require compositor-specific access, periodic work, and a separate
  solution for clicks. It is not the selected approach.
- Whole-scene parallax would require moving/redrawing the large still image
  and complicate occlusion, scaling, indoor layering and damage. Local object
  reactions provide immersion with much smaller repaint regions.

## Input behavior

- Add `[live] interactive = true`, enabled by default for the requested
  experience. `false` restores the current empty input region and click-through
  wallpaper. Describe the desktop-icon tradeoff in the example config/README:
  Wayland cannot deliver the same click to this surface and one beneath it.
- Bind available `wl_seat` globals and their pointer capability. Handle pointer
  enter, motion, leave and primary-button presses; release objects on capability
  or seat removal. Bind no keyboard and keep layer-shell keyboard interactivity
  at `None`. Windows above the background retain their ordinary input.
- Map the focused surface to its monitor. Pointer coordinates are logical
  surface coordinates: convert using the monitor's actual logical/buffer size,
  then `Layout.back`. This must work with integer/fractional scaling, rotated
  outputs and non-16:10 aspect ratios.
- Set a visible cursor on enter. Prefer the existing dependency's cursor-shape
  protocol; provide a small software arrow cursor fallback when unsupported.
- Pointer handlers record state and actions; they do not render or rebuild
  `Scene`. Coalesce motion to the latest position per animation tick. Dispatch
  discrete clicks in order through a small bounded queue, rate-limited to one
  accepted primary click per 120 ms per screen.
- Leave fades hover responses out. Held, hidden and sleeping screens ignore new
  reactions. View changes clear interaction state. Output resize/removal,
  restacking and seat removal clear stale focus and targets.

## Reactions and visual language

Each view supplies a small list of targets in design coordinates with a hit
shape, scene-aware visibility, action, effect color and occlusion layer.
Targets follow objects that already exist rather than introducing a UI overlay.
Use subtle warm/cool light changes or small motion around a hovered object,
with a short eased onset and release. A stationary pointer settles to a stable
response and does not cause perpetual redraws solely for hover.

| View | Mouse movement | Primary click |
| --- | --- | --- |
| `rooftops` | A nearby rooftop perch becomes attentive; gentle feedback around the clock tower. | Birds take off from the visible perch; a few seasonal leaves lift from nearby greenery. |
| `street` | The sill cat responds with a small ear/head movement when present; light catches the mug/plant as the pointer approaches. | The sill cat reacts; the roof perch scatters birds; wet glass or road produces a short local water response. |
| `city` | Subtle response at the station clock and rooftop perch. | Birds lift from the perch; a short local station/window light response that returns to its real state. |
| `coast` | Nearby water has a restrained glint; the keeper's window gives hover feedback. | Water produces expanding, fading ripples; the lighthouse perch releases birds. |
| `moon` | The nearest station light or rover acknowledges the pointer. | Ground produces a small ballistic regolith/dust burst; the station answers briefly with light. No atmospheric smoke or Earth-style birds. |
| `vale` | Runes/tree lanterns respond near the pointer. | A small bloom of firefly-like sparks around the tree/runes; lake ripples and tower birds where appropriate. |

Feedback must honor day/night illumination and the existing palette, depth and
weather. Do not change weather, time, machine status or persistent config through
clicks. Object responses return to their real state after the reaction ends.
No audio playback or generic click rings/tooltips. Right click and scrolling
do not trigger scene actions in this first implementation.

Keep effects finite: at most eight active click reactions per screen and at
most 96 interaction particles per screen. Apply explicit lifetime/cooldown
limits; repeated clicking must neither grow memory without bound nor keep
allocating new collections. An effect may replace the oldest effect at capacity.

## Implementation boundaries

- `src/interaction.rs`: pure hit testing, hover settling, rate limits, bounded
  transient effects, stepping and drawing. Test it without Wayland.
- `src/views/mod.rs` and view modules: a default-empty target contract and
  the six views' target definitions/object responses. Keep targets tied to
  existing geometry and presence conditions.
- `src/wl.rs`: seat/pointer/cursor lifecycle, surface coordinate mapping,
  per-screen interaction state and integration with frame pacing/restoration.
- `src/live.rs`: narrowly reuse existing bird simulation for explicit user
  reactions, with bounded population and correct monitor ownership. Do not
  make a click on one output produce a reaction on every output.
- `src/sources.rs`, `config.example.toml`, README and AGENTS: expose and explain
  the setting and new development/verification behavior.
- Headless exports: extend `live --frame` with reproducible pointer position
  and click inputs so interaction can be inspected/tested without a compositor.

Do not turn the already-large view/Wayland files into a general UI framework.
Keep pointer state, interaction simulation and engine bookkeeping distinct.

## Engine audit and optimization

Two concrete opportunities are visible in the current code:

1. `App::rebase` paints every configured screen when a visible screen needs a
   new still frame. Skip hidden/sleeping screens, track their stale base, and
   rebuild them immediately when visible. Avoid fade-driven drawing on hidden
   outputs too. Preserve shared world timing, per-output buffer correctness,
   initial mapping and immediate resume behavior.
2. In precipitation, `App::draw` restores individual prior damage even when
   it ultimately redraws/damages the entire buffer; stale rows may overlap that
   work. Restore full-screen damage with one contiguous copy and avoid duplicate
   restore copies. Reuse bounded damage/scratch storage where measurement shows
   an improvement. Keep packed output identical for the same frame inputs.

Audit ground painting, bloom scratch allocation, repeated window/shape work,
damage coalescing and per-frame collection creation next. Only retain renderer
changes with measured benefit or a clear reduction in redundant work and
pixel-equivalence coverage. Do not trade fidelity for benchmark numbers.

Keep pointer input event-driven. Active reactions can request the existing
configured FPS, but must not exceed it or extend past their useful lifetime.
Idle/off-screen interaction adds no timer/polling loop. Existing hold and
screen-sleep behavior takes precedence. Interactive hover must not trigger
an expensive full still-frame rebuild.

## Evidence and acceptance

Baseline on this checkout: `cargo test --locked --offline` passes six tests and
`cargo build --release --locked --offline` passes. A 30-frame 1920x1200 street
night benchmark measured 19.27 ms painting and 4.50 ms finishing on this host;
stage setup was 19.44 ms. Numbers are a local baseline, not a general guarantee.
Additional 20-frame release measurements (1920x1200, seed 12, 2026-10-13 21:00,
Africa/Casablanca, dark mode, isolated config with weather source `none`):

| View / preset | Stage ms | Paint ms | Finish ms |
| --- | ---: | ---: | ---: |
| rooftops / clear | 12.30 | 18.01 | 4.70 |
| street / clear | 20.32 | 19.32 | 4.49 |
| street / rain | 19.93 | 22.42 | 4.57 |
| city / clear | 15.00 | 14.29 | 4.43 |
| coast / rain | 11.03 | 16.76 | 4.54 |
| moon / clear | 4.14 | 9.39 | 4.50 |
| vale / clear | 20.17 | 14.23 | 4.62 |

Raw output is also at `/tmp/dormer-agent-review/engine-baseline.json` for local
before/after comparison. Use the same host and inputs; rerun baselines when
making a direct comparison rather than assuming these timings are stable.

Required verification:

- Tests written before implementation for scaled coordinate mapping, target
  visibility/priority, enter/motion/leave, click cooldown/capacity, effect
  expiry, pause/resume and single-monitor reaction ownership.
- Deterministic pixel checks for optimized buffer restoration/rendering;
  existing RNG/town/astronomy/damage tests remain passing.
- Full Rust suite and release build; bounded soak coverage over the registered
  views, including portrait/ultrawide sizes and combined weather inputs.
- Headless screenshots of all six interaction types, daylight/night where
  relevant, plus a rainy street indoor-layering check.
- Comparable release measurements for baseline scenes and interaction overhead.
  Report any tradeoffs, variance and unmeasured compositor costs honestly.
- If a usable Wayland session is available, exercise real pointer focus,
  default/fallback cursors, scaling, multiple outputs, view switching, hold,
  occlusion and resume. Otherwise report that integration limitation explicitly.

The delivery is complete when mouse input visibly produces appropriate,
finite reactions in all six views, input can be disabled, ordinary application
windows retain their input, resource use settles back after reactions, and
the retained engine optimizations preserve visual output and resume behavior.
