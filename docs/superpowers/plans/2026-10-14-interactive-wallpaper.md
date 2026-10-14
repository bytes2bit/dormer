# Interactive Wallpaper Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add immersive, bounded mouse reactions to all six views and eliminate redundant engine work without reducing fidelity.

**Architecture:** Native Wayland pointer events feed per-monitor interaction state. Views define targets using their existing geometry; a separate module simulates and paints finite effects through the current damage/depth renderer. Renderer optimizations are verified separately with equivalent pixels and comparable timings.

**Tech Stack:** Rust 2021, existing Wayland/client/protocol crates, software renderer, libc; no new dependencies.

**Spec:** `docs/superpowers/specs/2026-10-14-interactive-wallpaper-design.md`

## Global Constraints

- `[live] interactive = true` by default; false restores the empty input region.
- No keyboard input, global cursor polling, whole-scene parallax or audio playback.
- At most eight reactions, 96 interaction particles and one accepted click per 120 ms per screen.
- Preserve seeded geometry, existing palettes, resolution, weather density, antialiasing, bloom and grain.
- Ignore input while hidden, asleep or held; clear stale focus/state on view/output/seat changes.
- Never rebuild the still image in a pointer handler or exceed configured animation FPS.

## Review Focus

- Fractional scaling and portrait cropping: target hit testing must match the object actually drawn (Task 1 tests).
- Rapid click/motion floods: bounded storage and cooldown, ordered accepted clicks (Task 1 tests).
- A pointer leaving during a fade/hold: no stuck hover, stale focus or reactions on another monitor (Tasks 1/3 tests).
- Buffer reuse after precipitation stops: restore old full damage correctly, without trails (Task 4 tests).
- A hidden output during view switching or hotplug: rebuild the correct view before exposing it (Task 4 tests).

### Task 1: Pure interaction state and drawing

**Files:** Create `src/interaction.rs`; modify `src/main.rs` to declare the module.

**Interfaces:** Produce `Target` (design-space shape, action/color/depth), `Action` (birds/leaves/ripple/dust/sparks/light/cat), `Interaction` with pointer/click/step/draw/reset methods; and `pointer_position(logical, size, layout) -> Option<P>`. `Interaction` owns finite local state; consumes `Scene`, `Layout`, `Draw<Frame>` and targets.

- [ ] Write tests for 1.25x pointer mapping and portrait coordinates, overlapping-target priority, nonfinite/outside input, hover settling/leaving, cooldown, bounded click floods, expiry/reset, and isolated independent instances.
- [ ] Run `cargo test --locked --offline interaction::tests`; confirm missing behavior fails.
- [ ] Implement deterministic bounded effects and hover transitions. Birds use existing `props::bird` drawing and movement semantics locally to prevent cross-monitor reactions; other effects use the existing palette/drawing primitives.
- [ ] Run targeted and full suites; verify expired effects produce no dirty pixels and stationary hover reports no new animation requirement.
- [ ] Commit interaction module and record verification.

### Task 2: View targets and reproducible previews

**Files:** Modify `src/views/mod.rs`, all six view modules, `src/main.rs`, `src/wl.rs` headless export path.

**Interfaces:** Add default-empty `Stage::targets(&self, sc: &Scene, anchors: &Anchors) -> Vec<Target>`. Consume Task 1 `Target`/`Interaction`. Extend `live --frame` with `--pointer X,Y` and `--click X,Y` in 1600x1000 design coordinates.

- [ ] Write tests asserting every view exposes targets at suitable moments, absent cats/dry roads/void scenes expose no inappropriate target, and exported interactions touch bounded regions.
- [ ] Run tests and observe failures before adding target definitions.
- [ ] Define targets adjacent to existing object geometry, including perched birds, seasonal greenery, wet surfaces, the street cat, coast water, lunar ground and vale runes/tree. Use existing masks/depth and weather/visibility conditions.
- [ ] Parse preview coordinates with finite-value validation; drive the same state and drawing used by live input.
- [ ] Run full suite; render six headless reaction screenshots plus rainy indoor and portrait/ultrawide checks.
- [ ] Commit view contracts and previews.

### Task 3: Native pointer integration and configuration

**Files:** Create `src/pointer.rs` for seat/cursor lifecycle as needed; modify `src/wl.rs`, `src/sources.rs`, `config.example.toml`, README.

**Interfaces:** Consume Task 1 coordinate mapping/state and Task 2 targets. `Screen` owns its `Interaction`; pointer focus resolves to one screen ID. Native seat state owns pointer/cursor protocol objects; surface changes clear focus.

- [ ] Write tests for configuration default/disable, focused-output routing, pending motion/click order, pause/leave/seat removal cleanup and software cursor pixels/buffer dimensions.
- [ ] Run tests and observe failures.
- [ ] Bind seats and optional cursor-shape manager; release lifecycle objects; supply a software arrow fallback and integer/fractional cursor scale handling. Enable full wallpaper input only in interactive mode; retain keyboard `None`.
- [ ] Record motion and bounded clicks, integrate stepping/drawing into eligible screens and existing damage/pacing, freeze under hold and discard stale focus on mapping/view changes.
- [ ] Document desktop-icon/input tradeoff and headless controls. Run full suite and release build; exercise compositor input where available without replacing the installed wallpaper service.
- [ ] Commit native input/configuration.

### Task 4: Engine efficiency with unchanged pixels

**Files:** Create `src/buffer.rs` for tested restoration helpers as needed; modify `src/wl.rs`, and measured renderer hotspots in `src/gfx.rs`/`src/live.rs` only if beneficial.

**Interfaces:** Produce `restore_pixels(dst, base, w, stale, dirty)` with exact restore behavior and hidden-screen stale-base scheduling; consume current `Rect`, packed pixels and per-screen visibility state.

- [ ] Add restoration equivalence tests for stale/full/sparse/overlapping/clipped damage and precipitation ending; add screen eligibility/resume tests before changing scheduling.
- [ ] Run tests and observe failures.
- [ ] Skip base painting and fade-driven draws on hidden outputs; rebuild stale outputs immediately on resume, view changes and initial mapping. Choose a visible source screen for shared atmospheric anchors.
- [ ] Restore full-damage buffers with one contiguous copy and omit overlapping stale/dirty work. Reuse scratch collections where beneficial without changing rasterization.
- [ ] Compare baseline release binary and new binary using identical moments; verify still pixels match and benchmark representative calm/rain views. Retain only measured or clearly eliminated redundant work.
- [ ] Run full suite and bounded soak; commit optimizations.

### Task 5: Verification, review and delivery

**Files:** Update `AGENTS.md`, README and this plan's progress; record durable performance results in `docs/`.

- [ ] Run full Rust tests, release build and `git diff --check`.
- [ ] Inspect a batched screenshot set for all views and relevant weather/aspect ratios; fix material defects and verify once more.
- [ ] Check real pointer focus/cursor and hold/visibility behavior in an isolated Wayland test session when feasible; explicitly report anything unavailable.
- [ ] Compare equivalent baseline/new measurements, including active/settled interaction overhead and bounded storage.
- [ ] Request one fresh whole-change review, address important findings with tests, and rerun affected checks/full suite.
- [ ] Update agent/developer documentation, commit final verified work, and report the result and any integration limitations.
