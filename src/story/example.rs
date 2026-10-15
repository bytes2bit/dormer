// The complete example of docs/engine.md: a gate that is knocked off its latch, a neighbour who
// comes to put it right, and a cat that watches. It is compiled and run here, so that the guide
// cannot drift from the engine. Copy it into `src/views/episodes/<view>.rs` and bind the numbers
// to the view's real geometry.

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::{Gesture, StoryWorld, TriggerResult};

    #[test]
    fn the_guide_example_is_a_working_extension() {
        let mut w = StoryWorld::new(define(), 1).expect("the definition validates");
        let calm = crate::story::Facts { hour: 12.0, wind: 3.0, weekday: true, ..Default::default() };
        let run = |w: &mut StoryWorld, secs: f64| {
            let mut t = 0.0;
            while t < secs {
                w.step_facts(0.05, &calm);
                t += 0.05;
            }
        };
        run(&mut w, 1.0);
        // knocked off its latch by a hand: press, pull past the fixing, let go
        w.gesture(Gesture::Press((700.0, 640.0)));
        w.gesture(Gesture::Drag((740.0, 640.0)));
        w.gesture(Gesture::Release { at: (740.0, 640.0), velocity: (0.0, 0.0) });
        assert_eq!(w.snapshot().damage(GATE), Damage::Damaged);
        run(&mut w, 90.0);
        assert_eq!(w.snapshot().damage(GATE), Damage::Intact, "mended");
        assert_eq!(w.director().incidents_running(w.definition()), 0);
        assert_eq!(w.trigger(crate::story::StoryId(1)), TriggerResult::Started, "and it can happen again");
    }
}
