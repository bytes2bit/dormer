//! The living scene: what in a view can change, who is about, what happens.
//!
//! One `StoryWorld` belongs to one output. It holds typed state for the view's
//! objects, a bounded event bus, the local physics, a few actors and a
//! sequence director, and nothing about pixels or Wayland. A view says what is
//! in it with a `Definition`; the shared behaviors and sequences here do the
//! rest, so a new incident is data and a small sequence, not new plumbing.
//!
//! Time is simulation time. `step` is given wall time, turns it into fixed
//! ticks (a held or hidden output simply is not stepped, and a long gap is one
//! frame's worth, not a catch-up), and everything below runs on those ticks.
//!
pub mod actors;
pub mod behaviors;
pub mod director;
pub mod draw;
pub mod events;
pub mod model;
pub mod physics;
pub mod replay;

pub use actors::ActorSystem;
pub use director::{Facts, TriggerResult};
pub use model::*;
pub use physics::Gesture;

use behaviors::{Behavior, BehaviorContext, Command, CommandBuffer, MINE};
use director::{Change, Director, Parts};
use events::{EventBus, EventKind, SimClock};
use physics::Physics;

#[cfg(test)]
use crate::gfx::P;

/// A rectangle of the design space (1600 by 1000) that the base picture must repaint.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct DesignRect {
    pub x0: f32,
    pub y0: f32,
    pub x1: f32,
    pub y1: f32,
}

impl DesignRect {
    fn from(b: [f32; 4]) -> DesignRect {
        DesignRect { x0: b[0], y0: b[1], x1: b[2], y1: b[3] }
    }

    fn touches(&self, o: &DesignRect, pad: f32) -> bool {
        self.x0 - pad < o.x1 && o.x0 - pad < self.x1 && self.y0 - pad < o.y1 && o.y0 - pad < self.y1
    }

    fn join(&self, o: &DesignRect) -> DesignRect {
        DesignRect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }

    fn shifted(&self, dx: f32, dy: f32) -> DesignRect {
        DesignRect { x0: self.x0 + dx, y0: self.y0 + dy, x1: self.x1 + dx, y1: self.y1 + dy }
    }
}

/// What the base picture shows of an object. When it changes, the picture is repainted there.
#[derive(Clone, Copy, PartialEq, Debug)]
struct Form {
    damage: Damage,
    shown: bool,
    open: bool,
    pose: (i32, i32, i32),
}

fn form_of(s: &ObjectState) -> Form {
    let shown = s.visible && s.damage != Damage::Missing && s.at_rest();
    // a pose is only part of the picture where the object rests; while it moves it is not in it
    let pose = if shown { ((s.pose.dx * 2.0).round() as i32, (s.pose.dy * 2.0).round() as i32, (s.pose.turn * 20.0).round() as i32) } else { (0, 0, 0) };
    Form { damage: s.damage, shown, open: s.open, pose }
}

struct Runtime {
    behavior: Box<dyn Behavior>,
    mine: [Option<ActorId>; MINE],
}

/// The world of one output.
pub struct StoryWorld {
    definition: Definition,
    limits: Limits,
    objects: ObjectStore,
    seed: u64,
    clock: SimClock,
    bus: EventBus,
    physics: Physics,
    actors: ActorSystem,
    director: Director,
    runtimes: Vec<Runtime>,
    facts: Facts,
    facts_changed: bool,
    forms: Vec<(ObjectId, Form)>,
    damage_seen: Vec<(ObjectId, Damage)>,
    changes: Vec<DesignRect>,
    heard: Vec<events::Event>,
    snow_clock: u32,
    /// the number of ticks run: for diagnostics and replay traces
    pub ticks_run: u64,
}

impl StoryWorld {
    pub fn new(definition: Definition, seed: u64) -> Result<StoryWorld, DefinitionError> {
        let limits = Limits::default();
        definition.validate(&limits)?;
        let mut w = StoryWorld {
            objects: ObjectStore::new(&definition.objects),
            clock: SimClock::new(),
            bus: EventBus::new(),
            physics: Physics::new(seed),
            actors: ActorSystem::new(),
            director: Director::new(&limits),
            runtimes: Vec::new(),
            facts: Facts::default(),
            facts_changed: true,
            forms: Vec::new(),
            damage_seen: Vec::new(),
            changes: Vec::new(),
            heard: Vec::new(),
            snow_clock: 0,
            ticks_run: 0,
            definition,
            limits,
            seed,
        };
        w.build_runtimes();
        w.forms = w.objects.iter().map(|(id, s)| (id, form_of(s))).collect();
        w.damage_seen = w.objects.iter().map(|(id, s)| (id, s.damage)).collect();
        Ok(w)
    }

    fn build_runtimes(&mut self) {
        let mut rt: Vec<Runtime> = self.definition.behaviors.iter().map(|b| Runtime { behavior: b.build(), mine: [None; MINE] }).collect();
        rt.extend(self.definition.custom.iter().map(|f| Runtime { behavior: f(), mine: [None; MINE] }));
        self.runtimes = rt;
    }

    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot::new(&self.objects)
    }

    pub fn definition(&self) -> &Definition {
        &self.definition
    }

    pub fn actors(&self) -> &ActorSystem {
        &self.actors
    }

    pub fn physics(&self) -> &Physics {
        &self.physics
    }

    pub fn director(&self) -> &Director {
        &self.director
    }

    pub fn objects(&self) -> &ObjectStore {
        &self.objects
    }

    /// Helpers for the tests of each view's stories.
    #[cfg(test)]
    pub fn bus_emit_impact(&mut self, obj: ObjectId, origin: u64) {
        let at = self.definition.object(obj).map_or((0.0, 0.0), |d| d.at);
        self.bus.emit(events::Event::new(EventKind::Impact, obj, at, origin));
    }

    #[cfg(test)]
    pub fn bus_emit_obstruction(&mut self, obj: ObjectId, at: P, origin: u64) {
        self.bus.emit(events::Event::new(EventKind::Obstruction, obj, at, origin));
    }

    #[cfg(test)]
    pub fn place_object(&mut self, obj: ObjectId, at: P) {
        if let (Some(d), Some(s)) = (self.definition.object(obj), self.objects.get_mut(obj)) {
            s.pose = Pose { dx: at.0 - d.at.0, dy: at.1 - d.at.1, turn: 0.0 };
        }
    }

    /// For tests that set a scene up by hand.
    #[cfg(test)]
    pub fn objects_mut(&mut self) -> &mut ObjectStore {
        &mut self.objects
    }

    /// Simulation time, in seconds.
    pub fn time(&self) -> f64 {
        self.clock.seconds()
    }

    pub fn tick(&self) -> u64 {
        self.clock.tick
    }

    /// The view laid itself out again (a resize): keep the state of every object whose ID
    /// is still there, and the stories and actors that are in progress. On error nothing changes.
    pub fn rebind(&mut self, definition: Definition) -> Result<(), DefinitionError> {
        definition.validate(&self.limits)?;
        self.objects.rebind(&definition.objects);
        self.director.rebind(&self.definition, &definition);
        // the behaviors keep their actors if they are the same behaviors in the same order
        let same = definition.behaviors.len() + definition.custom.len() == self.runtimes.len();
        self.definition = definition;
        if !same {
            for r in self.runtimes.drain(..) {
                for a in r.mine.iter().flatten() {
                    self.actors.remove(*a, &mut self.objects, &self.definition.objects);
                }
            }
            self.build_runtimes();
        }
        // nothing is repainted for a move that only the layout made; the new base is painted
        // whole for the new size by whoever rebound
        self.forms = self.objects.iter().map(|(id, s)| (id, form_of(s))).collect();
        self.damage_seen = self.objects.iter().map(|(id, s)| (id, s.damage)).collect();
        self.changes.clear();
        Ok(())
    }

    /// Everything goes back to an ordinary scene: stories end, actors leave, objects are whole.
    pub fn reset(&mut self) {
        let seed = self.seed;
        let defs = self.definition.clone();
        let facts = self.facts;
        let objects_before: Vec<(ObjectId, Form)> = self.forms.clone();
        *self = StoryWorld::new(defs, seed).expect("a definition that was valid stays valid");
        self.facts = facts;
        // the picture must repaint what had been changed
        for (id, before) in objects_before {
            let ordinary = self.definition.object(id).map_or(ObjectState::default(), ObjectStore::fresh);
            if before != form_of(&ordinary) {
                if let Some(d) = self.definition.object(id) {
                    self.changes.push(DesignRect::from(d.bounds));
                }
            }
        }
    }

    pub fn gesture(&mut self, g: Gesture) {
        self.physics.gesture(g, &self.definition.objects, &mut self.objects, &mut self.bus);
    }

    pub fn trigger(&mut self, id: StoryId) -> TriggerResult {
        let mut parts = Parts {
            defs: &self.definition,
            clock: &self.clock,
            seed: self.seed,
            actors: &mut self.actors,
            objects: &mut self.objects,
            physics: &mut self.physics,
            bus: &mut self.bus,
            facts: &self.facts,
        };
        self.director.trigger(id, &mut parts)
    }

    /// Time has passed on the wall clock. Returns the number of ticks that ran.
    pub fn step_facts(&mut self, dt: f64, facts: &Facts) -> u32 {
        if *facts != self.facts {
            self.facts = *facts;
            self.facts_changed = true;
            // the wind that carries a sheet is the wind that is blowing
            self.physics.wind = Some(facts.wind_x * 150.0);
        }
        let n = self.clock.advance(dt);
        for _ in 0..n {
            self.tick_once();
        }
        n
    }

    /// Bring the world up to date after `dt` seconds of wall time. A long gap on a visible
    /// output (a wait for the next job) runs in chunks; at most two minutes of it, so no gap
    /// ever becomes an unbounded catch-up. Hidden or held outputs do not call this at all.
    pub fn advance(&mut self, dt: f64, scene: &crate::scene::Scene) {
        let facts = facts_of(scene);
        let mut left = if dt.is_finite() { dt.clamp(0.0, 120.0) } else { 0.0 };
        while left > 1e-9 {
            let chunk = left.min(events::MAX_FRAME);
            self.step_facts(chunk, &facts);
            left -= chunk;
        }
    }

    pub fn set_paused(&mut self, paused: bool) {
        self.clock.paused = paused;
    }

    pub fn paused(&self) -> bool {
        self.clock.paused
    }

    /// Advance exactly one tick whether or not the world is paused (the panel's Step).
    pub fn step_once(&mut self) {
        self.clock.tick += 1;
        self.tick_once_inner();
    }

    fn tick_once(&mut self) {
        self.tick_once_inner();
    }

    fn tick_once_inner(&mut self) {
        self.ticks_run += 1;
        let defs = &self.definition;
        // snow settles on the spots authored for it
        self.snow_clock += 1;
        if self.snow_clock >= 30 {
            self.snow_clock = 0;
            if self.facts.snow {
                self.physics.snowfall(&defs.surfaces, 0.15);
            }
        }
        self.physics.step(&defs.surfaces, &mut self.objects, &defs.objects, &mut self.bus);
        self.actors.step(physics::STEP, &defs.routes, &mut self.objects, &defs.objects, &mut self.bus);
        // what was put down becomes a physical thing
        for (obj, at) in self.actors.take_drops() {
            if let Some(def) = defs.object(obj) {
                let body = physics::Body::new_thrown(obj, at, def.material);
                self.physics.spawn(body);
            }
        }

        self.heard.clear();
        self.heard.extend(self.bus.drain());
        let heard = std::mem::take(&mut self.heard);
        {
            let mut parts = Parts {
                defs,
                clock: &self.clock,
                seed: self.seed,
                actors: &mut self.actors,
                objects: &mut self.objects,
                physics: &mut self.physics,
                bus: &mut self.bus,
                facts: &self.facts,
            };
            for e in &heard {
                self.director.hear(e, &mut parts);
                // a part put back near what it belongs to is a helping hand
                if e.kind == EventKind::Obstruction {
                    let mut helped = Vec::new();
                    for s in &parts.defs.stories {
                        for (part, near, within) in &s.assists {
                            if *part == e.source {
                                if let Some(n) = parts.defs.object(*near) {
                                    let d2 = (n.at.0 - e.at.0).powi(2) + (n.at.1 - e.at.1).powi(2);
                                    if d2 <= within * within {
                                        helped.push(s.id);
                                    }
                                }
                            }
                        }
                    }
                    for id in helped {
                        self.director.assisted(id, &mut parts);
                    }
                }
                // named triggers on an event
                for s in &parts.defs.stories {
                    if let director::Trigger::Event(kind, obj) = &s.trigger {
                        if *kind == e.kind && *obj == e.source && e.tags & (1 << 7) == 0 && !self.director.running(s.id, parts.defs) {
                            let id = s.id;
                            self.director.trigger(id, &mut parts);
                        }
                    }
                }
            }
            // damage that has just been done may start a story of its own
            let mut started = Vec::new();
            for (id, s) in parts.objects.iter() {
                let seen = self.damage_seen.iter().find(|d| d.0 == id).map(|d| d.1).unwrap_or(Damage::Intact);
                if s.damage != seen && s.damage != Damage::Intact {
                    for st in &parts.defs.stories {
                        if let director::Trigger::Damage(obj, level) = &st.trigger {
                            if *obj == id && damage_rank(s.damage) >= damage_rank(*level) {
                                started.push(st.id);
                            }
                        }
                    }
                }
            }
            for id in started {
                if !self.director.running(id, parts.defs) {
                    self.director.trigger(id, &mut parts);
                }
            }
            self.director.schedule(&mut parts);
            self.director.step(&mut parts);
        }
        self.damage_seen = self.objects.iter().map(|(id, s)| (id, s.damage)).collect();

        // behaviors
        let wake_all = std::mem::take(&mut self.facts_changed);
        let tick = self.clock.tick;
        let mut commands: Vec<(usize, Vec<Command>)> = Vec::new();
        for (i, rt) in self.runtimes.iter_mut().enumerate() {
            let due = rt.behavior.wake_at().is_some_and(|t| t <= tick);
            if !(wake_all || due || !heard.is_empty() || !rt.behavior.asleep()) {
                continue;
            }
            let ctx = BehaviorContext {
                defs,
                objects: Snapshot::new(&self.objects),
                actors: &self.actors,
                facts: &self.facts,
                tick,
                events: &heard,
                mine: &rt.mine,
                seed: self.seed,
            };
            let mut out = CommandBuffer::new();
            rt.behavior.update(&ctx, &mut out);
            if !out.is_empty() {
                commands.push((i, out.take()));
            }
        }
        self.heard = heard;
        let _ = defs;
        for (i, cmds) in commands {
            for c in cmds {
                self.apply(i, c);
            }
        }

        // what the base picture shows has changed here
        let mut shifts: Vec<DesignRect> = Vec::new();
        for (id, s) in self.objects.iter() {
            let now = form_of(s);
            if let Some(slot) = self.forms.iter_mut().find(|f| f.0 == id) {
                if slot.1 != now {
                    if let Some(d) = self.definition.object(id) {
                        let b = DesignRect::from(d.bounds);
                        // where it was, and where it is: the bounds move with the pose
                        let (old, new) = (slot.1.pose, now.pose);
                        shifts.push(b.shifted(old.0 as f32 / 2.0, old.1 as f32 / 2.0));
                        shifts.push(b.shifted(new.0 as f32 / 2.0, new.1 as f32 / 2.0));
                    }
                    slot.1 = now;
                }
            }
        }
        for r in shifts {
            self.mark(r);
        }
    }

    fn mark(&mut self, r: DesignRect) {
        // joined with a neighbour when they touch; otherwise kept apart
        for c in self.changes.iter_mut() {
            if c.touches(&r, 6.0) {
                *c = c.join(&r);
                return;
            }
        }
        if self.changes.len() < 32 {
            self.changes.push(r);
        } else if let Some(c) = self.changes.first_mut() {
            *c = c.join(&r);
        }
    }

    fn apply(&mut self, who: usize, c: Command) {
        match c {
            Command::Spawn { slot, spec } => {
                let rt = &mut self.runtimes[who];
                let slot = slot as usize % MINE;
                if let Some(old) = rt.mine[slot].take() {
                    self.actors.remove(old, &mut self.objects, &self.definition.objects);
                }
                rt.mine[slot] = self.actors.spawn(spec, &self.definition.routes).ok();
            }
            Command::Despawn(slot) => {
                if let Some(a) = self.runtimes[who].mine[slot as usize % MINE].take() {
                    self.actors.remove(a, &mut self.objects, &self.definition.objects);
                }
            }
            Command::Actor(id, cmd) => {
                let _ = self.actors.command(id, cmd, &self.definition.routes, &mut self.objects);
            }
            Command::Object(obj, change) => {
                if change == Change::Restore {
                    self.physics.forget(obj);
                    self.actors.release_object(obj);
                }
                director::apply_change(&mut self.objects, &self.definition, obj, change, None);
            }
            Command::Emit(e) => {
                self.bus.emit(e);
            }
            Command::Trigger(id) => {
                self.trigger(id);
            }
        }
    }

    /// Does anything need frames now: someone is moving or working, a body is falling.
    pub fn active(&self) -> bool {
        !self.clock.paused && (self.physics.active() || self.actors.count() > 0 || !self.bus.is_empty())
    }

    /// Simulation seconds at which something must next be looked at even if nothing is moving
    /// (a job falls due, a behavior wakes). None when the scene will sit quietly until disturbed.
    pub fn next_deadline(&self) -> Option<f64> {
        let mut at: Option<u64> = self.director.next_deadline();
        for rt in &self.runtimes {
            if let Some(t) = rt.behavior.wake_at() {
                at = Some(at.map_or(t, |a| a.min(t)));
            }
        }
        at.map(SimClock::at)
    }

    /// The parts of the picture that must be repainted, in design units, joined up. Taking them
    /// clears them.
    pub fn take_base_changes(&mut self) -> Vec<DesignRect> {
        std::mem::take(&mut self.changes)
    }

    pub fn has_base_changes(&self) -> bool {
        !self.changes.is_empty()
    }

    /// Where an object is, at its resting place plus its displacement.
    #[cfg(test)]
    pub fn place(&self, id: ObjectId) -> Option<P> {
        let d = self.definition.object(id)?;
        let s = self.objects.get(id)?;
        Some((d.at.0 + s.pose.dx, d.at.1 + s.pose.dy))
    }

    /// The things flying or lying about, for drawing.
    pub fn bodies(&self) -> &[physics::Body] {
        &self.physics.bodies
    }

}

fn damage_rank(d: Damage) -> u8 {
    match d {
        Damage::Intact => 0,
        Damage::Damaged => 1,
        Damage::Broken => 2,
        Damage::Missing => 3,
    }
}

/// What a story may ask about the sky and the hour.
pub fn facts_of(sc: &crate::scene::Scene) -> Facts {
    Facts {
        rain: sc.rain > 0.15,
        snow: sc.snow > 0.15,
        wind: sc.wind,
        wind_x: sc.wind_x,
        storm: sc.storm,
        night: sc.dark > 0.55,
        cold: sc.temp < 3.0,
        hour: sc.t as f32,
        season: sc.season as u8,
        weekday: sc.dow < 5,
    }
}

/// The pixels of the screen that a design-space region covers, with `margin` design units and
/// the bloom's own few pixels round it, for the light that spills from what is in it.
pub fn pixel_rect(r: &DesignRect, lay: &crate::props::Layout, margin: f32) -> crate::gfx::Rect {
    // the bloom works in cells of four pixels and blurs a little: three cells of it is enough
    const BLOOM_PX: f32 = 12.0;
    let (x0, y0) = (lay.dx(r.x0 - margin) - BLOOM_PX, lay.dy(r.y0 - margin) - BLOOM_PX);
    let (x1, y1) = (lay.dx(r.x1 + margin) + BLOOM_PX, lay.dy(r.y1 + margin) + BLOOM_PX);
    crate::gfx::Rect { x0: x0.floor() as i32, y0: y0.floor() as i32, x1: x1.ceil() as i32, y1: y1.ceil() as i32 }
}

/// How far, in design units, the glow of a lit object reaches round its bounds.
pub const LIGHT_MARGIN: f32 = 26.0;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod render_tests;
#[cfg(test)]
mod acceptance;
#[cfg(test)]
mod example;
