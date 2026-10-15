# The scene engine

How a view comes to have things in it that you can touch, and people who deal
with what you did. Read `README.md` first for what it looks like; this is for
adding to it.

## The idea

A view's picture is a still, painted once. The living scene never repaints it
wholesale. It holds **state** for a handful of named objects (is the hydrant
open, is the sign hanging by one fixing, is the lantern turning) and a small
cast of people, animals and vehicles who move along authored routes. When an
object's state changes, only the rectangle it covers is repainted from the
view's own drawing, which now reads that state. Everything moving is drawn over
the base by the live layer.

```text
gesture / trigger -> StoryWorld: events -> physics -> director (sequences) -> actors
                         |                                   |
                  object state (Snapshot)             commands on objects
                         |
        View::ground_state(snapshot) repaints only the changed rectangles
        View::story_alive / story_actor draw what moves, over the base
```

`StoryWorld` belongs to one output and knows nothing about pixels, Wayland or
config. It is stepped on a fixed simulation tick (1/30 s); a hidden, covered or
held output is simply not stepped, and a long gap is one frame's worth, not a
catch-up.

## What a view provides

A `Definition` (see `src/story/model.rs`), made from the view's own geometry:

| | |
| --- | --- |
| `objects` | `ObjectDef`: id, name, `Material`, where it rests, `bounds` (the most of the picture its drawing can cover in any form), what it `accepts`, an occlusion `layer`, a `break_pull`, whether it starts `hidden`. |
| `routes` | A `RouteGraph`: places (`Walk`, `Road`, `Roof`, `Ladder`, `Water`) with a perspective `scale` and the layer that hides someone standing there, joined by edges. People, cats, birds, vehicles and boats may only use the modes that suit them; an edge can be shut by an area reservation or by displaced objects. |
| `surfaces` | `SurfaceMap`: floors that things land on, water `Basin`s with a current, snags, snow spots, wind. |
| `stories` | `StoryDef`s: named stages of `Step`s, a `Trigger`, escalation labels, interference objects, a time limit. |
| `behaviors` | Shared continuous behaviors: `Perch` (birds), `Animal` (a cat), `Occupant` (a face at a window that you knock on). |

and in its `Stage`: `story()` returning the definition, `ground_state` (and
`indoors_state`) drawing from a `Snapshot`, `story_alive` for moving or carried
objects, `story_actor` to draw its own vehicles in its own style, `story_tone`
for the light people stand in, and, if an incident changes which windows are
lit, `lights_state`.

## Objects and gestures

Gestures arrive as `Press`, `Drag`, `Release` (with a measured velocity) and
`Cancel`. What an object does with them depends on what it `accepts`:

- `POKE`: a click is an `Impact`; stories wait for it as a trigger.
- `GRAB`: dragging pulls it. Past `break_pull` it is `Damaged` (one fixing lets
  go, it hangs by the other); past 1.8 times that it is `Broken` and falls as a
  body. Let go early and it swings back.
- `MOVE`: it is left where you put it (`Test::Displaced` becomes true: crews
  notice, and may tidy it or work round it).
- `THROW`: let go while moving and it is a body under gravity, wind and, over a
  `Basin`, buoyancy and a current.
- `KNOCK`: a window; someone may look out.

State is `ObjectState`: `pose` (displacement from where it was authored),
`damage` (`Intact`, `Damaged`, `Broken`, `Missing`), `attachment`
(`Fixed`, `Hanging`, `Dragged`, `Loose`, `Carried`), `owner`, `visible`, a
small `open` switch, `swinging`. A view's drawing reads these and nothing else.

## Stories

A `StoryDef` is data, run by the director: `Wait`, `Move`, `MoveNear`, `Pose`,
`Carry`/`Drop`, `Reserve`/`Release`, `Spawn`/`Despawn`, `SetState`, `Emit`,
`Jet`, `Branch` on a `Cond` (weather, object damage, displaced, helped,
interfered...), weighted `Choose`, `Await` (arrived, at rest, area free, an
event, calm, assisted) with a timeout, `Goto`, `End`.

Guarantees the director gives so a story author does not have to:

- **Bounded.** At most four incidents at once, a budget of instructions per
  tick, loops must wait (checked at definition time), every `Await` has a
  timeout, and every story has a hard `limit` after which it is cleaned up.
- **Interference.** Disturbing an object a story is working on counts. The
  story escalates up to three times (the labels you give), and then runs its
  `cleanup` stage, which must leave nothing broken, held, reserved or hidden.
- **Help.** Putting a part back where it belongs (`helped_by`) shortens the
  work.
- **Shared resources.** `Reserve` an area and no other crew works there;
  `Until::AreaFree` waits for it.
- **Ordinary life** is `Class::Ambient` stories on a timer with weather
  conditions, and the templates `visit`, `meeting` and `round`; incidents use
  `repair` for the arrive / work / depart / fallback shape.

The acceptance tests (`src/story/acceptance.rs`) hold every incident of every
view to the cleanup guarantee, repeatedly, by day and night, in rain and snow,
and under interference.

## Limits

| | |
| --- | --- |
| Incidents running | 4 |
| Actors | 24 (generation-checked slots) |
| Bodies | 128 |
| Objects per view | 64 |
| Events queued | 256 (32 reserved for the ones that finish a job) |
| Escalations | 3, then the protected cleanup |
| Instructions per tick | 256 |
| Queued gestures per screen | 16 |

A definition that exceeds them, or refers to a place, object or label that does
not exist, is refused when the view is loaded, with the story and step named.

## A complete example

A gate is knocked off its latch; a neighbour comes with a bucket, puts it
right and goes; a cat on the wall comes down to look. This is
`src/story/example.rs`, compiled and run by `cargo test` so that this page
cannot drift from the engine.

```rust
use crate::story::actors::{ActorPose, Kind, Mode, Role, RouteGraph};
use crate::story::behaviors::{repair, Animal, BehaviorSpec, CrewJob};
use crate::story::director::{Change, Class, Slot, Step, StoryDef, Trigger};
use crate::story::events::EventKind;
use crate::story::model::{Accepts, AreaId, Damage, Definition, Material, ObjectDef, ObjectId, RouteId};
use crate::story::physics::{Segment, SurfaceMap};

const GATE: ObjectId = ObjectId(1);
const BUCKET: ObjectId = ObjectId(2);

// places on the routes: a door, the gate, the lane beyond, and a wall for the cat
const DOOR: u16 = 1;
const AT_GATE: u16 = 2;
const LANE: u16 = 3;
const WALL: u16 = 4;
const STEP_: u16 = 5;

const NEIGHBOUR: Slot = Slot(0);

/// Everything one view says about its living scene.
pub fn define() -> Definition {
    // objects: where they rest, and the most of the picture they can cover (design units)
    let objects = vec![
        ObjectDef::new(1, "gate", Material::Wood, (700.0, 640.0), [680.0, 600.0, 724.0, 650.0]).accepting(Accepts::POKE.or(Accepts::GRAB)).breaks_at(30.0),
        ObjectDef::new(2, "bucket", Material::Metal, (610.0, 650.0), [600.0, 636.0, 620.0, 652.0]).accepting(Accepts::MOVE.or(Accepts::THROW)),
    ];

    // routes: people walk, cats use roofs and walls, and neither goes where it cannot
    let mut routes = RouteGraph::default();
    routes
        .add_node(DOOR, (520.0, 655.0), Mode::Walk, 0.5)
        .add_node(AT_GATE, (700.0, 655.0), Mode::Walk, 0.5)
        .add_node(LANE, (900.0, 655.0), Mode::Walk, 0.5)
        .add_node(WALL, (760.0, 612.0), Mode::Roof, 0.5)
        .add_node(STEP_, (640.0, 655.0), Mode::Walk, 0.5);
    routes.connect(DOOR, STEP_, Mode::Walk).connect(STEP_, AT_GATE, Mode::Walk).connect(AT_GATE, LANE, Mode::Walk).connect(AT_GATE, WALL, Mode::Roof);
    // what hides someone standing there: the view's own depth layers (None: nothing does)
    routes.behind(DOOR, 2);

    // the floor things land on
    let surfaces = SurfaceMap { segments: vec![Segment { a: (400.0, 656.0), b: (1100.0, 656.0), material: Material::Stone }], ..Default::default() };

    // a neighbour who mends the gate: arrive, work, depart; the template handles a way that is
    // shut, someone interrupting and the hand of the user
    let crew = CrewJob {
        who: NEIGHBOUR,
        role: Role::Resident,
        kind: Kind::Person,
        look: 2,
        depot: RouteId(DOOR),
        site: RouteId(AT_GATE),
        exit: RouteId(DOOR),
        area: AreaId(1),
        target: GATE,
        work_secs: 7.0,
        tool: Some(BUCKET),
        patience: 60.0,
    };
    let mut mend = StoryDef::new(1, "gate_off_its_latch", Class::Incident, Trigger::Damage(GATE, Damage::Damaged))
        .disturbed_by(&[GATE])
        .escalating(&["come_again", "come_again", "come_again"])
        .within(120.0)
        .stage("start", vec![Step::Emit { kind: EventKind::Damage, obj: GATE }, Step::Wait(2.0), Step::Goto("arrive")])
        .stage("come_again", vec![Step::Wait(1.0), Step::Goto("arrive")])
        .stage("cleanup", vec![Step::Release { who: NEIGHBOUR }, Step::SetState { obj: GATE, change: Change::Restore }, Step::Despawn { who: NEIGHBOUR }, Step::End])
        .stage("closed", vec![Step::Pose { who: NEIGHBOUR, pose: ActorPose::Wave }, Step::End]);
    mend.stages.extend(repair(["arrive", "work", "depart", "fallback"], "closed", &crew));

    Definition {
        view: "example",
        objects,
        routes,
        surfaces,
        stories: vec![mend],
        // a cat that lives on the wall and comes down to look at whatever goes on at the gate
        behaviors: vec![BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(WALL), haunts: vec![RouteId(AT_GATE)], attention: 400.0, look: 1, linger: 5.0 })],
        custom: Vec::new(),
    }
}
```

To make it a real view's scene, bind the numbers to the view's geometry the way
`views/episodes/city.rs` does (`CityBindings` is found from the stage, so it
holds at every screen size), return the definition from `Stage::story`, and
draw the objects from state:

```rust
fn ground_state(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
    // ...everything the view paints, as before, except:
    let gate = state.get(GATE);
    if gate.visible && state.in_base(GATE) {
        draw_gate(d, gate.damage, gate.pose);   // at rest: part of the base
    }
}

fn story_alive(&self, d: &mut Draw<Frame>, sc: &Scene, state: &Snapshot, t: f32, indoors: bool) {
    let gate = state.get(GATE);
    if gate.visible && !state.in_base(GATE) {
        draw_gate(d, gate.damage, gate.pose);   // being pulled or swinging: drawn live
    }
}
```

`in_base` is true when the object is visible, not missing and at rest; only
then does the still show it. Anything else is drawn by `story_alive`, where
`Frame.behind` should be set to the layer that hides it.

## Trying it

```sh
dormer scenes list --view street      # what can happen, and what starts it
dormer replay file.json --out dir/    # inputs at times -> frames and a trace
cargo test story::                    # the engine and every view's scenes
dormer soak --n 1000                  # random moments, stories and hands
```

The settings panel's *Scenes* page can start, pause, single-step and reset any
story, and `[activity] stories = false` turns the whole thing off.
