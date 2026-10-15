# Living scenes: what was verified, and what was not

Written at the end of the implementation of
`docs/superpowers/plans/2026-10-15-living-scenes.md` (preceded by
`2026-10-15-settings-ui.md`). It records evidence, with the commands that
produce it, and is explicit about what could not be exercised here.

## Automated checks (all pass)

```sh
cargo test --locked --offline              # 161 tests, 1 ignored (the cost report below)
cargo build --release --locked --offline   # no warnings
python3 tests/dev_panel.py                 # 7 real-PTY workflows of the settings panel
dormer soak --n 1500 --from 101            # and --from 202, --n 600 --from 31: no panic, limits asserted
```

What the Rust suite covers, beyond the sky, the hours and the seed-12 town
fingerprint:

- `story::model`, `events`, `physics`, `actors`, `director`, `behaviors`: the
  bounds (64 objects, 256 events with 32 reserved, 128 bodies, 24 actors, 4
  incidents, 256 instructions a tick, 3 escalations), stale actor handles,
  reservations released on removal, finite state after floods of gestures,
  cadence-independent results, deadlines that do not advance while paused.
- `story::render_tests`: omitted objects expose the real background, sparse
  base installation equals a full repaint, no change outside requested
  rectangles.
- Per view (`views::episodes::{rooftops,street,city,coast}`): replays of every
  incident from damage through arrival, work, recovery and departure, with its
  helped, interrupted and weather variants, plus the daily-life stories.
- `story::acceptance`: every incident of every view, three cycles by day, two
  by night in rain at 900x900 and one in snow at 1280x400, and a pestered run
  of each ending in the protected cleanup. After each, every object is as the
  view authored it, no reservation is held, bodies and actors are under their
  ceilings.
- `story::example`: the complete extension example in `docs/engine.md` is
  compiled and run.

## Rendering fidelity against the previous binary

Same moments and sizes through the build before this work and this one
(`draw --out`, 1280x800, 07:10, 18:00 clear and 21:00 rain, five views):

- `moon`: byte-identical stills and live frame.
- `rooftops`, `street`, `city`, `coast`: differ only inside the rectangles of
  props that are now authored objects, nothing elsewhere: the washing line with
  its sheet, the bucket and chimney cap (rooftops, about 1.5k pixels); the
  parcel stack on the kerb (street, 18x16 px); the signal mast, hoist and
  junction box (city, about 600 px); the crate on the jetty (coast, 10x10 px).
  Props a story brings out (ladders, cones, barriers, the line) are not in the
  still.
- Live frames of the four also differ, by the same props and by ambient birds
  and cats that now go through the scene.

## Cost (release, 1920x1200 definition, simulation only)

`cargo test --release --offline -- --ignored scene_costs --nocapture`:

| view | idle | one incident at a time | flood (every incident re-triggered and a full gesture each frame) |
| --- | --- | --- | --- |
| rooftops | 0.3 us/frame | 79 us/frame | 0.8 ms/frame (worst 19 ms: a base repaint) |
| street | 0.3 us | 84 us | 1.1 ms (worst 22 ms) |
| city | 0.2 us | 55 us | 1.8 ms (worst 19 ms) |
| coast | 0.1 us | 60 us | 0.1 ms (worst 14 ms) |

A replay of a street hydrant incident (70 s, 2100 ticks) makes 5 base repaints,
the widest 1.4% of the screen; the lighthouse fault (60 s) makes 3, the widest
0.6%. The base is repainted for rectangles whose object state changed, never
per frame. These are simulation and base-repaint costs from the headless
`Session`; they exclude the Wayland copy/commit path and the compositor.

Memory was not measured separately.

## Not exercised

Per the working agreement for this machine, nothing here was run against a
second compositor or against the running desktop session (an earlier attempt to
start a private Hyprland probed the real display). These need a real Wayland
session and were **not exercised**:

- real pointer enter/leave, press, drag, release and cancel on seat removal
  against live surfaces; focus and keyboard ownership of the wallpaper;
- fractional scaling and rotated outputs through `Layout.back`;
- hold/resume, occlusion and suspend with the scene frozen then resumed;
- two monitors reacting independently in a compositor;
- adoption of a saved config by a running live process (covered by unit tests
  of the snapshot swap and by inotify wiring, not by a native end-to-end run);
- the panel's opacity-fade of its own terminal on Hyprland.

The scene's own logic for each of those (queued gestures bounded at 16, input
ignored while hidden/held, no catch-up on resume, per-output worlds, coordinate
conversion) is unit tested without a compositor.

## Review

A whole-change review was done inline by reading the diff against the plan's
constraints (the `superpowers` skills were not used, at the user's
instruction). It found, and the work above fixed: a bucket left displaced after
the tank leak; `Snapshot::intact` showing props that are only brought out by a
story in the still (stills now take the authored snapshot of the view's
definition); the work-boat crew starting a minute's walk away; and unused
engine remnants (removed or marked as shared-kit API).

## Installation

On 2026-10-15 the release build (sha256 `4cb7bd78...79ef`, `dormer 0.2.0`) was
installed to `~/.local/bin/dormer`, the previous binary kept as
`~/.local/bin/dormer.pre-living-scenes` (sha256 `a25fce36...bb59e`) for
rollback, and the existing `dormer-live.service` restarted without changing
its unit file or the personal config. The running process (PID 3731844) has
the installed file as its executable (matching hash) and was still active
without restarts twenty seconds later. To roll back:

```sh
install -m755 ~/.local/bin/dormer.pre-living-scenes ~/.local/bin/dormer
systemctl --user restart dormer-live.service
```
