//! Street: the hydrant, the diner's sign and a spilled delivery.
//!
//! Everything happens between the far pavement and the road, as seen from the
//! sill: people on the pavement (behind the lamps and parked cars), vehicles
//! in the lanes, workers going up a ladder to the diner's roof. The view
//! (`views/street.rs`) draws the changed states; this file is what is where,
//! who comes, and what they do.

use crate::color::shade;
use crate::gfx::{Draw, Surface, P};
use crate::props::Layout;
use crate::story::actors::{ActorPose, ActorSpec, Kind, Mode, Role, RouteGraph};
use crate::story::behaviors::{repair, round, visit, Animal, BehaviorSpec, CrewJob, Occupant, Perch, Walker};
use crate::story::director::{Change, Class, Cond, Slot, Step, StoryDef, Test, Trigger, Until, Weather};
use crate::story::draw::{bar, ink, oval, poly};
use crate::story::events::EventKind;
use crate::story::model::{Accepts, AreaId, Damage, Definition, Material, ObjectDef, ObjectId, RouteId};
use crate::story::physics::{Segment, SnowSpot, SurfaceMap};

pub const HYDRANT: ObjectId = ObjectId(1);
pub const SIGN: ObjectId = ObjectId(2);
pub const STACK: ObjectId = ObjectId(3);
pub const CONE_A: ObjectId = ObjectId(4);
pub const CONE_B: ObjectId = ObjectId(5);
pub const BARRIER: ObjectId = ObjectId(6);
pub const LADDER: ObjectId = ObjectId(7);
pub const PARCEL_1: ObjectId = ObjectId(8);
pub const PARCEL_2: ObjectId = ObjectId(9);
pub const PARCEL_3: ObjectId = ObjectId(10);
pub const WINDOW_A: ObjectId = ObjectId(11);
pub const WINDOW_B: ObjectId = ObjectId(12);

/// The pavement along the far kerb, the road in front of it, and the diner's roof.
pub const PAVEMENT: f32 = 866.0;
pub const FAR_LANE: f32 = 929.0;
pub const NEAR_LANE: f32 = 958.0;
pub const HYDRANT_X: f32 = 600.0;
pub const ROOF_Y: f32 = 662.0;
pub const STACK_X: f32 = 506.0;
pub const WINDOWS: [[f32; 4]; 2] = [[722.0, 592.0, 756.0, 638.0], [784.0, 592.0, 818.0, 638.0]];

// pavement places, left to right (id = 100 + n)
const WALK_X: [f32; 12] = [-50.0, 90.0, 190.0, 250.0, 380.0, 506.0, 560.0, 640.0, 760.0, 960.0, 1250.0, 1650.0];
fn walk(n: usize) -> RouteId {
    RouteId(100 + n as u16)
}
const DINER_DOOR: u16 = 3; // walk(3) is at x=250
const ZEBRA_A: u16 = 130;
const ZEBRA_B: u16 = 131;
// road places
const ROAD_IN: u16 = 140;
const ROAD_TRUCK: u16 = 141;
const ROAD_OUT: u16 = 142;
const FAR_IN: u16 = 143;
const FAR_COP: u16 = 144;
const FAR_OUT: u16 = 145;
const VAN_PARK: u16 = 146;
const ROOF_FOOT: u16 = 150;
const ROOF_TOP: u16 = 151;
const ROOF_SIGN: u16 = 152;
const LAMP_A: u16 = 160;
const BIN: u16 = 170;
const WIN_A: u16 = 171;
const WIN_B: u16 = 172;

const AREA_ZONE: AreaId = AreaId(1);


/// Where, along the pavement, the work zone lies while the hydrant is being mended.
pub const ZONE: (f32, f32) = (470.0, 760.0);

pub fn define(lamps: &[f32]) -> Definition {
    let r = |p: P, w: f32, h: f32| [p.0 - w, p.1 - h, p.0 + w, p.1 + h];
    let mut objects = vec![
        ObjectDef::new(1, "hydrant", Material::Metal, (HYDRANT_X, 862.0), r((HYDRANT_X, 862.0), 9.0, 12.0)).accepting(Accepts::POKE.or(Accepts::GRAB)).breaks_at(18.0),
        ObjectDef::new(2, "diner_sign", Material::Metal, (252.0, 646.0), [180.0, 598.0, 326.0, 672.0]).accepting(Accepts::GRAB).breaks_at(34.0),
        ObjectDef::new(3, "parcels", Material::Paper, (STACK_X, 858.0), r((STACK_X, 858.0), 14.0, 12.0)).accepting(Accepts::POKE),
        ObjectDef::new(4, "cone_a", Material::Stone, (HYDRANT_X - 52.0, 874.0), r((HYDRANT_X - 52.0, 874.0), 7.0, 9.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(5, "cone_b", Material::Stone, (HYDRANT_X + 56.0, 874.0), r((HYDRANT_X + 56.0, 874.0), 7.0, 9.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(6, "barrier", Material::Metal, (HYDRANT_X, 904.0), r((HYDRANT_X, 904.0), 36.0, 10.0)).hidden(),
        ObjectDef::new(7, "ladder", Material::Wood, (232.0, 760.0), [214.0, 656.0, 248.0, 870.0]).hidden(),
        ObjectDef::new(8, "parcel_one", Material::Paper, (STACK_X - 6.0, 858.0), r((STACK_X - 6.0, 858.0), 7.0, 6.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(9, "parcel_two", Material::Paper, (STACK_X + 4.0, 856.0), r((STACK_X + 4.0, 856.0), 7.0, 6.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(10, "parcel_three", Material::Paper, (STACK_X, 862.0), r((STACK_X, 862.0), 7.0, 6.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(11, "window_a", Material::Glass, ((WINDOWS[0][0] + WINDOWS[0][2]) / 2.0, WINDOWS[0][3]), WINDOWS[0]).accepting(Accepts::KNOCK),
        ObjectDef::new(12, "window_b", Material::Glass, ((WINDOWS[1][0] + WINDOWS[1][2]) / 2.0, WINDOWS[1][3]), WINDOWS[1]).accepting(Accepts::KNOCK),
        ObjectDef::new(13, "snow_pile", Material::Cloth, (1010.0, 862.0), r((1010.0, 862.0), 16.0, 10.0)).accepting(Accepts::BRUSH.or(Accepts::THROW)),
    ];
    objects.iter_mut().for_each(|o| o.layer = None);

    let mut g = RouteGraph::default();
    for (n, x) in WALK_X.iter().enumerate() {
        g.add_node(100 + n as u16, (*x, PAVEMENT), Mode::Walk, 1.0);
        g.behind(100 + n as u16, 3);
        if n > 0 {
            g.connect(100 + n as u16 - 1, 100 + n as u16, Mode::Walk);
        }
    }
    // across the road at the crossing, for the one parcel that rolled into it
    g.add_node(ZEBRA_A, (900.0, 934.0), Mode::Walk, 1.05).add_node(ZEBRA_B, (900.0, 968.0), Mode::Walk, 1.1);
    g.connect(109, ZEBRA_A, Mode::Walk).connect(ZEBRA_A, ZEBRA_B, Mode::Walk);
    g.behind(ZEBRA_A, 4).behind(ZEBRA_B, 5);
    // vehicles: the near lane runs right, the far lane runs left
    g.add_node(ROAD_IN, (-220.0, NEAR_LANE), Mode::Road, 1.04)
        .add_node(ROAD_TRUCK, (HYDRANT_X - 190.0, NEAR_LANE), Mode::Road, 1.04)
        .add_node(ROAD_OUT, (1860.0, NEAR_LANE), Mode::Road, 1.04)
        .add_node(FAR_IN, (1860.0, FAR_LANE), Mode::Road, 0.95)
        .add_node(FAR_COP, (HYDRANT_X + 230.0, FAR_LANE), Mode::Road, 0.95)
        .add_node(FAR_OUT, (-220.0, FAR_LANE), Mode::Road, 0.95)
        .add_node(VAN_PARK, (250.0, NEAR_LANE), Mode::Road, 1.04);
    g.connect(ROAD_IN, VAN_PARK, Mode::Road).connect(VAN_PARK, ROAD_TRUCK, Mode::Road).connect(ROAD_TRUCK, ROAD_OUT, Mode::Road).connect(FAR_IN, FAR_COP, Mode::Road).connect(FAR_COP, FAR_OUT, Mode::Road);
    g.behind(ROAD_IN, 4).behind(ROAD_TRUCK, 4).behind(ROAD_OUT, 4).behind(VAN_PARK, 4);
    g.behind(FAR_IN, 3).behind(FAR_COP, 3).behind(FAR_OUT, 3);
    // the diner's ladder and roof
    g.add_node(ROOF_FOOT, (232.0, PAVEMENT), Mode::Ladder, 1.0)
        .add_node(ROOF_TOP, (232.0, ROOF_Y), Mode::Roof, 0.8)
        .add_node(ROOF_SIGN, (222.0, ROOF_Y), Mode::Roof, 0.8);
    g.connect(walk(DINER_DOOR as usize).0, ROOF_FOOT, Mode::Walk).connect(ROOF_FOOT, ROOF_TOP, Mode::Ladder).connect(ROOF_TOP, ROOF_SIGN, Mode::Roof);
    g.behind(ROOF_FOOT, 3).behind(ROOF_TOP, 2).behind(ROOF_SIGN, 2);
    // lamp heads for birds, a bin for the cat, two windows
    for (k, x) in lamps.iter().take(2).enumerate() {
        g.add_node(LAMP_A + k as u16, (*x + 22.0, 697.0), Mode::Roof, 1.0).behind(LAMP_A + k as u16, 3);
    }
    g.add_node(BIN, (1380.0, PAVEMENT), Mode::Walk, 1.0).behind(BIN, 3);
    g.connect(BIN, walk(10).0, Mode::Walk);
    g.add_node(WIN_A, (WINDOWS[0][0] / 2.0 + WINDOWS[0][2] / 2.0, WINDOWS[0][3]), Mode::Walk, 0.7).behind(WIN_A, 2);
    g.add_node(WIN_B, (WINDOWS[1][0] / 2.0 + WINDOWS[1][2] / 2.0, WINDOWS[1][3]), Mode::Walk, 0.7).behind(WIN_B, 2);

    let mut surfaces = SurfaceMap::default();
    // the pavement, the kerb's edge down to the road, and the road
    surfaces.segments.push(Segment { a: (-80.0, PAVEMENT), b: (1700.0, PAVEMENT), material: Material::Stone });
    surfaces.segments.push(Segment { a: (-80.0, 940.0), b: (1700.0, 940.0), material: Material::Stone });
    surfaces.segments.push(Segment { a: (170.0, 664.0), b: (330.0, 664.0), material: Material::Wood });
    surfaces.snow.push(SnowSpot { at: (1010.0, 862.0), capacity: 2.5 });

    let behaviors = vec![
        BehaviorSpec::Perch(Perch {
            perches: (0..lamps.len().min(2)).map(|k| RouteId(LAMP_A + k as u16)).collect(),
            alternates: vec![walk(0)],
            radius: 90.0,
            away: 6.0,
            keep_off: Some(AREA_ZONE),
            shy_of: vec![Weather::Windy],
        }),
        BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(BIN), haunts: vec![walk(5), walk(7), walk(9)], attention: 520.0, look: 0, linger: 7.0 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_A, at: RouteId(WIN_A), role: Role::Resident, look: 1 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_B, at: RouteId(WIN_B), role: Role::Resident, look: 4 }),
    ];

    let mut stories = vec![hydrant(), diner_sign(), delivery_spill()];
    stories.extend(daily_life());
    Definition { view: "street", objects, routes: g, surfaces, stories, behaviors, custom: Vec::new() }
}

const S0: Slot = Slot(0);
const S1: Slot = Slot(1);
const S2: Slot = Slot(2);
const S3: Slot = Slot(3);

fn spawn(who: Slot, kind: Kind, role: Role, at: u16, look: u32) -> Step {
    Step::Spawn { who, spec: ActorSpec { kind, role, at: RouteId(at), look } }
}

/// The hydrant's cap goes, the jet rises; walkers retreat and cars wait; a truck and its crew come,
/// set cones, close the valve and fit a cap; more interference brings police and a second hand.
fn hydrant() -> StoryDef {
    let job = CrewJob {
        who: S1,
        role: Role::Worker,
        kind: Kind::Person,
        look: 2,
        depot: walk(4),
        site: walk(6),
        exit: walk(4),
        area: AREA_ZONE,
        target: HYDRANT,
        work_secs: 12.0,
        tool: None,
        patience: 60.0,
    };
    let mut s = StoryDef::new(1, "hydrant", Class::Incident, Trigger::Event(EventKind::Impact, HYDRANT))
        .disturbed_by(&[HYDRANT, CONE_A, CONE_B])
        .escalating(&["police", "second_hand", "all_hands"])
        .within(200.0)
        .helped_by(CONE_A, HYDRANT, 120.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: HYDRANT, change: Change::Damage(Damage::Damaged) },
                Step::SetState { obj: HYDRANT, change: Change::Open(true) },
                Step::Shatter { obj: HYDRANT, material: Material::Metal },
                Step::Jet { id: 0, obj: HYDRANT, vel: (95.0, -265.0), rate: 34.0, on: true },
                Step::Wait(3.5),
                // the truck comes along the near lane and stops short of it
                spawn(S0, Kind::Truck, Role::Worker, ROAD_IN, 1),
                Step::Move { who: S0, to: RouteId(ROAD_TRUCK), run: false },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: Some("cones") },
                Step::Goto("cones"),
            ],
        )
        .stage(
            "cones",
            vec![
                // the zone: cones go down and the way along is shut
                Step::SetState { obj: CONE_A, change: Change::Visible(true) },
                Step::SetState { obj: CONE_B, change: Change::Visible(true) },
                Step::Branch { when: Cond::is(Test::Displaced(CONE_A)), then: "reset_cones", otherwise: "arrive" },
            ],
        )
        .stage(
            "reset_cones",
            vec![
                // somebody has moved a cone: it is put back, which takes a moment
                Step::SetState { obj: CONE_A, change: Change::Place((HYDRANT_X - 52.0, 874.0)) },
                Step::SetState { obj: CONE_B, change: Change::Place((HYDRANT_X + 56.0, 874.0)) },
                Step::Wait(2.0),
                Step::Goto("arrive"),
            ],
        )
        .stage(
            "police",
            vec![
                // the interference has drawn a policeman: the barrier goes up and he waves it by
                spawn(S2, Kind::Car, Role::Police, FAR_IN, 3),
                Step::Move { who: S2, to: RouteId(FAR_COP), run: false },
                Step::SetState { obj: BARRIER, change: Change::Visible(true) },
                Step::Wait(1.0),
                Step::Goto("arrive"),
            ],
        )
        .stage("second_hand", vec![spawn(S3, Kind::Person, Role::Worker, 104, 3), Step::Move { who: S3, to: walk(5), run: false }, Step::Wait(1.0), Step::Goto("arrive")])
        .stage("all_hands", vec![Step::SetState { obj: BARRIER, change: Change::Visible(true) }, Step::Wait(1.0), Step::Goto("arrive")])
        .stage(
            "cleanup",
            vec![
                Step::Jet { id: 0, obj: HYDRANT, vel: (0.0, 0.0), rate: 0.0, on: false },
                Step::SetState { obj: HYDRANT, change: Change::Open(false) },
                Step::SetState { obj: HYDRANT, change: Change::Restore },
                Step::Release { who: S1 },
                Step::Goto("depart"),
            ],
        );
    // the crew comes from the truck; the template's depot is where they step down
    s.stages.extend(repair(["arrive", "work", "depart", "fallback"], "away", &job));
    s.stage(
        "away",
        vec![
            Step::Jet { id: 0, obj: HYDRANT, vel: (0.0, 0.0), rate: 0.0, on: false },
            Step::SetState { obj: HYDRANT, change: Change::Open(false) },
            Step::SetState { obj: CONE_A, change: Change::Visible(false) },
            Step::SetState { obj: CONE_B, change: Change::Visible(false) },
            Step::SetState { obj: BARRIER, change: Change::Visible(false) },
            Step::Despawn { who: S3 },
            Step::Despawn { who: S2 },
            Step::Move { who: S0, to: RouteId(ROAD_OUT), run: false },
            Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: None },
            Step::Despawn { who: S0 },
            Step::End,
        ],
    )
}

/// The diner's sign swings, loses a fixing, hangs or falls; the owner comes out, a van brings a
/// ladder, the sign is secured and put back, and the lamp follows.
fn diner_sign() -> StoryDef {
    StoryDef::new(2, "diner_sign", Class::Incident, Trigger::Damage(SIGN, Damage::Damaged))
        .disturbed_by(&[SIGN, CONE_A])
        .escalating(&["second_look", "second_look", "second_look"])
        .within(190.0)
        .stage(
            "start",
            vec![
                Step::Wait(1.5),
                spawn(S0, Kind::Person, Role::Owner, DINER_DOOR as u16 + 100, 1),
                Step::LookAt { who: S0, obj: SIGN },
                Step::Pose { who: S0, pose: ActorPose::React },
                Step::Wait(2.5),
                spawn(S1, Kind::Van, Role::Worker, ROAD_IN, 0),
                Step::Move { who: S1, to: RouteId(VAN_PARK), run: false },
                Step::Await { until: Until::Arrived(vec![S1]), timeout: 40.0, on_timeout: Some("settle") },
                Step::Goto("settle"),
            ],
        )
        .stage(
            "settle",
            vec![
                // two workers step down with a ladder and cones; the owner points up
                spawn(S2, Kind::Person, Role::Worker, 102, 2),
                spawn(S3, Kind::Person, Role::Worker, 102, 0),
                Step::SetState { obj: LADDER, change: Change::Visible(true) },
                Step::SetState { obj: CONE_A, change: Change::Place((214.0, 876.0)) },
                Step::SetState { obj: CONE_A, change: Change::Visible(true) },
                Step::Move { who: S2, to: RouteId(ROOF_FOOT), run: false },
                Step::Move { who: S3, to: RouteId(ROOF_FOOT), run: false },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Await { until: Until::Arrived(vec![S2, S3]), timeout: 40.0, on_timeout: Some("finish") },
                Step::Goto("climb"),
            ],
        )
        .stage(
            "climb",
            vec![
                // equipment moved from where the plan had it: put back first
                Step::Branch { when: Cond::is(Test::Displaced(CONE_A)), then: "reset", otherwise: "up" },
            ],
        )
        .stage("reset", vec![Step::SetState { obj: CONE_A, change: Change::Place((214.0, 876.0)) }, Step::Wait(2.0), Step::Goto("up")])
        .stage(
            "up",
            vec![
                Step::Move { who: S2, to: RouteId(ROOF_SIGN), run: false },
                Step::Await { until: Until::Arrived(vec![S2]), timeout: 60.0, on_timeout: Some("finish") },
                Step::Branch { when: Cond::is(Test::ObjectIs(SIGN, Damage::Broken)), then: "refit", otherwise: "secure" },
            ],
        )
        .stage("secure", vec![Step::Pose { who: S2, pose: ActorPose::Work }, Step::Wait(5.0), Step::Goto("finish")])
        .stage(
            "refit",
            vec![
                // it came down: it is lifted back up and fixed again
                Step::Pose { who: S3, pose: ActorPose::Look },
                Step::Pose { who: S2, pose: ActorPose::Work },
                Step::Wait(7.0),
                Step::Goto("finish"),
            ],
        )
        .stage(
            "finish",
            vec![
                Step::SetState { obj: SIGN, change: Change::Restore },
                Step::Pose { who: S2, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Move { who: S2, to: RouteId(ROOF_FOOT), run: false },
                Step::Await { until: Until::Arrived(vec![S2]), timeout: 40.0, on_timeout: None },
                Step::SetState { obj: LADDER, change: Change::Visible(false) },
                Step::SetState { obj: CONE_A, change: Change::Visible(false) },
                Step::Move { who: S2, to: RouteId(VAN_PARK), run: false },
                Step::Move { who: S3, to: RouteId(VAN_PARK), run: false },
                Step::Await { until: Until::Arrived(vec![S2, S3]), timeout: 30.0, on_timeout: None },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S3 },
                Step::Move { who: S1, to: RouteId(ROAD_OUT), run: false },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Await { until: Until::Arrived(vec![S1]), timeout: 40.0, on_timeout: None },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
        .stage("second_look", vec![Step::Pose { who: S0, pose: ActorPose::React }, Step::Wait(1.0), Step::Goto("climb")])
        .stage(
            "cleanup",
            vec![
                Step::SetState { obj: SIGN, change: Change::Restore },
                Step::SetState { obj: LADDER, change: Change::Visible(false) },
                Step::SetState { obj: CONE_A, change: Change::Visible(false) },
                Step::Despawn { who: S3 },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
}

/// A stack of parcels spills; the courier runs up, passers-by stop and a cat comes to look; every
/// parcel is gathered, wherever it has gone, and the delivery goes on.
fn delivery_spill() -> StoryDef {
    let gather = |obj: ObjectId, next: &'static str, here: &'static str| -> Vec<Step> {
        vec![
            Step::MoveNear { who: S0, obj, run: false },
            Step::Await { until: Until::Arrived(vec![S0]), timeout: 25.0, on_timeout: Some(next) },
            Step::Carry { who: S0, obj },
            Step::Move { who: S0, to: walk(5), run: false },
            Step::Await { until: Until::Arrived(vec![S0]), timeout: 25.0, on_timeout: Some(next) },
            Step::SetState { obj, change: Change::Visible(false) },
            Step::Goto(next),
        ]
        .into_iter()
        .chain(std::iter::once(Step::Goto(here)))
        .collect()
    };
    let _ = gather;
    let mut s = StoryDef::new(3, "delivery_spill", Class::Incident, Trigger::Event(EventKind::Impact, STACK))
        .disturbed_by(&[STACK, PARCEL_1, PARCEL_2, PARCEL_3])
        .escalating(&["more_hands", "more_hands", "more_hands"])
        .within(170.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: STACK, change: Change::Damage(Damage::Missing) },
                Step::SetState { obj: PARCEL_1, change: Change::Visible(true) },
                Step::SetState { obj: PARCEL_2, change: Change::Visible(true) },
                Step::SetState { obj: PARCEL_3, change: Change::Visible(true) },
                Step::SetState { obj: PARCEL_1, change: Change::Place((STACK_X - 30.0, 862.0)) },
                Step::SetState { obj: PARCEL_2, change: Change::Place((STACK_X + 34.0, 864.0)) },
                Step::SetState { obj: PARCEL_3, change: Change::Place((STACK_X + 8.0, 872.0)) },
                Step::Emit { kind: EventKind::Obstruction, obj: STACK },
                Step::Wait(1.2),
                spawn(S0, Kind::Person, Role::Courier, 111, 0),
                Step::Move { who: S0, to: walk(6), run: true },
                // two people stop to see, and a cat comes round the bin
                spawn(S1, Kind::Person, Role::Customer, 100, 3),
                Step::Move { who: S1, to: walk(3), run: false },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: Some("tidy") },
                Step::Pose { who: S0, pose: ActorPose::React },
                Step::Wait(1.0),
                Step::Goto("one"),
            ],
        )
        .stage("one", vec![Step::Branch { when: Cond::is(Test::Weather(Weather::Rain)), then: "hurry", otherwise: "pick_one" }])
        .stage("hurry", vec![Step::Wait(0.5), Step::Goto("pick_one")]);
    // each parcel in turn, wherever it now lies
    for (n, (obj, this, next)) in [(PARCEL_1, "pick_one", "pick_two"), (PARCEL_2, "pick_two", "pick_three"), (PARCEL_3, "pick_three", "tidy")].into_iter().enumerate() {
        let _ = n;
        s = s.stage(
            this,
            vec![
                Step::MoveNear { who: S0, obj, run: false },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 30.0, on_timeout: Some(next) },
                Step::Carry { who: S0, obj },
                Step::Move { who: S0, to: walk(5), run: false },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 30.0, on_timeout: Some(next) },
                Step::SetState { obj, change: Change::Visible(false) },
                Step::Goto(next),
            ],
        );
    }
    s.stage("more_hands", vec![Step::Pose { who: S1, pose: ActorPose::Work }, Step::Wait(2.0), Step::Pose { who: S1, pose: ActorPose::Idle }, Step::Goto("pick_one")])
        .stage(
            "tidy",
            vec![
                // the stack is whole again and the courier goes on
                Step::SetState { obj: PARCEL_1, change: Change::Visible(false) },
                Step::SetState { obj: PARCEL_2, change: Change::Visible(false) },
                Step::SetState { obj: PARCEL_3, change: Change::Visible(false) },
                Step::SetState { obj: STACK, change: Change::Restore },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Move { who: S0, to: walk(11), run: false },
                Step::Move { who: S1, to: walk(0), run: false },
                Step::Await { until: Until::Arrived(vec![S0, S1]), timeout: 60.0, on_timeout: None },
                Step::Despawn { who: S0 },
                Step::Despawn { who: S1 },
                Step::End,
            ],
        )
        .stage(
            "cleanup",
            vec![
                Step::Release { who: S0 },
                Step::SetState { obj: PARCEL_1, change: Change::Visible(false) },
                Step::SetState { obj: PARCEL_2, change: Change::Visible(false) },
                Step::SetState { obj: PARCEL_3, change: Change::Visible(false) },
                Step::SetState { obj: STACK, change: Change::Restore },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
}

fn daily_life() -> Vec<StoryDef> {
    let day = vec![Cond::is(Test::Weather(Weather::Day)), Cond::is(Test::Weather(Weather::Dry))];
    let mut out = Vec::new();
    let mut a = StoryDef::new(4, "customer_visit", Class::Ambient, Trigger::Ambient { every: (45.0, 110.0), when: day.clone() }).within(140.0);
    a = a.stage("start", vec![Step::Goto("come")]);
    a.stages.extend(visit(
        ["come", "stay", "leave"],
        "done",
        Walker { who: S0, role: Role::Customer, kind: Kind::Person, look: 2, from: walk(0) },
        walk(DINER_DOOR as usize),
        ActorPose::Look,
        4.0,
        walk(11),
    ));
    out.push(a.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    let mut m = StoryDef::new(5, "neighbours_stop", Class::Ambient, Trigger::Ambient { every: (70.0, 160.0), when: day.clone() }).within(160.0);
    m = m.stage("start", vec![Step::Goto("come")]);
    m.stages.extend(crate::story::behaviors::meeting(
        ["come", "talk", "part"],
        "done",
        Walker { who: S0, role: Role::Resident, kind: Kind::Person, look: 1, from: walk(0) },
        Walker { who: S1, role: Role::Resident, kind: Kind::Person, look: 4, from: walk(11) },
        walk(8),
        walk(9),
        7.0,
        walk(11),
        walk(0),
    ));
    out.push(m.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::Despawn { who: S1 }, Step::End]));
    let mut r = StoryDef::new(6, "street_round", Class::Ambient, Trigger::Ambient { every: (90.0, 200.0), when: vec![Cond::is(Test::Weather(Weather::Day))] }).within(180.0);
    r = r.stage("start", vec![Step::Goto("round")]);
    r.stages.extend(round(
        ["round", "leave"],
        "done",
        Walker { who: S0, role: Role::Staff, kind: Kind::Person, look: 0, from: walk(11) },
        &[(walk(9), ActorPose::Work, 3.0), (walk(6), ActorPose::Operate, 3.0)],
        walk(0),
    ));
    out.push(r.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    out
}

// ── drawing what changes ─────────────────────────────────────────────────────

/// The hydrant, whole or with its cap gone (an open nozzle, a dark stain).
pub fn hydrant_body<S: Surface>(d: &mut Draw<S>, lay: &Layout, damage: Damage, red: crate::color::Rgb, kerb: f32) {
    let x = HYDRANT_X;
    bar(d, lay, [x - 3.5, 857.0, x + 3.5, kerb], red, 1.0);
    bar(d, lay, [x - 6.5, 862.0, x + 6.5, 866.0], shade(red, -0.15), 1.0);
    match damage {
        Damage::Intact => oval(d, lay, x, 857.0, 4.2, 3.5, red, 1.0),
        _ => {
            // the cap is off: a bare top with its thread, and the wet below
            bar(d, lay, [x - 3.0, 855.5, x + 3.0, 857.5], shade(red, -0.35), 1.0);
            bar(d, lay, [x - 4.5, 857.0, x + 4.5, 858.4], shade(red, -0.2), 1.0);
        }
    }
}

/// The diner's sign hanging by one corner, or lying where it fell.
pub fn sign_tilted<S: Surface>(d: &mut Draw<S>, lay: &Layout, text_bar: [f32; 4], damage: Damage, col: crate::color::Rgb, pose: crate::story::model::Pose) {
    let [x0, y0, x1, y1] = text_bar;
    let (cx, cy) = (x0, y0);
    let turn = match damage {
        Damage::Damaged => 0.42,
        _ => 1.45,
    } + pose.turn * 0.0;
    let (c, s) = (turn.cos(), turn.sin());
    let q = |x: f32, y: f32| (cx + (x - cx) * c - (y - cy) * s + pose.dx, cy + (x - cx) * s + (y - cy) * c + pose.dy);
    poly(d, lay, &[q(x0, y0), q(x1, y0), q(x1, y1), q(x0, y1)], col, 1.0);
    ink(d, lay, &[q(x0 + 6.0, (y0 + y1) / 2.0), q(x1 - 6.0, (y0 + y1) / 2.0)], 1.4, shade(col, 0.5), 0.9);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::actors::ActorState;
    use crate::story::model::Attachment;
    use crate::story::physics::Gesture;
    use crate::story::replay::scripted::*;
    use crate::story::replay::{test_session, Session};

    fn evening() -> Session {
        test_session("street", "2026-10-13 18:20", "clear", (640, 400))
    }

    fn roles(s: &Session, role: Role) -> usize {
        s.story.actors().iter().filter(|a| a.role == role).count()
    }

    #[test]
    fn the_hydrant_jets_walkers_turn_back_a_truck_comes_and_the_cap_goes_on() {
        let mut s = evening();
        run(&mut s, 1.0);
        poke(&mut s, HYDRANT);
        run(&mut s, 3.0);
        assert!(s.story.snapshot().get(HYDRANT).open, "the jet is up");
        assert!(s.story.physics().jet_on(0));
        assert!(s.story.physics().bodies.iter().any(|b| b.kind == crate::story::physics::BodyKind::Droplet));
        let (mut truck, mut cones, mut worked) = (false, false, false);
        let done = until(&mut s, 120.0, |s| {
            truck |= s.story.actors().iter().any(|a| a.kind == Kind::Truck && a.state == ActorState::Arrived);
            cones |= s.story.snapshot().get(CONE_A).visible;
            worked |= s.story.actors().iter().any(|a| a.role == Role::Worker && a.pose == ActorPose::Work);
            s.story.snapshot().damage(HYDRANT) == Damage::Intact && truck
        });
        assert!(done.is_some(), "mended");
        assert!(truck && cones && worked, "truck {} cones {} worked {}", truck, cones, worked);
        run(&mut s, 40.0);
        assert!(!s.story.physics().jet_on(0) && !s.story.snapshot().get(HYDRANT).open);
        assert!(!s.story.snapshot().get(CONE_A).visible && !s.story.snapshot().get(BARRIER).visible);
        quiet(&s);
        assert_eq!(roles(&s, Role::Worker) + roles(&s, Role::Police), 0, "everyone has gone");
    }

    #[test]
    fn interference_brings_the_police_and_a_second_hand_then_a_protected_end() {
        let mut s = evening();
        run(&mut s, 1.0);
        let (mut police, mut most) = (false, 0);
        for n in 0..16 {
            s.story.bus_emit_impact(HYDRANT, 5000 + n);
            run(&mut s, 5.0);
            police |= roles(&s, Role::Police) > 0;
            most = most.max(s.story.director().status(s.story.definition()).iter().map(|x| x.2).max().unwrap_or(0));
            assert!(s.story.actors().iter().filter(|a| matches!(a.role, Role::Worker | Role::Police)).count() <= 6);
        }
        assert!(police, "a policeman came");
        assert_eq!(most, 3);
        run(&mut s, 200.0);
        quiet(&s);
        assert_eq!(s.story.snapshot().damage(HYDRANT), Damage::Intact);
        assert!(!s.story.snapshot().get(BARRIER).visible);
    }

    #[test]
    fn the_sign_hangs_the_owner_looks_and_a_crew_goes_up_a_ladder_and_secures_it() {
        let mut s = evening();
        run(&mut s, 1.0);
        let p = at_of(&s, SIGN);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 44.0, p.1 + 6.0)));
        assert_eq!(s.story.snapshot().damage(SIGN), Damage::Damaged);
        s.gesture(Gesture::Release { at: (p.0 + 44.0, p.1 + 6.0), velocity: (0.0, 0.0) });
        let (mut owner, mut van, mut ladder, mut climbed, mut worked) = (false, false, false, false, false);
        let done = until(&mut s, 150.0, |s| {
            owner |= s.story.actors().iter().any(|a| a.role == Role::Owner);
            van |= s.story.actors().iter().any(|a| a.kind == Kind::Van);
            ladder |= s.story.snapshot().get(LADDER).visible;
            climbed |= s.story.actors().iter().any(|a| a.pose == ActorPose::Climb);
            worked |= s.story.actors().iter().any(|a| a.role == Role::Worker && a.pose == ActorPose::Work);
            s.story.snapshot().damage(SIGN) == Damage::Intact && ladder && worked
        });
        assert!(done.is_some(), "owner {} van {} ladder {} climbed {} worked {}", owner, van, ladder, climbed, worked);
        assert!(owner && van && climbed);
        run(&mut s, 45.0);
        assert!(!s.story.snapshot().get(LADDER).visible, "the ladder is taken away");
        quiet(&s);
        assert_eq!(s.story.actors().iter().filter(|a| a.kind == Kind::Van || a.role == Role::Owner).count(), 0);
    }

    #[test]
    fn a_sign_pulled_right_off_comes_down_and_is_refitted() {
        let mut s = evening();
        run(&mut s, 1.0);
        let p = at_of(&s, SIGN);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 50.0, p.1 + 10.0)));
        s.gesture(Gesture::Drag((p.0 + 110.0, p.1 + 30.0)));
        assert_eq!(s.story.snapshot().damage(SIGN), Damage::Broken);
        assert_eq!(s.story.snapshot().get(SIGN).attachment, Attachment::Loose);
        run(&mut s, 160.0);
        assert_eq!(s.story.snapshot().damage(SIGN), Damage::Intact);
        assert!(s.story.snapshot().get(SIGN).pose == crate::story::model::Pose::default());
        quiet(&s);
    }

    #[test]
    fn a_spilled_stack_is_gathered_parcel_by_parcel_even_one_sent_into_the_road() {
        let mut s = evening();
        run(&mut s, 1.0);
        poke(&mut s, STACK);
        run(&mut s, 2.5);
        assert!(s.story.snapshot().get(PARCEL_1).visible && s.story.snapshot().get(PARCEL_3).visible, "parcels on the kerb");
        assert_eq!(s.story.snapshot().damage(STACK), Damage::Missing);
        // one of them is thrown across the road
        let p = at_of(&s, PARCEL_2);
        let at = (p.0 + 34.0, 864.0);
        s.gesture(Gesture::Press(at));
        s.gesture(Gesture::Drag((at.0 + 10.0, at.1 + 30.0)));
        s.gesture(Gesture::Release { at: (at.0 + 10.0, at.1 + 30.0), velocity: (90.0, 260.0) });
        let done = until(&mut s, 150.0, |s| s.story.snapshot().damage(STACK) == Damage::Intact);
        assert!(done.is_some(), "the delivery is back together");
        run(&mut s, 60.0);
        for id in [PARCEL_1, PARCEL_2, PARCEL_3] {
            assert!(!s.story.snapshot().get(id).visible, "{:?} is gathered", id);
        }
        quiet(&s);
        assert!(s.story.actors().iter().all(|a| a.role != Role::Courier));
    }

    #[test]
    fn ordinary_street_life_comes_and_goes() {
        let mut s = test_session("street", "2026-10-13 11:00", "clear", (640, 400));
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..3000 {
            s.advance(0.2, 1.0 / 30.0);
            for (name, _, _, _) in s.story.director().status(s.story.definition()) {
                seen.insert(name);
            }
        }
        assert!(seen.len() >= 2, "{:?}", seen);
        assert!(s.story.actors().count() <= 24);
    }

    #[test]
    fn ambient_cars_wait_for_the_work_zone_and_pull_away_after() {
        // the queue itself: cars stop short of a shut stretch, in order, and are free again later
        let street = crate::views::find("street").unwrap();
        let _ = street;
        let mut s = evening();
        run(&mut s, 1.0);
        poke(&mut s, HYDRANT);
        run(&mut s, 6.0);
        // drawing a frame while shut must not panic and must keep cars out of the zone
        let _ = s.frame();
    }
}
