# Interaction and efficiency verification

The current changes retain the existing software rasterizer, image resolution,
antialiasing, weather density, bloom and grain. Optimization targets were
redundant work and idle wakeups, not visual simplification.

## Changes checked

- Native Wayland pointer focus, motion and primary clicks on exposed wallpaper.
  Interaction is local to a screen, with eight finite reactions and up to
  96 analytically drawn particles; no cursor polling or keyboard capture.
- Scene-specific targets in all six views, including weather/visibility-aware
  cats, perches, water, lunar dust, tree lights and the actual standing stones.
- Hidden/sleeping outputs skip base painting and animation; stale outputs
  rebuild on wake without depending on another output's scene age. Visible
  view changes retain their crossfade.
- Full-buffer restoration uses one contiguous copy. If stale rows and old
  precipitation damage overlap the full buffer, it avoids copying the image
  twice. Full-damage frames omit individual shape-damage collection.
- Grouped dev controls, incremental SGR mouse parsing, locked dragging,
  wheel/keyboard adjustment, compact preview, and reset/quit cleanup.
- Panel updates are combined at up to 30 Hz; idle redraws stop, and the still
  worker sleeps on a bounded event channel instead of polling every 40 ms.

## Evidence

The Rust suite currently contains 27 tests. `python3 tests/dev_panel.py` drives
a real terminal PTY with a temporary config and a harmless `true` wallpaper
backend. It checks initial selection, fragmented terminal reads, dragging out
of the original row, release, compact preview, resetting from an empty group,
idle writes/wakeups and restoration on quit.

18 no-input PNG exports were byte-identical to the pre-feature release binary:
each of the six views at night in clear weather (960x600), snow (540x960), and
an animated rain frame (960x600). This covers unchanged imagery separately
from the interaction tests. A 36-moment randomized soak covered all registered
views and the existing six-size matrix. Reaction screenshots were inspected
in a six-view contact sheet.

A private Hyprland instance bootstrapped through its Wayland backend was used
for native checks, then continued with its virtual fallback output. Observed
real pointer enter/motion/button events and cursor-shape requests, floating
panel sizing, group selection, opacity changes and blur-free compact preview.
After transient fades settled, a held wallpaper's user CPU tick count remained
unchanged during the sampled interval; resume was also exercised. These checks
did not replace the installed wallpaper or its service.

The software cursor fallback is covered by raster/transparency unit tests;
the native compositor advertised cursor-shape support. Fractional/portrait
coordinate mapping and stale-output scheduling are covered by unit tests.
Physical multi-monitor hotplug, rotated monitors, hardware sleep and a
compositor without cursor-shape support still merit manual integration checks.

## Timings and limits

On this host, three 1920x1200 headless animation samples per case produced:

| Scene | Before, ms | After, ms |
| --- | --- | --- |
| Street, clear | 0.19 / 0.21 / 0.22 | 0.22 / 0.19 / 0.23 |
| Street, rain | 1.72 / 1.70 / 1.71 | 1.67 / 1.68 / 1.75 |
| Street, snow | 0.95 / 0.95 / 0.95 | 0.93 / 0.91 / 0.98 |

These timings use the existing `live --frame` estimator and show similar draw
cost within run-to-run variance; they are not a measured desktop CPU percentage
or a broad speedup claim. Very quiet moon frames were below this estimator's
reliable range. Headless draw timings exclude the real Wayland restoration,
copy/commit path, compositor cost and visibility savings.

The reproducible improvements are fewer operations: full stale+rain restore
copies one image instead of two, hidden-output base paints are skipped, and
the idle panel worker no longer wakes 25 times per second. Tests assert the
copied-pixel count, visibility decisions, equivalent raster pixels and idle
context switches. Still-frame rendering remains unchanged, so no still-paint
speedup is claimed. See the design spec for the original still-frame timings.

Hyprland panel fading uses the current `opacity` and `no_blur` properties, with
an older `alpha` fallback. Upstream lists those dynamic properties in the
[dispatcher documentation](https://wiki.hypr.land/0.55.0/Configuring/Basics/Dispatchers/)
and [window-rule documentation](https://wiki.hypr.land/0.55.0/Configuring/Basics/Window-Rules/).
The dedicated terminal is the only window altered. With `dev --here`, the host
terminal controls its own opacity; other compositors retain terminal-supported
background transparency and the compact preview.
