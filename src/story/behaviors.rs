//! Reusable behaviors: what birds, cats, people at windows and repair crews do.
//!
//! These are shared by every view. A view does not copy them, it says where:
//! which places are perches, which window has someone behind it, where a
//! crew starts from and where it works. A behavior reads a bounded, read-only
//! look at the world and answers with typed commands; it never touches state
//! directly, so it obeys the same ownership, timing and limits as a scripted
//! sequence.
//!
//! Unusual behaviors implement `Behavior` themselves. A behavior that spends
//! more than its command budget is stopped and noted, not trusted.

use super::actors::{Actor, ActorCommand, ActorPose, ActorSpec, ActorState, ActorSystem, Kind, Role};
use super::director::{Change, Facts, Label, Slot, Stage, Step, Until};
use super::events::{Event, EventKind};
use super::model::{ActorId, AreaId, Definition, ObjectId, RouteId, Snapshot, StoryId};
use super::physics::{TAG_GLASS, TAG_METAL};
use crate::gfx::P;

/// Commands one behavior may issue in one tick.
pub const COMMAND_BUDGET: usize = 32;
/// Actor slots a behavior may hold.
pub const MINE: usize = 8;

#[derive(Clone, Debug, PartialEq)]
pub enum Command {
    Actor(ActorId, ActorCommand),
    Spawn { slot: u8, spec: ActorSpec },
    Despawn(u8),
    Object(ObjectId, Change),
    Emit(Event),
    Trigger(StoryId),
}

/// Where a behavior leaves its commands. Bounded: what does not fit is counted.
pub struct CommandBuffer {
    commands: Vec<Command>,
    pub refused: u32,
}

impl CommandBuffer {
    pub fn new() -> CommandBuffer {
        CommandBuffer { commands: Vec::with_capacity(COMMAND_BUDGET), refused: 0 }
    }

    pub fn push(&mut self, c: Command) -> bool {
        if self.commands.len() >= COMMAND_BUDGET {
            self.refused += 1;
            false
        } else {
            self.commands.push(c);
            true
        }
    }

    pub fn len(&self) -> usize {
        self.commands.len()
    }

    pub fn is_empty(&self) -> bool {
        self.commands.is_empty()
    }

    pub fn take(&mut self) -> Vec<Command> {
        std::mem::take(&mut self.commands)
    }
}

impl Default for CommandBuffer {
    fn default() -> Self {
        CommandBuffer::new()
    }
}

/// What a behavior may look at.
pub struct BehaviorContext<'a> {
    pub defs: &'a Definition,
    pub objects: Snapshot<'a>,
    pub actors: &'a ActorSystem,
    pub facts: &'a Facts,
    pub tick: u64,
    /// the events this tick
    pub events: &'a [Event],
    /// the actors this behavior has spawned, by its own slot numbers
    pub mine: &'a [Option<ActorId>; MINE],
    /// a number that is the same every run of the same seed
    pub seed: u64,
}

impl BehaviorContext<'_> {
    pub fn actor(&self, slot: u8) -> Option<&Actor> {
        self.mine.get(slot as usize).copied().flatten().and_then(|id| self.actors.get(id))
    }

    pub fn seconds(&self) -> f64 {
        self.tick as f64 * super::events::STEP
    }

    /// Where an object is now.
    pub fn place(&self, obj: ObjectId) -> Option<P> {
        let d = self.defs.object(obj)?;
        let s = self.objects.get(obj);
        Some((d.at.0 + s.pose.dx, d.at.1 + s.pose.dy))
    }

    /// The node an event is nearest to, among these places.
    pub fn nearest(&self, at: P, among: &[RouteId]) -> Option<(RouteId, f32)> {
        among
            .iter()
            .filter_map(|id| self.defs.routes.node(*id).map(|n| (*id, ((n.at.0 - at.0).powi(2) + (n.at.1 - at.1).powi(2)).sqrt())))
            .min_by(|a, b| a.1.partial_cmp(&b.1).unwrap_or(std::cmp::Ordering::Equal))
    }
}

pub trait Behavior {
    fn name(&self) -> &'static str;

    /// Look at the world and leave commands. Called every tick while `asleep` is false, and
    /// when an event arrives.
    fn update(&mut self, ctx: &BehaviorContext, out: &mut CommandBuffer);

    /// Nothing to do until an event or the clock wakes it: costs no work.
    fn asleep(&self) -> bool {
        false
    }

    /// The next tick it wants to be called at, when it is not event-driven.
    fn wake_at(&self) -> Option<u64> {
        None
    }
}

/// A custom behavior is built from a plain function, so a `Definition` stays copyable.
pub type Factory = fn() -> Box<dyn Behavior>;

// ── birds on a perch ─────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Perch {
    /// the places birds sit
    pub perches: Vec<RouteId>,
    /// where they go instead while it is not safe at home
    pub alternates: Vec<RouteId>,
    /// how near a disturbance has to be to put them up
    pub radius: f32,
    /// how long they stay away, in seconds, once it is quiet
    pub away: f32,
    /// they will not come back to a place while this area is held (a worksite)
    pub keep_off: Option<AreaId>,
    /// the roof or tree is out of use in this weather
    pub shy_of: Vec<super::director::Weather>,
}

enum PerchState {
    Away,
    Sitting,
    /// away; `landed` is when they all reached the other roof
    Fled { landed: Option<u64> },
}

pub struct PerchBehavior {
    spec: Perch,
    state: PerchState,
    flight: u32,
}

impl PerchBehavior {
    pub fn new(spec: Perch) -> PerchBehavior {
        PerchBehavior { spec, state: PerchState::Away, flight: 0 }
    }

    fn weather_ok(&self, f: &Facts) -> bool {
        !f.night && !f.storm && !f.rain && f.wind < 40.0 && !self.spec.shy_of.iter().any(|w| f.holds(*w))
    }
}

impl Behavior for PerchBehavior {
    fn name(&self) -> &'static str {
        "perch"
    }

    fn update(&mut self, ctx: &BehaviorContext, out: &mut CommandBuffer) {
        let n = self.spec.perches.len().min(MINE);
        let ok = self.weather_ok(ctx.facts);
        match self.state {
            PerchState::Away => {
                if ok {
                    for (i, p) in self.spec.perches.iter().take(n).enumerate() {
                        out.push(Command::Spawn { slot: i as u8, spec: ActorSpec { kind: Kind::Bird, role: Role::Resident, at: *p, look: i as u32 } });
                    }
                    self.state = PerchState::Sitting;
                }
            }
            PerchState::Sitting => {
                if !ok {
                    for i in 0..n {
                        out.push(Command::Despawn(i as u8));
                    }
                    self.state = PerchState::Away;
                    return;
                }
                // anything loud or falling near a perch puts them up
                let startled = ctx.events.iter().find(|e| {
                    matches!(e.kind, EventKind::Impact | EventKind::Damage | EventKind::Noise | EventKind::Obstruction)
                        && self.spec.perches.iter().any(|p| ctx.defs.routes.node(*p).is_some_and(|node| (node.at.0 - e.at.0).powi(2) + (node.at.1 - e.at.1).powi(2) <= self.spec.radius * self.spec.radius))
                });
                if startled.is_some() {
                    self.flight += 1;
                    for i in 0..n {
                        let alt = self.spec.alternates.get((i + self.flight as usize) % self.spec.alternates.len().max(1)).copied();
                        if let (Some(a), Some(to)) = (ctx.mine[i], alt) {
                            out.push(Command::Actor(a, ActorCommand::Move(to)));
                        }
                    }
                    self.state = PerchState::Fled { landed: None };
                }
            }
            PerchState::Fled { landed } => {
                let quiet = (self.spec.away as f64 / super::events::STEP) as u64;
                // disturbed again while away: stay away a little longer
                let again = ctx.events.iter().any(|e| matches!(e.kind, EventKind::Impact | EventKind::Damage | EventKind::Noise) && e.tags & (1 << 7) == 0);
                if again {
                    self.state = PerchState::Fled { landed: landed.map(|_| ctx.tick) };
                    return;
                }
                // the time away counts from the landing, not from the take-off
                let down = (0..n).all(|i| ctx.actor(i as u8).is_none_or(|a| !matches!(a.state, ActorState::Moving | ActorState::Waiting)));
                let landed = match landed {
                    Some(t) => t,
                    None if down => {
                        self.state = PerchState::Fled { landed: Some(ctx.tick) };
                        return;
                    }
                    None => return,
                };
                let held = self.spec.keep_off.is_some_and(|a| ctx.actors.reserved_by(a).is_some());
                if ctx.tick >= landed + quiet && !held {
                    for (i, p) in self.spec.perches.iter().take(n).enumerate() {
                        if let Some(a) = ctx.mine[i] {
                            out.push(Command::Actor(a, ActorCommand::Move(*p)));
                        }
                    }
                    self.state = PerchState::Sitting;
                }
            }
        }
    }

    /// Sitting or away it waits for an event or a change of weather; fleeing it watches for the landing.
    fn asleep(&self) -> bool {
        !matches!(self.state, PerchState::Fled { .. })
    }

    fn wake_at(&self) -> Option<u64> {
        None
    }
}

// ── someone at a window ──────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Occupant {
    pub window: ObjectId,
    /// where the face appears
    pub at: RouteId,
    pub role: Role,
    pub look: u32,
}

enum Phase {
    Out,
    Looking { since: u64 },
    Cross { until: u64 },
}

pub struct OccupantBehavior {
    spec: Occupant,
    phase: Phase,
    knocks: u8,
    last_knock: u64,
}

impl OccupantBehavior {
    pub fn new(spec: Occupant) -> OccupantBehavior {
        OccupantBehavior { spec, phase: Phase::Out, knocks: 0, last_knock: 0 }
    }
}

impl Behavior for OccupantBehavior {
    fn name(&self) -> &'static str {
        "occupant"
    }

    fn update(&mut self, ctx: &BehaviorContext, out: &mut CommandBuffer) {
        let knocked = ctx.events.iter().any(|e| e.source == self.spec.window && matches!(e.kind, EventKind::Noise | EventKind::Impact));
        let now = ctx.tick;
        if knocked {
            // knocks that come in a rush add up; a long quiet forgets them
            if now.saturating_sub(self.last_knock) > 20 * 30 {
                self.knocks = 0;
            }
            self.knocks = self.knocks.saturating_add(1);
            self.last_knock = now;
        }
        match self.phase {
            Phase::Out => {
                if knocked {
                    out.push(Command::Spawn { slot: 0, spec: ActorSpec { kind: Kind::Person, role: self.spec.role, at: self.spec.at, look: self.spec.look } });
                    out.push(Command::Object(self.spec.window, Change::Open(true)));
                    self.phase = Phase::Looking { since: now };
                }
            }
            Phase::Looking { since } => {
                let me = ctx.mine[0];
                if self.knocks >= 3 {
                    // that is enough: a sharp look, and the shutters close
                    if let Some(a) = me {
                        out.push(Command::Actor(a, ActorCommand::Pose(ActorPose::React)));
                    }
                    out.push(Command::Object(self.spec.window, Change::Open(false)));
                    self.phase = Phase::Cross { until: now + 3 * 30 };
                    return;
                }
                if let Some(a) = me {
                    // look out first, then wave
                    let pose = if now - since < 45 { ActorPose::Look } else { ActorPose::Wave };
                    out.push(Command::Actor(a, ActorCommand::Pose(pose)));
                }
                if now - since > 5 * 30 && !knocked {
                    out.push(Command::Despawn(0));
                    out.push(Command::Object(self.spec.window, Change::Open(false)));
                    self.phase = Phase::Out;
                }
            }
            Phase::Cross { until } => {
                if now >= until {
                    out.push(Command::Despawn(0));
                    self.phase = Phase::Out;
                    self.knocks = 0;
                }
            }
        }
    }

    fn wake_at(&self) -> Option<u64> {
        match self.phase {
            Phase::Cross { until } => Some(until),
            Phase::Looking { since } => Some(since + 5 * 30 + 1),
            Phase::Out => None,
        }
    }

    fn asleep(&self) -> bool {
        matches!(self.phase, Phase::Out)
    }
}

// ── a cat that notices ───────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Animal {
    pub kind: Kind,
    pub home: RouteId,
    /// places it will go to look at something
    pub haunts: Vec<RouteId>,
    /// how far off it notices, in design units
    pub attention: f32,
    pub look: u32,
    /// seconds it stays out before going home
    pub linger: f32,
}

enum Mood {
    Absent,
    Home,
    Looking,
    /// gone to see; `landed` is when it got there
    Out { landed: Option<u64> },
}

pub struct AnimalBehavior {
    spec: Animal,
    mood: Mood,
}

impl AnimalBehavior {
    pub fn new(spec: Animal) -> AnimalBehavior {
        AnimalBehavior { spec, mood: Mood::Absent }
    }
}

impl Behavior for AnimalBehavior {
    fn name(&self) -> &'static str {
        "animal"
    }

    fn update(&mut self, ctx: &BehaviorContext, out: &mut CommandBuffer) {
        let fine = !ctx.facts.rain && !ctx.facts.snow && !ctx.facts.storm;
        let Some(home) = ctx.defs.routes.node(self.spec.home).map(|n| n.at) else { return };
        match self.mood {
            Mood::Absent => {
                if fine {
                    out.push(Command::Spawn { slot: 0, spec: ActorSpec { kind: self.spec.kind, role: Role::Resident, at: self.spec.home, look: self.spec.look } });
                    self.mood = Mood::Home;
                }
            }
            Mood::Home | Mood::Looking | Mood::Out { .. } => {
                if !fine {
                    out.push(Command::Despawn(0));
                    self.mood = Mood::Absent;
                    return;
                }
                let Some(me) = ctx.mine[0] else { return };
                let near = |e: &Event| (e.at.0 - home.0).powi(2) + (e.at.1 - home.1).powi(2) <= self.spec.attention * self.spec.attention;
                let loud = ctx.events.iter().any(|e| e.tags & (TAG_GLASS | TAG_METAL) != 0 && matches!(e.kind, EventKind::Damage | EventKind::Impact) && near(e));
                let interesting = ctx.events.iter().find(|e| matches!(e.kind, EventKind::Obstruction | EventKind::Impact | EventKind::Noise) && e.tags & (1 << 7) == 0 && near(e));
                if loud {
                    // a bang: it is away, and home
                    out.push(Command::Actor(me, ActorCommand::Move(self.spec.home)));
                    self.mood = Mood::Home;
                } else if let Some(e) = interesting {
                    match ctx.nearest(e.at, &self.spec.haunts) {
                        Some((place, _)) => {
                            out.push(Command::Actor(me, ActorCommand::Look(e.at)));
                            out.push(Command::Actor(me, ActorCommand::Move(place)));
                            self.mood = Mood::Out { landed: None };
                        }
                        None => {
                            out.push(Command::Actor(me, ActorCommand::Look(e.at)));
                            self.mood = Mood::Looking;
                        }
                    }
                } else if let Mood::Out { landed } = self.mood {
                    let still = ctx.actor(0).is_some_and(|a| !matches!(a.state, ActorState::Moving));
                    let linger = (self.spec.linger as f64 / super::events::STEP) as u64;
                    match landed {
                        None if still => self.mood = Mood::Out { landed: Some(ctx.tick) },
                        Some(t) if ctx.tick >= t + linger => {
                            out.push(Command::Actor(me, ActorCommand::Move(self.spec.home)));
                            self.mood = Mood::Home;
                        }
                        _ => {}
                    }
                }
            }
        }
    }

    /// While it is out it watches for itself getting there; otherwise it waits for something to happen.
    fn asleep(&self) -> bool {
        !matches!(self.mood, Mood::Out { .. })
    }
}

/// The data a view hands over; the world builds the behavior from it.
#[derive(Clone, Debug)]
pub enum BehaviorSpec {
    Perch(Perch),
    Occupant(Occupant),
    Animal(Animal),
}

impl BehaviorSpec {
    pub fn build(&self) -> Box<dyn Behavior> {
        match self {
            BehaviorSpec::Perch(p) => Box::new(PerchBehavior::new(p.clone())),
            BehaviorSpec::Occupant(o) => Box::new(OccupantBehavior::new(o.clone())),
            BehaviorSpec::Animal(a) => Box::new(AnimalBehavior::new(a.clone())),
        }
    }

    /// The places and objects it refers to, for validation.
    pub fn references(&self) -> (Vec<RouteId>, Vec<ObjectId>) {
        match self {
            BehaviorSpec::Perch(p) => (p.perches.iter().chain(&p.alternates).copied().collect(), Vec::new()),
            BehaviorSpec::Occupant(o) => (vec![o.at], vec![o.window]),
            BehaviorSpec::Animal(a) => (std::iter::once(a.home).chain(a.haunts.iter().copied()).collect(), Vec::new()),
        }
    }
}

// ── a crew that comes, works, and goes ───────────────────────────────────────

/// Who comes, from where, to do what to which object.
#[derive(Clone, Copy, Debug)]
pub struct CrewJob {
    pub who: Slot,
    pub role: Role,
    pub kind: Kind,
    pub look: u32,
    /// where they appear
    pub depot: RouteId,
    /// where they work
    pub site: RouteId,
    /// where they leave to
    pub exit: RouteId,
    pub area: AreaId,
    pub target: ObjectId,
    pub work_secs: f32,
    /// something they bring and carry to the site
    pub tool: Option<ObjectId>,
    /// the most they will take to get there before the job is done without them
    pub patience: f32,
}

/// The stages of a repair: arrive, work, depart, and the fallback when the way is not there.
/// Labels: [arrive, work, depart, fallback]; the last thing each does is go to `then`.
pub fn repair(labels: [Label; 4], then: Label, j: &CrewJob) -> Vec<Stage> {
    let [arrive, work, depart, fallback] = labels;
    let mut come = vec![Step::Spawn { who: j.who, spec: ActorSpec { kind: j.kind, role: j.role, at: j.depot, look: j.look } }];
    if let Some(tool) = j.tool {
        come.push(Step::Carry { who: j.who, obj: tool });
    }
    come.push(Step::Move { who: j.who, to: j.site, run: false });
    come.push(Step::Await { until: Until::Arrived(vec![j.who]), timeout: j.patience, on_timeout: Some(fallback) });
    come.push(Step::Branch { when: super::director::Cond::is(super::director::Test::Failed(j.who)), then: fallback, otherwise: work });
    let mut work_steps = vec![Step::Reserve { who: j.who, area: j.area }, Step::Pose { who: j.who, pose: ActorPose::Work }];
    if let Some(tool) = j.tool {
        // the tool is set down at the site to be worked with
        work_steps.push(Step::Drop { who: j.who });
        work_steps.push(Step::Await { until: Until::AtRest(tool), timeout: 4.0, on_timeout: None });
        work_steps.push(Step::Pose { who: j.who, pose: ActorPose::Work });
    }
    work_steps.extend([
        // a helping hand from the user shortens it
        Step::Await { until: Until::Assisted, timeout: j.work_secs, on_timeout: None },
        Step::SetState { obj: j.target, change: Change::Restore },
        Step::Emit { kind: EventKind::RepairProgress, obj: j.target },
        Step::Release { who: j.who },
        Step::Pose { who: j.who, pose: ActorPose::Idle },
        Step::Goto(depart),
    ]);
    let leave = vec![
        Step::Move { who: j.who, to: j.exit, run: false },
        Step::Await { until: Until::Arrived(vec![j.who]), timeout: j.patience, on_timeout: None },
        Step::Despawn { who: j.who },
        Step::Goto(then),
    ];
    // when they cannot get there, the job is done without them: nothing is left half done
    let stand_in = vec![
        Step::Release { who: j.who },
        Step::SetState { obj: j.target, change: Change::Restore },
        Step::Emit { kind: EventKind::RepairProgress, obj: j.target },
        Step::Despawn { who: j.who },
        Step::Goto(then),
    ];
    vec![
        Stage { label: arrive, steps: come },
        Stage { label: work, steps: work_steps },
        Stage { label: depart, steps: leave },
        Stage { label: fallback, steps: stand_in },
    ]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::actors::{Mode, RouteGraph};
    use crate::story::events::{Event, EventKind};
    use crate::story::model::{ActorId, Material, ObjectDef, ObjectStore};

    fn defs() -> Definition {
        let mut routes = RouteGraph::default();
        routes
            .add_node(1, (100.0, 200.0), Mode::Roof, 0.5)
            .add_node(2, (160.0, 200.0), Mode::Roof, 0.5)
            .add_node(3, (400.0, 200.0), Mode::Roof, 0.5)
            .add_node(10, (100.0, 400.0), Mode::Walk, 1.0);
        routes.connect(1, 2, Mode::Roof).connect(2, 3, Mode::Roof);
        Definition {
            view: "fixture",
            objects: vec![ObjectDef::new(1, "window", Material::Glass, (500.0, 300.0), [480.0, 280.0, 520.0, 320.0])],
            routes,
            ..Default::default()
        }
    }

    struct World {
        defs: Definition,
        objects: ObjectStore,
        actors: ActorSystem,
        mine: [Option<ActorId>; MINE],
        tick: u64,
        facts: Facts,
    }

    impl World {
        fn new() -> World {
            let defs = defs();
            World { objects: ObjectStore::new(&defs.objects), defs, actors: ActorSystem::new(), mine: [None; MINE], tick: 0, facts: Facts::default() }
        }

        fn run(&mut self, b: &mut dyn Behavior, events: &[Event], ticks: u64) {
            for _ in 0..ticks {
                self.tick += 1;
                let mut out = CommandBuffer::new();
                {
                    let ctx = BehaviorContext {
                        defs: &self.defs,
                        objects: Snapshot::new(&self.objects),
                        actors: &self.actors,
                        facts: &self.facts,
                        tick: self.tick,
                        events: if ticks == 1 || self.tick % 1 == 0 { events } else { &[] },
                        mine: &self.mine,
                        seed: 1,
                    };
                    b.update(&ctx, &mut out);
                }
                for c in out.take() {
                    match c {
                        Command::Spawn { slot, spec } => {
                            if let Some(old) = self.mine[slot as usize].take() {
                                self.actors.remove(old, &mut self.objects, &self.defs.objects);
                            }
                            self.mine[slot as usize] = self.actors.spawn(spec, &self.defs.routes).ok();
                        }
                        Command::Despawn(slot) => {
                            if let Some(a) = self.mine[slot as usize].take() {
                                self.actors.remove(a, &mut self.objects, &self.defs.objects);
                            }
                        }
                        Command::Actor(a, cmd) => {
                            let _ = self.actors.command(a, cmd, &self.defs.routes, &mut self.objects);
                        }
                        Command::Object(o, ch) => {
                            if let (Change::Open(v), Some(s)) = (ch, self.objects.get_mut(o)) {
                                s.open = v;
                            }
                        }
                        _ => {}
                    }
                }
                self.actors.step(1.0 / 30.0, &self.defs.routes, &mut self.objects, &self.defs.objects, &mut crate::story::events::EventBus::new());
            }
        }
    }

    fn day() -> Facts {
        Facts { hour: 12.0, ..Default::default() }
    }

    #[test]
    fn perched_birds_leave_for_an_alternate_and_come_back_when_it_is_quiet() {
        let mut w = World::new();
        w.facts = day();
        let mut b = PerchBehavior::new(Perch { perches: vec![RouteId(1), RouteId(2)], alternates: vec![RouteId(3)], radius: 60.0, away: 4.0, keep_off: None, shy_of: vec![] });
        w.run(&mut b, &[], 2);
        assert_eq!(w.actors.count(), 2, "two birds sit on the perches");
        let before: Vec<P> = w.actors.iter().map(|a| a.pos).collect();
        assert!(before.contains(&(100.0, 200.0)) && before.contains(&(160.0, 200.0)));
        // a noise by the first perch: both leave their actual perch, for the other roof
        let bang = Event::new(EventKind::Impact, ObjectId(1), (110.0, 205.0), 5);
        w.run(&mut b, &[bang], 1);
        w.run(&mut b, &[], 250);
        assert!(w.actors.iter().all(|a| (a.pos.0 - 400.0).abs() < 1.0), "they landed on the alternate: {:?}", w.actors.iter().map(|a| a.pos).collect::<Vec<_>>());
        // and after the quiet, home again
        w.run(&mut b, &[], 600);
        let after: Vec<P> = w.actors.iter().map(|a| a.pos).collect();
        assert!(after.contains(&(100.0, 200.0)) && after.contains(&(160.0, 200.0)), "{:?}", after);
        assert_eq!(w.actors.count(), 2, "no duplicate birds were left behind");
    }

    #[test]
    fn birds_stay_off_a_held_worksite_and_roost_in_bad_weather() {
        let mut w = World::new();
        w.facts = day();
        let mut b = PerchBehavior::new(Perch { perches: vec![RouteId(1)], alternates: vec![RouteId(3)], radius: 60.0, away: 1.0, keep_off: Some(AreaId(9)), shy_of: vec![] });
        w.run(&mut b, &[], 2);
        let holder = w.actors.spawn(ActorSpec { kind: Kind::Person, role: Role::Roofer, at: RouteId(10), look: 0 }, &w.defs.routes).unwrap();
        w.actors.command(holder, ActorCommand::Reserve(AreaId(9)), &w.defs.routes, &mut w.objects).unwrap();
        w.run(&mut b, &[Event::new(EventKind::Damage, ObjectId(1), (100.0, 200.0), 9)], 1);
        w.run(&mut b, &[], 900);
        let bird = w.actors.iter().find(|a| a.kind == Kind::Bird).unwrap();
        assert!((bird.pos.0 - 400.0).abs() < 1.0, "still away while the roofer works: {:?}", bird.pos);
        w.actors.command(holder, ActorCommand::Release, &w.defs.routes, &mut w.objects).unwrap();
        w.run(&mut b, &[], 600);
        let bird = w.actors.iter().find(|a| a.kind == Kind::Bird).unwrap();
        assert!((bird.pos.0 - 100.0).abs() < 1.0, "reclaims the roof when the work is done");
        // rain: they are not there at all
        w.facts.rain = true;
        w.run(&mut b, &[], 2);
        assert_eq!(w.actors.iter().filter(|a| a.kind == Kind::Bird).count(), 0);
    }

    #[test]
    fn a_knock_brings_someone_to_the_window_and_repeats_close_it() {
        let mut w = World::new();
        let mut b = OccupantBehavior::new(Occupant { window: ObjectId(1), at: RouteId(10), role: Role::Resident, look: 2 });
        assert!(b.asleep());
        let knock = |n| Event::new(EventKind::Noise, ObjectId(1), (500.0, 300.0), n);
        w.run(&mut b, &[knock(1)], 1);
        assert_eq!(w.actors.count(), 1);
        assert!(w.objects.get(ObjectId(1)).unwrap().open, "the curtain opens");
        w.run(&mut b, &[], 20);
        assert_eq!(w.actors.iter().next().unwrap().pose, ActorPose::Look);
        w.run(&mut b, &[], 40);
        assert_eq!(w.actors.iter().next().unwrap().pose, ActorPose::Wave, "then a wave");
        // left alone, they go away again and the window closes
        w.run(&mut b, &[], 200);
        assert_eq!(w.actors.count(), 0);
        assert!(!w.objects.get(ObjectId(1)).unwrap().open);
        // three knocks in a rush: a sharp look and the shutters close
        for n in 0..3 {
            w.run(&mut b, &[knock(10 + n)], 1);
        }
        assert_eq!(w.actors.iter().next().unwrap().pose, ActorPose::React);
        assert!(!w.objects.get(ObjectId(1)).unwrap().open);
        w.run(&mut b, &[], 120);
        assert_eq!(w.actors.count(), 0);
    }

    #[test]
    fn a_cat_looks_chases_and_flees() {
        let mut w = World::new();
        let mut b = AnimalBehavior::new(Animal { kind: Kind::Cat, home: RouteId(1), haunts: vec![RouteId(2), RouteId(3)], attention: 500.0, look: 0, linger: 2.0 });
        w.run(&mut b, &[], 2);
        let cat = *w.mine[0].as_ref().unwrap();
        assert_eq!(w.actors.get(cat).unwrap().pos, (100.0, 200.0));
        // something lands on the far roof: it goes to see
        w.run(&mut b, &[Event::new(EventKind::Obstruction, ObjectId(1), (390.0, 210.0), 3)], 1);
        w.run(&mut b, &[], 1100);
        let pos = w.actors.get(cat).unwrap().pos;
        assert!((pos.0 - 100.0).abs() < 1.0, "after a while it comes home: {:?}", pos);
        // a bang of glass: straight home, never towards it
        w.run(&mut b, &[Event::new(EventKind::Obstruction, ObjectId(1), (390.0, 210.0), 4)], 1);
        w.run(&mut b, &[], 60);
        let mut bang = Event::new(EventKind::Damage, ObjectId(1), (120.0, 210.0), 5);
        bang.tags = TAG_GLASS;
        w.run(&mut b, &[bang], 1);
        w.run(&mut b, &[], 600);
        assert!((w.actors.get(cat).unwrap().pos.0 - 100.0).abs() < 1.0, "{:?}", w.actors.get(cat).unwrap().pos);
        // rain: it is indoors
        w.facts.rain = true;
        w.run(&mut b, &[], 2);
        assert_eq!(w.actors.count(), 0);
    }

    #[test]
    fn a_behavior_that_overspends_its_command_budget_is_cut_off_and_counted() {
        let mut out = CommandBuffer::new();
        for _ in 0..1000 {
            out.push(Command::Despawn(0));
        }
        assert_eq!(out.len(), COMMAND_BUDGET);
        assert_eq!(out.refused as usize, 1000 - COMMAND_BUDGET);
    }
}
