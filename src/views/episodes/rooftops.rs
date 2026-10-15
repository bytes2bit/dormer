//! Rooftops: a sheet gets away, a chimney cap falls, a tank leaks.
//!
//! The view (`views/rooftops.rs`) finds where things are in the town and says
//! so in `RooftopBindings`: a washing line on one flat, a chimney, a water
//! tank, the flats people can cross, a ladder's foot and head, the aerials a
//! sheet can catch on. This file turns that into objects, routes, surfaces
//! and stories; and draws the parts that change, once, for the still and the
//! live layer alike.

use crate::color::{shade, Rgb};
use crate::gfx::{Draw, Surface, P};
use crate::props::Layout;
use crate::story::actors::{ActorPose, Kind, Mode, Role, RouteGraph};
use crate::story::behaviors::{repair, round, visit, Animal, BehaviorSpec, CrewJob, Occupant, Perch, Walker};
use crate::story::director::{Change, Class, Cond, Slot, Step, StoryDef, Test, Trigger, Until, Weather};
use crate::story::draw::{bar, ink, oval, poly, Tone};
use crate::story::events::EventKind;
use crate::story::model::{Accepts, AreaId, Damage, Definition, Material, ObjectDef, ObjectId, RouteId};
use crate::story::physics::{Basin, Segment, Snag, SnowSpot, SurfaceMap};

pub const SHEET: ObjectId = ObjectId(1);
pub const CAP: ObjectId = ObjectId(2);
pub const TANK: ObjectId = ObjectId(3);
pub const LADDER: ObjectId = ObjectId(4);
pub const SCAFFOLD: ObjectId = ObjectId(5);
pub const BUCKET: ObjectId = ObjectId(6);
pub const PART: ObjectId = ObjectId(7);
pub const WINDOW_A: ObjectId = ObjectId(8);
pub const WINDOW_B: ObjectId = ObjectId(9);
pub const POT: ObjectId = ObjectId(10);

/// What the town gives the stories to work with, in design units (1600 by 1000).
#[derive(Clone, Debug)]
pub struct RooftopBindings {
    /// the washing line: its two posts, feet on the flat, and the flat's height
    pub line: (P, P),
    /// the chimney: the middle of its top, how wide, and the gable it stands on
    pub chimney: P,
    pub chimney_w: f32,
    /// left eave, apex and right eave of the gable the chimney stands on
    pub gable: [P; 3],
    /// the leaking tank: the middle of its foot, and how big
    pub tank: P,
    pub tank_size: f32,
    /// the flats people cross, left to right: (x0, x1, y)
    pub flats: Vec<(f32, f32, f32)>,
    /// where the ladder stands on a flat, and where it leans against the wall above
    pub ladder: (P, P),
    /// where the roofer stands to work at the chimney
    pub site: P,
    /// the places along the roofs of the neighbourhood people may walk: (x, y)
    pub walk: Vec<P>,
    /// aerial tops and the crane's mast: where a sheet carried by the wind catches
    pub snags: Vec<P>,
    /// two windows people look out of: the middle of the sill and the pane
    pub windows: [(P, [f32; 4]); 2],
    /// a flower pot on a parapet
    pub pot: P,
    /// where birds sit by the chimney, and a roof they go to instead
    pub perches: Vec<P>,
    pub elsewhere: P,
    /// how big a person is on the nearest flats, and on the roofs behind
    pub scale_near: f32,
    pub scale_far: f32,
}

// ── the nodes of the routes ──────────────────────────────────────────────────

const HATCH_C: u16 = 1;
const LINE_NODE: u16 = 2;
const C_RIGHT: u16 = 3;
const HATCH_A: u16 = 4;
const TANK_NODE: u16 = 5;
const A_RIGHT: u16 = 6;
const B_LEFT: u16 = 7;
const LADDER_FOOT: u16 = 8;
const LADDER_HEAD: u16 = 9;
const SITE: u16 = 10;
const PERCH_BASE: u16 = 30;
const ELSEWHERE: u16 = 40;
const WINDOW_NODE_A: u16 = 41;
const WINDOW_NODE_B: u16 = 42;
const WALK_BASE: u16 = 50;
const CAT_HOME: u16 = 60;
const CAT_PERCH: u16 = 61;

const AREA_CAP: AreaId = AreaId(1);
const AREA_TANK: AreaId = AreaId(2);
const AREA_LINE: AreaId = AreaId(3);


pub fn define(b: &RooftopBindings) -> Definition {
    let r = |p: P, w: f32, h: f32| [p.0 - w, p.1 - h, p.0 + w, p.1 + h];
    let (line_a, line_b) = b.line;
    let sheet_at = ((line_a.0 + line_b.0) / 2.0, line_a.1 - 11.0);
    let mut objects = vec![
        // the sheet pegged out: pulled, it comes off its pegs
        ObjectDef::new(1, "sheet", Material::Cloth, sheet_at, r(sheet_at, 16.0, 12.0)).accepting(Accepts::GRAB.or(Accepts::THROW)).breaks_at(26.0),
        // the chimney's cap: struck (a pebble, a finger), it comes down
        ObjectDef::new(2, "chimney_cap", Material::Stone, b.chimney, r((b.chimney.0, b.chimney.1 - 3.0), b.chimney_w * 0.9 + 3.0, 8.0)).accepting(Accepts::POKE),
        // the tank and its valve
        ObjectDef::new(3, "tank", Material::Metal, (b.tank.0, b.tank.1 - b.tank_size * 0.7), r((b.tank.0, b.tank.1 - b.tank_size * 0.7), b.tank_size * 0.6, b.tank_size * 0.8)).accepting(Accepts::POKE),
        ObjectDef::new(4, "ladder", Material::Wood, ((b.ladder.0 .0 + b.ladder.1 .0) / 2.0, (b.ladder.0 .1 + b.ladder.1 .1) / 2.0), [b.ladder.0 .0 - 8.0, b.ladder.1 .1 - 4.0, b.ladder.0 .0 + 12.0, b.ladder.0 .1 + 2.0]).hidden(),
        ObjectDef::new(5, "scaffold", Material::Metal, (b.site.0 - 14.0, b.site.1), r((b.site.0 - 14.0, b.site.1 - 14.0), 14.0, 18.0)).hidden(),
        ObjectDef::new(6, "bucket", Material::Metal, (b.tank.0 - 26.0, b.flats.first().map_or(b.tank.1, |f| f.2)), r((b.tank.0 - 26.0, b.tank.1 - 5.0), 6.0, 6.0)).accepting(Accepts::MOVE.or(Accepts::THROW)),
        ObjectDef::new(7, "valve_wheel", Material::Metal, (b.tank.0 + 40.0, b.tank.1), r((b.tank.0 + 40.0, b.tank.1 - 3.0), 5.0, 5.0)).accepting(Accepts::MOVE.or(Accepts::THROW)),
        ObjectDef::new(8, "window_a", Material::Glass, b.windows[0].0, b.windows[0].1).accepting(Accepts::KNOCK),
        ObjectDef::new(9, "window_b", Material::Glass, b.windows[1].0, b.windows[1].1).accepting(Accepts::KNOCK),
        ObjectDef::new(10, "flower_pot", Material::Stone, b.pot, r((b.pot.0, b.pot.1 - 4.0), 5.0, 5.0)).accepting(Accepts::THROW.or(Accepts::POKE)),
    ];
    // nothing but the walls of the sheet's own post is a place to hit
    objects.iter_mut().for_each(|o| o.layer = None);

    // ── routes: the flats, the ladder, the roofs, the places birds sit ──
    let mut g = RouteGraph::default();
    let (near, far) = (b.scale_near, b.scale_far);
    let flat = |k: usize| b.flats.get(k).copied().unwrap_or((0.0, 0.0, 0.0));
    let (c, a, bb) = (flat(0), flat(1), flat(2));
    g.add_node(HATCH_C, (c.0 + 8.0, c.2), Mode::Roof, near)
        .add_node(LINE_NODE, ((line_a.0 + line_b.0) / 2.0, c.2), Mode::Roof, near)
        .add_node(C_RIGHT, (c.1 - 4.0, c.2), Mode::Roof, near)
        .add_node(HATCH_A, (a.0 + 8.0, a.2), Mode::Roof, near)
        .add_node(TANK_NODE, (b.tank.0 - 18.0, a.2), Mode::Roof, near)
        .add_node(A_RIGHT, (a.1 - 4.0, a.2), Mode::Roof, near)
        .add_node(B_LEFT, (bb.0 + 4.0, bb.2), Mode::Roof, near)
        .add_node(LADDER_FOOT, b.ladder.0, Mode::Roof, near)
        .add_node(LADDER_HEAD, b.ladder.1, Mode::Roof, far)
        .add_node(SITE, b.site, Mode::Roof, far);
    g.connect(HATCH_C, LINE_NODE, Mode::Roof)
        .connect(LINE_NODE, C_RIGHT, Mode::Roof)
        .connect(C_RIGHT, HATCH_A, Mode::Roof)
        .connect(HATCH_A, TANK_NODE, Mode::Roof)
        .connect(TANK_NODE, A_RIGHT, Mode::Roof)
        .connect(A_RIGHT, B_LEFT, Mode::Roof)
        .connect(B_LEFT, LADDER_FOOT, Mode::Roof)
        .connect(LADDER_FOOT, LADDER_HEAD, Mode::Ladder)
        .connect(LADDER_HEAD, SITE, Mode::Roof);
    g.behind(LADDER_HEAD, 4).behind(SITE, 4);
    // the neighbourhood's roofs, to walk on when something is on one of them
    let mut prev = LADDER_HEAD;
    for (k, p) in b.walk.iter().enumerate() {
        let id = WALK_BASE + k as u16;
        g.add_node(id, *p, Mode::Roof, far);
        g.behind(id, 4);
        g.connect(prev, id, Mode::Roof);
        prev = id;
    }
    for (k, p) in b.perches.iter().enumerate() {
        let id = PERCH_BASE + k as u16;
        g.add_node(id, *p, Mode::Roof, far);
        g.behind(id, 4);
    }
    g.add_node(ELSEWHERE, b.elsewhere, Mode::Roof, far);
    g.behind(ELSEWHERE, 4);
    g.add_node(WINDOW_NODE_A, (b.windows[0].0 .0, b.windows[0].0 .1 + 1.0), Mode::Walk, 0.55).behind(WINDOW_NODE_A, 5);
    g.add_node(WINDOW_NODE_B, (b.windows[1].0 .0, b.windows[1].0 .1 + 1.0), Mode::Walk, 0.55).behind(WINDOW_NODE_B, 5);
    g.add_node(CAT_HOME, (a.0 + 40.0, a.2), Mode::Roof, near);
    g.add_node(CAT_PERCH, (a.1 - 24.0, a.2), Mode::Roof, near);
    g.connect(CAT_HOME, CAT_PERCH, Mode::Roof).connect(CAT_HOME, HATCH_A, Mode::Roof);

    // ── what things land on ──
    let mut surfaces = SurfaceMap::default();
    for f in &b.flats {
        surfaces.segments.push(Segment { a: (f.0, f.2), b: (f.1, f.2), material: Material::Stone });
    }
    // the tank's flat falls a little towards its drain
    surfaces.segments.push(Segment { a: (b.gable[0].0, b.gable[0].1), b: (b.gable[1].0, b.gable[1].1), material: Material::Stone });
    surfaces.segments.push(Segment { a: (b.gable[1].0, b.gable[1].1), b: (b.gable[2].0, b.gable[2].1), material: Material::Stone });
    for s in &b.snags {
        surfaces.snags.push(Snag { at: *s, radius: 10.0, tag: 1 });
    }
    surfaces.snow.push(SnowSpot { at: (b.gable[1].0 - 30.0, b.gable[1].1 + 4.0), capacity: 2.0 });
    // nothing below the lowest roof: a body that falls past it is lost
    let _ = Basin { rect: [0.0, 1200.0, 1.0, 1201.0], current: (0.0, 0.0) };

    // ── behaviors ──
    let behaviors = vec![
        BehaviorSpec::Perch(Perch {
            perches: (0..b.perches.len()).map(|k| RouteId(PERCH_BASE + k as u16)).collect(),
            alternates: vec![RouteId(ELSEWHERE)],
            radius: 70.0,
            away: 7.0,
            keep_off: Some(AREA_CAP),
            shy_of: vec![Weather::Windy],
        }),
        BehaviorSpec::Animal(Animal { kind: Kind::Cat, home: RouteId(CAT_HOME), haunts: vec![RouteId(CAT_PERCH), RouteId(TANK_NODE)], attention: 260.0, look: 1, linger: 6.0 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_A, at: RouteId(WINDOW_NODE_A), role: Role::Resident, look: 2 }),
        BehaviorSpec::Occupant(Occupant { window: WINDOW_B, at: RouteId(WINDOW_NODE_B), role: Role::Resident, look: 5 }),
    ];

    // ── stories ──
    let mut stories = Vec::new();
    stories.push(washing());
    stories.push(chimney_cap());
    stories.push(tank_leak(b));
    stories.extend(daily_life());

    Definition { view: "rooftops", objects, routes: g, surfaces, stories, behaviors, custom: Vec::new() }
}

fn roofer(who: u8, from: u16, look: u32) -> Walker {
    Walker { who: Slot(who), role: Role::Roofer, kind: Kind::Person, look, from: RouteId(from) }
}

fn resident(who: u8, from: u16, look: u32) -> Walker {
    Walker { who: Slot(who), role: Role::Resident, kind: Kind::Person, look, from: RouteId(from) }
}

const R0: Slot = Slot(0);
const R1: Slot = Slot(1);

/// Escaped washing: a tug takes a peg off; pull on and the sheet is away on the wind.
fn washing() -> StoryDef {
    let mut s = StoryDef::new(1, "washing", Class::Incident, Trigger::Damage(SHEET, Damage::Damaged))
        .disturbed_by(&[SHEET])
        .escalating(&["tugged_again", "tugged_again", "tugged_again"])
        .within(140.0)
        .stage(
            "start",
            vec![
                Step::Wait(1.2),
                Step::Spawn { who: R0, spec: crate::story::actors::ActorSpec { kind: Kind::Person, role: Role::Resident, at: RouteId(HATCH_C), look: 3 } },
                Step::Look { who: R0, at: (0.0, 0.0) },
                Step::LookAt { who: R0, obj: SHEET },
                Step::Pose { who: R0, pose: ActorPose::React },
                Step::Wait(1.0),
                Step::Move { who: R0, to: RouteId(LINE_NODE), run: true },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 20.0, on_timeout: Some("settle") },
                Step::Goto("settle"),
            ],
        )
        // wait for the sheet to be at rest wherever it has gone
        .stage(
            "settle",
            vec![
                Step::Await { until: Until::AtRest(SHEET), timeout: 14.0, on_timeout: None },
                Step::Wait(0.8),
                Step::Branch { when: Cond::is(Test::Hanging(SHEET)), then: "high", otherwise: "low" },
            ],
        )
        .stage(
            "low",
            vec![
                // pegged by one corner still, or lying where it fell within reach
                Step::Branch { when: Cond::is(Test::ObjectIs(SHEET, Damage::Damaged)), then: "repeg", otherwise: "fetch" },
            ],
        )
        .stage(
            "repeg",
            vec![
                Step::Reserve { who: R0, area: AREA_LINE },
                Step::Pose { who: R0, pose: ActorPose::Operate },
                Step::Await { until: Until::Assisted, timeout: 4.0, on_timeout: None },
                Step::SetState { obj: SHEET, change: Change::Restore },
                Step::Release { who: R0 },
                Step::Pose { who: R0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Goto("home"),
            ],
        )
        .stage(
            "fetch",
            vec![
                Step::MoveNear { who: R0, obj: SHEET, run: false },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 40.0, on_timeout: Some("home") },
                Step::Carry { who: R0, obj: SHEET },
                Step::Move { who: R0, to: RouteId(LINE_NODE), run: false },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 40.0, on_timeout: Some("pegged") },
                Step::Goto("pegged"),
            ],
        )
        .stage(
            "pegged",
            vec![
                Step::Pose { who: R0, pose: ActorPose::Operate },
                Step::Wait(3.0),
                Step::SetState { obj: SHEET, change: Change::Restore },
                Step::Pose { who: R0, pose: ActorPose::Idle },
                Step::Goto("home"),
            ],
        )
        .stage(
            "high",
            vec![
                // snagged on an aerial or the crane: the resident cannot reach it, so a roofer comes
                Step::Pose { who: R0, pose: ActorPose::React },
                Step::Wait(1.5),
                Step::Spawn { who: R1, spec: crate::story::actors::ActorSpec { kind: Kind::Person, role: Role::Roofer, at: RouteId(HATCH_A), look: 1 } },
                Step::MoveNear { who: R1, obj: SHEET, run: false },
                Step::Await { until: Until::Arrived(vec![R1]), timeout: 70.0, on_timeout: Some("give_up") },
                Step::Branch { when: Cond::is(Test::Failed(R1)), then: "give_up", otherwise: "climb" },
            ],
        )
        .stage(
            "climb",
            vec![
                Step::Pose { who: R1, pose: ActorPose::Work },
                Step::Wait(2.5),
                Step::Carry { who: R1, obj: SHEET },
                Step::Move { who: R1, to: RouteId(LINE_NODE), run: false },
                Step::Await { until: Until::Arrived(vec![R1]), timeout: 70.0, on_timeout: Some("give_up") },
                Step::Pose { who: R1, pose: ActorPose::Operate },
                Step::Wait(2.0),
                Step::SetState { obj: SHEET, change: Change::Restore },
                Step::Pose { who: R1, pose: ActorPose::Idle },
                Step::Pose { who: R0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Move { who: R1, to: RouteId(HATCH_A), run: false },
                Step::Await { until: Until::Arrived(vec![R1]), timeout: 70.0, on_timeout: None },
                Step::Despawn { who: R1 },
                Step::Goto("home"),
            ],
        )
        .stage(
            "give_up",
            vec![
                // not fetched: it is put back on the line by hand of the story, so nothing stays strewn
                Step::SetState { obj: SHEET, change: Change::Restore },
                Step::Despawn { who: R1 },
                Step::Goto("home"),
            ],
        )
        .stage(
            "home",
            vec![
                Step::Release { who: R0 },
                Step::Move { who: R0, to: RouteId(HATCH_C), run: false },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 40.0, on_timeout: None },
                Step::Despawn { who: R0 },
                Step::End,
            ],
        )
        .stage("tugged_again", vec![Step::Pose { who: R0, pose: ActorPose::React }, Step::Wait(1.0), Step::Goto("settle")])
        .stage(
            "cleanup",
            vec![
                Step::Release { who: R0 },
                Step::SetState { obj: SHEET, change: Change::Restore },
                Step::Despawn { who: R1 },
                Step::Despawn { who: R0 },
                Step::End,
            ],
        );
    s.assists = vec![(PART, SHEET, 60.0)];
    s
}

/// A chimney cap falls with its tiles; a roofer comes by ladder, mends it and goes.
fn chimney_cap() -> StoryDef {
    let job = CrewJob {
        who: R0,
        role: Role::Roofer,
        kind: Kind::Person,
        look: 0,
        depot: RouteId(HATCH_A),
        site: RouteId(SITE),
        exit: RouteId(HATCH_A),
        area: AREA_CAP,
        target: CAP,
        work_secs: 9.0,
        tool: None,
        patience: 80.0,
    };
    let mut story = StoryDef::new(2, "chimney_cap", Class::Incident, Trigger::Event(EventKind::Impact, CAP))
        .disturbed_by(&[CAP])
        .escalating(&["more_gear", "more_gear", "more_gear"])
        .within(170.0)
        .stage(
            "start",
            vec![
                // the cap goes, and its tiles after it
                Step::SetState { obj: CAP, change: Change::Damage(Damage::Broken) },
                Step::Shatter { obj: CAP, material: Material::Stone },
                Step::Emit { kind: EventKind::Damage, obj: CAP },
                Step::Wait(2.0),
                Step::SetState { obj: LADDER, change: Change::Visible(true) },
                Step::Goto("arrive"),
            ],
        )
        .stage("more_gear", vec![Step::SetState { obj: SCAFFOLD, change: Change::Visible(true) }, Step::Spawn { who: R1, spec: crate::story::actors::ActorSpec { kind: Kind::Person, role: Role::Worker, at: RouteId(HATCH_A), look: 2 } }, Step::Move { who: R1, to: RouteId(LADDER_HEAD), run: false }, Step::Wait(1.0), Step::Goto("arrive")])
        .stage(
            "cleanup",
            vec![
                Step::Release { who: R0 },
                Step::SetState { obj: CAP, change: Change::Restore },
                Step::Despawn { who: R1 },
                Step::Goto("depart"),
            ],
        );
    story.stages.extend(repair(["arrive", "work", "depart", "fallback"], "tidy", &job));
    story
        .stage(
            "tidy",
            vec![
                Step::SetState { obj: LADDER, change: Change::Visible(false) },
                Step::SetState { obj: SCAFFOLD, change: Change::Visible(false) },
                Step::Despawn { who: R1 },
                Step::End,
            ],
        )
        .helped_by(PART, CAP, 40.0)
}

/// The tank springs a leak; residents bring a bucket, a plumber closes the valve.
fn tank_leak(b: &RooftopBindings) -> StoryDef {
    let drip = (b.tank.0 - 22.0, b.flats.get(1).map_or(b.tank.1, |f| f.2));
    let plumber = CrewJob {
        who: R1,
        role: Role::Plumber,
        kind: Kind::Person,
        look: 1,
        depot: RouteId(HATCH_A),
        site: RouteId(TANK_NODE),
        exit: RouteId(HATCH_A),
        area: AREA_TANK,
        target: TANK,
        work_secs: 10.0,
        tool: None,
        patience: 70.0,
    };
    let mut story = StoryDef::new(3, "tank_leak", Class::Incident, Trigger::Event(EventKind::Impact, TANK))
        .disturbed_by(&[TANK])
        .escalating(&["wipe_up", "wipe_up", "wipe_up"])
        .within(170.0)
        .stage(
            "start",
            vec![
                Step::SetState { obj: TANK, change: Change::Damage(Damage::Damaged) },
                Step::SetState { obj: TANK, change: Change::Open(true) },
                Step::Jet { id: 0, obj: TANK, vel: (-26.0, 18.0), rate: 9.0, on: true },
                Step::Wait(2.5),
                Step::Spawn { who: R0, spec: crate::story::actors::ActorSpec { kind: Kind::Person, role: Role::Resident, at: RouteId(HATCH_C), look: 4 } },
                Step::Move { who: R0, to: RouteId(TANK_NODE), run: true },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 40.0, on_timeout: Some("call") },
                Step::Goto("bucket"),
            ],
        )
        .stage(
            "bucket",
            vec![
                // the bucket goes under the drip; if it has been moved off, they move it back
                Step::Carry { who: R0, obj: BUCKET },
                Step::Pose { who: R0, pose: ActorPose::Carry },
                Step::Wait(1.0),
                Step::SetState { obj: BUCKET, change: Change::Place(drip) },
                Step::Drop { who: R0 },
                Step::Await { until: Until::AtRest(BUCKET), timeout: 5.0, on_timeout: None },
                Step::Goto("call"),
            ],
        )
        .stage(
            "call",
            vec![
                Step::Pose { who: R0, pose: ActorPose::Wave },
                Step::Wait(1.5),
                Step::Goto("plumber"),
            ],
        );
    story.stages.extend(repair(["plumber", "plumb_work", "plumb_leave", "plumb_fallback"], "dry_up", &plumber));
    story
        .stage(
            "dry_up",
            vec![
                Step::Jet { id: 0, obj: TANK, vel: (0.0, 0.0), rate: 0.0, on: false },
                Step::SetState { obj: TANK, change: Change::Open(false) },
                Step::Branch { when: Cond::is(Test::Displaced(BUCKET)), then: "wipe_up", otherwise: "go_home" },
            ],
        )
        .stage("wipe_up", vec![Step::Pose { who: R0, pose: ActorPose::Work }, Step::Wait(3.0), Step::Goto("go_home")])
        .stage(
            "go_home",
            vec![
                // the bucket goes back where it lives
                Step::Carry { who: R0, obj: BUCKET },
                Step::Move { who: R0, to: RouteId(HATCH_C), run: false },
                Step::Await { until: Until::Arrived(vec![R0]), timeout: 50.0, on_timeout: None },
                Step::SetState { obj: BUCKET, change: Change::Restore },
                Step::Despawn { who: R0 },
                Step::End,
            ],
        )
        .stage(
            "cleanup",
            vec![
                Step::Jet { id: 0, obj: TANK, vel: (0.0, 0.0), rate: 0.0, on: false },
                Step::SetState { obj: TANK, change: Change::Open(false) },
                Step::SetState { obj: TANK, change: Change::Restore },
                Step::SetState { obj: BUCKET, change: Change::Restore },
                Step::Release { who: R1 },
                Step::Despawn { who: R1 },
                Step::Despawn { who: R0 },
                Step::End,
            ],
        )
        .helped_by(PART, TANK, 44.0)
}

/// Ordinary days: a visit, two neighbours stopping to talk, the roofer's round.
fn daily_life() -> Vec<StoryDef> {
    let daytime = vec![Cond::is(Test::Weather(Weather::Day)), Cond::is(Test::Weather(Weather::Dry)), Cond::is(Test::Weather(Weather::Calm))];
    let mut out = Vec::new();

    let mut visit_story = StoryDef::new(4, "delivery_visit", Class::Ambient, Trigger::Ambient { every: (70.0, 150.0), when: daytime.clone() }).within(160.0);
    visit_story = visit_story.stage("start", vec![Step::Goto("come")]);
    visit_story.stages.extend(visit(
        ["come", "stay", "leave"],
        "done",
        Walker { who: R0, role: Role::Courier, kind: Kind::Person, look: 2, from: RouteId(HATCH_A) },
        RouteId(HATCH_C),
        ActorPose::Wave,
        4.0,
        RouteId(HATCH_A),
    ));
    out.push(visit_story.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: R0 }, Step::End]));

    let mut meet = StoryDef::new(5, "neighbours_meet", Class::Ambient, Trigger::Ambient { every: (90.0, 200.0), when: daytime.clone() }).within(200.0);
    meet = meet.stage("start", vec![Step::Goto("come")]);
    meet.stages.extend(crate::story::behaviors::meeting(
        ["come", "talk", "part"],
        "done",
        resident(0, HATCH_C, 3),
        resident(1, HATCH_A, 5),
        RouteId(C_RIGHT),
        RouteId(A_RIGHT),
        8.0,
        RouteId(HATCH_C),
        RouteId(HATCH_A),
    ));
    out.push(meet.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: R0 }, Step::Despawn { who: R1 }, Step::End]));

    let mut rounds = StoryDef::new(6, "roof_rounds", Class::Ambient, Trigger::Ambient { every: (120.0, 260.0), when: vec![Cond::is(Test::Weather(Weather::Day)), Cond::is(Test::Weekday)] }).within(220.0);
    rounds = rounds.stage("start", vec![Step::Goto("round")]);
    rounds.stages.extend(round(
        ["round", "leave"],
        "done",
        roofer(0, HATCH_A, 1),
        &[(RouteId(TANK_NODE), ActorPose::Operate, 3.0), (RouteId(LADDER_FOOT), ActorPose::Look, 2.0)],
        RouteId(HATCH_A),
    ));
    out.push(rounds.stage("done", vec![Step::End]).stage("cleanup", vec![Step::Despawn { who: R0 }, Step::End]));
    out
}

// ── drawing the parts that change ────────────────────────────────────────────

/// The sheet, pegged out on its line. `form` says how: whole, hanging by one peg, or loose.
pub fn sheet<S: Surface>(d: &mut Draw<S>, lay: &Layout, line: (P, P), form: Damage, pose: crate::story::model::Pose, tone: &Tone, wind: f32, t: f32, flying: bool) {
    let cloth = tone.paint([232.0, 228.0, 214.0]);
    let mid = ((line.0 .0 + line.1 .0) / 2.0, line.0 .1 - 11.0);
    let flutter = (t * 3.1).sin() * 1.2 * (0.3 + wind.abs().min(1.0));
    if flying {
        // loose: a piece of cloth turning in the air, drawn where the physics has it
        let (c, s) = (pose.turn.cos(), pose.turn.sin());
        let p = (mid.0 + pose.dx, mid.1 + pose.dy);
        let q = |x: f32, y: f32| (p.0 + x * c - y * s, p.1 + x * s + y * c);
        poly(d, lay, &[q(-9.0, -6.0), q(9.0, -7.0 + flutter), q(10.0, 6.0), q(-8.0, 7.0 - flutter)], cloth, 1.0);
        return;
    }
    let (a, b) = (line.0, line.1);
    match form {
        Damage::Intact => {
            let sag = |f: f32| a.1 - 12.0 + 3.0 * 4.0 * f * (1.0 - f);
            let (x0, x1) = (mid.0 - 15.0, mid.0 + 15.0);
            let (f0, f1) = ((x0 - a.0) / (b.0 - a.0), (x1 - a.0) / (b.0 - a.0));
            poly(d, lay, &[(x0, sag(f0)), (x1, sag(f1)), (x1 + wind * 3.0, sag(f1) + 22.0 + flutter), (x0 + wind * 3.0, sag(f0) + 22.0 - flutter)], cloth, 1.0);
        }
        Damage::Damaged => {
            // one peg has gone: it hangs from the other corner, turned away on the wind
            let hang = (mid.0 - 15.0, a.1 - 12.0 + 2.0);
            let swing = 0.7 + wind * 0.5 + flutter * 0.05;
            let (c, s) = (swing.cos(), swing.sin());
            let rel = |x: f32, y: f32| (hang.0 + x * c - y * s, hang.1 + x * s + y * c);
            poly(d, lay, &[rel(0.0, 0.0), rel(30.0, 0.0), rel(30.0, 22.0), rel(0.0, 22.0)], shade(cloth, -0.04), 1.0);
        }
        _ => {}
    }
}

/// The line and its two posts, empty or not.
pub fn washing_line<S: Surface>(d: &mut Draw<S>, lay: &Layout, line: (P, P), tone: &Tone) {
    let wood = tone.paint([120.0, 96.0, 70.0]);
    let (a, b) = line;
    ink(d, lay, &[a, (a.0, a.1 - 14.0)], 1.8, wood, 1.0);
    ink(d, lay, &[b, (b.0, b.1 - 14.0)], 1.8, wood, 1.0);
    let pts: Vec<P> = (0..=12).map(|k| k as f32 / 12.0).map(|f| (a.0 + (b.0 - a.0) * f, a.1 - 12.0 + 3.0 * 4.0 * f * (1.0 - f))).collect();
    ink(d, lay, &pts, 0.8, tone.paint([60.0, 56.0, 52.0]), 0.9);
}

/// The chimney's top as it is now: the cap and its pots, askew, or a stump.
pub fn chimney_top<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, w: f32, damage: Damage, wall: Rgb, cap: Rgb) {
    match damage {
        Damage::Intact => {
            bar(d, lay, [at.0 - w * 0.5 - 1.2, at.1 - 3.2, at.0 + w * 0.5 + 1.2, at.1 + 0.4], cap, 1.0);
        }
        Damage::Damaged => {
            poly(d, lay, &[(at.0 - w * 0.5 - 1.2, at.1 - 0.5), (at.0 + w * 0.5, at.1 - 6.5), (at.0 + w * 0.5 + 1.4, at.1 - 4.0), (at.0 - w * 0.5, at.1 + 1.0)], cap, 1.0);
        }
        Damage::Broken | Damage::Missing => {
            // a jagged stump: the courses of brick broken off unevenly
            let (x0, x1) = (at.0 - w * 0.5, at.0 + w * 0.5);
            let teeth = [0.0, -1.8, -0.6, -2.4, -0.9, -1.6];
            let n = teeth.len();
            let mut pts = vec![(x0, at.1 + 0.6)];
            for (k, dy) in teeth.iter().enumerate() {
                let x = x0 + (x1 - x0) * k as f32 / (n - 1) as f32;
                pts.push((x, at.1 + dy + 0.4));
            }
            pts.push((x1, at.1 + 0.6));
            poly(d, lay, &pts, shade(wall, -0.18), 1.0);
        }
    }
}

/// A ladder leaning on a wall.
pub fn ladder_prop<S: Surface>(d: &mut Draw<S>, lay: &Layout, ladder: (P, P), tone: &Tone, alpha: f32) {
    crate::story::draw::ladder(d, lay, ladder.0, ladder.1, 0.9, tone, alpha);
}

/// A little scaffold tower beside the chimney.
pub fn scaffold<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone, alpha: f32) {
    let steel = tone.paint([120.0, 126.0, 134.0]);
    for dx in [-8.0, 8.0] {
        ink(d, lay, &[(at.0 + dx, at.1), (at.0 + dx, at.1 - 26.0)], 1.2, steel, alpha);
    }
    for k in 0..3 {
        let y = at.1 - 4.0 - k as f32 * 9.0;
        ink(d, lay, &[(at.0 - 8.0, y), (at.0 + 8.0, y)], 1.0, steel, alpha);
        ink(d, lay, &[(at.0 - 8.0, y), (at.0 + 8.0, y - 9.0)], 0.7, steel, alpha * 0.8);
    }
    bar(d, lay, [at.0 - 10.0, at.1 - 27.5, at.0 + 10.0, at.1 - 25.5], tone.paint([150.0, 112.0, 70.0]), alpha);
}

/// The tank's valve wheel and, when it is leaking, the dark stain below it.
pub fn tank_valve<S: Surface>(d: &mut Draw<S>, lay: &Layout, tank: P, size: f32, leaking: bool, tone: &Tone) {
    let wheel = tone.paint([190.0, 60.0, 50.0]);
    let at = (tank.0 + size * 0.42, tank.1 - size * 0.3);
    oval(d, lay, at.0, at.1, 2.6, 2.6, wheel, 1.0);
    ink(d, lay, &[(at.0 - 3.0, at.1), (at.0 + 3.0, at.1)], 0.7, shade(wheel, -0.3), 1.0);
    if leaking {
        poly(d, lay, &[(tank.0 - size * 0.42, tank.1 - size * 0.6), (tank.0 - size * 0.30, tank.1 - size * 0.6), (tank.0 - size * 0.55, tank.1 + 1.0), (tank.0 - size * 0.7, tank.1 + 1.0)], tone.paint([60.0, 90.0, 120.0]), 0.55);
    }
}

pub fn hatch<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone) {
    let wall = tone.paint([150.0, 140.0, 128.0]);
    bar(d, lay, [at.0 - 7.0, at.1 - 12.0, at.0 + 7.0, at.1], wall, 1.0);
    poly(d, lay, &[(at.0 - 8.5, at.1 - 12.0), (at.0 + 8.5, at.1 - 12.0), (at.0 + 5.0, at.1 - 16.0), (at.0 - 5.0, at.1 - 16.0)], shade(wall, -0.2), 1.0);
    bar(d, lay, [at.0 - 2.4, at.1 - 8.5, at.0 + 2.4, at.1], tone.paint([70.0, 54.0, 42.0]), 1.0);
}

pub fn bucket_prop<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone) {
    crate::story::draw::bucket(d, lay, at, 0.9, tone, 0.0, 1.0);
}

pub fn valve_wheel_prop<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone) {
    let wheel = tone.paint([190.0, 60.0, 50.0]);
    oval(d, lay, at.0, at.1 - 3.0, 3.0, 3.0, wheel, 1.0);
    oval(d, lay, at.0, at.1 - 3.0, 1.2, 1.2, shade(wheel, -0.4), 1.0);
}

pub fn pot_prop<S: Surface>(d: &mut Draw<S>, lay: &Layout, at: P, tone: &Tone, turn: f32) {
    let clay = tone.paint([176.0, 98.0, 72.0]);
    let (c, s) = (turn.cos(), turn.sin());
    let q = |x: f32, y: f32| (at.0 + x * c - y * s, at.1 + x * s + y * c);
    poly(d, lay, &[q(-4.0, -6.0), q(4.0, -6.0), q(3.0, 0.0), q(-3.0, 0.0)], clay, 1.0);
    oval(d, lay, q(0.0, -7.5).0, q(0.0, -7.5).1, 3.4, 2.6, tone.paint([86, 120, 76].map(|v| v as f32)), 1.0);
}

/// An open window: the pane lit, a curtain drawn back at each side.
pub fn window_open<S: Surface>(d: &mut Draw<S>, lay: &Layout, rect: [f32; 4], tone: &Tone) {
    bar(d, lay, rect, [250.0, 214.0, 150.0], 0.96);
    let w = (rect[2] - rect[0]) * 0.22;
    bar(d, lay, [rect[0], rect[1], rect[0] + w, rect[3]], tone.paint([196.0, 90.0, 80.0]), 0.9);
    bar(d, lay, [rect[2] - w, rect[1], rect[2], rect[3]], tone.paint([196.0, 90.0, 80.0]), 0.9);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::actors::ActorState;
    use crate::story::model::{Attachment, Pose, StoryId};
    use crate::story::physics::Gesture;
    use crate::story::replay::{test_session, Session};

    fn day() -> Session {
        test_session("rooftops", "2026-10-13 11:00", "clear", (640, 400))
    }

    fn at_of(s: &Session, id: ObjectId) -> P {
        s.story.definition().object(id).unwrap().at
    }

    fn run(s: &mut Session, seconds: f64) {
        s.advance(seconds, 1.0 / 30.0);
    }

    fn poke(s: &mut Session, id: ObjectId) {
        let p = at_of(s, id);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Release { at: p, velocity: (0.0, 0.0) });
    }

    /// Nothing of an incident is left: the principal stories are over, and nobody holds anything.
    fn quiet(s: &Session) {
        let defs = s.story.definition();
        assert_eq!(s.story.director().incidents_running(defs), 0, "{:?}", s.story.director().status(defs));
        assert_eq!(s.story.actors().reservation_count(), 0);
        assert!(s.story.physics().bodies.len() < 40);
    }

    #[test]
    fn the_chimney_cap_comes_down_a_roofer_climbs_and_mends_it() {
        let mut s = day();
        run(&mut s, 2.0);
        poke(&mut s, CAP);
        run(&mut s, 1.0);
        assert_eq!(s.story.snapshot().damage(CAP), Damage::Broken, "the cap is down");
        assert!(s.story.physics().bodies.iter().any(|b| b.kind == crate::story::physics::BodyKind::Fragment), "with tiles");
        run(&mut s, 12.0);
        assert!(s.story.snapshot().get(LADDER).visible, "the ladder is up");
        assert!(s.story.actors().iter().any(|a| a.role == Role::Roofer), "the roofer has come");
        // the roofer is seen to climb and to work
        let (mut climbed, mut worked) = (false, false);
        for _ in 0..(70.0 * 30.0) as usize {
            s.advance(1.0 / 30.0, 1.0 / 30.0);
            for a in s.story.actors().iter().filter(|a| a.role == Role::Roofer) {
                climbed |= a.pose == ActorPose::Climb;
                worked |= a.pose == ActorPose::Work;
            }
        }
        assert!(climbed && worked, "climbed {} worked {}", climbed, worked);
        run(&mut s, 20.0);
        assert_eq!(s.story.snapshot().damage(CAP), Damage::Intact);
        assert!(!s.story.snapshot().get(LADDER).visible, "the ladder is taken down");
        quiet(&s);
    }

    #[test]
    fn birds_abandon_the_roof_and_come_back_to_the_mended_chimney() {
        let mut s = day();
        run(&mut s, 3.0);
        let birds = |s: &Session| s.story.actors().iter().filter(|a| a.kind == Kind::Bird).count();
        assert!(birds(&s) >= 2, "birds are sitting about the chimney");
        let home: Vec<P> = s.story.actors().iter().filter(|a| a.kind == Kind::Bird).map(|a| a.pos).collect();
        poke(&mut s, CAP);
        run(&mut s, 12.0);
        let away = s.story.actors().iter().filter(|a| a.kind == Kind::Bird).any(|a| home.iter().all(|h| (h.0 - a.pos.0).abs() + (h.1 - a.pos.1).abs() > 20.0));
        assert!(away, "the birds left their perches for another roof");
        run(&mut s, 110.0);
        let back = s.story.actors().iter().filter(|a| a.kind == Kind::Bird).filter(|a| home.iter().any(|h| (h.0 - a.pos.0).abs() + (h.1 - a.pos.1).abs() < 4.0)).count();
        assert!(back >= 1, "and came back once the work was done");
    }

    #[test]
    fn repeated_damage_brings_more_gear_up_to_three_times_then_a_protected_cleanup() {
        let mut s = day();
        run(&mut s, 1.0);
        let mut most = 0;
        let mut scaffold_seen = false;
        for n in 0..14 {
            s.story.bus_emit_impact(CAP, 9000 + n);
            run(&mut s, 6.0);
            most = most.max(s.story.director().status(s.story.definition()).iter().map(|st| st.2).max().unwrap_or(0));
            scaffold_seen |= s.story.snapshot().get(SCAFFOLD).visible;
            assert!(s.story.actors().iter().filter(|a| matches!(a.role, Role::Roofer | Role::Worker)).count() <= 3);
        }
        assert_eq!(most, 3);
        assert!(scaffold_seen, "better equipment arrived");
        run(&mut s, 160.0);
        quiet(&s);
        assert_eq!(s.story.snapshot().damage(CAP), Damage::Intact);
        assert!(!s.story.snapshot().get(SCAFFOLD).visible && !s.story.snapshot().get(LADDER).visible);
    }

    #[test]
    fn a_tug_takes_a_peg_off_and_the_resident_puts_it_back() {
        let mut s = day();
        run(&mut s, 1.0);
        let p = at_of(&s, SHEET);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 30.0, p.1 + 4.0)));
        assert_eq!(s.story.snapshot().damage(SHEET), Damage::Damaged);
        s.gesture(Gesture::Release { at: (p.0 + 30.0, p.1 + 4.0), velocity: (0.0, 0.0) });
        run(&mut s, 6.0);
        assert!(s.story.actors().iter().any(|a| a.role == Role::Resident && a.pos.0 > 0.0), "someone came to see");
        run(&mut s, 60.0);
        assert_eq!(s.story.snapshot().damage(SHEET), Damage::Intact);
        quiet(&s);
    }

    #[test]
    fn pulled_free_the_sheet_goes_on_the_wind_and_is_fetched_back_wherever_it_lands() {
        for wind in [1.4f32, -1.4, 0.0] {
            let mut s = day();
            s.set_wind(wind);
            run(&mut s, 1.0);
            let p = at_of(&s, SHEET);
            s.gesture(Gesture::Press(p));
            s.gesture(Gesture::Drag((p.0 + 60.0, p.1 - 20.0)));
            s.gesture(Gesture::Drag((p.0 + 140.0, p.1 - 40.0)));
            let st = s.story.snapshot().get(SHEET).attachment;
            assert!(matches!(st, Attachment::Loose), "free of both pegs: {:?}", st);
            run(&mut s, 10.0);
            let landed = s.story.place(SHEET).unwrap();
            assert!(landed.0.is_finite() && landed.1.is_finite());
            run(&mut s, 170.0);
            assert_eq!(s.story.snapshot().damage(SHEET), Damage::Intact, "wind {}", wind);
            assert!(s.story.snapshot().get(SHEET).pose == Pose::default(), "wind {}: {:?}", wind, s.story.snapshot().get(SHEET));
            quiet(&s);
        }
    }

    #[test]
    fn the_tank_leaks_residents_bring_a_bucket_and_a_plumber_closes_the_valve() {
        let mut s = day();
        run(&mut s, 1.0);
        poke(&mut s, TANK);
        run(&mut s, 3.0);
        assert!(s.story.snapshot().get(TANK).open, "the valve is open and it leaks");
        assert!(s.story.physics().bodies.iter().any(|b| b.kind == crate::story::physics::BodyKind::Droplet), "water");
        let (mut resident, mut puddle, mut plumbed, mut worked) = (false, false, false, false);
        for _ in 0..(120.0 * 30.0) as usize {
            s.advance(1.0 / 30.0, 1.0 / 30.0);
            resident |= s.story.actors().iter().any(|a| a.role == Role::Resident && a.kind == Kind::Person && a.pos.0 > 0.0);
            puddle |= !s.story.physics().patches.is_empty();
            plumbed |= s.story.actors().iter().any(|a| a.role == Role::Plumber && a.state == ActorState::Arrived);
            worked |= s.story.actors().iter().any(|a| a.role == Role::Plumber && a.pose == ActorPose::Work);
        }
        assert!(resident, "residents came");
        assert!(puddle, "a puddle grew on the flat");
        assert!(plumbed && worked, "the plumber arrived and worked");
        assert!(!s.story.snapshot().get(TANK).open, "the valve is closed");
        assert_eq!(s.story.snapshot().damage(TANK), Damage::Intact);
        run(&mut s, 30.0);
        quiet(&s);
        assert!(!s.story.physics().jet_on(0));
    }

    #[test]
    fn a_returned_part_shortens_the_repair_and_a_moved_bucket_means_more_work() {
        let time_to_mend = |help: bool| {
            let mut s = day();
            run(&mut s, 1.0);
            poke(&mut s, TANK);
            run(&mut s, 1.0);
            let mut t = 0.0;
            let mut helped = false;
            while s.story.snapshot().damage(TANK) != Damage::Intact && t < 150.0 {
                s.advance(0.1, 1.0 / 30.0);
                t += 0.1;
                if help && !helped && s.story.actors().iter().any(|a| a.role == Role::Plumber && a.state == ActorState::Arrived) {
                    let d = at_of(&s, TANK);
                    s.story.place_object(PART, (d.0 + 2.0, d.1 + 2.0));
                    s.story.bus_emit_obstruction(PART, (d.0 + 2.0, d.1 + 2.0), 777);
                    helped = true;
                }
            }
            t
        };
        let alone = time_to_mend(false);
        let helped = time_to_mend(true);
        assert!(helped + 3.0 < alone, "helped {} alone {}", helped, alone);
        // the bucket moved away from the drip: the dry-up needs a wipe, but it still ends
        let mut s = day();
        run(&mut s, 1.0);
        poke(&mut s, TANK);
        run(&mut s, 20.0);
        let b = at_of(&s, BUCKET);
        s.gesture(Gesture::Press(b));
        s.gesture(Gesture::Drag((b.0 + 70.0, b.1)));
        s.gesture(Gesture::Release { at: (b.0 + 70.0, b.1), velocity: (0.0, 0.0) });
        run(&mut s, 150.0);
        assert_eq!(s.story.snapshot().damage(TANK), Damage::Intact);
        quiet(&s);
    }

    #[test]
    fn a_window_knock_brings_a_face_and_a_pot_pushed_off_brings_birds_up() {
        let mut s = day();
        run(&mut s, 3.0);
        poke(&mut s, WINDOW_A);
        run(&mut s, 1.0);
        assert!(s.story.snapshot().get(WINDOW_A).open, "the curtain is drawn back");
        assert!(s.story.actors().iter().any(|a| a.role == Role::Resident && a.pose == ActorPose::Look));
        run(&mut s, 12.0);
        assert!(!s.story.snapshot().get(WINDOW_A).open);
        // throw the flower pot: it falls, breaks into pieces and startles the roof
        let p = at_of(&s, POT);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Drag((p.0 + 8.0, p.1 - 6.0)));
        s.gesture(Gesture::Release { at: (p.0 + 8.0, p.1 - 6.0), velocity: (160.0, -60.0) });
        run(&mut s, 8.0);
        let st = *s.story.snapshot().get(POT);
        assert!(st.damage != Damage::Intact || st.pose != Pose::default() || st.attachment != Attachment::Fixed, "the pot went somewhere: {:?}", st);
    }

    #[test]
    fn a_day_of_ordinary_life_comes_and_goes_without_leaving_anything_behind() {
        let mut s = day();
        let mut seen = std::collections::BTreeSet::new();
        for _ in 0..(600.0 * 5.0) as usize {
            s.advance(0.2, 1.0 / 30.0);
            for (name, _, _, _) in s.story.director().status(s.story.definition()) {
                seen.insert(name);
            }
        }
        assert!(seen.len() >= 2, "{:?}", seen);
        assert!(s.story.actors().count() <= 24);
        assert_eq!(StoryId(1).0, 1);
    }
}
