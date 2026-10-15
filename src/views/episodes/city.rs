//! City: the signal fails, the crane's load catches, a block loses its power.
//!
//! The viaduct runs across the picture with a station on it; a tower is
//! going up under a crane; below and behind are the ordinary windows of the
//! streets. The view finds where these are and passes them as
//! `CityBindings`; this file says what happens to them.

use crate::color::shade;
use crate::gfx::{Draw, Surface, P};
use crate::props::Layout;
use crate::story::actors::{ActorPose, ActorSpec, Kind, Mode, Role, RouteGraph};
use crate::story::behaviors::{repair, round, visit, Animal, BehaviorSpec, CrewJob, Occupant, Perch, Walker};
use crate::story::director::{Change, Class, Cond, Slot, Step, StoryDef, Test, Trigger, Until, Weather};
use crate::story::draw::{bar, ink, oval, poly, Tone};
use crate::story::events::EventKind;
use crate::story::model::{Accepts, AreaId, Damage, Definition, Material, ObjectDef, ObjectId, RouteId};
use crate::story::physics::{Segment, SurfaceMap};

pub const SIGNAL: ObjectId = ObjectId(1);
pub const LOAD: ObjectId = ObjectId(2);
pub const PART: ObjectId = ObjectId(3);
pub const BOX: ObjectId = ObjectId(4);
pub const CONE: ObjectId = ObjectId(5);
pub const BARRIER: ObjectId = ObjectId(6);
pub const WINDOW_A: ObjectId = ObjectId(7);
pub const WINDOW_B: ObjectId = ObjectId(8);
/// the block of windows on the box's supply: scenery, which says where the picture changes
pub const BLOCK: ObjectId = ObjectId(9);

pub const DECK_Y: f32 = 898.0;
/// where people stand: on the deck, in front of the train that stops beside them
const WALK_Y: f32 = DECK_Y + 2.0;

#[derive(Clone, Debug)]
pub struct CityBindings {
    /// the signal's mast: x on the parapet
    pub signal_x: f32,
    /// the tower going up: left, right and roof height
    pub works: (f32, f32, f32),
    /// where the hoist's pallet hangs, and how far down
    pub pallet: P,
    /// a junction box on the nearest roofs
    pub box_at: P,
    /// where the cat sits, and the clock tower's tip where birds sit
    pub cat: P,
    pub perch: P,
    /// the windows that go dark together, as a rectangle of design units
    pub group: [f32; 4],
    /// two windows to look out of
    pub windows: [[f32; 4]; 2],
}

const PL: [u16; 3] = [1, 2, 3];
const DOOR: u16 = 4;
const WALK_A: u16 = 5;
const SIG_FOOT: u16 = 6;
const WALK_B: u16 = 7;
const DECK_FAR: u16 = 8;
const TRAIN_IN: u16 = 10;
const TRAIN_STOP: u16 = 11;
const TRAIN_OUT: u16 = 12;
const TOP_H: u16 = 20;
const TOP_LOAD: u16 = 21;
const TOP_SCAF: u16 = 22;
const NR_H: u16 = 30;
const NR_BOX: u16 = 31;
const NR_FAR: u16 = 32;
const CAT_HOME: u16 = 40;
const CAT_SEAT: u16 = 41;
const PERCH_A: u16 = 42;
const PERCH_B: u16 = 43;
const WIN_A: u16 = 44;
const WIN_B: u16 = 45;
const FAR_ROOF: u16 = 46;

const AREA_PLATFORM: AreaId = AreaId(1);
const AREA_BOX: AreaId = AreaId(3);


/// Where the train stops short, as the x of its nose.
pub fn stop_nose(b: &CityBindings) -> f32 {
    b.signal_x - 36.0
}

pub fn define(b: &CityBindings) -> Definition {
    let r = |p: P, w: f32, h: f32| [p.0 - w, p.1 - h, p.0 + w, p.1 + h];
    let (wx0, wx1, wy) = b.works;
    let win = |k: usize| ((b.windows[k][0] + b.windows[k][2]) / 2.0, b.windows[k][3]);
    let objects = vec![
        ObjectDef::new(1, "station_signal", Material::Metal, (b.signal_x, DECK_Y - 18.0), r((b.signal_x, DECK_Y - 18.0), 6.0, 20.0)).accepting(Accepts::POKE).behind(3),
        ObjectDef::new(2, "hoist_load", Material::Wood, b.pallet, r(b.pallet, 12.0, 10.0)).accepting(Accepts::GRAB).breaks_at(22.0).behind(1),
        ObjectDef::new(3, "steel_plate", Material::Metal, (b.pallet.0 + 6.0, b.pallet.1 + 4.0), r((b.pallet.0 + 6.0, b.pallet.1 + 4.0), 6.0, 4.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden().behind(3),
        ObjectDef::new(4, "junction_box", Material::Metal, b.box_at, r(b.box_at, 7.0, 8.0)).accepting(Accepts::POKE),
        ObjectDef::new(5, "cone", Material::Stone, (b.signal_x - 20.0, DECK_Y), r((b.signal_x - 20.0, DECK_Y - 5.0), 5.0, 7.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden().behind(3),
        ObjectDef::new(6, "barrier", Material::Metal, (b.signal_x - 70.0, DECK_Y), r((b.signal_x - 70.0, DECK_Y - 6.0), 14.0, 6.0)).hidden().behind(3),
        ObjectDef::new(7, "window_a", Material::Glass, win(0), b.windows[0]).accepting(Accepts::KNOCK).behind(2),
        ObjectDef::new(8, "window_b", Material::Glass, win(1), b.windows[1]).accepting(Accepts::KNOCK).behind(2),
        ObjectDef::new(9, "supplied_block", Material::Stone, (b.group[0], b.group[1]), b.group).behind(2),
    ];

    let mut g = RouteGraph::default();
    let sc = 0.26;
    for (k, x) in [300.0f32, 352.0, 410.0].iter().enumerate() {
        g.add_node(PL[k], (*x, WALK_Y), Mode::Walk, sc);
    }
    g.add_node(DOOR, (448.0, WALK_Y), Mode::Walk, sc)
        .add_node(WALK_A, (520.0, WALK_Y), Mode::Walk, sc)
        .add_node(SIG_FOOT, (b.signal_x - 10.0, WALK_Y), Mode::Walk, sc)
        .add_node(WALK_B, (b.signal_x + 70.0, WALK_Y), Mode::Walk, sc);
    g.connect(PL[0], PL[1], Mode::Walk).connect(PL[1], PL[2], Mode::Walk).connect(PL[2], DOOR, Mode::Walk).connect(DOOR, WALK_A, Mode::Walk).connect(WALK_A, SIG_FOOT, Mode::Walk).connect(SIG_FOOT, WALK_B, Mode::Walk);
    // the rails: a train comes in at the left, stops short of the signal, and goes on to the right
    g.add_node(TRAIN_IN, (-60.0, DECK_Y + 1.0), Mode::Road, 1.0)
        .add_node(TRAIN_STOP, (stop_nose(b), DECK_Y + 1.0), Mode::Road, 1.0)
        .add_node(TRAIN_OUT, (DW_OUT, DECK_Y + 1.0), Mode::Road, 1.0);
    g.connect(TRAIN_IN, TRAIN_STOP, Mode::Road).connect(TRAIN_STOP, TRAIN_OUT, Mode::Road);
    // the tower going up
    g.add_node(TOP_H, (wx0 + 6.0, wy), Mode::Roof, 0.2)
        .add_node(TOP_LOAD, (b.pallet.0, wy), Mode::Roof, 0.2)
        .add_node(TOP_SCAF, (b.pallet.0 + 22.0, wy), Mode::Roof, 0.2);
    g.connect(TOP_H, TOP_LOAD, Mode::Roof).connect(TOP_LOAD, TOP_SCAF, Mode::Roof);
    // the works stand behind the mid row; the platform and deck are hidden only by the nearest roofs
    for n in [TOP_H, TOP_LOAD, TOP_SCAF] {
        g.behind(n, 1);
    }
    g.add_node(DECK_FAR, (b.pallet.0 + 6.0, WALK_Y), Mode::Walk, sc).connect(WALK_B, DECK_FAR, Mode::Walk);
    for n in [PL[0], PL[1], PL[2], DOOR, WALK_A, SIG_FOOT, WALK_B, DECK_FAR, TRAIN_IN, TRAIN_STOP, TRAIN_OUT] {
        g.behind(n, 3);
    }
    for n in [WIN_A, WIN_B] {
        g.behind(n, 2);
    }
    // the nearest roofs: a hatch, the box, a far end
    g.add_node(NR_H, (b.box_at.0 - 70.0, b.box_at.1 + 8.0), Mode::Roof, 0.30)
        .add_node(NR_BOX, (b.box_at.0 - 12.0, b.box_at.1 + 8.0), Mode::Roof, 0.30)
        .add_node(NR_FAR, (b.box_at.0 + 8.0, b.box_at.1 + 8.0), Mode::Roof, 0.30);
    g.connect(NR_H, NR_BOX, Mode::Roof).connect(NR_BOX, NR_FAR, Mode::Roof);
    g.add_node(CAT_HOME, b.cat, Mode::Roof, 0.3).add_node(CAT_SEAT, (b.cat.0 + 40.0, b.box_at.1 + 8.0), Mode::Roof, 0.3);
    g.connect(CAT_HOME, CAT_SEAT, Mode::Roof).connect(CAT_SEAT, NR_H, Mode::Roof);
    g.add_node(PERCH_A, (b.perch.0 - 6.0, b.perch.1), Mode::Roof, 0.3).add_node(PERCH_B, (b.perch.0 + 8.0, b.perch.1 + 10.0), Mode::Roof, 0.3);
    g.add_node(FAR_ROOF, (wx1 + 40.0, wy - 40.0), Mode::Roof, 0.3);
    g.add_node(WIN_A, ((b.windows[0][0] + b.windows[0][2]) / 2.0, b.windows[0][3]), Mode::Walk, 0.25);
    g.add_node(WIN_B, ((b.windows[1][0] + b.windows[1][2]) / 2.0, b.windows[1][3]), Mode::Walk, 0.25);

    let mut surfaces = SurfaceMap::default();
    surfaces.segments.push(Segment { a: (wx0, wy), b: (wx1, wy), material: Material::Stone });
    surfaces.segments.push(Segment { a: (-60.0, DECK_Y + 2.0), b: (1700.0, DECK_Y + 2.0), material: Material::Stone });
    surfaces.segments.push(Segment { a: (b.box_at.0 - 120.0, b.box_at.1 + 9.0), b: (b.box_at.0 + 140.0, b.box_at.1 + 9.0), material: Material::Stone });

    let behaviors = vec![
        BehaviorSpec::Perch(Perch { perches: vec![RouteId(PERCH_A), RouteId(PERCH_B)], alternates: vec![RouteId(FAR_ROOF)], radius: 80.0, away: 6.0, keep_off: None, shy_of: vec![Weather::Windy] }),
        BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(CAT_HOME), haunts: vec![RouteId(CAT_SEAT), RouteId(NR_H)], attention: 300.0, look: 2, linger: 6.0 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_A, at: RouteId(WIN_A), role: Role::Resident, look: 1 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_B, at: RouteId(WIN_B), role: Role::Resident, look: 4 }),
    ];
    let mut stories = vec![station_signal(b), crane_snag(b), local_power(b)];
    stories.extend(daily_life());
    Definition { view: "city", objects, routes: g, surfaces, stories, behaviors, custom: Vec::new() }
}

const DW_OUT: f32 = 1800.0;
const S0: Slot = Slot(0);
const S1: Slot = Slot(1);
const S2: Slot = Slot(2);
const S3: Slot = Slot(3);
const S4: Slot = Slot(4);

fn spawn(who: Slot, kind: Kind, role: Role, at: u16, look: u32) -> Step {
    Step::Spawn { who, spec: ActorSpec { kind, role, at: RouteId(at), look } }
}

/// The signal dies. The train stops short of it, passengers gather at the platform's edge, staff
/// shut the way, technicians come along the parapet and test it; an obstruction changes their way.
fn station_signal(b: &CityBindings) -> StoryDef {
    let tech = CrewJob {
        who: S3,
        role: Role::Technician,
        kind: Kind::Person,
        look: 1,
        depot: RouteId(DOOR),
        site: RouteId(SIG_FOOT),
        exit: RouteId(DOOR),
        area: AREA_PLATFORM,
        target: SIGNAL,
        work_secs: 9.0,
        tool: None,
        patience: 70.0,
    };
    let mut s = StoryDef::new(1, "station_signal", Class::Incident, Trigger::Event(EventKind::Impact, SIGNAL))
        .disturbed_by(&[SIGNAL, CONE])
        .escalating(&["more_staff", "more_staff", "more_staff"])
        .within(200.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: SIGNAL, change: Change::Damage(Damage::Damaged) },
                Step::Emit { kind: EventKind::Damage, obj: SIGNAL },
                // the train comes in on the schedule's own line and stops where the signal tells it
                spawn(S0, Kind::Train, Role::Staff, TRAIN_IN, 0),
                Step::Move { who: S0, to: RouteId(TRAIN_STOP), run: false },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: Some("gather") },
                Step::Goto("gather"),
            ],
        )
        .stage(
            "gather",
            vec![
                // people come to the platform's edge, and stand clear of the rail
                spawn(S1, Kind::Person, Role::Passenger, PL[0], 2),
                spawn(S2, Kind::Person, Role::Passenger, PL[1], 4),
                Step::Move { who: S1, to: RouteId(PL[1]), run: false },
                Step::Move { who: S2, to: RouteId(PL[2]), run: false },
                Step::Wait(2.0),
                spawn(S4, Kind::Person, Role::Staff, DOOR, 0),
                Step::Move { who: S4, to: RouteId(WALK_A), run: false },
                Step::Await { until: Until::Arrived(vec![S4]), timeout: 30.0, on_timeout: Some("cones") },
                Step::Pose { who: S4, pose: ActorPose::Operate },
                Step::Goto("cones"),
            ],
        )
        .stage(
            "cones",
            vec![
                Step::SetState { obj: CONE, change: Change::Visible(true) },
                Step::SetState { obj: BARRIER, change: Change::Visible(true) },
                // equipment moved from where the plan had it changes the order of work
                Step::Branch { when: Cond::is(Test::Displaced(CONE)), then: "tidy_cone", otherwise: "technicians" },
            ],
        )
        .stage(
            "tidy_cone",
            vec![Step::SetState { obj: CONE, change: Change::Place((b.signal_x - 20.0, DECK_Y)) }, Step::Wait(2.0), Step::Goto("technicians")],
        )
        .stage("more_staff", vec![spawn(S2, Kind::Person, Role::Staff, DOOR, 3), Step::Move { who: S2, to: RouteId(WALK_A), run: false }, Step::Wait(1.0), Step::Goto("technicians")])
        .stage(
            "cleanup",
            vec![
                Step::Release { who: S3 },
                Step::SetState { obj: SIGNAL, change: Change::Restore },
                Step::Goto("let_go"),
            ],
        );
    s.stages.extend(repair(["technicians", "test", "withdraw", "fallback"], "let_go", &tech));
    s.stage(
        "let_go",
        vec![
            Step::SetState { obj: CONE, change: Change::Visible(false) },
            Step::SetState { obj: BARRIER, change: Change::Visible(false) },
            // the signal is green: the train goes, the people disperse
            Step::Move { who: S0, to: RouteId(TRAIN_OUT), run: false },
            Step::Move { who: S1, to: RouteId(DOOR), run: false },
            Step::Move { who: S2, to: RouteId(DOOR), run: false },
            Step::Move { who: S4, to: RouteId(DOOR), run: false },
            Step::Await { until: Until::Arrived(vec![S0, S1, S2, S4]), timeout: 60.0, on_timeout: None },
            Step::Despawn { who: S0 },
            Step::Despawn { who: S1 },
            Step::Despawn { who: S2 },
            Step::Despawn { who: S4 },
            Step::End,
        ],
    )
}

/// The hoist's load catches on the scaffold; work stops; a crew frees it and the work restarts. Pulled
/// again, a plate comes off and falls to the viaduct and has to be lifted back.
fn crane_snag(b: &CityBindings) -> StoryDef {
    let up: Vec<Step> = (1..=6)
        .flat_map(|k| {
            let f = k as f32 / 6.0;
            vec![
                Step::SetState { obj: PART, change: Change::Place((b.pallet.0 + 6.0, DECK_Y - 4.0 + (b.pallet.1 + 4.0 - (DECK_Y - 4.0)) * f)) },
                Step::Wait(0.6),
            ]
        })
        .collect();
    let mut recover = vec![
        // out of a hatch in the viaduct below the works
        spawn(S2, Kind::Person, Role::Worker, DECK_FAR, 2),
        Step::MoveNear { who: S2, obj: PART, run: false },
        Step::Await { until: Until::Arrived(vec![S2]), timeout: 60.0, on_timeout: Some("release") },
        Step::Pose { who: S2, pose: ActorPose::Work },
        Step::Wait(3.0),
        Step::SetState { obj: PART, change: Change::Place((b.pallet.0 + 6.0, DECK_Y - 4.0)) },
    ];
    recover.extend(up);
    recover.push(Step::SetState { obj: PART, change: Change::Visible(false) });
    recover.push(Step::Move { who: S2, to: RouteId(DECK_FAR), run: false });
    recover.push(Step::Await { until: Until::Arrived(vec![S2]), timeout: 60.0, on_timeout: None });
    recover.push(Step::Despawn { who: S2 });
    recover.push(Step::Goto("release"));
    StoryDef::new(2, "crane_snag", Class::Incident, Trigger::Damage(LOAD, Damage::Damaged))
        .disturbed_by(&[LOAD])
        .escalating(&["dropped", "dropped", "dropped"])
        .within(180.0)
        .stage(
            "start",
            vec![
                // operations stop: the load has caught and the crane is held
                Step::SetState { obj: LOAD, change: Change::Damage(Damage::Damaged) },
                Step::Wait(1.2),
                spawn(S0, Kind::Person, Role::Worker, TOP_H, 0),
                spawn(S1, Kind::Person, Role::Worker, TOP_H, 3),
                Step::Move { who: S0, to: RouteId(TOP_LOAD), run: true },
                Step::Move { who: S1, to: RouteId(TOP_SCAF), run: true },
                Step::Await { until: Until::Arrived(vec![S0, S1]), timeout: 40.0, on_timeout: Some("release") },
                Step::Pose { who: S0, pose: ActorPose::Look },
                Step::Wait(1.5),
                Step::Pose { who: S1, pose: ActorPose::Work },
                Step::Wait(6.0),
                Step::Goto("release"),
            ],
        )
        .stage(
            "dropped",
            {
                // the plate comes off and falls to the deck: it is seen going
                let (x, y0) = (b.pallet.0 + 6.0, b.pallet.1 + 4.0);
                let mut v = vec![Step::SetState { obj: PART, change: Change::Visible(true) }, Step::Shatter { obj: LOAD, material: Material::Wood }];
                for k in 1..=5 {
                    let f = (k as f32 / 5.0).powi(2);
                    v.push(Step::SetState { obj: PART, change: Change::Place((x, y0 + (DECK_Y - 4.0 - y0) * f)) });
                    v.push(Step::Wait(0.14));
                }
                v.push(Step::Wait(1.2));
                v.push(Step::Goto("lift"));
                v
            },
        )
        .stage("lift", recover)
        .stage(
            "release",
            vec![
                Step::SetState { obj: LOAD, change: Change::Restore },
                Step::SetState { obj: PART, change: Change::Visible(false) },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Wait(2.0),
                Step::Move { who: S0, to: RouteId(TOP_H), run: false },
                Step::Move { who: S1, to: RouteId(TOP_H), run: false },
                Step::Await { until: Until::Arrived(vec![S0, S1]), timeout: 50.0, on_timeout: None },
                Step::Despawn { who: S0 },
                Step::Despawn { who: S1 },
                Step::End,
            ],
        )
        .stage(
            "cleanup",
            vec![
                Step::SetState { obj: LOAD, change: Change::Restore },
                Step::SetState { obj: PART, change: Change::Visible(false) },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
        .helped_by(PART, LOAD, 30.0)
}

/// A box on the roofs fails and a block of windows goes dark; people look out, a crew traces it.
fn local_power(_b: &CityBindings) -> StoryDef {
    let crew = CrewJob {
        who: S1,
        role: Role::Electrician,
        kind: Kind::Person,
        look: 2,
        depot: RouteId(NR_H),
        site: RouteId(NR_BOX),
        exit: RouteId(NR_H),
        area: AREA_BOX,
        target: BOX,
        work_secs: 11.0,
        tool: None,
        patience: 60.0,
    };
    let mut s = StoryDef::new(3, "local_power", Class::Incident, Trigger::Event(EventKind::Impact, BOX))
        .disturbed_by(&[BOX])
        .escalating(&["second_electrician", "second_electrician", "second_electrician"])
        .within(170.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: BOX, change: Change::Damage(Damage::Damaged) },
                Step::SetState { obj: BLOCK, change: Change::Damage(Damage::Damaged) },
                Step::Emit { kind: EventKind::Damage, obj: BOX },
                Step::Wait(2.5),
                // someone at a window looks out at the dark
                Step::SetState { obj: WINDOW_A, change: Change::Open(true) },
                spawn(S0, Kind::Person, Role::Resident, WIN_A, 1),
                Step::Pose { who: S0, pose: ActorPose::Look },
                Step::Wait(4.0),
                Step::Goto("trace"),
            ],
        )
        .stage(
            "trace",
            vec![
                // a bad weather day sends the crew a different way round (slower: it is wet)
                Step::Branch { when: Cond::is(Test::Weather(Weather::Rain)), then: "wet_start", otherwise: "arrive" },
            ],
        )
        .stage("wet_start", vec![Step::Wait(3.0), Step::Goto("arrive")])
        .stage("second_electrician", vec![spawn(S2, Kind::Person, Role::Electrician, NR_H, 4), Step::Move { who: S2, to: RouteId(NR_FAR), run: false }, Step::Wait(1.0), Step::Goto("arrive")])
        .stage(
            "cleanup",
            vec![
                Step::Release { who: S1 },
                Step::SetState { obj: BOX, change: Change::Restore },
                Step::SetState { obj: BLOCK, change: Change::Restore },
                Step::SetState { obj: WINDOW_A, change: Change::Open(false) },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        );
    s.stages.extend(repair(["arrive", "work", "depart", "fallback"], "lights_on", &crew));
    s.stage(
        "lights_on",
        vec![
            Step::SetState { obj: BLOCK, change: Change::Restore },
            Step::SetState { obj: WINDOW_A, change: Change::Open(false) },
            Step::Despawn { who: S2 },
            Step::Despawn { who: S0 },
            Step::End,
        ],
    )
}

fn daily_life() -> Vec<StoryDef> {
    let day = vec![Cond::is(Test::Weather(Weather::Day)), Cond::is(Test::Weather(Weather::Dry))];
    let mut out = Vec::new();
    let mut a = StoryDef::new(4, "passenger_visit", Class::Ambient, Trigger::Ambient { every: (60.0, 130.0), when: day.clone() }).within(140.0);
    a = a.stage("start", vec![Step::Goto("come")]);
    a.stages.extend(visit(["come", "stay", "leave"], "done", Walker { who: S0, role: Role::Passenger, kind: Kind::Person, look: 3, from: RouteId(DOOR) }, RouteId(PL[0]), ActorPose::Look, 5.0, RouteId(DOOR)));
    out.push(a.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    let mut m = StoryDef::new(5, "staff_exchange", Class::Ambient, Trigger::Ambient { every: (80.0, 170.0), when: day.clone() }).within(160.0);
    m = m.stage("start", vec![Step::Goto("come")]);
    m.stages.extend(crate::story::behaviors::meeting(
        ["come", "talk", "part"],
        "done",
        Walker { who: S0, role: Role::Staff, kind: Kind::Person, look: 0, from: RouteId(DOOR) },
        Walker { who: S1, role: Role::Staff, kind: Kind::Person, look: 2, from: RouteId(WALK_B) },
        RouteId(WALK_A),
        RouteId(SIG_FOOT),
        6.0,
        RouteId(DOOR),
        RouteId(WALK_B),
    ));
    out.push(m.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::Despawn { who: S1 }, Step::End]));
    let mut r = StoryDef::new(6, "station_inspection", Class::Ambient, Trigger::Ambient { every: (100.0, 220.0), when: vec![Cond::is(Test::Weather(Weather::Day))] }).within(180.0);
    r = r.stage("start", vec![Step::Goto("round")]);
    r.stages.extend(round(
        ["round", "leave"],
        "done",
        Walker { who: S0, role: Role::Technician, kind: Kind::Person, look: 1, from: RouteId(DOOR) },
        &[(RouteId(SIG_FOOT), ActorPose::Operate, 3.0), (RouteId(PL[1]), ActorPose::Work, 3.0)],
        RouteId(DOOR),
    ));
    out.push(r.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    out
}

// ── drawing ──────────────────────────────────────────────────────────────────

/// The signal mast: green when it works, dark when it has failed (the live layer blinks it).
pub fn signal_mast<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, damage: Damage, tone: &Tone) {
    let steel = tone.paint([90.0, 96.0, 104.0]);
    ink(d, lay, &[(x, DECK_Y), (x, DECK_Y - 32.0)], 1.4, steel, 1.0);
    bar(d, lay, [x - 3.5, DECK_Y - 40.0, x + 3.5, DECK_Y - 28.0], shade(steel, -0.3), 1.0);
    let ok = damage == Damage::Intact;
    oval(d, lay, x, DECK_Y - 34.0, 1.8, 1.8, if ok { [90.0, 220.0, 120.0] } else { [58.0, 52.0, 48.0] }, 1.0);
    oval(d, lay, x, DECK_Y - 38.0, 1.8, 1.8, if ok { [74.0, 44.0, 44.0] } else { [58.0, 52.0, 48.0] }, 1.0);
}

/// The failed signal's lamp, blinking orange; nothing when it is between flashes.
pub fn signal_blink<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, t: f32) {
    if (t * 2.2).sin() > 0.1 {
        oval(d, lay, x, DECK_Y - 38.0, 2.2, 2.2, [255.0, 150.0, 44.0], 1.0);
    }
}

/// The hoist at the top of the works and its pallet on a rope: steady, hanging askew, or gone.
pub fn hoist<S: Surface>(d: &mut Draw<S>, lay: &Layout, works: (f32, f32, f32), pallet: P, damage: Damage, tone: &Tone, pose: crate::story::model::Pose) {
    let steel = tone.paint([110.0, 116.0, 124.0]);
    let (_, _, wy) = works;
    ink(d, lay, &[(pallet.0 + 8.0, wy), (pallet.0 + 8.0, pallet.1 - 20.0), (pallet.0 - 4.0, pallet.1 - 20.0)], 1.4, steel, 1.0);
    let rope = tone.paint([190.0, 168.0, 120.0]);
    let hang = (pallet.0 + pose.dx, pallet.1 - 10.0 + pose.dy);
    match damage {
        Damage::Intact => {
            ink(d, lay, &[(pallet.0 - 4.0, pallet.1 - 20.0), (hang.0, hang.1 + 2.0)], 0.7, rope, 1.0);
            bar(d, lay, [hang.0 - 9.0, hang.1 + 2.0, hang.0 + 9.0, hang.1 + 11.0], tone.paint([170.0, 130.0, 86.0]), 1.0);
        }
        Damage::Damaged => {
            // caught: one rope slack, the pallet tilted against the scaffold
            ink(d, lay, &[(pallet.0 - 4.0, pallet.1 - 20.0), (hang.0 + 14.0, hang.1 + 4.0)], 0.7, rope, 1.0);
            poly(d, lay, &[(hang.0 + 5.0, hang.1 - 4.0), (hang.0 + 22.0, hang.1 + 4.0), (hang.0 + 18.0, hang.1 + 13.0), (hang.0 + 1.0, hang.1 + 5.0)], tone.paint([170.0, 130.0, 86.0]), 1.0);
        }
        _ => {
            // let go: the rope hangs empty
            ink(d, lay, &[(pallet.0 - 4.0, pallet.1 - 20.0), (pallet.0 - 4.0, pallet.1 - 6.0)], 0.7, rope, 1.0);
        }
    }
}

pub fn junction_box<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, damage: Damage, tone: &Tone) {
    let grey = tone.paint([120.0, 124.0, 130.0]);
    bar(d, lay, [at.0 - 6.0, at.1 - 9.0, at.0 + 6.0, at.1 + 7.0], grey, 1.0);
    if damage != Damage::Intact {
        // door hanging open, a scorch above
        poly(d, lay, &[(at.0 + 6.0, at.1 - 9.0), (at.0 + 13.0, at.1 - 5.0), (at.0 + 13.0, at.1 + 4.0), (at.0 + 6.0, at.1 + 7.0)], shade(grey, -0.25), 1.0);
        oval(d, lay, at.0, at.1 - 12.0, 5.0, 3.0, [40.0, 38.0, 36.0], 0.6);
    } else {
        bar(d, lay, [at.0 - 4.5, at.1 - 6.0, at.0 + 4.5, at.1 + 4.0], shade(grey, -0.1), 1.0);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::actors::ActorState;
    use crate::story::physics::Gesture;
    use crate::story::replay::scripted::*;
    use crate::story::replay::{test_session, Session};

    fn evening() -> Session {
        test_session("city", "2026-10-13 18:20", "clear", (640, 400))
    }

    fn roles(s: &Session, role: Role) -> usize {
        s.story.actors().iter().filter(|a| a.role == role).count()
    }

    #[test]
    fn the_signal_fails_a_train_stops_people_gather_and_technicians_restore_it() {
        let mut s = evening();
        run(&mut s, 1.0);
        poke(&mut s, SIGNAL);
        run(&mut s, 0.5);
        assert_eq!(s.story.snapshot().damage(SIGNAL), Damage::Damaged);
        let (mut train, mut passengers, mut tech, mut cones) = (false, false, false, false);
        let done = until(&mut s, 160.0, |s| {
            train |= s.story.actors().iter().any(|a| a.kind == Kind::Train && a.state == ActorState::Arrived);
            passengers |= roles(s, Role::Passenger) > 0;
            tech |= s.story.actors().iter().any(|a| a.role == Role::Technician && a.pose == ActorPose::Work);
            cones |= s.story.snapshot().get(CONE).visible;
            s.story.snapshot().damage(SIGNAL) == Damage::Intact && train && tech
        });
        assert!(done.is_some(), "train {} passengers {} technician {} cones {}", train, passengers, tech, cones);
        assert!(passengers && cones);
        run(&mut s, 80.0);
        assert!(!s.story.snapshot().get(CONE).visible && !s.story.snapshot().get(BARRIER).visible);
        quiet(&s);
        assert!(s.story.actors().iter().all(|a| a.kind != Kind::Train), "the train has gone");
        assert_eq!(roles(&s, Role::Technician) + roles(&s, Role::Passenger), 0);
    }

    #[test]
    fn a_pulled_load_catches_the_crane_stops_and_it_is_freed_or_recovered() {
        let mut s = evening();
        run(&mut s, 1.0);
        let p = at_of(&s, LOAD);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 24.0, p.1 + 6.0)));
        assert_ne!(s.story.snapshot().damage(LOAD), Damage::Intact);
        s.gesture(Gesture::Release { at: (p.0 + 24.0, p.1 + 6.0), velocity: (0.0, 0.0) });
        let mut workers = false;
        let done = until(&mut s, 160.0, |s| {
            workers |= roles(s, Role::Worker) > 0;
            s.story.snapshot().damage(LOAD) == Damage::Intact && workers
        });
        assert!(done.is_some(), "workers {}", workers);
        run(&mut s, 60.0);
        quiet(&s);
        assert_eq!(roles(&s, Role::Worker), 0);
        assert!(!s.story.snapshot().get(PART).visible);
    }

    #[test]
    fn a_load_pulled_further_drops_a_plate_that_is_lifted_back() {
        let mut s = evening();
        run(&mut s, 1.0);
        let p = at_of(&s, LOAD);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 30.0, p.1 + 8.0)));
        s.gesture(Gesture::Drag((p.0 + 70.0, p.1 + 24.0)));
        assert_eq!(s.story.snapshot().damage(LOAD), Damage::Broken);
        let (mut fell, mut lifted) = (false, false);
        let done = until(&mut s, 200.0, |s| {
            fell |= s.story.snapshot().get(PART).visible && s.story.snapshot().get(PART).pose.dy > 100.0;
            lifted |= s.story.actors().iter().any(|a| a.role == Role::Worker && a.pose == ActorPose::Work);
            s.story.snapshot().damage(LOAD) == Damage::Intact && fell && lifted
        });
        assert!(done.is_some(), "plate fell {} lifted {}", fell, lifted);
        run(&mut s, 30.0);
        assert!(!s.story.snapshot().get(PART).visible, "the plate is back up");
        quiet(&s);
        assert_eq!(roles(&s, Role::Worker), 0);
    }

    #[test]
    fn a_box_fails_a_block_goes_dark_and_a_crew_brings_the_lights_back() {
        let mut s = evening();
        run(&mut s, 1.0);
        poke(&mut s, BOX);
        run(&mut s, 0.5);
        assert_eq!(s.story.snapshot().damage(BOX), Damage::Damaged);
        assert_eq!(s.story.snapshot().damage(BLOCK), Damage::Damaged, "the block is dark");
        let (mut looked, mut crew) = (false, false);
        let done = until(&mut s, 160.0, |s| {
            looked |= roles(s, Role::Resident) > 0;
            crew |= s.story.actors().iter().any(|a| a.role == Role::Electrician && a.pose == ActorPose::Work);
            s.story.snapshot().damage(BOX) == Damage::Intact && crew
        });
        assert!(done.is_some(), "looked {} crew {}", looked, crew);
        assert!(looked);
        run(&mut s, 40.0);
        assert!(!s.story.snapshot().get(WINDOW_A).open);
        assert_eq!(s.story.snapshot().damage(BLOCK), Damage::Intact, "the lights are back");
        quiet(&s);
    }

    #[test]
    fn interference_escalates_at_most_three_times_then_ends_clean() {
        let mut s = evening();
        run(&mut s, 1.0);
        let mut most = 0;
        for n in 0..16 {
            s.story.bus_emit_impact(SIGNAL, 7000 + n);
            run(&mut s, 5.0);
            most = most.max(s.story.director().status(s.story.definition()).iter().map(|x| x.2).max().unwrap_or(0));
            assert!(s.story.actors().count() <= 24);
        }
        assert_eq!(most, 3);
        run(&mut s, 240.0);
        quiet(&s);
        assert_eq!(s.story.snapshot().damage(SIGNAL), Damage::Intact);
        assert!(!s.story.snapshot().get(CONE).visible);
    }

    #[test]
    fn ordinary_city_life_comes_and_goes() {
        let mut s = test_session("city", "2026-10-13 11:00", "clear", (640, 400));
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
    fn the_definition_is_sound_at_every_size() {
        for size in [(640, 400), (1280, 800), (900, 900), (2560, 1080)] {
            let s = test_session("city", "2026-10-13 18:20", "clear", size);
            assert!(s.story.definition().objects.len() >= 8, "{:?}", size);
        }
    }
}
