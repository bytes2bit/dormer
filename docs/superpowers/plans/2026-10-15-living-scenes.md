# Living Scenes Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace cosmetic pointer effects with a versatile Rust scene engine and inhabited, interruptible stories in rooftops, street, city and coast; remove vale and preserve moon.

**Architecture:** Each monitor owns typed object/actor state and a bounded event/sequence director. Rust view definitions bind stable geometry, shared behaviors and branching stories; state-aware base painting plus dynamic depth/damage rendering makes the changes real without full-base painting every animation frame.

**Tech Stack:** Rust 2021, existing CPU rasterizer/Wayland client, libc, existing JSON for replay test inputs, typed Rust scene definitions. No external scene language, plugin loader, ECS framework or GPU engine.

**Spec:** `docs/superpowers/specs/2026-10-15-living-scenes-design.md`

## Global Constraints

- First execute `docs/superpowers/plans/2026-10-15-settings-ui.md`; preserve native execution in this session and the current feature branch.
- This plan and the settings/TUI plan form one delivery. Complete and verify the requested content before final installation.
- Four developed views: rooftops, street, city, coast. Remove vale; keep moon's ordinary still/ambient rendering without new narrative content.
- Shared mechanisms, behaviors and cutscenes are Rust; TOML remains user settings only.
- Preserve Python-compatible seeded geometry, real sky/weather and truthful machine signals. Use separate incident props where damage would otherwise corrupt those meanings.
- Per output: four active principal incidents, 24 narrative actors, 128 dynamic debris/effect bodies, 64 interactive objects, 256 pending events; at most 256 director instructions per simulation tick.
- At most three escalations per incident, then protected final cleanup. No input flood can grow queues or restart that cleanup indefinitely.
- Configured FPS remains the ceiling; hidden/held/power-saving outputs freeze stories without resetting their damage or accumulating catch-up steps.
- No full base paint on every drag/animation tick, no cursor polling, no per-tick network calls and no unbounded allocations.

## Review Focus

1. A 20-second weather refresh occurs mid-repair: damage, actors and reservations remain coherent (Tasks 1, 6–7).
2. Dragging breaks an attachment, then the pointer/seat disappears: release ownership cleanly and preserve the displaced object (Tasks 3, 7).
3. Actors or equipment become unavailable during a sequence: bounded timeout/fallback completes cleanup (Tasks 4–5, 8–11).
4. A broken object's original mask remains in the cache: scenery, rain and actors must not be hidden by nonexistent geometry (Task 6).
5. A retired vale selection, moon launch or monitor resize encounters new state: valid fallback, intact moon and correct stable object routing (Tasks 1, 7, 12).

## File boundaries and dependency order

Create `src/story/` with one responsibility per file:

- `mod.rs`: public facade and `StoryWorld` orchestration.
- `model.rs`: typed IDs, definitions, object snapshots, limits and validation.
- `events.rs`: bounded event routing/deduplication and deadline queue.
- `physics.rs`: finite local contacts, materials and water/snow patches.
- `actors.rs`: route movement, reservations, jobs and poses.
- `director.rs`: typed sequence execution, branches, timeouts and escalation.
- `behaviors.rs`: reusable grab, bird, animal, occupant, repair and ambient behaviors.
- `draw.rs`: shared prop/actor/effect drawing and local damage bounds.
- `replay.rs`: deterministic input schedule, traces and frame exports.

Create `src/views/episodes/{mod,rooftops,street,city,coast}.rs` for Rust story
definitions and bindings; keep existing view modules responsible for their
geometry and scene painting. Extract local drawing helpers only where needed
to share intact/damaged/moving forms. Do not reformat entire existing files.

Modify `src/paint.rs`/`src/views/mod.rs` for state-aware painting,
`src/gfx.rs`/`src/buffer.rs` for mutable depth/rectangular base invalidation,
`src/live.rs` for ordered narrative draw hooks and ambient response, and
`src/wl.rs` for per-output state, input and pacing. Delete the old cosmetic
reaction implementation while retaining finite coordinate parsing/mapping.

### Task 1: Stable typed world state and five-view registry

**Files:** Create `src/story/{mod,model}.rs`; modify `src/main.rs`, `src/views/mod.rs`, `src/sources.rs`, current README/config; delete `src/views/vale.rs`.

**Interfaces:** Typed `ObjectId`, `ActorId`, `RouteId`, `StoryId`; final `Definition { objects, routes, stories }` starts with object bindings here, adds route definitions in Task 4 and sequence definitions in Task 5. `ObjectState` includes pose, material, damage, attachment, owner and visibility; bounded `ObjectStore::get(id:ObjectId)->Option<&ObjectState>` and `get_mut` resolve stable IDs. `Snapshot<'a>` borrows the store and queries absent IDs as Intact. `StoryWorld::new(definition: Definition, seed:u64) -> Result<Self, DefinitionError>`; `StoryWorld::snapshot(&self) -> Snapshot<'_>`; `StoryWorld::rebind(&mut self, definition: Definition) -> Result<(), DefinitionError>` preserves matching object state on resize. `Limits::default()` carries the exact global ceilings.

- [ ] Write failing `story::model::tests::stable_ids_survive_rebind`, `invalid_bindings_are_diagnostic`, `definition_limits_are_enforced`, and registry/migration tests: `views::ALL.len()==5`, no vale lookup, moon present, legacy saved vale falls back to rooftops with a diagnostic, explicit vale selection errors with valid IDs.
- [ ] Run `cargo test --locked --offline story::model::` and registry tests; confirm failures before implementation.
- [ ] Implement typed state and construction validation (duplicates, finite positions, ID references, capacity). Remove vale's active registry/docs/examples; update existing tests that refer directly to it without erasing unrelated assertions. No change to RNG call order or moon drawing.
- [ ] Run full suite and fixed-config moon still/live export; compare the ordinary rendered imagery with the pre-change binary. Existing cosmetic moon effects may disappear only when Task 7 removes the old system.
- [ ] Commit typed state and view retirement.

### Task 2: Bounded event routing and deterministic deadlines

**Files:** Create `src/story/events.rs`; modify `src/story/mod.rs`.

**Interfaces:** `Event { kind:EventKind, source:ObjectId, at:P, layer:u8, origin:u64, depth:u8 }`; `EventBus::emit(&mut self,event:Event)->EmitResult`; `EventBus::drain(&mut self)->impl Iterator<Item=Event>`; `EventKind` covers Impact, Damage, Noise, Obstruction, Arrival, RepairProgress, WeatherChanged and Assistance. `DeadlineQueue::pop_due(&mut self, tick:u64)` returns due jobs only. Use stable tick/order keys.

- [ ] Write failing tests `event_storm_is_bounded_and_deduplicated`, `distance_layer_and_tags_filter_subscribers`, `deadlines_preserve_order_and_pause`, `propagation_stops_feedback`. Assert queue never exceeds 256, propagation depth is capped at four, duplicate origin/receiver deliveries are coalesced, and deadlines do not advance during hidden time.
- [ ] Run `cargo test --locked --offline story::events::`; confirm failures.
- [ ] Implement fixed-capacity/reused storage, bounded fan-out and ordered deadlines; no events allocate subscribers during a tick. Full queues coalesce compatible disturbances before rejecting excess; critical cleanup/completion events use reserved capacity.
- [ ] Run target/full tests, including an event loop that would otherwise self-trigger indefinitely; commit event scheduling.

### Task 3: Local physical objects and material consequences

**Files:** Create `src/story/physics.rs`; modify model/draw contracts and `src/interaction.rs` coordinate utilities.

**Interfaces:** `Gesture { Hover(P), Press(P), Drag(P), Release { at:P, velocity:P }, Cancel }`; `Body` contains position/velocity/material/contact/rest state; `SurfaceMap` contains bounded authored contact segments, slopes and water regions. `Physics::step(&mut self, dt:f32, surfaces:&SurfaceMap, bus:&mut EventBus)`; `Physics::gesture(&mut self, gesture:Gesture, objects:&mut ObjectStore, bus:&mut EventBus)`; `ObjectStore` is the stable bounded object collection from Task 1.

- [ ] Write failing tests for bounded drag/release velocity, broken attachments, glass/wood/metal/stone contact differences, ballistic impacts, resting bodies, water flowing to an authored low point, snow accumulation/brush/roll, and a cancel after a seat disappears. Assert 128-body ceiling after 10,000 inputs, no nonfinite state and no restarted protected cleanup.
- [ ] Run `cargo test --locked --offline story::physics::`; confirm failures.
- [ ] Implement 1/30-second fixed-step motion, simple segment contacts/restitution, spring/hinge constraints and authored break thresholds. Water/snow use bounded patches rather than full-screen grids. Release capture on Cancel without deleting damage. Sleeping bodies stop requesting animation; effects emit actual impact events and finite debris.
- [ ] Run deterministic simulations under different presentation frame cadences; final fixed-tick state must agree. Commit local physics and gestures.

### Task 4: Actors, route reservations and reusable drawing

**Files:** Create `src/story/{actors,draw}.rs`; add shared helpers in `src/props.rs` only when useful.

**Interfaces:** `RouteGraph` contains typed nodes/edges with Walk, Road, Roof, Ladder and Water modes. `ActorSystem::spawn(&mut self, spec:ActorSpec)->Result<ActorId,Capacity>`; `ActorSystem::command(&mut self,id:ActorId,command:ActorCommand)->Result<(),ActorError>`; `ActorSystem::step(&mut self,dt:f32,objects:&mut ObjectStore,bus:&mut EventBus)`. Commands: Move(RouteId), Look(P), Pose(Pose), Carry(ObjectId), Drop(P), Reserve(AreaId), Release. Define typed `AreaId` here; actor IDs include a generation so delayed commands cannot target a reused slot. Shared drawing consumes actor/object snapshots and `Draw<Frame>`.

- [ ] Write failing tests `route_modes_and_obstacles_choose_valid_paths`, `actor_and_prop_have_one_owner`, `unavailable_route_has_timeout_event`, `removal_releases_reservations`, `population_is_capped`, `stale_actor_command_cannot_control_reused_slot`. Assert carried tools move with their owner and dropped tools become physical objects; never exceed 24 actors.
- [ ] Run `cargo test --locked --offline story::actors::`; confirm failures.
- [ ] Implement deterministic route traversal and local authored detours/waits; distinct walking/running/climbing/carrying/working/looking poses, worker/police/resident/courier roles, van/truck/boat movement. Keep actor drawing aligned with existing scale/palette/daylight and report actual touched rectangles.
- [ ] Render a batched shared-pose/contact sheet; verify visible feet, hands/equipment, route endpoints and occlusion bounds. Run tests; commit actors and reusable drawings.

### Task 5: Typed director, shared behaviors and ordinary life

**Files:** Create `src/story/{director,behaviors}.rs`; modify model/runtime, `src/live.rs` for ambient flock response.

**Interfaces:** `Step` enum: Wait, Move, Look, Pose, Carry, Drop, SetState, Emit, Reserve, Release, Branch and Await. `Condition` is a typed enum over local facts/weather/session memory; `Sequence` uses typed stage labels. `Behavior::update(&mut self, ctx:&BehaviorContext, commands:&mut CommandBuffer)` supports unusual Rust behaviors; the context borrows read-only scene/object/actor snapshots, and the bounded command buffer carries typed actor/object/event commands. `StoryWorld::trigger(&mut self,id:StoryId)->TriggerResult`; `StoryWorld::gesture(&mut self,g:Gesture)`; `StoryWorld::step(&mut self,dt:f32,scene:&Scene)`; `StoryWorld::active(&self)->bool`; `StoryWorld::next_deadline(&self)->Option<f64>`. Advance stored simulation time only through `step`; report deadlines in that time domain and convert delays to monotonic time at the Wayland scheduler boundary.

- [ ] Write failing tests for guarded branches, weighted deterministic variants, await success/timeout, exactly three escalations then protected cleanup, cancelled/failed jobs releasing resources and instruction budget yielding after 256 steps. Test cycles that never Wait are rejected at definition validation or stopped with a diagnostic if a custom behavior violates its budget.
- [ ] Run director/behavior tests; confirm failures.
- [ ] Implement move/arrival/work/repair/departure composition and shared behaviors: occupant knock reactions, actual perch evacuation/alternate landing/return, animal attention/chase/flee/return, helpful tool delivery and routine maintenance/delivery/meeting jobs. Route impacts through shared events; user incidents preempt ambient jobs without duplicating actors. Adapt actual ambient birds/animals rather than leaving a static duplicate behind.
- [ ] Test two different fixture views using the same repair and animal behavior with different geometry. Run many complete/flooded cycles; assert empty cleanup reservations, stable allocations/counts and no active timer solely for sleeping behaviors. Commit director and shared behavior library.

### Task 6: State-aware paint, dynamic depth and rectangular base updates

**Files:** Modify `src/views/mod.rs`, `src/paint.rs`, `src/gfx.rs`, `src/buffer.rs`, `src/live.rs`; add render tests in `src/story/draw.rs`.

**Interfaces:** `Stage::story(&self)->Option<Definition>` defaults None; `Stage::ground_state(&self,d:&mut Draw<Canvas>,sc:&Scene,lights:&Lights,bloom:&mut Bloom,out:&mut Anchors,state:&Snapshot)` defaults `ground`; `Stage::indoors_state(&self,d:&mut Draw<Canvas>,sc:&Scene,bloom:&mut Bloom,out:&mut Anchors,state:&Snapshot)` defaults `indoors`; `Stage::story_depth(&self,state:&Snapshot)->Option<Depth>` defaults None. `paint::render_state(sc:&mut Scene,cache:&Cache,state:&Snapshot)->(Canvas,Anchors)`; existing `render` uses an intact snapshot. `StoryWorld::take_base_changes(&mut self)->Vec<Rect>` transfers coalesced object/lighting bounds. `buffer::install_regions(base:&mut[u32],new:&[u32],w:usize,regions:&[Rect])` updates only these regions. Convert model bounds through `Layout` here; do not store pixel bounds in resize-persistent world state.

- [ ] Write failing pixel tests for omitted/replaced props exposing their actual background, a removed pole not occluding an actor, rain behind indoor objects, bloom margins and sparse base install equivalence. Assert no changes outside requested bounds and no ghost image in either shared-memory buffer after an object moves.
- [ ] Run paint/buffer/draw tests and observe missing behavior.
- [ ] Implement state-aware composition and depth snapshots at static transitions. Separate immutable terrain masks from replaceable object geometry; precompute/cache composed depth at transitions, never clone full depth per animation frame. Repaint base only for coalesced static transitions; install affected regions and invalidate both buffers accurately.
- [ ] Add explicit outdoor-before-weather and indoor-after-weather narrative draw hooks in `World::draw`; preserve view alive hooks and full-damage fast paths. Draw moving objects over a base that omitted them once; when resting, bake their new state once. Do not paint opaque color rectangles over intact scenery as a destruction shortcut.
- [ ] Run deterministic sparse/full restoration tests and no-incident baseline exports; record unchanged pixels outside explicitly extracted prop bounds. Commit state-aware rendering.

### Task 7: Native lifecycle, persistence across refreshes and replay tools

**Files:** Modify `src/wl.rs`, `src/pointer.rs`, `src/main.rs`, `src/interaction.rs`, `src/dev/model.rs`; create `src/story/replay.rs`.

**Interfaces:** Each `Screen` stores one lightweight session slot per registered view and current `StoryWorld`; `rebind` handles resize, input resolves stable current-output IDs. `Replay::read(path:&Path)->Result<Replay,ReplayError>` reads JSON `{view,seed,seconds,inputs,frames}`; inputs carry finite time plus gesture or named trigger. `replay::run(replay:&Replay,out:&Path,settings:&RuntimeSettings)->Result<ReplayReport,ReplayError>`. Story control actions Trigger, Pause, Step and Reset do not alter saved profile.

- [ ] Write failing tests for refresh mid-repair, hidden/hold freeze and exact resume, resize stable IDs, cancelled drag on seat removal, view return retaining lightweight state, output removal releasing resources and two monitors reacting independently. Assert discarded wall time creates no catch-up loop. Replay tests reject backwards/nonfinite/oversized inputs and reproduce state at requested ticks.
- [ ] Run targeted tests and confirm failures.
- [ ] Replace old `Action::Light`/cosmetic reactions and view `targets` APIs with story hit/gesture dispatch; retain finite coordinate conversion/parser tests. Capture primary release and derive bounded throw velocity from recent event timestamps. Input handlers record commands; perform no base painting. Rebind targets after geometry changes; paused/hidden input cannot start stories.
- [ ] Wire runtime deadlines/active state into existing FPS/poll pacing and live settings snapshots. Expose `scenes list --view NAME`, replay exports/traces and TUI trigger/pause/step/reset/status. No new scene-language loader. Keep old headless `--click`/`--pointer` as translated test gestures on applicable props.
- [ ] Run unit+PTY checks, native private-compositor enter/drag/release/hold tests and replay traces. Commit integration and remove dead cosmetic code/tests.

### Task 8: Rooftops stories and physical bindings

**Files:** Create `src/views/episodes/{mod,rooftops}.rs`; modify `src/views/rooftops.rs`, `src/town.rs` only for read-only geometry access if needed.

**Interfaces:** `episodes::rooftops::define(stage:&RooftopBindings)->Definition` registers `washing`, `chimney_cap`, `tank_leak`; `RooftopBindings` contains actual roof/ladder/contact geometry and stable prop/route IDs. Use Task 5 shared behaviors and Task 6 state paint; do not duplicate physics/director in the view.

- [ ] Write failing deterministic replay tests for each incident's damage→arrival→work→recovery→departure; assert sheet/crane snag variant, bird interruption variant, helpful/repositioned bucket variant and escalating equipment. Each ends with repaired props, no abandoned jobs and bounded debris/actors.
- [ ] Run rooftops episode tests and observe missing registered incidents/states.
- [ ] Extract reusable intact/damaged forms for washing attachments, a non-machine chimney cap and tank/valve/bucket. Bind actual roof slopes and believable access/ladder routes; retain original Town RNG ordering. Add knockable windows, throwables, bird/animal targets, eligible snow handling and physical light/shutter handling where drawn.
- [ ] Author three full Rust sequences with weighted variants/timeouts, and ambient resident delivery, meeting and maintenance. Ensure weather redirects retrieval and useful player actions advance the work.
- [ ] Render batched daylight/night and portrait repair stages, including motion/depth; run episode/full suite and seed fingerprint. Commit completed rooftops content.

### Task 9: Street stories and surrounding traffic

**Files:** Create `src/views/episodes/street.rs`; modify `src/views/street.rs` and shared actor rendering where required.

**Interfaces:** `episodes::street::define(stage:&StreetBindings)->Definition` registers `hydrant`, `diner_sign`, `delivery_spill`. Bind existing HYDRANT, sign/kerb/road/door/window geometry; view ambient walkers/traffic consume bounded work-zone/obstruction facts from the snapshot.

- [ ] Write failing replay tests for water jet/retreat/traffic response/utility repair, sign attachment failure/owner/ladder reinstall, and parcel retrieval/animal branch. Verify helped versions, an obstructed vehicle route, interference bringing police after escalation and guaranteed final cleanup.
- [ ] Run street episode tests; confirm failures.
- [ ] Implement actual replacement/omission of hydrant/sign/parcel states, water contacts, worker equipment and road reservations. Suppress conflicting ambient traffic in the work zone; preserve the CPU manhole, machine desk lamp and ordinary indoor layering.
- [ ] Author three full incidents plus customer/courier delivery, street meeting and routine cleanup. Bind shared window knocking, branch/sign pulling, throwing, cat stalking, snow and movable local light where appropriate.
- [ ] Inspect rainy night and clear day replay stages at two aspect ratios; check no water/debris overpaints the viewer's sill. Run tests; commit completed street content.

### Task 10: City stories and station/crane coordination

**Files:** Create `src/views/episodes/city.rs`; modify `src/views/downtown.rs` and shared sequence/draw code only for reusable capability gaps.

**Interfaces:** `episodes::city::define(stage:&CityBindings)->Definition` registers `station_signal`, `crane_snag`, `local_power`. Bind station signal/platform routes, existing crane and explicit ordinary-window group; ambient train/crane behavior reads service/operation facts from snapshot.

- [ ] Write failing replays for train stopping short/safe staff access/signal repair, crane stopped during scaffold snag and recovery lift variant, and local power restoration with an unavailable tool. Assert no repair actor crosses a moving train route; machine indicators keep real values.
- [ ] Run city episode tests; confirm failures.
- [ ] Add a visible station signal fitted to existing geometry; implement interrupted crane/load attachments and ordinary light group replacement. Use shared reservations to coordinate trains/crew, and authored access routes rather than workers appearing directly at the fault.
- [ ] Author three incidents with interference/help branches, plus passenger/staff visit, meeting/exchange and station inspection/cleanup. Instantiate shared knockable windows, roof props/throwables, birds/animals and weather-appropriate snow/water responses.
- [ ] Inspect all repair stages by day/night, rain and ultrawide; run tests and commit completed city content.

### Task 11: Coast stories and water routes

**Files:** Create `src/views/episodes/coast.rs`; modify `src/views/coast.rs` and shared water/boat drawing if needed.

**Interfaces:** `episodes::coast::define(stage:&CoastBindings)->Definition` registers `loose_mooring`, `lighthouse_fault`, `cargo_overboard`. Bind actual mooring/jetty/water regions, keeper access route and lantern mechanism, independent of machine-status window.

- [ ] Write failing replays for drifting boat/owner/rescue line/return, keeper carrying parts upstairs/dropped-part recovery/lantern test, and floating crate/gulls/retrieval-order variant. Assert actors move on land/water-appropriate routes and local currents stay in authored regions.
- [ ] Run coast episode tests; confirm failures.
- [ ] Extract stateful ropes, boats, lantern mechanism, crates and doors/windows. Add water splash/contact/buoyancy and wind/current paths through shared physics; preserve shared weather and real machine-status window/boat lantern meanings.
- [ ] Author three incidents and keeper/dock delivery/visit, meeting/exchange and routine maintenance. Reuse shared birds/animals, window knocking, pulling/throwing, physical lights and snow where conditionally present.
- [ ] Inspect dusk/rain and clear daylight, repair/departure routes and cropped portrait; run tests and commit completed coast content.

### Task 12: Whole-world validation, documentation and installation

**Files:** Create `docs/engine.md`, `docs/living-scenes-verification.md`; update README, AGENTS, config comments, `docs/views.jpg`, `docs/weather.jpg` as needed and both plans.

**Interfaces:** A complete Rust extension example uses existing `Definition`, shared behavior and sequence APIs. No new public interface is introduced just for documentation.

- [ ] Add repeated-cycle acceptance tests for all 12 named incidents, helped/interrupted variants and ambient jobs. Assert all reservations/owners release, damage survives intermediate refresh, final cleanup completes, memory/count ceilings hold and no-activity runtime has no due polling wake.
- [ ] Run full Rust suite, release build, PTY test, five-view bounded soak and deterministic replay matrix. Capture intact/damaged/arrival/work/recovery states, daylight/night/rain and contrasting aspect ratios. Compare moon and original geometry baseline; confirm no active vale references outside historical records.
- [ ] Measure release idle/active/flood behavior at identical sizes/moments: CPU/frame times, damage area, base rebuild count, actor/event/body counts and memory. Verify no full-base render per moving frame; retain fidelity and report actual overhead rather than promise universal speedups. Confirm native focus, drag, depth, hold/resume and config saves in an isolated compositor.
- [ ] Request one fresh whole-change review under requesting-code-review, address important findings with focused regressions and rerun affected/full checks. Use the preserved native execution method; do not create implementation subagents task by task.
- [ ] Write extension/behavior/sequence contracts, source precedence, removed-view migration, complete Rust example, measured results and material integration limits. Rebuild the current view reference sheet with five views. Run `git diff --check`, commit final verified work and confirm clean status.
- [ ] Install the verified release to the already authorized local binary path with a rollback backup, restart the existing live service and verify its running executable hash/PID. Do not regenerate service definitions or overwrite personal config unnecessarily. Report installed behavior and remaining physical-device integration limits; retain the feature branch unless integration is separately chosen.

## Coverage and completion audit

- Spec shared mechanics: physics/gestures Task 3, actors/drawing Task 4,
  reusable bird/animal/occupant/help/ambient responses Task 5, actual view
  instantiation Tasks 8–11, stateful physical lighting Tasks 6 and 8–11.
- Spec 12 incidents and daily life: all are explicit deliverables in Tasks
  8–11; terminal states, variants and repeated cycles are checked in Task 12.
- Spec engine extensibility: typed interfaces Tasks 1–5, same behavior in two
  fixture views Task 5, complete Rust extension guide/example Task 12.
- Spec efficient correctness: bounded storage Tasks 1–5, state/depth/restoration
  Task 6, lifecycle/pacing Task 7, measured regression checks Task 12.
- Spec TUI/source/save requirements: companion settings plan Tasks 1–5, with
  narrative controls connected in Task 7. No TOML scene/mod subsystem.
- Spec vale removal/moon preservation: Tasks 1, 7 and 12, including current
  docs/references and legacy-selection behavior.

Do not mark the overall work complete if any principal incident is a placeholder,
if damage is just an overlay on intact geometry, if the TUI cannot save and
retain settings, or if only the shared runtime has been implemented.
