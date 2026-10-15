//! The sequence director: stories as typed steps, run on fixed ticks.
//!
//! A `StoryDef` is a set of labelled stages of `Step`s: wait, move someone,
//! set an object's state, branch on weather or on what the user has done,
//! await an arrival or an event with a timeout. The director runs a few of
//! them at once, at most 256 instructions in a tick, in simulation time
//! only. Three rules make every story end: waiting always has a timeout; an
//! incident can be escalated by interference three times, after which a
//! protected cleanup runs that no further input can restart; and a hard time
//! limit forces that cleanup if a story has somehow not finished.

use super::actors::{ActorCommand, ActorPose, ActorSpec, ActorState, ActorSystem, RouteGraph};
use super::events::{Event, EventBus, EventKind, SimClock};
use super::model::{ActorId, AreaId, Attachment, Damage, Definition, DefinitionError, ObjectId, ObjectStore, Owner, Pose, RouteId, StoryId};
use super::physics::{Physics, TAG_METAL, TAG_WATER, TAG_WOOD};
use crate::gfx::P;
use crate::rng::hash2;

pub type Label = &'static str;

/// A named place in a story for one of its actors: the first worker, the owner.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Slot(pub u8);

pub const SLOTS: usize = 6;
pub const MAX_RUNS: usize = 8;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Weather {
    Rain,
    Dry,
    Snow,
    Windy,
    Calm,
    Night,
    Day,
    Storm,
    Cold,
}

/// What is true of the sky and the hour, for a story to ask about.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Facts {
    pub rain: bool,
    pub snow: bool,
    pub wind: f32,
    pub storm: bool,
    pub night: bool,
    pub cold: bool,
    pub hour: f32,
    pub season: u8,
    /// a working day
    pub weekday: bool,
}

impl Facts {
    pub fn holds(&self, w: Weather) -> bool {
        match w {
            Weather::Rain => self.rain,
            Weather::Dry => !self.rain && !self.snow,
            Weather::Snow => self.snow,
            Weather::Windy => self.wind > 22.0,
            Weather::Calm => self.wind <= 22.0,
            Weather::Night => self.night,
            Weather::Day => !self.night,
            Weather::Storm => self.storm,
            Weather::Cold => self.cold,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum MemoryKey {
    /// times the user has disturbed something that was being put right
    Disturbances,
    /// times the user has helped
    Helped,
    /// times any incident has run to its end
    Repairs,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Test {
    Weather(Weather),
    ObjectIs(ObjectId, Damage),
    /// it lies where it was not put: the user (or an accident) has moved it
    Displaced(ObjectId),
    Memory(MemoryKey, u16),
    /// the actor in this slot could not get where it was going, or is gone
    Failed(Slot),
    /// the user has helped this run
    Assisted,
    /// the user has disturbed this run since it began
    Interfered,
    /// a flag set by `Step::Flag`
    Flag(u8),
    /// the nth escalation has been reached
    Escalated(u8),
    /// the weekday (a working one)
    Weekday,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Cond {
    pub test: Test,
    pub not: bool,
}

impl Cond {
    pub fn is(test: Test) -> Cond {
        Cond { test, not: false }
    }

    pub fn not(test: Test) -> Cond {
        Cond { test, not: true }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum Until {
    /// every one of these has arrived (or failed, or is gone: ask `Failed` after)
    Arrived(Vec<Slot>),
    ObjectIs(ObjectId, Damage),
    AtRest(ObjectId),
    AreaFree(AreaId),
    /// an event of this kind, from this object (or any), since the wait began
    Event(EventKind, Option<ObjectId>),
    Assisted,
    /// seconds without anyone touching the incident
    Calm(f32),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Change {
    Damage(Damage),
    Visible(bool),
    Open(bool),
    /// intact, in place, at rest
    Restore,
    /// at this place, at rest
    Place(P),
    /// owned by this story alone
    Claim,
    /// no longer anybody's
    Free,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Step {
    Wait(f32),
    Move { who: Slot, to: RouteId, run: bool },
    Look { who: Slot, at: P },
    LookAt { who: Slot, obj: ObjectId },
    Pose { who: Slot, pose: ActorPose },
    Carry { who: Slot, obj: ObjectId },
    Drop { who: Slot },
    SetState { obj: ObjectId, change: Change },
    Emit { kind: EventKind, obj: ObjectId },
    Reserve { who: Slot, area: AreaId },
    Release { who: Slot },
    Spawn { who: Slot, spec: ActorSpec },
    Despawn { who: Slot },
    /// water, or any jet: from where the object is
    Jet { id: u8, obj: ObjectId, vel: P, rate: f32, on: bool },
    Flag(u8),
    Remember(MemoryKey),
    Branch { when: Cond, then: Label, otherwise: Label },
    /// a weighted choice, the same way every time for the same seed
    Choose(Vec<(u32, Label)>),
    Goto(Label),
    Await { until: Until, timeout: f32, on_timeout: Option<Label> },
    End,
}

impl Step {
    /// Does this step give up the rest of the tick?
    fn yields(&self) -> bool {
        matches!(self, Step::Wait(_) | Step::Await { .. } | Step::End)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Stage {
    pub label: Label,
    pub steps: Vec<Step>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Trigger {
    /// only when asked: the scene panel, a replay, or code
    Named,
    /// when this object reaches at least this much damage
    Damage(ObjectId, Damage),
    /// when this event happens to (or from) this object
    Event(EventKind, ObjectId),
    /// now and then, on its own, between these many seconds, when all of these hold
    Ambient { every: (f32, f32), when: Vec<Cond> },
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Class {
    /// a principal incident: counted against the four
    Incident,
    Ambient,
}

#[derive(Clone, Debug)]
pub struct StoryDef {
    pub id: StoryId,
    pub name: &'static str,
    pub class: Class,
    pub trigger: Trigger,
    pub start: Label,
    pub stages: Vec<Stage>,
    /// where interference sends the story, the first time, the second, the third
    pub escalations: Vec<Label>,
    /// the protected ending
    pub cleanup: Label,
    /// disturbing any of these while the story runs is interference
    pub interference: Vec<ObjectId>,
    /// the longest it may take before the cleanup is forced, in simulation seconds
    pub limit: f32,
    /// parts the user can return: putting `part` down within `within` of `near` is a helping hand
    pub assists: Vec<(ObjectId, ObjectId, f32)>,
}

impl StoryDef {
    pub fn new(id: u16, name: &'static str, class: Class, trigger: Trigger) -> StoryDef {
        StoryDef { id: StoryId(id), name, class, trigger, start: "start", stages: Vec::new(), escalations: Vec::new(), cleanup: "cleanup", interference: Vec::new(), limit: 150.0, assists: Vec::new() }
    }

    pub fn stage(mut self, label: Label, steps: Vec<Step>) -> StoryDef {
        self.stages.push(Stage { label, steps });
        self
    }

    pub fn escalating(mut self, labels: &[Label]) -> StoryDef {
        self.escalations = labels.to_vec();
        self
    }

    pub fn disturbed_by(mut self, objects: &[ObjectId]) -> StoryDef {
        self.interference = objects.to_vec();
        self
    }

    pub fn helped_by(mut self, part: ObjectId, near: ObjectId, within: f32) -> StoryDef {
        self.assists.push((part, near, within));
        self
    }

    pub fn within(mut self, seconds: f32) -> StoryDef {
        self.limit = seconds;
        self
    }

    fn stage_index(&self, label: Label) -> Option<usize> {
        self.stages.iter().position(|s| s.label == label)
    }

    /// Check one story: labels exist, jumps land somewhere, every wait is bounded, and no loop
    /// can run without giving up the tick.
    pub fn validate(&self, defs: &Definition, view: &'static str) -> Result<(), DefinitionError> {
        let fail = |binding: String, why: String| DefinitionError { view, story: Some(self.name.to_string()), binding, why };
        for (n, s) in self.stages.iter().enumerate() {
            if self.stages[..n].iter().any(|o| o.label == s.label) {
                return Err(fail(format!("stage '{}'", s.label), "this label is used twice".into()));
            }
        }
        for want in std::iter::once(self.start).chain(std::iter::once(self.cleanup)).chain(self.escalations.iter().copied()) {
            if self.stage_index(want).is_none() {
                return Err(fail(format!("stage '{}'", want), "is named but not defined".into()));
            }
        }
        if self.escalations.len() > 3 {
            return Err(fail("escalations".into(), format!("{} escalations is more than the 3 allowed", self.escalations.len())));
        }
        if !(self.limit.is_finite() && self.limit > 1.0) {
            return Err(fail("limit".into(), "must be a finite number of seconds".into()));
        }
        let object = |id: ObjectId| defs.object(id).is_some();
        for id in &self.interference {
            if !object(*id) {
                return Err(fail(format!("interference object {}", id.0), "is not an object of this view".into()));
            }
        }
        if let Trigger::Damage(o, _) | Trigger::Event(_, o) = &self.trigger {
            if !object(*o) {
                return Err(fail(format!("trigger object {}", o.0), "is not an object of this view".into()));
            }
        }
        if let Trigger::Ambient { every, .. } = &self.trigger {
            if !(every.0.is_finite() && every.1.is_finite() && every.0 > 1.0 && every.1 >= every.0) {
                return Err(fail("ambient interval".into(), "must be finite, over a second, and not backwards".into()));
            }
        }
        // every step, in every stage
        for stage in &self.stages {
            for (k, step) in stage.steps.iter().enumerate() {
                let at = format!("stage '{}' step {}", stage.label, k + 1);
                let jump = |l: Label| -> Result<(), DefinitionError> {
                    if self.stage_index(l).is_none() {
                        Err(fail(at.clone(), format!("jumps to '{}', which is not a stage", l)))
                    } else {
                        Ok(())
                    }
                };
                match step {
                    Step::Wait(s) if !(s.is_finite() && *s >= 0.0 && *s < 600.0) => {
                        return Err(fail(at, "waits for a time that is not finite, or over ten minutes".into()));
                    }
                    Step::Move { to, .. } if defs.routes.node(*to).is_none() => {
                        return Err(fail(at, format!("moves to place {}, which is not on the view's routes", to.0)));
                    }
                    Step::Look { at: p, .. } if !(p.0.is_finite() && p.1.is_finite()) => return Err(fail(at, "looks at a point that is not finite".into())),
                    Step::LookAt { obj, .. } | Step::Carry { obj, .. } | Step::SetState { obj, .. } | Step::Emit { obj, .. } | Step::Jet { obj, .. } if !object(*obj) => {
                        return Err(fail(at, format!("names object {}, which is not an object of this view", obj.0)));
                    }
                    Step::Spawn { spec, .. } if defs.routes.node(spec.at).is_none() => {
                        return Err(fail(at, format!("starts an actor at place {}, which is not on the view's routes", spec.at.0)));
                    }
                    Step::Branch { then, otherwise, .. } => {
                        jump(then)?;
                        jump(otherwise)?;
                    }
                    Step::Choose(options) => {
                        if options.is_empty() || options.iter().all(|o| o.0 == 0) {
                            return Err(fail(at, "chooses between nothing".into()));
                        }
                        for o in options {
                            jump(o.1)?;
                        }
                    }
                    Step::Goto(l) => jump(l)?,
                    Step::Await { timeout, on_timeout, .. } => {
                        if !(timeout.is_finite() && *timeout > 0.0 && *timeout < 600.0) {
                            return Err(fail(at, "awaits with no usable timeout: every wait must end".into()));
                        }
                        if let Some(l) = on_timeout {
                            jump(l)?;
                        }
                    }
                    Step::Move { who, .. } | Step::Look { who, .. } | Step::LookAt { who, .. } | Step::Pose { who, .. } | Step::Carry { who, .. } | Step::Drop { who }
                    | Step::Reserve { who, .. } | Step::Release { who } | Step::Spawn { who, .. } | Step::Despawn { who }
                        if who.0 as usize >= SLOTS =>
                    {
                        return Err(fail(at, format!("uses actor slot {}, and a story has {}", who.0, SLOTS)));
                    }
                    _ => {}
                }
            }
        }
        // a loop that never waits would spin the tick: look for one among the jumps
        self.check_loops(view)
    }

    fn check_loops(&self, view: &'static str) -> Result<(), DefinitionError> {
        // nodes: (stage, step). Edges: to the next step, and to the head of any stage jumped to,
        // through steps that do not yield.
        let n: usize = self.stages.iter().map(|s| s.steps.len().max(1)).sum();
        let base: Vec<usize> = self.stages.iter().scan(0, |acc, s| {
            let b = *acc;
            *acc += s.steps.len().max(1);
            Some(b)
        }).collect();
        let head = |l: Label| self.stage_index(l).map(|i| base[i]);
        let mut edges: Vec<Vec<usize>> = vec![Vec::new(); n];
        let mut name = vec![String::new(); n];
        for (si, stage) in self.stages.iter().enumerate() {
            for (k, step) in stage.steps.iter().enumerate() {
                let me = base[si] + k;
                name[me] = format!("stage '{}' step {}", stage.label, k + 1);
                if step.yields() {
                    continue; // control leaves the tick here
                }
                let next = if k + 1 < stage.steps.len() { Some(me + 1) } else { None };
                match step {
                    Step::Goto(l) => edges[me].extend(head(l)),
                    Step::Branch { then, otherwise, .. } => {
                        edges[me].extend(head(then));
                        edges[me].extend(head(otherwise));
                    }
                    Step::Choose(opts) => {
                        for o in opts {
                            edges[me].extend(head(o.1));
                        }
                    }
                    _ => edges[me].extend(next),
                }
            }
        }
        // depth-first, looking for a cycle
        let mut state = vec![0u8; n];
        fn walk(u: usize, edges: &[Vec<usize>], state: &mut [u8]) -> Option<usize> {
            state[u] = 1;
            for &v in &edges[u] {
                if state[v] == 1 {
                    return Some(v);
                }
                if state[v] == 0 {
                    if let Some(c) = walk(v, edges, state) {
                        return Some(c);
                    }
                }
            }
            state[u] = 2;
            None
        }
        for u in 0..n {
            if state[u] == 0 {
                if let Some(at) = walk(u, &edges, &mut state) {
                    return Err(DefinitionError {
                        view,
                        story: Some(self.name.to_string()),
                        binding: name[at].clone(),
                        why: "is part of a loop that never waits, which would spin forever".into(),
                    });
                }
            }
        }
        Ok(())
    }
}

/// Put one change into an object's state. `story` is who claims it when asked to.
pub fn apply_change(objects: &mut ObjectStore, defs: &Definition, obj: ObjectId, change: Change, story: Option<StoryId>) {
    let (Some(s), Some(def)) = (objects.get_mut(obj), defs.object(obj)) else { return };
    match change {
        Change::Damage(d) => s.damage = d,
        Change::Visible(v) => s.visible = v,
        Change::Open(o) => s.open = o,
        Change::Restore => {
            s.damage = Damage::Intact;
            s.pose = Pose::default();
            s.attachment = Attachment::Fixed;
            s.swinging = false;
            s.visible = true;
            s.owner = None;
        }
        Change::Place(at) => {
            s.pose = Pose { dx: at.0 - def.at.0, dy: at.1 - def.at.1, turn: 0.0 };
            s.attachment = Attachment::Fixed;
            s.swinging = false;
        }
        Change::Claim => {
            if let Some(id) = story {
                s.owner = Some(Owner::Story(id));
            }
        }
        Change::Free => s.owner = None,
    }
    s.revision += 1;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Memory {
    pub disturbances: u16,
    pub helped: u16,
    pub repairs: u16,
}

impl Memory {
    pub fn get(&self, k: MemoryKey) -> u16 {
        match k {
            MemoryKey::Disturbances => self.disturbances,
            MemoryKey::Helped => self.helped,
            MemoryKey::Repairs => self.repairs,
        }
    }
}

struct Wait {
    until: Until,
    deadline: u64,
    on_timeout: Option<Label>,
    /// the events seen while waiting are only those after it began
    began: u64,
}

struct Run {
    story: usize,
    stage: usize,
    step: usize,
    actors: [Option<ActorId>; SLOTS],
    failed: [bool; SLOTS],
    wake: u64,
    waiting: Option<Wait>,
    escalations: u8,
    protected: bool,
    started: u64,
    last_touched: u64,
    assisted: bool,
    interfered: bool,
    flags: u8,
    serial: u64,
    heard: Vec<(EventKind, Option<ObjectId>, u64)>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum TriggerResult {
    Started,
    /// it is running already: the request counts as a disturbance
    AlreadyRunning,
    /// four incidents are running
    Busy,
    Unknown,
}

/// Everything a story step can touch, borrowed for one tick.
pub struct Parts<'a> {
    pub defs: &'a Definition,
    pub clock: &'a SimClock,
    pub seed: u64,
    pub actors: &'a mut ActorSystem,
    pub objects: &'a mut ObjectStore,
    pub physics: &'a mut Physics,
    pub bus: &'a mut EventBus,
    pub facts: &'a Facts,
}

pub struct Director {
    runs: Vec<Run>,
    pub memory: Memory,
    serial: u64,
    /// when each ambient story may next start, by story index
    ambient_due: Vec<(usize, u64)>,
    /// no ambient job starts before this tick (an incident has just been through)
    quiet_until: u64,
    /// instructions run by the busiest tick so far (for the tests that watch the budget)
    pub peak_instructions: usize,
    /// the times a tick used up its whole budget and had to carry on at the next
    pub yielded: u64,
    /// the diagnostic of a story that was stopped for breaking its limits
    pub diagnostics: Vec<String>,
    max_instructions: usize,
    max_incidents: usize,
    max_escalations: u8,
}

impl Director {
    pub fn new(limits: &super::model::Limits) -> Director {
        Director {
            runs: Vec::with_capacity(MAX_RUNS),
            memory: Memory::default(),
            serial: 0,
            ambient_due: Vec::new(),
            quiet_until: 0,
            peak_instructions: 0,
            yielded: 0,
            diagnostics: Vec::new(),
            max_instructions: limits.instructions,
            max_incidents: limits.incidents,
            max_escalations: limits.escalations as u8,
        }
    }

    /// The view's stories have changed (a resize rebuilt them): keep what runs, by story id.
    pub fn rebind(&mut self, old: &Definition, new: &Definition) {
        let mut keep = Vec::new();
        for r in self.runs.drain(..) {
            let id = old.stories.get(r.story).map(|s| s.id);
            if let Some(i) = id.and_then(|id| new.stories.iter().position(|s| s.id == id)) {
                keep.push(Run { story: i, ..r });
            }
        }
        self.runs = keep;
        self.ambient_due.clear();
    }

    pub fn running(&self, id: StoryId, defs: &Definition) -> bool {
        self.runs.iter().any(|r| defs.stories.get(r.story).is_some_and(|s| s.id == id))
    }

    pub fn incidents_running(&self, defs: &Definition) -> usize {
        self.runs.iter().filter(|r| defs.stories.get(r.story).is_some_and(|s| s.class == Class::Incident)).count()
    }

    pub fn active(&self) -> bool {
        !self.runs.is_empty()
    }

    /// The next tick anything is due, or None when nothing is waiting on a clock.
    pub fn next_deadline(&self) -> Option<u64> {
        let runs = self.runs.iter().map(|r| match &r.waiting {
            Some(w) => w.deadline.min(r.wake.max(1)),
            None => r.wake,
        });
        runs.chain(self.ambient_due.iter().map(|a| a.1)).min()
    }

    /// Start a story by id. An incident that is already running is disturbed instead.
    pub fn trigger(&mut self, id: StoryId, p: &mut Parts) -> TriggerResult {
        let Some(si) = p.defs.stories.iter().position(|s| s.id == id) else { return TriggerResult::Unknown };
        if self.runs.iter().any(|r| r.story == si) {
            self.disturb_story(si, p, true);
            return TriggerResult::AlreadyRunning;
        }
        let class = p.defs.stories[si].class;
        if class == Class::Incident {
            if self.incidents_running(p.defs) >= self.max_incidents {
                return TriggerResult::Busy;
            }
            // ambient life makes way: nobody is in two jobs at once
            let ambient: Vec<usize> = (0..self.runs.len()).filter(|i| p.defs.stories.get(self.runs[*i].story).is_some_and(|s| s.class == Class::Ambient)).collect();
            for i in ambient.into_iter().rev() {
                let run = self.runs.remove(i);
                self.release(&run, p);
            }
        } else if self.runs.len() >= MAX_RUNS {
            return TriggerResult::Busy;
        }
        self.begin(si, p);
        TriggerResult::Started
    }

    fn begin(&mut self, si: usize, p: &mut Parts) {
        let story = &p.defs.stories[si];
        let Some(stage) = story.stage_index(story.start) else { return };
        self.serial += 1;
        let tick = p.clock.tick;
        self.runs.push(Run {
            story: si,
            stage,
            step: 0,
            actors: [None; SLOTS],
            failed: [false; SLOTS],
            wake: tick,
            waiting: None,
            escalations: 0,
            protected: false,
            started: tick,
            last_touched: tick,
            assisted: false,
            interfered: false,
            flags: 0,
            serial: self.serial,
            heard: Vec::new(),
        });
    }

    /// An object was disturbed (struck, pulled, moved, broken). Stories that watch it escalate.
    pub fn disturbed(&mut self, obj: ObjectId, p: &mut Parts) {
        let hit: Vec<usize> = self
            .runs
            .iter()
            .filter(|r| p.defs.stories.get(r.story).is_some_and(|s| s.interference.contains(&obj)))
            .map(|r| r.story)
            .collect();
        for si in hit {
            self.disturb_story(si, p, false);
        }
    }

    fn disturb_story(&mut self, si: usize, p: &mut Parts, asked: bool) {
        let Some(ri) = self.runs.iter().position(|r| r.story == si) else { return };
        self.memory.disturbances = self.memory.disturbances.saturating_add(1);
        let tick = p.clock.tick;
        let (escalations, protected, last) = (self.runs[ri].escalations, self.runs[ri].protected, self.runs[ri].last_touched);
        self.runs[ri].last_touched = tick;
        self.runs[ri].interfered = true;
        if protected || (asked && escalations >= self.max_escalations && protected) {
            return; // the cleanup is under way: nothing restarts it
        }
        // a flood of input in one second is one disturbance, not forty
        if tick.saturating_sub(last) < 45 && escalations > 0 {
            return;
        }
        let story = &p.defs.stories[si];
        let next = self.runs[ri].escalations as usize;
        let target = if next < story.escalations.len() && (next as u8) < self.max_escalations {
            self.runs[ri].escalations += 1;
            story.escalations[next]
        } else {
            self.runs[ri].protected = true;
            story.cleanup
        };
        if let Some(stage) = story.stage_index(target) {
            let r = &mut self.runs[ri];
            r.stage = stage;
            r.step = 0;
            r.waiting = None;
            r.wake = tick;
        }
    }

    /// The user helped (returned a part, steadied something): this story's running incident is told.
    pub fn assisted(&mut self, story: StoryId, p: &mut Parts) {
        for r in self.runs.iter_mut() {
            if p.defs.stories.get(r.story).is_some_and(|s| s.id == story) {
                r.assisted = true;
                self.memory.helped = self.memory.helped.saturating_add(1);
            }
        }
    }

    /// Give every actor and reservation a run held back.
    fn release(&mut self, run: &Run, p: &mut Parts) {
        for a in run.actors.iter().flatten() {
            p.actors.remove(*a, p.objects, &p.defs.objects);
        }
        // objects the story claimed go back to nobody
        for (_, s) in p.objects.iter().map(|(i, s)| (i, *s)).collect::<Vec<_>>() {
            let _ = s;
        }
        let story_id = p.defs.stories.get(run.story).map(|s| s.id);
        if let Some(id) = story_id {
            let ids: Vec<ObjectId> = p.objects.iter().filter(|(_, s)| s.owner == Some(Owner::Story(id))).map(|(i, _)| i).collect();
            for i in ids {
                if let Some(s) = p.objects.get_mut(i) {
                    s.owner = None;
                }
            }
        }
    }

    /// Stop everything: a reset, or a view going away.
    pub fn abort_all(&mut self, p: &mut Parts) {
        let runs = std::mem::take(&mut self.runs);
        for r in &runs {
            self.release(r, p);
        }
        self.ambient_due.clear();
    }

    /// Heard of an event: waits on events and the escalation rules take note.
    pub fn hear(&mut self, e: &Event, p: &mut Parts) {
        let tick = p.clock.tick;
        for r in &mut self.runs {
            if r.heard.len() < 16 {
                r.heard.push((e.kind, Some(e.source), tick));
            }
        }
        // a thing happened to something an incident watches
        if matches!(e.kind, EventKind::Impact | EventKind::Damage | EventKind::Obstruction | EventKind::Noise) && e.tags & (1 << 7) == 0 {
            self.disturbed(e.source, p);
        }
        let _ = (TAG_METAL, TAG_WATER, TAG_WOOD);
    }

    /// Ambient stories wait on a deadline of their own; incidents may start from damage or an event.
    pub fn schedule(&mut self, p: &mut Parts) {
        let tick = p.clock.tick;
        for (si, s) in p.defs.stories.iter().enumerate() {
            if let Trigger::Ambient { every, .. } = &s.trigger {
                if !self.ambient_due.iter().any(|a| a.0 == si) {
                    let u = hash2((p.seed ^ si as u64) as u32, (p.seed >> 32) as u32 ^ 77);
                    let secs = every.0 + (every.1 - every.0) * u;
                    self.ambient_due.push((si, tick + (secs as f64 / super::events::STEP) as u64));
                }
            }
        }
    }

    /// Run the stories for one tick.
    pub fn step(&mut self, p: &mut Parts) {
        let tick = p.clock.tick;
        // the ambient ones that have come due
        let due: Vec<usize> = self.ambient_due.iter().filter(|a| a.1 <= tick).map(|a| a.0).collect();
        for si in due {
            self.ambient_due.retain(|a| a.0 != si);
            let story = &p.defs.stories[si];
            let Trigger::Ambient { every, when } = &story.trigger else { continue };
            let (lo, hi) = *every;
            let ok = when.iter().all(|c| self.test(c, None, p));
            let free = self.incidents_running(p.defs) == 0 && tick >= self.quiet_until && !self.runs.iter().any(|r| r.story == si) && self.runs.len() < MAX_RUNS;
            if ok && free {
                self.begin(si, p);
            }
            let u = hash2((p.seed ^ (si as u64 * 31 + tick)) as u32, 1234);
            let secs = lo + (hi - lo) * u;
            self.ambient_due.push((si, tick + (secs as f64 / super::events::STEP).max(1.0) as u64));
        }
        let mut budget = self.max_instructions;
        let mut ri = 0;
        let mut used_total = 0;
        while ri < self.runs.len() {
            let before = budget;
            let finished = self.advance(ri, p, &mut budget);
            used_total += before - budget;
            if finished {
                let run = self.runs.remove(ri);
                self.release(&run, p);
                let class = p.defs.stories.get(run.story).map(|s| s.class);
                self.memory.repairs = self.memory.repairs.saturating_add(1);
                if class == Some(Class::Incident) {
                    self.quiet_until = tick + 20 * 30;
                }
            } else {
                ri += 1;
            }
        }
        self.peak_instructions = self.peak_instructions.max(used_total);
        if budget == 0 {
            self.yielded += 1;
        }
    }

    /// Run one story until it waits, finishes or the budget is used up. True when it is over.
    fn advance(&mut self, ri: usize, p: &mut Parts, budget: &mut usize) -> bool {
        let tick = p.clock.tick;
        let si = self.runs[ri].story;
        // the hard limit: past it, only the protected cleanup is allowed
        let limit_ticks = (p.defs.stories[si].limit as f64 / super::events::STEP) as u64;
        if tick - self.runs[ri].started > limit_ticks && !self.runs[ri].protected {
            let story = &p.defs.stories[si];
            if let Some(stage) = story.stage_index(story.cleanup) {
                let r = &mut self.runs[ri];
                r.stage = stage;
                r.step = 0;
                r.waiting = None;
                r.protected = true;
                r.wake = tick;
            }
        }
        loop {
            if self.runs[ri].wake > tick {
                return false;
            }
            // a wait
            if let Some(w) = &self.runs[ri].waiting {
                if self.satisfied(ri, &w.until, w.began, p) {
                    self.runs[ri].waiting = None;
                } else if tick >= w.deadline {
                    let jump = w.on_timeout;
                    self.runs[ri].waiting = None;
                    if let Some(l) = jump {
                        self.jump(ri, l, p);
                    }
                } else {
                    return false;
                }
            }
            if *budget == 0 {
                return false;
            }
            let story = &p.defs.stories[si];
            let (stage, k) = (self.runs[ri].stage, self.runs[ri].step);
            let Some(step) = story.stages.get(stage).and_then(|s| s.steps.get(k)).cloned() else {
                // fell off the end of a stage: the story is over
                return true;
            };
            *budget -= 1;
            self.runs[ri].step += 1;
            match step {
                Step::End => return true,
                Step::Wait(secs) => {
                    self.runs[ri].wake = tick + ((secs as f64 / super::events::STEP).ceil() as u64).max(1);
                    return false;
                }
                Step::Await { until, timeout, on_timeout } => {
                    let began = tick;
                    self.runs[ri].heard.clear();
                    self.runs[ri].waiting = Some(Wait { until, deadline: tick + ((timeout as f64 / super::events::STEP).ceil() as u64).max(1), on_timeout, began });
                    // it may already hold: the loop checks on its next turn without losing the tick
                    self.runs[ri].wake = tick;
                }
                other => self.exec(ri, other, p),
            }
        }
    }

    fn jump(&mut self, ri: usize, label: Label, p: &Parts) {
        let story = &p.defs.stories[self.runs[ri].story];
        if let Some(i) = story.stage_index(label) {
            self.runs[ri].stage = i;
            self.runs[ri].step = 0;
        }
    }

    fn actor(&self, ri: usize, who: Slot) -> Option<ActorId> {
        self.runs[ri].actors.get(who.0 as usize).copied().flatten()
    }

    fn exec(&mut self, ri: usize, step: Step, p: &mut Parts) {
        let graph: &RouteGraph = &p.defs.routes;
        match step {
            Step::Move { who, to, run } => {
                if let Some(a) = self.actor(ri, who) {
                    let cmd = if run { ActorCommand::Run(to) } else { ActorCommand::Move(to) };
                    let _ = p.actors.command(a, cmd, graph, p.objects);
                }
            }
            Step::Look { who, at } => {
                if let Some(a) = self.actor(ri, who) {
                    let _ = p.actors.command(a, ActorCommand::Look(at), graph, p.objects);
                }
            }
            Step::LookAt { who, obj } => {
                if let (Some(a), Some(def)) = (self.actor(ri, who), p.defs.object(obj)) {
                    let at = p.objects.get(obj).map_or(def.at, |s| (def.at.0 + s.pose.dx, def.at.1 + s.pose.dy));
                    let _ = p.actors.command(a, ActorCommand::Look(at), graph, p.objects);
                }
            }
            Step::Pose { who, pose } => {
                if let Some(a) = self.actor(ri, who) {
                    let _ = p.actors.command(a, ActorCommand::Pose(pose), graph, p.objects);
                }
            }
            Step::Carry { who, obj } => {
                if let Some(a) = self.actor(ri, who) {
                    if p.actors.command(a, ActorCommand::Carry(obj), graph, p.objects).is_err() {
                        self.runs[ri].failed[who.0 as usize] = true;
                    }
                }
            }
            Step::Drop { who } => {
                if let Some(a) = self.actor(ri, who) {
                    let at = p.actors.get(a).map(|x| (x.pos.0 + x.facing * 6.0 * x.scale, x.pos.1 - 14.0 * x.scale));
                    if let Some(at) = at {
                        let _ = p.actors.command(a, ActorCommand::Drop(at), graph, p.objects);
                    }
                }
            }
            Step::SetState { obj, change } => {
                let sid = p.defs.stories[self.runs[ri].story].id;
                apply_change(p.objects, p.defs, obj, change, Some(sid));
            }
            Step::Emit { kind, obj } => {
                let at = p.defs.object(obj).map_or((0.0, 0.0), |d| d.at);
                let origin = (self.runs[ri].serial << 20) | self.runs[ri].step as u64;
                p.bus.emit(Event::new(kind, obj, at, origin));
            }
            Step::Reserve { who, area } => {
                if let Some(a) = self.actor(ri, who) {
                    let _ = p.actors.command(a, ActorCommand::Reserve(area), graph, p.objects);
                }
            }
            Step::Release { who } => {
                if let Some(a) = self.actor(ri, who) {
                    let _ = p.actors.command(a, ActorCommand::Release, graph, p.objects);
                }
            }
            Step::Spawn { who, spec } => {
                let slot = who.0 as usize;
                // somebody already in the slot is replaced, not duplicated
                if let Some(old) = self.runs[ri].actors[slot].take() {
                    p.actors.remove(old, p.objects, &p.defs.objects);
                }
                match p.actors.spawn(spec, graph) {
                    Ok(id) => {
                        self.runs[ri].actors[slot] = Some(id);
                        self.runs[ri].failed[slot] = false;
                    }
                    Err(_) => self.runs[ri].failed[slot] = true,
                }
            }
            Step::Despawn { who } => {
                if let Some(a) = self.runs[ri].actors[who.0 as usize].take() {
                    p.actors.remove(a, p.objects, &p.defs.objects);
                }
            }
            Step::Jet { id, obj, vel, rate, on } => {
                if on {
                    if let Some(def) = p.defs.object(obj) {
                        p.physics.set_jet(id as usize, def.at, vel, rate);
                    }
                } else {
                    p.physics.stop_jet(id as usize);
                }
            }
            Step::Flag(f) => self.runs[ri].flags |= 1 << (f % 8),
            Step::Remember(k) => match k {
                MemoryKey::Disturbances => self.memory.disturbances = self.memory.disturbances.saturating_add(1),
                MemoryKey::Helped => self.memory.helped = self.memory.helped.saturating_add(1),
                MemoryKey::Repairs => self.memory.repairs = self.memory.repairs.saturating_add(1),
            },
            Step::Branch { when, then, otherwise } => {
                let label = if self.test(&when, Some(ri), p) { then } else { otherwise };
                self.jump(ri, label, p);
            }
            Step::Choose(options) => {
                let total: u32 = options.iter().map(|o| o.0).sum();
                if total > 0 {
                    let run = &self.runs[ri];
                    let r = hash2((p.seed as u32) ^ (run.serial as u32).wrapping_mul(0x9E37_79B1), (run.stage * 131 + run.step) as u32);
                    let mut at = (r * total as f32) as u32;
                    let mut chosen = options[0].1;
                    for o in &options {
                        if at < o.0 {
                            chosen = o.1;
                            break;
                        }
                        at -= o.0;
                    }
                    self.jump(ri, chosen, p);
                }
            }
            Step::Goto(l) => self.jump(ri, l, p),
            Step::Wait(_) | Step::Await { .. } | Step::End => {}
        }
    }

    fn arrived_or_gone(&self, ri: usize, slot: Slot, p: &Parts) -> bool {
        match self.actor(ri, slot).and_then(|a| p.actors.get(a)) {
            Some(a) => matches!(a.state, ActorState::Arrived | ActorState::Failed(_) | ActorState::Idle),
            None => true,
        }
    }

    fn satisfied(&self, ri: usize, until: &Until, began: u64, p: &Parts) -> bool {
        let run = &self.runs[ri];
        match until {
            Until::Arrived(slots) => slots.iter().all(|s| self.arrived_or_gone(ri, *s, p)),
            Until::ObjectIs(o, d) => p.objects.get(*o).is_some_and(|s| s.damage == *d),
            Until::AtRest(o) => p.objects.get(*o).map_or(true, |s| s.at_rest() && !p.physics.bodies.iter().any(|b| matches!(b.kind, super::physics::BodyKind::Thrown(t) if t == *o))),
            Until::AreaFree(a) => p.actors.reserved_by(*a).is_none(),
            Until::Event(k, o) => run.heard.iter().any(|h| h.0 == *k && h.1.is_none_or(|x| o.is_none_or(|w| w == x)) && h.2 >= began),
            Until::Assisted => run.assisted,
            Until::Calm(secs) => (p.clock.tick - run.last_touched) as f64 * super::events::STEP >= *secs as f64,
        }
    }

    fn test(&self, c: &Cond, ri: Option<usize>, p: &Parts) -> bool {
        let run = ri.map(|i| &self.runs[i]);
        let v = match c.test {
            Test::Weather(w) => p.facts.holds(w),
            Test::ObjectIs(o, d) => p.objects.get(o).is_some_and(|s| s.damage == d),
            Test::Displaced(o) => p.objects.get(o).is_some_and(|s| s.pose.dx.abs() > 6.0 || s.pose.dy.abs() > 6.0),
            Test::Memory(k, n) => self.memory.get(k) >= n,
            Test::Failed(slot) => match run {
                Some(r) => r.failed[slot.0 as usize % SLOTS] || r.actors[slot.0 as usize % SLOTS].and_then(|a| p.actors.get(a)).map_or(true, |a| matches!(a.state, ActorState::Failed(_))),
                None => false,
            },
            Test::Assisted => run.is_some_and(|r| r.assisted),
            Test::Interfered => run.is_some_and(|r| r.interfered),
            Test::Flag(f) => run.is_some_and(|r| r.flags & (1 << (f % 8)) != 0),
            Test::Escalated(n) => run.is_some_and(|r| r.escalations >= n),
            Test::Weekday => p.facts.weekday,
        };
        v != c.not
    }

    /// For tests and the scene panel: what each run is doing.
    pub fn status(&self, defs: &Definition) -> Vec<(&'static str, &'static str, u8, bool)> {
        self.runs
            .iter()
            .filter_map(|r| {
                let s = defs.stories.get(r.story)?;
                Some((s.name, s.stages.get(r.stage).map_or("", |g| g.label), r.escalations, r.protected))
            })
            .collect()
    }

    /// Does this story hold any actors or reservations right now?
    pub fn holds_resources(&self, p: &Parts) -> bool {
        self.runs.iter().any(|r| r.actors.iter().flatten().count() > 0) || p.actors.reservation_count() > 0
    }

    pub fn run_count(&self) -> usize {
        self.runs.len()
    }
}
