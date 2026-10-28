# Persistent Settings and TUI Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make the terminal controls readable, stable and capable of saving settings that remain active after exit.

**Architecture:** A typed settings snapshot separates saved configuration from an editable draft and transient story playback. A comment-preserving file layer writes config safely; an event-driven terminal model presents it through one keyboard/mouse action system.

**Tech Stack:** Rust 2021, existing libc/JSON/TOML stack, direct `toml_edit` dependency at the already locked version if needed; ANSI terminal/PTY. No web framework.

**Spec:** `docs/superpowers/specs/2026-10-15-living-scenes-design.md`

## Global Constraints

- Execute natively in the current session, preserving the previously selected execution method; inspect current status/diff before edits.
- This is the first of two implementation plans. Continue with `2026-10-15-living-scenes.md` afterward; finishing this plan alone does not finish the request.
- Preserve config comments, unknown settings, permissions and unrelated sections; failed writes preserve the draft.
- Live time/weather/machine inputs remain Auto unless explicitly Fixed; persist fixed local time as HH:MM and fixed date as ISO date.
- Scenario playback position and spawned incidents are transient and never saved as config.
- Never resize/reposition the terminal or move a control during a gesture; preserve host terminal opacity with `--here`.
- No idle polling worker; input and save operations must not introduce recurring idle work.

## Review Focus

1. Existing persisted view overrides config: Save must make the selected view effective after restart (Task 2).
2. Another editor changes config while the panel is open: retain external edits and offer reload/explicit overwrite (Task 2).
3. Rain, midnight, DST and changing machine values while editing: Auto keeps updating; Fixed remains explicit and stable (Task 1).
4. Resize, fragmented escape sequences and a drag leaving the terminal: no stuck drag, accidental save or moving control (Tasks 3–4).
5. Save fails or quit is cancelled: draft and focus remain available; never report Saved after an error (Tasks 2–4).

## File boundaries

- `src/settings/mod.rs`: typed `Settings`, source modes, immutable snapshots and validation.
- `src/settings/profile.rs`: applying persistent fixed inputs to `Moment` in system-local time.
- `src/settings/file.rs`: source fingerprints, comment-preserving edits, backups and atomic writes.
- `src/dev/model.rs`: draft, focus, editing and actions; no terminal escape generation.
- `src/dev/input.rs`: incremental keyboard/SGR mouse decoding, moved from `dev.rs`.
- `src/dev/layout.rs`: cell rectangles, wrapping and scrolling; same hit map used to render and dispatch.
- `src/dev/render.rs`: terminal presentation of layout/model state.
- `src/dev.rs`: launcher, terminal lifecycle, event loop, fade and painter orchestration.
- `src/sources.rs`, `src/main.rs`, `src/wl.rs`: initial load, input precedence and live snapshot replacement.
- `tests/dev_panel.py`: real PTY flow and persistent exit/restart assertions.

### Task 1: Typed saved settings and source modes

**Files:** Create `src/settings/{mod,profile}.rs`; modify `src/main.rs`, `src/sources.rs`, `src/scene.rs`, `config.example.toml`.

**Interfaces:** `Source<T> { Auto, Fixed(T) }`; `Settings::load(path: &Path) -> Result<Settings, SettingsError>`; `Settings::apply(&self, moment: &mut Moment) -> Result<(), SettingsError>`; `RuntimeSettings` contains view/mode, FPS, interaction, activity rates, per-view numeric settings and source choices. Publish editable values as `Arc<RuntimeSettings>`; retain process-start integration fields separately. Task 2 adds original document metadata without making this task depend on its file editor.

- [x] Write failing `settings::tests::auto_and_fixed_have_explicit_precedence`, `fixed_clock_and_date_use_local_calendar`, `invalid_values_keep_last_snapshot`: assert defaults Auto, fixed time 21:30, fixed date 2026-10-15, invalid dates/nonfinite numbers rejected, and real machine readings retained in Auto. Test a DST transition using a fixed TZ in an isolated subprocess.
- [x] Run `cargo test --locked --offline settings::`; confirm missing behavior fails.
- [x] Implement the interfaces and documented `[scene]` fixed sources, `[activity]` rates/story enablement, `[views.<id>]` knobs. Precedence: real inputs → saved profile → live dev draft → explicit CLI arguments. Preserve existing place/weather/backend defaults. Keep machine fixed values visibly named simulation values.
- [x] Run targeted and full Rust tests; explicit file exports with saved fixed inputs must agree after a fresh process. Verify invalid profiles never silently become an all-default config.
- [x] Commit typed settings and synced config comments.

### Task 2: Durable round-trip saving and live reload

**Files:** Create `src/settings/file.rs`; modify `src/settings/mod.rs`, `src/sources.rs`, `src/wl.rs`.

**Interfaces:** `ConfigFile::open(path: &Path) -> Result<ConfigFile, SaveError>`; `ConfigFile::save(&mut self, desired: &RuntimeSettings, policy: SavePolicy) -> Result<SaveReceipt, SaveError>`; `SavePolicy { IfUnchanged, Overwrite }`; `SaveError` distinguishes Parse, Conflict and Io. `SaveReceipt` identifies config/backup and applied revision. `App` owns `Arc<RuntimeSettings>`; `reload_settings(&mut self) -> Result<(), SettingsError>` swaps only validated snapshots.

- [x] Write failing `settings::file::tests::roundtrip_preserves_comments_unknown_keys_and_permissions`, `conflict_preserves_external_edit_and_draft`, `view_override_is_reconciled`, and `failed_second_file_update_rolls_back_owned_write`. Use temporary XDG paths; inject I/O failure deterministically. Assert config content and old view selection are recoverable after a failed save.
- [x] Run `cargo test --locked --offline settings::file::`; confirm failures.
- [x] Use `toml_edit` for targeted edits, source fingerprints for conflicts, same-directory temp+rename, backup and permission preservation. Reconcile the existing state/view file with saved look.view; journal/rollback the two-file update and never overwrite a newer concurrent edit during recovery. Show partial recovery explicitly if filesystem rollback itself fails.
- [x] Add config-parent watches to existing inotify handling. Adopt snapshots on valid saves, retain previous on malformed external edits, and reconfigure only affected caches/resources; FPS/interaction changes must not require a process restart. Label remaining integration fields restart-required.
- [x] Run file tests/full suite; test save→close→new process retains view/fixed inputs and a running isolated live process adopts a new revision. Commit persistence/reload.

### Task 3: TUI state and stable responsive layout

**Files:** Create `src/dev/{model,input,layout}.rs`; modify `src/dev.rs`.

**Interfaces:** `Draft { saved: Arc<RuntimeSettings>, edited: RuntimeSettings }`; `Action` includes Focus, Adjust, EditValue, SetSource, Save, Discard, ResetField, Preview, Quit and DialogChoice. `Model::dispatch(&mut self, action: Action) -> Vec<Effect>` produces bounded Save/Publish/Exit requests. `Layout::build(size: (u16,u16), model: &Model) -> Layout` produces control `CellRect`s and hit targets; `Layout::hit(x:u16,y:u16) -> Option<ControlId>` is the sole mouse routing map.

- [x] Write failing `dev::model::tests::unsaved_quit_supports_save_discard_cancel`, `failed_save_keeps_edit_and_focus`, `source_mode_and_direct_entry_are_explicit`, and `dev::layout::tests::actions_remain_reachable_at_supported_sizes`. Assert 100x30, 70x24 and 40x12 layouts stay in bounds; view names wrap; focus survives category/view changes.
- [x] Run `cargo test --locked --offline dev::`; confirm new tests fail.
- [x] Extract the existing incremental decoder without altering its proven fragmented-read behavior. Add text editing/Enter/Escape and button/toggle actions. Implement categories Appearance, Sky/Weather, Machine Preview, Activity/Stories and View; hide empty categories. Wheel scrolls the list; arrows adjust the selected control; direct entry validates units/range before applying.
- [x] Build stable layout and drag capture keyed by control ID and original slider span. Cancel capture safely on terminal resize/focus loss; release outside the original row ends the gesture. Global Save/Discard/Help remain reachable even in empty categories.
- [x] Run parser/model/layout tests and full suite; commit the interaction model.

### Task 4: Presentation, transparency and complete save workflow

**Files:** Create `src/dev/render.rs`; modify `src/dev.rs`, `tests/dev_panel.py`, README.

**Interfaces:** `render(model: &Model, layout: &Layout) -> String`; existing `Fade` changes only opacity/blur of the dedicated terminal. The model receives save results, publishes preview changes at at most 30 Hz and exits only after the selected exit action succeeds.

- [x] Read Impeccable's craft-floor immediately before UI edits. Write PTY regressions for keyboard/mouse Save, fixed source labels, direct entry, Discard, quit Save/Discard/Cancel, conflict and error feedback, and persistence after relaunch. Preserve fragmented mouse/idle wakeup coverage; replace old coordinate assertions with the new layout's expected rectangles. *(Impeccable was not available in this environment; the panel was inspected directly in PTY renders.)*
- [x] Run `python3 tests/dev_panel.py` against the pre-change release; observe new workflow failures.
- [x] Render view/category hierarchy, readable labels/units, Auto/Fixed controls, status with dirty count, visible Save/Discard/Reset/Preview/Help actions, inline error text and explicit conflict/quit choices. During adjustment fade optional background only; keep the selected control at the same cell, hide other hit targets, and remove all preview resize dispatches. Escape dismisses pinned preview.
- [x] Wire save results to Saved only on success; close removes only transient dev state, while saved profile remains active. Discard restores the saved snapshot. Show active config path and backup on save, and retain unsaved edits on failure. Keep ordinary settings distinct from playback/reset controls.
- [x] Build release/run Rust+PTY tests. Inspect a batched normal/edit/preview/error set at 70x24 and 40x12, fix material defects together, confirm once at most. Commit the complete settings UI.

### Task 5: Independent settings acceptance and handoff to scene runtime

**Files:** Update AGENTS, README, config example and this plan.

**Interfaces:** Scene plan consumes `Arc<RuntimeSettings>` and `Action`/`Effect` to add story listing/trigger/pause/step/reset without bypassing persistence rules.

- [x] Run `cargo test --locked --offline`, `cargo build --release --locked --offline`, `python3 tests/dev_panel.py`, and `git diff --check`; record actual results.
- [ ] In isolated native Wayland, verify opacity leaves hit positions fixed, saved config updates live, cancel retains edits and quitting after Save retains the scene. Check normal wallpaper/app keyboard ownership. *(Not exercised: no isolated native Wayland session was available; see `docs/living-scenes-verification.md`.)*
- [x] Document source precedence, Save/Discard and config reload behavior; update dated agent guidance. Commit verified docs and mark plan checkboxes from evidence.
- [x] Continue with the living-scenes plan. Do not install this intermediate build or call the overall request complete merely because settings now work.
