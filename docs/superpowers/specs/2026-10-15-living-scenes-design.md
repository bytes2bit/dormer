# Living scenes, a versatile Rust engine and usable settings

## Intent and agreed direction

dormer should feel like a small inhabited world. Touching it changes actual
objects and affects the surrounding activity. Authored stories provide
coherent consequences, but routes, conditions, interruptions and local events
make their execution vary. Everyday behavior remains believable; interference
can escalate into exaggerated chain reactions, stubborn characters and comic
repairs. A pulse drawn over an unchanged object does not satisfy this brief.

The user approved expanding beyond the first cosmetic interaction pass and
asked for shared interactions, view-specific scenes, a versatile engine, an
optimized implementation and a redesigned TUI with working config saving.
The user clarified that engine extensibility means reusable Rust architecture,
not an external TOML mod-pack system. Rust remains the language for the engine,
behaviors, effects and cutscenes; TOML remains ordinary user configuration.
This specification supersedes the October 14 interaction design where the
two disagree. Its implementation has not started at specification review.

Scope:

- Develop rooftops, street, city and coast. Each receives shared interactions
  and three complete principal incidents, described below.
- Remove vale from the implementation, registry, controls, current docs,
  examples and test coverage. Historical design documents can remain historical.
- Keep moon and its existing still/ambient rendering. Remove the previous
  cosmetic interaction system globally; new narrative content for moon is
  deferred. Moon remains selectable and must continue rendering correctly.
- Preserve the real sky, weather, stable seeded geometry and the meaning of
  machine signals. Incident lamps and faults must not impersonate the agent,
  container, CPU or battery indicators.
- Replace the dev panel's interaction model; retain the terminal/native
  Wayland architecture. No browser UI, GPU migration or general physics engine.

## Chosen architecture and alternatives

Use a bounded scene simulation and a typed sequence director, with reusable
Rust actor, prop, material, effect and behavior components. Views assemble
objects, routes and stories through Rust structs, enums, traits and builders.
Adding a new story should use these interfaces without changing Wayland input,
buffer management or unrelated views. Adding a genuinely new behavior extends
a focused Rust component rather than a large central view-name switch.

Writing an isolated bespoke simulation for every cutscene would duplicate
movement, physics, timing, rendering and cleanup. An external scripting or
configuration language would add an unnecessary authoring layer. The chosen
Rust architecture shares those mechanisms while allowing typed branching
sequences and custom Rust behaviors when a story needs them. Finite local
physics serves these scenes without unrestricted whole-world simulation.

Work has three dependent tracks: the scene runtime and content; the settings
and TUI that control it; removal/migration and delivery checks. They belong to
one delivery, with separately testable modules and implementation milestones.

## Objects, actors and events

Each output owns a scene runtime, separate from `Scene`'s real-world inputs
and the shared atmospheric `live::World`. A stable object ID identifies the
same lamp, sign or window across base refreshes and resizes.

An object definition supplies a hit shape, transform, material, available
states, attachment points, render/depth layer and permitted interactions.
State includes pose, damage, ownership/reservation and relevant local facts
such as a valve being open. Object state is authoritative: damaged glass is
absent or fractured, a broken sign hangs differently, a fallen cap stays down.
Normal 20-second sky refreshes must not repair anything.

Actors have explicit jobs and poses: idle, look, walk, run, carry, operate,
work, climb and react. They use authored route graphs for roads, pavements,
roofs, ladders and water. Reserved work areas and local obstacles select
alternate authored routes or cause a wait; this is not unrestricted navigation
over arbitrary image pixels. Vehicles and boats use suitable routes and
arrival/departure poses. Equipment can be carried, dropped and recovered.

Events connect these systems: impact, damage, noise, obstruction, arrival,
repair progress, weather change and assistance. Consumers subscribe by tag,
distance and layer. Each event carries origin and propagation depth; bounded
fan-out, deduplication and cooldowns prevent feedback loops. A falling tile
may hit a sign, startle birds and interrupt a worker without restarting every
incident on the screen.

Small session memories change reactions: disturbance count, last repair
method, occupied perches and recent helpful actions. Repeated interference
can bring barriers, another worker or police assistance. Police secure and
manage the scene; they do not capture keyboard input or take over the camera.

## Shared interaction vocabulary

Native pointer events support hover, primary press/release, drag and release
velocity. Objects declare which gestures they accept. Empty-space movement
does not spawn effects. No keyboard focus is taken by the wallpaper.

The following are reusable mechanisms, instantiated only where appropriate:

| Mechanism | Concrete consequence |
| --- | --- |
| Grab, pull, release | Branches, aerials, washing lines and signs bend or swing; their attachments can fail at an authored threshold. |
| Throw loose objects | Pebbles, snowballs, tools and debris follow bounded ballistic motion, bounce or splash, and generate material-specific impacts. |
| Knock on windows | Occupants approach, look out, wave, open curtains or close shutters; repetition changes their response. |
| Disturb birds | Birds leave their actual perch, use alternate landing points, circle back and interact with equipment. Do not merely spawn a replacement flock over unchanged birds. |
| Animal attention | Cats look toward nearby movement, stalk a dragged object, chase a thrown item and flee or return after an incident. |
| Material damage | Glass fragments, wood splinters, bent metal and masonry pieces use different motion, damaged drawings and repair actions. |
| Local water | Jets, leaks and spills follow authored surface slopes, accumulate in bounded patches and affect nearby actors. Rain/water bodies supply splashes and ripples as consequences. |
| Obstruction | Actors and vehicles stop, wait or take alternate routes around displaced objects and work zones. |
| Snow handling | Brush snow from eligible props, roll/throw snowballs and release small roof accumulations; actors can clear the resulting mound. |
| Moving physical light | Moving a lamp or shutter changes its local illumination/occlusion rather than adding a generic hover halo. |
| Helping | Return equipment, clear debris, deliver a needed part or steady an object; the director advances or simplifies the task. |

Ground contacts, surface slopes, landing points and attachment points come
from view geometry. Physics handles small nearby bodies, simple segment
contacts, restitution and rest; it must not claim full-world fluid or rigid
body simulation. Per-view availability is explicit, including seasonal snow
and real animal presence. Shared capabilities do not mean every object must
respond to every gesture.

## Principal authored incidents

Each entry below is complete only when it includes a visible initial change,
surrounding reactions, arrival or movement of responders, work, recovery,
departure and at least one meaningful branch or interruption. Scene-specific
objects must occupy real, accessible positions, not arbitrary click regions.

| View | Incident | Required sequence and variation |
| --- | --- | --- |
| rooftops | Escaped washing | A tug releases a sheet; wind carries it to an aerial or crane. A resident attempts retrieval; a roofer helps if it snags high. Another tug can change the snag and recovery route. |
| rooftops | Chimney cap | The cap falls with tile debris, birds abandon the roof, a roofer arrives by a believable ladder route and repairs it. A bird can reclaim the worksite, and repeated damage brings better equipment. Use a chimney unrelated to the CPU signal. |
| rooftops | Tank leak | Water follows roof slopes, residents move belongings or bring buckets, and a plumber closes the valve and patches the tank. Returning a part helps; moving the bucket creates a new local problem. |
| street | Hydrant | A struck or pulled cap releases a water jet. Walkers retreat, traffic slows, a utility truck arrives, workers establish a work zone, close the valve and replace the part. Continued interference brings police assistance and a more elaborate repair. |
| street | Diner sign | The sign swings, loses an attachment and hangs or falls; its light follows its state. The owner investigates, a van brings a ladder, and workers secure/reinstall it. Displacing equipment interrupts their plan. |
| street | Delivery spill | A courier drops parcels, passersby react and an animal takes interest. The courier retrieves them and resumes delivery. The user can help return parcels or send one into a different recovery route. |
| city | Station signal | The signal fails visibly; a train stops short, people gather safely and staff secure the platform. Technicians arrive and test/restore it; an obstruction changes their route or repair order. |
| city | Crane snag | A pulled/swinging load catches an eligible scaffold or attachment. Operations stop, workers approach and coordinate release, then restore the rig. Interference can drop a part and require a recovery lift. |
| city | Local power fault | A defined group of ordinary windows/signs loses power, occupants investigate and an electrical crew traces and fixes the fault. Weather or a displaced tool changes the work sequence. Machine-status lights remain truthful. |
| coast | Loose mooring | A rope releases, a boat drifts according to the local wind/current path, its owner runs to the jetty and a second boat or thrown line helps recover it. Moving the line changes the approach. |
| coast | Lighthouse fault | The lantern mechanism fails visibly; the keeper investigates, carries parts upstairs and repairs/tests the sweep. Wind and a dropped part complicate the job. The keeper's machine-status window remains independent. |
| coast | Cargo overboard | A displaced crate falls from the jetty, splashes and floats along a bounded water route. Gulls investigate, dock workers react and a boat collects it. Interception or a second loose item changes the recovery order. |

## Ordinary life and escalation

Each developed view also receives autonomous daily activity using the same
actors/director: a delivery or visit, a meeting/exchange, and routine
maintenance/cleanup. Select view-appropriate versions: residents and roofers,
shop customers/couriers, station staff/passengers, keeper/dock workers.
Weather can interrupt an activity, such as someone retrieving washing in rain.
Animals can occupy equipment or take interest in dropped items.

Autonomous starts use low-frequency deadlines and scene conditions. They must
not constantly trigger destructive incidents. Player-triggered stories take
priority; ambient jobs yield and actors remain owned by one job at a time.
Incidents can interact through the shared event system, but a bounded number
run concurrently. Escalation has authored endings; returning to an ordinary
inhabited scene is part of every sequence. Each incident allows at most three
escalations before a protected final cleanup. Further input cannot restart
that cleanup or grow the repair crew indefinitely.

## Sequence director

Typed Rust definitions express triggers, guards, weighted variants and named stages.
Steps include wait, move along route, face/look, play pose, carry/drop,
set object state, emit effect/event, reserve/release area, branch, and await
actor/event. Branches inspect local facts, weather and session memory.
Movement has completion/failure events and a timeout/fallback; a missing actor
or blocked route cannot strand a repair forever.

The director advances on fixed simulation ticks and due deadlines. Sequence
time is visible simulation time, not wall time. Hold, power saver and hidden
outputs freeze stories without resetting damage or accumulating catch-up
steps. Resume continues coherently. View switches retain lightweight state
per output/view during the process; geometry-dependent caches can be discarded.
Resizing rebinds stable IDs and routes. Process restart restores the intact
world; persistent destruction is outside this release.

## Rendering and performance

The renderer must support omission/replacement of interactive parts at their
actual paint position. No cloning nearby pixels to cover an intact object.
View code exposes stable object geometry and static/dynamic paint hooks;
still exports use the same intact object drawing.

Terrain and buildings retain their existing cached rendering. When an object
changes static state, a state-aware base render reconstructs the real
background, correct lighting and replacement geometry. These infrequent
changes are coalesced, and only affected rectangles plus bloom margins are
installed in the displayed base/buffers. Dragging and animation do not perform
a full base render each frame: the moving part is omitted once and drawn
through `Draw<Frame>` until it rests or changes state.

Immutable terrain depth and mutable object masks are kept distinct. Removed
or displaced parts must not leave stale occlusion; characters, debris and
weather must cross indoor/outdoor and foreground boundaries correctly.
Retain existing full-damage restoration, visibility suppression, frame
callbacks, buffer ownership and per-output input routing.

Default hard limits per output: four active principal incidents, 24 narrative
actors, 128 dynamic debris/effect bodies, 64 interactive objects, and 256
pending events. At most 256 director instructions execute per simulation tick;
loops must yield. Limits are engine ceilings, not targets for constant
activity. Excess disturbances coalesce into existing incidents or receive a
bounded local response. Reuse storage; no unbounded input queue or allocations
on every simulation tick. Configured FPS remains the ceiling.

Unchanged, idle scenes retain their existing resolution, antialiasing, bloom,
grain and weather density. No periodic cursor polling, network work in actor
updates, full-screen physics, or busy-waiting sequences.

## Rust extension interfaces and developer tools

Keep responsibilities separate: object state/attachments, material contacts,
actor movement/poses, event routing, sequence scheduling and drawing. Modules
use typed IDs and commands rather than stringly typed action dictionaries.
Prefer enums and compact data for the common path; use traits at actual
extension boundaries without allocating a boxed object for each particle.

A view supplies geometry bindings and registers its objects, routes and
stories. Shared Rust prefabs implement characters, vehicles and common props;
small typed sequences compose their actions. More unusual interactions can
implement a Rust behavior interface that reads a bounded context and emits
validated commands/events. They must obey the same ownership, scheduling,
damage and resource limits as built-in sequence steps. Avoid a general ECS or
plugin ABI unless the actual implementation demonstrates a need.

Compilation checks action/parameter types. Scene construction checks stable
ID references, finite coordinates/durations, route connectivity, yielding
sequence control flow and population ceilings. Diagnostics name the view,
story and offending binding. Asset data needed by built-in content is embedded
or shipped through the existing binary installation contract; image decoding,
if used, is cached and bounded. No external pack loader, hot-reload scripting,
mod marketplace or TOML scene language is part of this work.

Provide `dormer scenes list --view NAME` and
`dormer replay FILE --out DIRECTORY`. Replay input accepts timed pointer
gestures and named triggers, seed and duration. This is a test input format,
not an engine programming language. Export deterministic frames at chosen
times plus a state/event trace. Scene controls in the panel can also trigger
any listed incident in the active view.

Document a small complete Rust example that adds a prop, a shared behavior
and a branching story without touching Wayland or buffer code. Verify reuse
with the same behavior instantiated in two different views. The engine remains
part of dormer; extracting a standalone engine crate is not a prerequisite.

## TUI job and design direction

The user should be able to choose a view, adjust it while watching the scene,
understand what is temporary, save the desired settings, and leave without
surprises. This is an Operate surface: readable values, stable controls and
familiar actions outrank decorative terminal effects.

Retain the existing terminal identity and restrained color vocabulary. Replace
the shifting slider-only layout with a view selector, grouped controls,
explicit source modes and a persistent action/status area. Settings include
View/Appearance, Sky/Weather, Machine preview, Activity/Stories and per-view
controls. Scene tools list available incidents with trigger, pause/step and
reset actions, plus active actor/incident status.

Wide terminals can use a category column and control pane; 70x24 uses compact
tabs and a scrolling list; small terminals retain reachable Save/Discard and
help rather than clipping controls. Empty categories are hidden or clearly
explained. Selection is restored by stable key when switching groups/views.

Interaction requirements:

- Click/drag sliders, arrows for adjustment, Enter for direct value entry,
  Tab/Shift-Tab between focusable controls, and wheel for list scrolling.
  Buttons/toggles are drawn and behave as buttons/toggles, not numeric sliders.
- A focused control shows its units, limits and short help. Keyboard and mouse
  use the same actions, including visible Save, Discard, Reset and Preview.
- While dragging, optionally fade the dedicated terminal and retain the
  selected control at its original cell position. Never resize/reposition the
  terminal or move the target during a gesture. Hidden controls stop accepting
  clicks. Preview can be pinned or dismissed with Escape.
- Maintain readable foreground text under transparency. `--here` continues
  respecting the host terminal's opacity; no unrelated window is changed.
- Show Saved, Unsaved changes, Preview only, Saving, and actionable error
  states explicitly. Color is supplementary, not the sole state indicator.
- Quit with unsaved settings offers Save, Discard or Cancel. Crash/termination
  removes transient preview state; it does not write unsaved config.

## Saving and source semantics

Saving is an actual durable operation on `config.toml`, not keeping a cache
producer alive. Persist normal settings (view, mode, FPS, interaction, activity
rates and per-view settings). The selected view must also
reconcile the existing higher-priority persisted `view` file, so saving a view
cannot silently leave another view active.

Live time/weather/machine inputs remain live by default. Each adjustable live
input has an explicit Auto or Fixed source. A changed value is visibly Fixed;
the user can return it to Auto. Saving fixed values persists them under a
documented scene-override section. Fixed local time uses an HH:MM value; fixed
date uses an ISO calendar date, not an aging relative day offset. Machine
values marked Fixed remain clearly labelled simulation/preview values.
Scenario playback position and spawned incidents are transient and never
saved as config. The save summary lists fixed sources as well as ordinary
settings, so persisting simulated weather or machine readings is explicit.

Config writes preserve comments, unknown keys and unrelated sections, use
same-directory atomic replacement, retain file permissions and keep a backup.
Detect external edits since load and require reload or an explicit overwrite
choice; never erase a concurrent edit silently. On parse/I/O failure, keep the
file untouched and the panel's dirty edits available. Use the already locked
TOML editing stack for round-trip editing, adding a direct dependency only
where needed for its API.

Saved settings take effect in the live process through a validated immutable
settings snapshot and existing file-watch wakeups. Replace relevant reliance
on process-lifetime `OnceLock` for editable runtime settings. Layout/resource
changes rebuild only necessary caches. Restart-required integration settings
must be labelled as such. Saving and closing the panel must leave the saved
scene active; Discard returns to the saved baseline, not unrelated defaults.

## Vale migration and compatibility

Delete the active vale source/module and registry entry. Remove current CLI,
config and README references and replace the visual reference sheet so it no
longer advertises a removed view. Explicit `--view vale`/`view vale` errors list
valid views. A legacy saved/configured vale selection falls back to rooftops
with a clear diagnostic; no old local file is changed until saving settings.
Registry-driven soak and TUI discovery must find five views, including moon.

Maintain native click-through configuration and app input ownership. Existing
ordinary PNG commands and services continue working. Bump/document settings
and saved-setting behavior where necessary; do not reset personal config as an
upgrade mechanism.

## Verification and completion criteria

Write focused failing tests before each runtime or persistence behavior.
Retain the RNG fingerprint, astronomy, buffer restoration and pointer tests.
Replace tests that assert the discarded cosmetic reactions.

Required evidence:

1. Deterministic tests for object state transitions, material contacts,
   reservations, branching/timeouts, event fan-out and click/drag floods.
2. Every principal incident reaches a repaired, cleaned-up state; interrupted
   and helped variants also terminate and release actors/resources. Autonomous
   activities do not duplicate actors or grow memory across repeated cycles.
3. Hidden/held scenes freeze and resume; base refresh and resizing preserve
   damage; view/output changes cannot route input to stale objects.
4. Invalid references/routes, nonfinite parameters and non-yielding or
   oversized sequences are diagnosed. A documented Rust example adds an
   incident without modifying core input/render scheduling; the same shared
   behavior works in two views without duplicated simulation code.
5. PTY checks for keyboard and fragmented mouse input, stable dragging,
   direct entry, responsive layouts, Save/Discard/Cancel, save failure,
   concurrent edits, comment preservation and persistence after process exit.
6. Visual inspections of intact, damaged, arrival, repair and recovery frames
   in all four developed views, including night/day, rain and contrasting
   aspect ratios. Verify motion/occlusion in native Wayland as available.
7. Moon baseline still/live rendering remains correct. Five-view bounded soak,
   full Rust suite, release build and clean diff checks pass.
8. Compare idle and active release measurements at the same sizes/moments:
   frame time, damaged area, base rebuild frequency, actor/event counts and
   bounded memory. Establish that idle director work sleeps and pointer floods
   cannot force full-base painting every frame. Report measured cost honestly.

Delivery includes updated AGENTS/README/config, Rust extension guide and example,
replay tools, performance/visual evidence and an installed build for the user
to try after verification. The new implementation replaces the old cosmetic
effects; it must not be called complete on the strength of one repair demo
or a framework with the promised content still missing.
