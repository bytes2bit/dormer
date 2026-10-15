//! Coast: a boat comes off its mooring, the lantern stops turning, a crate goes into the water.
//!
//! The headland, its tower and the jetty are drawn in a layout slid sideways by `land`; the
//! view passes that and where things stand in its own numbers as `CoastBindings`, and this file
//! says what happens to them. The keeper's lamp in the cottage window is not touched: it means
//! the machine, and nothing here is allowed to say anything about that.

use crate::color::{mix, shade};
use crate::gfx::{Draw, Surface, P};
use crate::props::Layout;
use crate::story::actors::{ActorPose, ActorSpec, Kind, Mode, Role, RouteGraph};
use crate::story::behaviors::{meeting, round, visit, Animal, BehaviorSpec, Perch, Walker};
use crate::story::director::{Change, Class, Cond, Slot, Step, StoryDef, Test, Trigger, Until, Weather};
use crate::story::draw::{bar, ink, oval, Tone};
use crate::story::events::EventKind;
use crate::story::model::{Accepts, AreaId, Damage, Definition, Material, ObjectDef, ObjectId, RouteId};
use crate::story::physics::{Basin, Segment, SurfaceMap};

pub const BOAT_A: ObjectId = ObjectId(1);
pub const BOAT_B: ObjectId = ObjectId(2);
pub const LANTERN: ObjectId = ObjectId(3);
pub const PART: ObjectId = ObjectId(4);
pub const CRATE: ObjectId = ObjectId(5);
pub const LINE: ObjectId = ObjectId(6);

#[derive(Clone, Debug)]
pub struct CoastBindings {
    /// how far the headland's numbers are slid to the right of design units
    pub land: f32,
    /// the tower: x of its axis, y of the foot, y of the lantern's centre
    pub light: (f32, f32, f32),
    /// the jetty: x of its landward end, of its far end, y of the deck's top
    pub jetty: (f32, f32, f32),
    /// the cottage door
    pub door: P,
    /// where the boats lie, in the headland's numbers
    pub boats: [(f32, f32, bool); 8],
    /// where a crate stands on the jetty
    pub crate_at: P,
}

// ── routes ──────────────────────────────────────────────────────────────────
const K_DOOR: u16 = 1;
const K_YARD: u16 = 2;
const K_TDOOR: u16 = 3;
const K_S1: u16 = 4;
const K_S2: u16 = 5;
const K_S3: u16 = 6;
const K_GAL: u16 = 7;
const K_GROUND: u16 = 8;
const C_TOP: u16 = 10;
const C_MID: u16 = 11;
const C_LOW: u16 = 12;
const J_ROOT: u16 = 13;
const J_1: u16 = 14;
const J_2: u16 = 15;
const J_END: u16 = 16;
const M_A: u16 = 20;
const M_B: u16 = 21;
const D_0: u16 = 22;
const D_1: u16 = 23;
const D_2: u16 = 24;
const D_3: u16 = 25;
const D_END: u16 = 26;
const QUAY: u16 = 27;
const P_TOWER: u16 = 30;
const P_POST: u16 = 31;
const P_FAR: u16 = 32;
const G_AIR: u16 = 33;
const CAT_HOME: u16 = 34;
const CAT_WALL: u16 = 35;

const AREA_WATER: AreaId = AreaId(1);

pub const STORIES: [&str; 6] = ["loose_mooring", "lighthouse_fault", "cargo_overboard", "dock_delivery", "keeper_meeting", "keeper_round"];

const S0: Slot = Slot(0);
const S1: Slot = Slot(1);
const S2: Slot = Slot(2);
const S3: Slot = Slot(3);
const S4: Slot = Slot(4);

fn spawn(who: Slot, kind: Kind, role: Role, at: u16, look: u32) -> Step {
    Step::Spawn { who, spec: ActorSpec { kind, role, at: RouteId(at), look } }
}

fn go(who: Slot, to: u16, run: bool) -> Step {
    Step::Move { who, to: RouteId(to), run }
}

fn arrive(who: &[Slot], timeout: f32, otherwise: &'static str) -> Step {
    Step::Await { until: Until::Arrived(who.to_vec()), timeout, on_timeout: Some(otherwise) }
}

/// A point of the headland, in design units.
fn at(b: &CoastBindings, x: f32, y: f32) -> P {
    (x + b.land, y)
}

pub fn define(b: &CoastBindings) -> Definition {
    let l = b.land;
    let (lx, lfoot, lamp_y) = b.light;
    let (j0, j1, jy) = b.jetty;
    let r = |p: P, w: f32, h: f32| [p.0 - w, p.1 - h, p.0 + w, p.1 + h];
    let boat_at = |k: usize| at(b, b.boats[k].0, b.boats[k].1);
    let (a_at, bb_at) = (boat_at(2), boat_at(3));
    let lantern = at(b, lx, lamp_y);
    let part_at = at(b, lx + 12.0, 380.0);
    let crate_at = at(b, b.crate_at.0, b.crate_at.1);
    let line_at = at(b, j1 - 14.0, jy - 2.0);
    let objects = vec![
        ObjectDef::new(1, "moored_boat", Material::Wood, a_at, r((a_at.0 + 2.0, a_at.1 - 6.0), 18.0, 12.0)).accepting(Accepts::GRAB).breaks_at(24.0).behind(4),
        ObjectDef::new(2, "work_boat", Material::Wood, bb_at, r((bb_at.0 + 2.0, bb_at.1 - 6.0), 18.0, 12.0)).behind(4),
        ObjectDef::new(3, "lantern", Material::Glass, lantern, [lantern.0 - 9.0, lamp_y - 12.0, lantern.0 + 9.0, lamp_y + 12.0]).accepting(Accepts::POKE).behind(2),
        ObjectDef::new(4, "lens_part", Material::Metal, part_at, r(part_at, 4.0, 3.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden(),
        ObjectDef::new(5, "crate", Material::Wood, crate_at, r(crate_at, 7.0, 6.0)).accepting(Accepts::GRAB.or(Accepts::THROW).or(Accepts::MOVE)).behind(3),
        ObjectDef::new(6, "line", Material::Rope, line_at, r(line_at, 6.0, 3.0)).accepting(Accepts::MOVE.or(Accepts::THROW)).hidden().behind(3),
    ];

    let mut g = RouteGraph::default();
    // the keeper's way: cottage door, the yard, the tower's door, the stair inside it, the gallery
    let s = 0.34;
    let n = |x: f32, y: f32| at(b, x, y);
    g.add_node(K_DOOR, n(b.door.0, b.door.1), Mode::Walk, s).add_node(K_YARD, n(262.0, 520.0), Mode::Walk, s).add_node(K_TDOOR, n(lx, lfoot - 1.0), Mode::Walk, s);
    g.add_node(K_S1, n(lx, lfoot - 20.0), Mode::Ladder, s).add_node(K_S2, n(lx, 460.0), Mode::Ladder, s).add_node(K_S3, n(lx, 414.0), Mode::Ladder, s).add_node(K_GAL, n(lx + 12.0, 382.0), Mode::Ladder, s);
    g.add_node(K_GROUND, n(lx + 28.0, 523.0), Mode::Walk, s);
    g.connect(K_DOOR, K_YARD, Mode::Walk).connect(K_YARD, K_TDOOR, Mode::Walk).connect(K_TDOOR, K_GROUND, Mode::Walk);
    g.connect(K_TDOOR, K_S1, Mode::Ladder).connect(K_S1, K_S2, Mode::Ladder).connect(K_S2, K_S3, Mode::Ladder).connect(K_S3, K_GAL, Mode::Ladder);
    // down the cliff to the jetty: over the edge, hidden in the rock, out at its foot
    g.add_node(C_TOP, n(440.0, 548.0), Mode::Walk, s).add_node(C_MID, n(470.0, 650.0), Mode::Ladder, s).add_node(C_LOW, n(498.0, 760.0), Mode::Ladder, s);
    g.add_node(J_ROOT, n(j0 + 8.0, jy - 2.5), Mode::Walk, 0.3).add_node(J_1, n(j0 + (j1 - j0) * 0.35, jy - 2.5), Mode::Walk, 0.3).add_node(J_2, n(j0 + (j1 - j0) * 0.7, jy - 2.5), Mode::Walk, 0.3).add_node(J_END, n(j1 - 8.0, jy - 2.5), Mode::Walk, 0.3);
    g.connect(K_GROUND, C_TOP, Mode::Walk).connect(C_TOP, C_MID, Mode::Ladder).connect(C_MID, C_LOW, Mode::Ladder).connect(C_LOW, J_ROOT, Mode::Walk);
    g.connect(J_ROOT, J_1, Mode::Walk).connect(J_1, J_2, Mode::Walk).connect(J_2, J_END, Mode::Walk);
    // what hides someone standing there: the headland hides the stair inside the tower and the cliff
    for id in [K_DOOR, K_YARD, K_TDOOR, K_GROUND, C_TOP] {
        g.behind(id, 2);
    }
    for id in [K_S1, K_S2, K_S3, K_GAL, C_MID, C_LOW] {
        g.behind(id, 1);
    }
    for id in [J_ROOT, J_1, J_2, J_END] {
        g.behind(id, 3);
    }
    // the water: the moorings, the way things drift, the quay
    let w = |x: f32, y: f32| n(x, y);
    g.add_node(M_A, w(b.boats[2].0, b.boats[2].1), Mode::Water, 0.6)
        .add_node(M_B, w(b.boats[3].0, b.boats[3].1), Mode::Water, 0.6)
        .add_node(QUAY, w(j1 + 10.0, 811.0), Mode::Water, 0.6)
        .add_node(D_0, w(740.0, 812.0), Mode::Water, 0.6)
        .add_node(D_1, w(800.0, 812.0), Mode::Water, 0.6)
        .add_node(D_2, w(860.0, 812.0), Mode::Water, 0.6)
        .add_node(D_3, w(920.0, 812.0), Mode::Water, 0.6)
        .add_node(D_END, w(980.0, 812.0), Mode::Water, 0.6);
    g.connect(M_A, QUAY, Mode::Water).connect(QUAY, D_0, Mode::Water).connect(M_B, D_0, Mode::Water).connect(M_B, D_1, Mode::Water).connect(D_0, D_1, Mode::Water).connect(D_1, D_2, Mode::Water).connect(D_2, D_3, Mode::Water).connect(D_3, D_END, Mode::Water);
    for id in [M_A, M_B, QUAY, D_0, D_1, D_2, D_3, D_END] {
        g.behind(id, 3);
    }
    // gulls: the top of the tower, the end post of the jetty, a further post; and the air over the water
    g.add_node(P_TOWER, n(lx, 372.0), Mode::Roof, 0.4).add_node(P_POST, n(j1 - 4.0, jy - 14.0), Mode::Roof, 0.4).add_node(P_FAR, n(910.0, 790.0), Mode::Roof, 0.4).add_node(G_AIR, n(830.0, 770.0), Mode::Roof, 0.4);
    g.connect(P_TOWER, G_AIR, Mode::Roof).connect(P_POST, G_AIR, Mode::Roof).connect(P_FAR, G_AIR, Mode::Roof);
    g.add_node(CAT_HOME, n(266.0, 512.5), Mode::Roof, 0.34).add_node(CAT_WALL, n(lx + 28.0, 523.0), Mode::Roof, 0.34);
    g.connect(CAT_HOME, K_YARD, Mode::Roof).connect(K_YARD, CAT_WALL, Mode::Roof);

    let mut surfaces = SurfaceMap::default();
    // the jetty's deck, the turf along the headland, the water
    surfaces.segments.push(Segment { a: n(j0, jy - 2.5), b: n(j1, jy - 2.5), material: Material::Wood });
    surfaces.segments.push(Segment { a: n(200.0, 517.5), b: n(300.0, 521.5), material: Material::Stone });
    surfaces.segments.push(Segment { a: n(300.0, 521.5), b: n(430.0, 541.0), material: Material::Stone });
    surfaces.basins.push(Basin { rect: [n(j0 + 4.0, 0.0).0, 811.0, n(1500.0, 0.0).0, 1000.0], current: (8.0, 0.0) });
    let _ = l;

    let behaviors = vec![
        BehaviorSpec::Perch(Perch { perches: vec![RouteId(P_TOWER), RouteId(P_POST)], alternates: vec![RouteId(P_FAR)], radius: 90.0, away: 6.0, keep_off: None, shy_of: vec![Weather::Windy] }),
        BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(CAT_HOME), haunts: vec![RouteId(CAT_WALL)], attention: 260.0, look: 2, linger: 6.0 }),
    ];
    let mut stories = vec![loose_mooring(b), lighthouse_fault(b), cargo_overboard(b)];
    stories.extend(daily_life());
    Definition { view: "coast", objects, routes: g, surfaces, stories, behaviors, custom: Vec::new() }
}

/// The rope lets go; the boat goes with the water. Its owner runs to the jetty; the work boat goes
/// out for it, or, if somebody has moved the line, the owner hauls it in himself.
fn loose_mooring(b: &CoastBindings) -> StoryDef {
    let _ = b;
    StoryDef::new(1, "loose_mooring", Class::Incident, Trigger::Damage(BOAT_A, Damage::Damaged))
        .disturbed_by(&[BOAT_A, LINE])
        .escalating(&["extra_hand", "extra_hand", "extra_hand"])
        .within(220.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: BOAT_A, change: Change::Damage(Damage::Damaged) },
                Step::Emit { kind: EventKind::Damage, obj: BOAT_A },
                Step::Wait(0.8),
                // the boat is the water's now: it is carried by an actor on the drift line
                Step::SetState { obj: BOAT_A, change: Change::Visible(false) },
                spawn(S4, Kind::Boat, Role::Owner, M_A, 1),
                go(S4, D_END, false),
                spawn(S0, Kind::Person, Role::Owner, J_ROOT, 3),
                go(S0, J_END, true),
                Step::Goto("raise"),
            ],
        )
        .stage(
            "raise",
            vec![
                arrive(&[S0], 50.0, "decide"),
                Step::Pose { who: S0, pose: ActorPose::React },
                Step::Wait(1.0),
                // the line is put out, and a few seconds are given for somebody to take it up
                Step::SetState { obj: LINE, change: Change::Visible(true) },
                Step::Wait(6.0),
                Step::Goto("decide"),
            ],
        )
        .stage(
            "decide",
            vec![Step::Branch { when: Cond::is(Test::Displaced(LINE)), then: "haul", otherwise: "work_boat" }],
        )
        .stage(
            "haul",
            vec![
                // the line was moved: the owner takes it and hauls the boat in from the jetty
                Step::Pose { who: S0, pose: ActorPose::Work },
                Step::Await { until: Until::Arrived(vec![S4]), timeout: 60.0, on_timeout: None },
                go(S4, M_A, false),
                arrive(&[S4], 90.0, "home"),
                Step::Goto("home"),
            ],
        )
        .stage(
            "work_boat",
            vec![
                Step::Await { until: Until::AreaFree(AREA_WATER), timeout: 90.0, on_timeout: Some("haul") },
                spawn(S2, Kind::Boat, Role::Dockhand, M_B, 2),
                Step::SetState { obj: BOAT_B, change: Change::Visible(false) },
                Step::Reserve { who: S2, area: AREA_WATER },
                go(S2, D_END, false),
                arrive(&[S2], 90.0, "home"),
                // they go back together: the rescuer to its mooring, the boat to its own
                go(S4, M_A, false),
                go(S2, M_B, false),
                arrive(&[S4, S2], 100.0, "home"),
                Step::Goto("home"),
            ],
        )
        .stage(
            "extra_hand",
            vec![spawn(S3, Kind::Person, Role::Dockhand, J_ROOT, 0), go(S3, J_1, false), Step::Wait(1.0), Step::Goto("decide")],
        )
        .stage(
            "home",
            vec![
                Step::SetState { obj: BOAT_A, change: Change::Restore },
                Step::SetState { obj: BOAT_B, change: Change::Visible(true) },
                Step::SetState { obj: LINE, change: Change::Visible(false) },
                Step::Release { who: S2 },
                Step::Despawn { who: S4 },
                Step::Despawn { who: S2 },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Wait(2.0),
                go(S0, J_ROOT, false),
                Step::Despawn { who: S3 },
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: None },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
        .stage(
            "cleanup",
            vec![
                Step::SetState { obj: BOAT_A, change: Change::Restore },
                Step::SetState { obj: BOAT_B, change: Change::Visible(true) },
                Step::SetState { obj: LINE, change: Change::Visible(false) },
                Step::Release { who: S2 },
                Step::Despawn { who: S4 },
                Step::Despawn { who: S3 },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
}

/// The lantern stops turning. The keeper goes up the tower, finds the fault, tests the sweep. In a
/// wind a part is lost over the gallery's rail and has to be fetched.
fn lighthouse_fault(b: &CoastBindings) -> StoryDef {
    let (lx, _, _) = b.light;
    let from = at(b, lx + 12.0, 380.0);
    let ground = at(b, lx + 28.0, 520.0);
    let mut gust = vec![Step::SetState { obj: PART, change: Change::Visible(true) }, Step::SetState { obj: PART, change: Change::Place(from) }];
    for k in 1..=6 {
        let f = (k as f32 / 6.0).powi(2);
        gust.push(Step::SetState { obj: PART, change: Change::Place((from.0 + (ground.0 - from.0) * (k as f32 / 6.0), from.1 + (ground.1 - from.1) * f)) });
        gust.push(Step::Wait(0.13));
    }
    gust.extend([
        Step::Wait(1.0),
        go(S0, K_GROUND, false),
        arrive(&[S0], 60.0, "work"),
        Step::Pose { who: S0, pose: ActorPose::Work },
        Step::Wait(2.0),
        Step::SetState { obj: PART, change: Change::Visible(false) },
        go(S0, K_GAL, false),
        arrive(&[S0], 90.0, "work"),
        Step::Goto("work"),
    ]);
    StoryDef::new(2, "lighthouse_fault", Class::Incident, Trigger::Event(EventKind::Impact, LANTERN))
        .disturbed_by(&[LANTERN, PART])
        .escalating(&["extra_hand", "extra_hand", "extra_hand"])
        .within(240.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: LANTERN, change: Change::Damage(Damage::Damaged) },
                Step::Emit { kind: EventKind::Damage, obj: LANTERN },
                Step::Wait(2.5),
                spawn(S0, Kind::Person, Role::Keeper, K_DOOR, 1),
                go(S0, K_TDOOR, true),
                arrive(&[S0], 70.0, "inside"),
                Step::Pose { who: S0, pose: ActorPose::Look },
                Step::Wait(1.2),
                Step::Goto("inside"),
            ],
        )
        .stage(
            "inside",
            vec![go(S0, K_GAL, false), arrive(&[S0], 120.0, "fix"), Step::Goto("fix")],
        )
        .stage(
            "fix",
            vec![Step::Branch { when: Cond::is(Test::Weather(Weather::Windy)), then: "gust", otherwise: "work" }],
        )
        .stage("gust", gust)
        .stage(
            "work",
            vec![
                Step::Pose { who: S0, pose: ActorPose::Work },
                Step::Wait(7.0),
                // the sweep is tried before anyone trusts it
                Step::SetState { obj: LANTERN, change: Change::Open(true) },
                Step::Wait(6.0),
                Step::SetState { obj: LANTERN, change: Change::Open(false) },
                Step::SetState { obj: LANTERN, change: Change::Restore },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Goto("descend"),
            ],
        )
        .stage(
            "descend",
            vec![go(S0, K_DOOR, false), go(S1, K_TDOOR, false), arrive(&[S0], 120.0, "done"), Step::Goto("done")],
        )
        .stage("extra_hand", vec![spawn(S1, Kind::Person, Role::Dockhand, J_ROOT, 2), go(S1, K_YARD, true), Step::Wait(1.0), Step::Goto("inside")])
        .stage(
            "done",
            vec![Step::Despawn { who: S0 }, Step::Despawn { who: S1 }, Step::SetState { obj: PART, change: Change::Visible(false) }, Step::End],
        )
        .stage(
            "cleanup",
            vec![
                Step::SetState { obj: LANTERN, change: Change::Restore },
                Step::SetState { obj: PART, change: Change::Visible(false) },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
        .helped_by(PART, LANTERN, 40.0)
}

/// A crate goes off the jetty. Gulls come to look, the dockhand watches from the end of the jetty,
/// and the work boat goes out for it as soon as the water is free.
fn cargo_overboard(b: &CoastBindings) -> StoryDef {
    let _ = b;
    StoryDef::new(3, "cargo_overboard", Class::Incident, Trigger::Event(EventKind::Noise, CRATE))
        .disturbed_by(&[CRATE])
        .escalating(&["gulls", "gulls", "gulls"])
        .within(220.0)
        .stage(
            "start",
            vec![
                spawn(S0, Kind::Person, Role::Dockhand, J_ROOT, 0),
                go(S0, J_END, true),
                spawn(S1, Kind::Bird, Role::Resident, P_TOWER, 0),
                go(S1, G_AIR, false),
                Step::Wait(1.5),
                Step::Goto("boat"),
            ],
        )
        .stage("gulls", vec![spawn(S3, Kind::Bird, Role::Resident, P_POST, 1), go(S3, G_AIR, false), Step::Wait(1.0), Step::Goto("boat")])
        .stage(
            "boat",
            vec![
                // the one work boat does one job at a time: whoever has the water first
                Step::Await { until: Until::AreaFree(AREA_WATER), timeout: 120.0, on_timeout: Some("home") },
                spawn(S2, Kind::Boat, Role::Dockhand, M_B, 2),
                Step::SetState { obj: BOAT_B, change: Change::Visible(false) },
                Step::Reserve { who: S2, area: AREA_WATER },
                Step::MoveNear { who: S2, obj: CRATE, run: false },
                arrive(&[S2], 90.0, "home"),
                Step::Pose { who: S2, pose: ActorPose::Work },
                Step::Wait(1.5),
                // it comes aboard; and the way back
                Step::SetState { obj: CRATE, change: Change::Visible(false) },
                go(S2, QUAY, false),
                arrive(&[S2], 90.0, "home"),
                Step::SetState { obj: CRATE, change: Change::Restore },
                Step::Pose { who: S0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Goto("home"),
            ],
        )
        .stage(
            "home",
            vec![
                Step::SetState { obj: CRATE, change: Change::Restore },
                Step::SetState { obj: BOAT_B, change: Change::Visible(true) },
                Step::Release { who: S2 },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S3 },
                go(S0, J_ROOT, false),
                Step::Await { until: Until::Arrived(vec![S0]), timeout: 40.0, on_timeout: None },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
        .stage(
            "cleanup",
            vec![
                Step::SetState { obj: CRATE, change: Change::Restore },
                Step::SetState { obj: BOAT_B, change: Change::Visible(true) },
                Step::Release { who: S2 },
                Step::Despawn { who: S3 },
                Step::Despawn { who: S2 },
                Step::Despawn { who: S1 },
                Step::Despawn { who: S0 },
                Step::End,
            ],
        )
}

fn daily_life() -> Vec<StoryDef> {
    let day = vec![Cond::is(Test::Weather(Weather::Day)), Cond::is(Test::Weather(Weather::Dry))];
    let mut out = Vec::new();
    // a dockhand brings the post up to the cottage
    let mut a = StoryDef::new(4, "dock_delivery", Class::Ambient, Trigger::Ambient { every: (70.0, 150.0), when: day.clone() }).within(200.0);
    a = a.stage("start", vec![Step::Goto("come")]);
    a.stages.extend(visit(["come", "stay", "leave"], "done", Walker { who: S0, role: Role::Dockhand, kind: Kind::Person, look: 2, from: RouteId(J_ROOT) }, RouteId(K_YARD), ActorPose::Wave, 4.0, RouteId(J_ROOT)));
    out.push(a.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    // the keeper and a dockhand meet in the yard
    let mut m = StoryDef::new(5, "keeper_meeting", Class::Ambient, Trigger::Ambient { every: (90.0, 190.0), when: day.clone() }).within(220.0);
    m = m.stage("start", vec![Step::Goto("come")]);
    m.stages.extend(meeting(
        ["come", "talk", "part"],
        "done",
        Walker { who: S0, role: Role::Keeper, kind: Kind::Person, look: 1, from: RouteId(K_DOOR) },
        Walker { who: S1, role: Role::Dockhand, kind: Kind::Person, look: 3, from: RouteId(J_ROOT) },
        RouteId(K_YARD),
        RouteId(K_GROUND),
        6.0,
        RouteId(K_DOOR),
        RouteId(J_ROOT),
    ));
    out.push(m.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::Despawn { who: S1 }, Step::End]));
    // the keeper's rounds: the tower's foot, the end of the jetty
    let mut r = StoryDef::new(6, "keeper_round", Class::Ambient, Trigger::Ambient { every: (110.0, 230.0), when: vec![Cond::is(Test::Weather(Weather::Day))] }).within(240.0);
    r = r.stage("start", vec![Step::Goto("round")]);
    r.stages.extend(round(
        ["round", "leave"],
        "done",
        Walker { who: S0, role: Role::Keeper, kind: Kind::Person, look: 1, from: RouteId(K_DOOR) },
        &[(RouteId(K_TDOOR), ActorPose::Look, 3.0), (RouteId(J_END), ActorPose::Look, 4.0)],
        RouteId(K_DOOR),
    ));
    out.push(r.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: S0 }, Step::End]));
    out
}

// ── drawing ──────────────────────────────────────────────────────────────────

/// A coil of line lying on the jetty.
pub fn coil<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone, alpha: f32) {
    let rope = tone.paint([196.0, 172.0, 120.0]);
    oval(d, lay, at.0, at.1 - 1.4, 4.4, 2.2, rope, alpha);
    oval(d, lay, at.0, at.1 - 1.6, 2.4, 1.1, shade(rope, -0.35), alpha);
    ink(d, lay, &[(at.0 + 4.0, at.1 - 1.2), (at.0 + 9.0, at.1 - 0.4)], 1.0, rope, alpha);
}

/// The lens part, a small bright plate.
pub fn part<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, turn: f32, tone: &Tone, alpha: f32) {
    let c = tone.paint([210.0, 214.0, 190.0]);
    let (cs, sn) = (turn.cos(), turn.sin());
    let p = |x: f32, y: f32| (at.0 + x * cs - y * sn, at.1 + x * sn + y * cs);
    crate::story::draw::poly(d, lay, &[p(-3.5, -2.0), p(3.5, -2.0), p(3.5, 2.0), p(-3.5, 2.0)], c, alpha);
    bar(d, lay, [at.0 - 0.6, at.1 - 0.6, at.0 + 0.6, at.1 + 0.6], mix(c, [255.0, 255.0, 255.0], 0.5), alpha);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::actors::ActorState;
    use crate::story::physics::Gesture;
    use crate::story::replay::scripted::*;
    use crate::story::replay::{test_session, Session};

    fn calm_day() -> Session {
        test_session("coast", "2026-10-13 11:00", "clear", (640, 400))
    }

    fn windy_dusk() -> Session {
        test_session("coast", "2026-10-13 18:20", "storm", (640, 400))
    }

    fn roles(s: &Session, role: Role) -> usize {
        s.story.actors().iter().filter(|a| a.role == role).count()
    }

    #[test]
    fn a_pulled_mooring_lets_the_boat_go_and_it_is_brought_back() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        let p = at_of(&s, BOAT_A);
        s.gesture(Gesture::Press((p.0 + 2.0, p.1 - 6.0)));
        s.gesture(Gesture::Drag((p.0 + 40.0, p.1 - 6.0)));
        s.gesture(Gesture::Release { at: (p.0 + 40.0, p.1 - 6.0), velocity: (0.0, 0.0) });
        assert_eq!(s.story.snapshot().damage(BOAT_A), Damage::Damaged);
        let (mut drifted, mut owner, mut work_boat, mut line) = (false, false, false, false);
        let done = until(&mut s, 220.0, |s| {
            drifted |= s.story.actors().iter().any(|a| a.kind == Kind::Boat && a.role == Role::Owner && a.pos.0 > p.0 + 60.0);
            owner |= roles(s, Role::Owner) > 1 || s.story.actors().iter().any(|a| a.kind == Kind::Person && a.role == Role::Owner);
            work_boat |= s.story.actors().iter().any(|a| a.kind == Kind::Boat && a.role == Role::Dockhand);
            line |= s.story.snapshot().get(LINE).visible;
            s.story.snapshot().damage(BOAT_A) == Damage::Intact && s.story.snapshot().get(BOAT_A).visible && drifted
        });
        assert!(done.is_some(), "drifted {} owner {} work boat {} line {}", drifted, owner, work_boat, line);
        assert!(owner && work_boat && line);
        run(&mut s, 30.0);
        quiet(&s);
        assert!(s.story.snapshot().get(BOAT_B).visible && !s.story.snapshot().get(LINE).visible);
        assert_eq!(s.story.actors().iter().filter(|a| matches!(a.kind, Kind::Boat)).count(), 0);
    }

    #[test]
    fn a_moved_line_brings_the_owner_to_haul_the_boat_himself() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        s.story.trigger(crate::story::model::StoryId(1));
        let (mut worked, mut other_boat) = (false, false);
        // wait for the line to be put out, then move it
        let shown = until(&mut s, 80.0, |s| s.story.snapshot().get(LINE).visible);
        assert!(shown.is_some(), "the line is put out");
        let p = at_of(&s, LINE);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 - 20.0, p.1)));
        s.gesture(Gesture::Release { at: (p.0 - 20.0, p.1), velocity: (0.0, 0.0) });
        let done = until(&mut s, 220.0, |s| {
            worked |= s.story.actors().iter().any(|a| a.role == Role::Owner && a.kind == Kind::Person && a.pose == ActorPose::Work);
            other_boat |= s.story.actors().iter().any(|a| a.kind == Kind::Boat && a.role == Role::Dockhand);
            s.story.snapshot().damage(BOAT_A) == Damage::Intact && s.story.snapshot().get(BOAT_A).visible
        });
        assert!(done.is_some(), "worked {} other boat {}", worked, other_boat);
        assert!(worked && !other_boat, "the owner hauled it in without the work boat");
    }

    #[test]
    fn the_lantern_stops_the_keeper_climbs_fixes_and_tests_it() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        poke(&mut s, LANTERN);
        run(&mut s, 0.5);
        assert_eq!(s.story.snapshot().damage(LANTERN), Damage::Damaged);
        let (mut climbed, mut gallery, mut tested) = (false, false, false);
        let done = until(&mut s, 220.0, |s| {
            climbed |= s.story.actors().iter().any(|a| a.role == Role::Keeper && a.pose == ActorPose::Climb);
            gallery |= s.story.actors().iter().any(|a| a.role == Role::Keeper && a.pose == ActorPose::Work);
            tested |= s.story.snapshot().get(LANTERN).open;
            s.story.snapshot().damage(LANTERN) == Damage::Intact && tested
        });
        assert!(done.is_some(), "climbed {} worked {} tested {}", climbed, gallery, tested);
        assert!(climbed && gallery);
        run(&mut s, 60.0);
        quiet(&s);
        assert_eq!(roles(&s, Role::Keeper), 0);
        assert!(!s.story.snapshot().get(LANTERN).open);
    }

    #[test]
    fn in_a_wind_a_part_is_dropped_and_fetched() {
        let mut s = windy_dusk();
        s.set_wind(1.6);
        run(&mut s, 1.0);
        poke(&mut s, LANTERN);
        let (mut fell, mut fetched) = (false, false);
        let done = until(&mut s, 260.0, |s| {
            fell |= s.story.snapshot().get(PART).visible;
            fetched |= fell && !s.story.snapshot().get(PART).visible;
            s.story.snapshot().damage(LANTERN) == Damage::Intact && fetched
        });
        assert!(done.is_some(), "fell {} fetched {}", fell, fetched);
        run(&mut s, 60.0);
        quiet(&s);
    }

    #[test]
    fn a_thrown_crate_floats_gulls_come_and_the_work_boat_brings_it_back() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        let p = at_of(&s, CRATE);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 20.0, p.1 + 6.0)));
        s.gesture(Gesture::Release { at: (p.0 + 20.0, p.1 + 6.0), velocity: (160.0, 30.0) });
        let (mut floated, mut gulls, mut boat) = (false, false, false);
        let done = until(&mut s, 220.0, |s| {
            floated |= s.story.physics().bodies.iter().any(|b| b.floating);
            gulls |= s.story.actors().iter().any(|a| a.kind == Kind::Bird && matches!(a.state, ActorState::Moving | ActorState::Arrived));
            boat |= s.story.actors().iter().any(|a| a.kind == Kind::Boat && a.role == Role::Dockhand);
            floated && boat && s.story.snapshot().get(CRATE).visible && s.story.snapshot().damage(CRATE) == Damage::Intact && s.story.snapshot().in_base(CRATE)
        });
        assert!(done.is_some(), "floated {} gulls {} boat {}", floated, gulls, boat);
        assert!(gulls);
        run(&mut s, 30.0);
        quiet(&s);
        assert!(s.story.snapshot().get(BOAT_B).visible);
    }

    #[test]
    fn two_losses_at_once_share_the_one_work_boat() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        s.story.trigger(crate::story::model::StoryId(1));
        run(&mut s, 2.0);
        let p = at_of(&s, CRATE);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 20.0, p.1 + 6.0)));
        s.gesture(Gesture::Release { at: (p.0 + 20.0, p.1 + 6.0), velocity: (160.0, 30.0) });
        let mut at_once = 0;
        let done = until(&mut s, 400.0, |s| {
            at_once = at_once.max(s.story.actors().iter().filter(|a| a.kind == Kind::Boat && a.role == Role::Dockhand).count());
            s.story.snapshot().damage(BOAT_A) == Damage::Intact && s.story.snapshot().in_base(CRATE) && !s.story.director().active()
        });
        assert!(done.is_some(), "both are recovered, one after the other");
        assert!(at_once <= 1, "only one work boat: {}", at_once);
        quiet(&s);
    }

    #[test]
    fn interference_escalates_at_most_three_times_then_ends_clean() {
        let mut s = calm_day();
        run(&mut s, 1.0);
        let mut most = 0;
        for n in 0..16 {
            s.story.bus_emit_impact(LANTERN, 9000 + n);
            run(&mut s, 5.0);
            most = most.max(s.story.director().status(s.story.definition()).iter().map(|x| x.2).max().unwrap_or(0));
            assert!(s.story.actors().count() <= 24);
        }
        assert_eq!(most, 3);
        run(&mut s, 260.0);
        quiet(&s);
        assert_eq!(s.story.snapshot().damage(LANTERN), Damage::Intact);
    }

    #[test]
    fn ordinary_coast_life_comes_and_goes() {
        let mut s = calm_day();
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
}
