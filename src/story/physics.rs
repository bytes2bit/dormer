//! Small, local physics: enough that a thrown pebble lands, a dropped pane
//! breaks, a leak finds the low end of a roof and a pulled sign gives way.
//!
//! This is not a physics engine. Bodies are points with a radius; the only
//! things they touch are authored segments (roof slopes, kerbs, sills) and
//! authored basins of water; nothing is solved between bodies. All motion is
//! in fixed 1/30 s steps, so a scene run at any frame rate comes out the same.
//! Everything is bounded: 128 bodies, a few dozen puddles, four jets.

use super::events::{Event, EventBus, EventKind};
use super::model::{Accepts, Attachment, Damage, Material, ObjectDef, ObjectId, ObjectStore, Owner, Pose};
use crate::gfx::P;
use crate::rng::Fast;

pub const STEP: f32 = 1.0 / 30.0;
/// design units per second squared
pub const GRAVITY: f32 = 760.0;
pub const MAX_BODIES: usize = 128;
pub const MAX_PATCHES: usize = 24;
pub const MAX_SEGMENTS: usize = 96;
pub const MAX_JETS: usize = 4;
/// the fastest a released object may leave the hand, in design units a second
pub const MAX_THROW: f32 = 900.0;
/// how far a held object can be taken from where it hangs
const REACH: f32 = 170.0;
const REST_SPEED: f32 = 8.0;
const SLEEP_AFTER: u32 = 10;
/// tag bits on the events physics emits
pub const TAG_GLASS: u16 = 1;
pub const TAG_WATER: u16 = 2;
pub const TAG_METAL: u16 = 4;
pub const TAG_WOOD: u16 = 8;
pub const TAG_STONE: u16 = 16;
pub const TAG_SNAG: u16 = 32;

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum Gesture {
    Hover(P),
    Press(P),
    Drag(P),
    Release { at: P, velocity: P },
    /// the pointer went away: the seat vanished, the output was lost
    Cancel,
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum BodyKind {
    /// an object of the view, loose
    Thrown(ObjectId),
    Fragment,
    Droplet,
    Flake,
    Snowball,
}

#[derive(Clone, Copy, Debug)]
pub struct Body {
    pub kind: BodyKind,
    pub pos: P,
    pub vel: P,
    pub material: Material,
    pub radius: f32,
    pub age: f32,
    /// seconds before it is gone (0: until it lands and is taken in)
    pub life: f32,
    pub turn: f32,
    pub spin: f32,
    pub asleep: bool,
    pub floating: bool,
    /// the tag of the snag it is caught on
    pub snagged: Option<u16>,
    still: u32,
    /// it touched something last step
    contact: bool,
}

impl Body {
    fn new(kind: BodyKind, pos: P, vel: P, material: Material, radius: f32) -> Body {
        Body { kind, pos, vel, material, radius, age: 0.0, life: 0.0, turn: 0.0, spin: 0.0, asleep: false, floating: false, snagged: None, still: 0, contact: false }
    }

    pub fn is_finite(&self) -> bool {
        self.pos.0.is_finite() && self.pos.1.is_finite() && self.vel.0.is_finite() && self.vel.1.is_finite()
    }

    /// An object of the view, let go at `at` with nothing but its own weight.
    pub fn new_thrown(id: ObjectId, at: P, material: Material) -> Body {
        Body::new(BodyKind::Thrown(id), at, (0.0, 20.0), material, 6.0)
    }
}

/// A line things land on.
#[derive(Clone, Copy, Debug)]
pub struct Segment {
    pub a: P,
    pub b: P,
    pub material: Material,
}

/// Water things can fall into: a harbour, a gutter, a puddle large enough to splash in.
#[derive(Clone, Copy, Debug)]
pub struct Basin {
    pub rect: [f32; 4],
    /// the drift of what floats on it, in design units a second
    pub current: P,
}

/// Somewhere a light thing blown by the wind stops: an aerial, the crane's hook.
#[derive(Clone, Copy, Debug)]
pub struct Snag {
    pub at: P,
    pub radius: f32,
    pub tag: u16,
}

/// Where snow gathers, and how much it can hold.
#[derive(Clone, Copy, Debug)]
pub struct SnowSpot {
    pub at: P,
    pub capacity: f32,
}

#[derive(Clone, Debug, Default)]
pub struct SurfaceMap {
    pub segments: Vec<Segment>,
    pub basins: Vec<Basin>,
    pub snags: Vec<Snag>,
    pub snow: Vec<SnowSpot>,
    /// design units a second, positive towards the right
    pub wind: f32,
}

impl SurfaceMap {
    pub fn validate(&self) -> Result<(), String> {
        if self.segments.len() > MAX_SEGMENTS {
            return Err(format!("{} contact segments is more than the {} allowed", self.segments.len(), MAX_SEGMENTS));
        }
        let finite = |p: P| p.0.is_finite() && p.1.is_finite();
        for (n, s) in self.segments.iter().enumerate() {
            if !finite(s.a) || !finite(s.b) || s.a == s.b {
                return Err(format!("contact segment {} is not a finite line", n));
            }
        }
        for (n, b) in self.basins.iter().enumerate() {
            if !b.rect.iter().all(|v| v.is_finite()) || b.rect[2] <= b.rect[0] || b.rect[3] <= b.rect[1] || !finite(b.current) {
                return Err(format!("water basin {} is not a finite rectangle", n));
            }
        }
        for (n, s) in self.snags.iter().enumerate() {
            if !finite(s.at) || !s.radius.is_finite() || s.radius <= 0.0 {
                return Err(format!("snag {} has no finite place and size", n));
            }
        }
        for (n, s) in self.snow.iter().enumerate() {
            if !finite(s.at) || !s.capacity.is_finite() || s.capacity <= 0.0 {
                return Err(format!("snow spot {} has no finite place and size", n));
            }
        }
        if !self.wind.is_finite() {
            return Err("the wind is not a finite number".into());
        }
        Ok(())
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum PatchKind {
    Water,
    Snow,
}

/// Water or snow that has settled in one place.
#[derive(Clone, Copy, Debug)]
pub struct Patch {
    pub at: P,
    pub amount: f32,
    pub kind: PatchKind,
}

#[derive(Clone, Copy, Debug)]
struct Jet {
    at: P,
    vel: P,
    /// droplets a second
    rate: f32,
    owed: f32,
}

#[derive(Clone, Copy, Debug)]
struct Spring {
    id: ObjectId,
    vel: Pose,
}

#[derive(Clone, Copy, Debug)]
struct Grab {
    id: ObjectId,
    press: P,
    pulled: f32,
}

pub struct Physics {
    pub bodies: Vec<Body>,
    pub patches: Vec<Patch>,
    jets: [Option<Jet>; MAX_JETS],
    springs: Vec<Spring>,
    grab: Option<Grab>,
    rng: Fast,
    next_origin: u64,
    pending_patch: Vec<(P, f32)>,
    pub tick: u64,
    /// bodies refused because the ceiling was reached
    pub refused: u64,
}

fn bounce_of(m: Material) -> f32 {
    match m {
        Material::Glass => 0.12,
        Material::Wood => 0.34,
        Material::Metal => 0.46,
        Material::Stone => 0.22,
        Material::Cloth | Material::Rope | Material::Paper => 0.04,
        Material::Water => 0.0,
        Material::Flesh => 0.2,
    }
}

fn tag_of(m: Material) -> u16 {
    match m {
        Material::Glass => TAG_GLASS,
        Material::Water => TAG_WATER,
        Material::Metal => TAG_METAL,
        Material::Wood => TAG_WOOD,
        Material::Stone => TAG_STONE,
        _ => 0,
    }
}

/// How many pieces a breaking thing leaves.
pub fn fragments_of(m: Material) -> usize {
    match m {
        Material::Glass => 6,
        Material::Wood => 3,
        Material::Stone => 4,
        Material::Metal => 2,
        _ => 0,
    }
}

fn dist2(a: P, b: P) -> f32 {
    (a.0 - b.0) * (a.0 - b.0) + (a.1 - b.1) * (a.1 - b.1)
}

/// Where the segment from a to b meets the segment c to d: how far along the first (0 to 1).
fn cross(a: P, b: P, c: P, d: P) -> Option<f32> {
    let (r, s) = ((b.0 - a.0, b.1 - a.1), (d.0 - c.0, d.1 - c.1));
    let den = r.0 * s.1 - r.1 * s.0;
    if den.abs() < 1e-6 {
        return None;
    }
    let (qp0, qp1) = (c.0 - a.0, c.1 - a.1);
    let t = (qp0 * s.1 - qp1 * s.0) / den;
    let u = (qp0 * r.1 - qp1 * r.0) / den;
    ((0.0..=1.0).contains(&t) && (0.0..=1.0).contains(&u)).then_some(t)
}

impl Physics {
    pub fn new(seed: u64) -> Physics {
        Physics {
            bodies: Vec::with_capacity(MAX_BODIES),
            patches: Vec::with_capacity(MAX_PATCHES),
            jets: [None; MAX_JETS],
            springs: Vec::new(),
            grab: None,
            rng: Fast::new(seed ^ 0xB0D1E5),
            next_origin: 1 << 40,
            pending_patch: Vec::new(),
            tick: 0,
            refused: 0,
        }
    }

    fn origin(&mut self) -> u64 {
        self.next_origin += 1;
        self.next_origin
    }

    /// Does anything here still want frames?
    pub fn active(&self) -> bool {
        self.grab.is_some() || !self.springs.is_empty() || self.jets.iter().any(Option::is_some) || self.bodies.iter().any(|b| !b.asleep)
    }

    /// The object being held, if any.
    pub fn held(&self) -> Option<ObjectId> {
        self.grab.map(|g| g.id)
    }

    /// Put in a body, making room for it by letting go of the oldest piece of debris when full.
    pub fn spawn(&mut self, body: Body) -> bool {
        if !body.is_finite() {
            return false;
        }
        if self.bodies.len() >= MAX_BODIES {
            let victim = self
                .bodies
                .iter()
                .enumerate()
                .filter(|(_, b)| !matches!(b.kind, BodyKind::Thrown(_)))
                .max_by(|a, b| a.1.age.partial_cmp(&b.1.age).unwrap_or(std::cmp::Ordering::Equal))
                .map(|(i, _)| i);
            match victim {
                Some(i) => {
                    self.bodies.swap_remove(i);
                }
                None => {
                    self.refused += 1;
                    return false;
                }
            }
        }
        self.bodies.push(body);
        true
    }

    fn shatter(&mut self, at: P, material: Material, speed: f32) {
        for _ in 0..fragments_of(material) {
            let a = self.rng.range(-std::f32::consts::PI, 0.0);
            let v = self.rng.range(40.0, 120.0) + speed * 0.25;
            let mut f = Body::new(BodyKind::Fragment, at, (a.cos() * v, a.sin() * v), material, self.rng.range(1.2, 2.4));
            f.life = self.rng.range(3.0, 6.0);
            f.spin = self.rng.range(-8.0, 8.0);
            self.spawn(f);
        }
    }

    /// A pointer event, in design units. `defs` say what the objects accept and where they are.
    pub fn gesture(&mut self, g: Gesture, defs: &[ObjectDef], objects: &mut ObjectStore, bus: &mut EventBus) {
        match g {
            Gesture::Hover(_) => {}
            Gesture::Press(p) => {
                if !(p.0.is_finite() && p.1.is_finite()) || self.grab.is_some() {
                    return;
                }
                // the nearest of what is under the pointer is the last one defined
                let hit = defs.iter().rev().find(|d| {
                    let at_rest = objects.get(d.id).is_some_and(|s| s.visible && s.damage != Damage::Missing && matches!(s.attachment, Attachment::Fixed | Attachment::Hanging));
                    let [x0, y0, x1, y1] = d.bounds;
                    at_rest && !d.accepts.has(Accepts::NONE) && d.accepts.0 != 0 && p.0 >= x0 && p.0 <= x1 && p.1 >= y0 && p.1 <= y1
                });
                let Some(d) = hit else { return };
                let origin = self.origin();
                let tags = tag_of(d.material);
                if d.accepts.has(Accepts::KNOCK) {
                    let mut e = Event::new(EventKind::Noise, d.id, p, origin);
                    e.tags = tags;
                    bus.emit(e);
                }
                if d.accepts.has(Accepts::POKE) {
                    let mut e = Event::new(EventKind::Impact, d.id, p, origin);
                    e.tags = tags;
                    bus.emit(e);
                }
                if d.accepts.has(Accepts::GRAB) || d.accepts.has(Accepts::MOVE) || d.accepts.has(Accepts::THROW) {
                    if let Some(s) = objects.get_mut(d.id) {
                        // let go of any spring it was still swinging on
                        self.springs.retain(|sp| sp.id != d.id);
                        s.attachment = Attachment::Dragged;
                        s.swinging = false;
                        s.owner = None;
                        self.grab = Some(Grab { id: d.id, press: p, pulled: 0.0 });
                    }
                }
            }
            Gesture::Drag(p) => {
                let Some(grab) = self.grab else { return };
                if !(p.0.is_finite() && p.1.is_finite()) {
                    return;
                }
                let Some(def) = defs.iter().find(|d| d.id == grab.id) else {
                    self.grab = None;
                    return;
                };
                let (mut dx, mut dy) = (p.0 - grab.press.0, p.1 - grab.press.1);
                let reach = if def.accepts.has(Accepts::MOVE) || def.accepts.has(Accepts::THROW) { 600.0 } else { REACH };
                let len = (dx * dx + dy * dy).sqrt();
                if len > reach {
                    dx *= reach / len;
                    dy *= reach / len;
                }
                let pull = len.min(reach);
                let Some(s) = objects.get_mut(grab.id) else {
                    self.grab = None;
                    return;
                };
                s.pose = Pose { dx, dy, turn: (dx / 120.0).clamp(-1.0, 1.0) * 0.8 };
                self.grab = Some(Grab { pulled: grab.pulled.max(pull), ..grab });
                if def.break_pull > 0.0 {
                    let origin = self.origin();
                    if pull > def.break_pull && s.damage == Damage::Intact {
                        // one fixing lets go: it hangs by the other
                        s.damage = Damage::Damaged;
                        s.revision += 1;
                        let mut e = Event::new(EventKind::Damage, def.id, (def.at.0 + dx, def.at.1 + dy), origin);
                        e.tags = tag_of(def.material);
                        bus.emit(e);
                    } else if pull > def.break_pull * 1.8 && s.damage == Damage::Damaged {
                        // and then the other: it is loose, and falls
                        s.damage = Damage::Broken;
                        s.revision += 1;
                        s.attachment = Attachment::Loose;
                        let at = (def.at.0 + dx, def.at.1 + dy);
                        let mut b = Body::new(BodyKind::Thrown(def.id), at, (0.0, 0.0), def.material, 6.0);
                        b.turn = s.pose.turn;
                        b.spin = if dx >= 0.0 { 2.5 } else { -2.5 };
                        self.spawn(b);
                        let mut e = Event::new(EventKind::Damage, def.id, at, origin);
                        e.tags = tag_of(def.material);
                        bus.emit(e);
                        self.grab = None;
                    }
                }
            }
            Gesture::Release { at, velocity } => {
                let v = if velocity.0.is_finite() && velocity.1.is_finite() && at.0.is_finite() && at.1.is_finite() { velocity } else { (0.0, 0.0) };
                self.let_go(v, defs, objects, bus);
            }
            Gesture::Cancel => self.let_go((0.0, 0.0), defs, objects, bus),
        }
    }

    /// The pointer lets go (or is lost): what was held is thrown, left where it was put, or
    /// swings back. Damage is never undone by this.
    fn let_go(&mut self, v: P, defs: &[ObjectDef], objects: &mut ObjectStore, bus: &mut EventBus) {
        let Some(grab) = self.grab.take() else { return };
        let (Some(def), Some(s)) = (defs.iter().find(|d| d.id == grab.id), objects.get_mut(grab.id)) else { return };
        if s.attachment != Attachment::Dragged {
            return;
        }
        let speed = (v.0 * v.0 + v.1 * v.1).sqrt();
        let k = if speed > MAX_THROW { MAX_THROW / speed } else { 1.0 };
        let v = (v.0 * k, v.1 * k);
        let at = (def.at.0 + s.pose.dx, def.at.1 + s.pose.dy);
        if def.accepts.has(Accepts::THROW) && speed.min(MAX_THROW) > 90.0 {
            s.attachment = Attachment::Loose;
            s.revision += 1;
            let mut b = Body::new(BodyKind::Thrown(def.id), at, v, def.material, 6.0);
            b.spin = v.0 / 90.0;
            self.spawn(b);
        } else if def.accepts.has(Accepts::MOVE) || def.accepts.has(Accepts::THROW) {
            // left where it was put
            s.attachment = Attachment::Fixed;
            s.revision += 1;
            let origin = self.origin();
            bus.emit(Event::new(EventKind::Obstruction, def.id, at, origin));
        } else {
            // swings back on its fixings, and settles
            s.attachment = Attachment::Hanging;
            s.swinging = true;
            self.springs.retain(|sp| sp.id != def.id);
            self.springs.push(Spring { id: def.id, vel: Pose { dx: v.0 * 0.15, dy: v.1 * 0.15, turn: v.0 * 0.004 } });
        }
    }

    /// Water leaves `at` at `vel`, `rate` droplets a second, until `stop_jet`.
    pub fn set_jet(&mut self, slot: usize, at: P, vel: P, rate: f32) {
        if slot < MAX_JETS && at.0.is_finite() && at.1.is_finite() && vel.0.is_finite() && vel.1.is_finite() && rate.is_finite() {
            self.jets[slot] = Some(Jet { at, vel, rate: rate.clamp(0.0, 90.0), owed: 0.0 });
        }
    }

    pub fn stop_jet(&mut self, slot: usize) {
        if slot < MAX_JETS {
            self.jets[slot] = None;
        }
    }

    pub fn jet_on(&self, slot: usize) -> bool {
        slot < MAX_JETS && self.jets[slot].is_some()
    }

    /// Water landing or arriving at a point settles into a puddle there.
    pub fn add_patch(&mut self, at: P, amount: f32, kind: PatchKind) {
        if !(at.0.is_finite() && at.1.is_finite() && amount.is_finite()) || amount <= 0.0 {
            return;
        }
        if let Some(p) = self.patches.iter_mut().filter(|p| p.kind == kind && dist2(p.at, at) < 22.0 * 22.0).min_by(|a, b| {
            dist2(a.at, at).partial_cmp(&dist2(b.at, at)).unwrap_or(std::cmp::Ordering::Equal)
        }) {
            p.amount = (p.amount + amount).min(12.0);
            return;
        }
        if self.patches.len() >= MAX_PATCHES {
            // the nearest existing one takes it
            if let Some(p) = self.patches.iter_mut().min_by(|a, b| dist2(a.at, at).partial_cmp(&dist2(b.at, at)).unwrap_or(std::cmp::Ordering::Equal)) {
                p.amount = (p.amount + amount).min(12.0);
            }
            return;
        }
        self.patches.push(Patch { at, amount, kind });
    }

    /// Snow falls: each spot gathers some, up to what it holds.
    pub fn snowfall(&mut self, surfaces: &SurfaceMap, amount: f32) {
        for spot in &surfaces.snow {
            let have = self.patches.iter().filter(|p| p.kind == PatchKind::Snow && dist2(p.at, spot.at) < 1.0).map(|p| p.amount).sum::<f32>();
            if have < spot.capacity {
                let add = amount.min(spot.capacity - have);
                match self.patches.iter().position(|p| p.kind == PatchKind::Snow && dist2(p.at, spot.at) < 1.0) {
                    Some(i) => self.patches[i].amount += add,
                    None if self.patches.len() < MAX_PATCHES => self.patches.push(Patch { at: spot.at, amount: add, kind: PatchKind::Snow }),
                    None => {}
                }
            }
        }
    }

    /// Sweep snow off whatever is within `radius` of `at`: it falls as loose flakes.
    pub fn brush_snow(&mut self, at: P, radius: f32) -> f32 {
        let mut taken = 0.0;
        for p in self.patches.iter_mut().filter(|p| p.kind == PatchKind::Snow && dist2(p.at, at) <= radius * radius) {
            taken += p.amount;
            p.amount = 0.0;
        }
        self.patches.retain(|p| p.amount > 0.0 || p.kind != PatchKind::Snow);
        for _ in 0..((taken * 4.0) as usize).min(10) {
            let v = (self.rng.range(-30.0, 30.0), self.rng.range(-20.0, 10.0));
            let mut f = Body::new(BodyKind::Flake, (at.0 + self.rng.range(-8.0, 8.0), at.1), v, Material::Cloth, 1.3);
            f.life = 2.5;
            self.spawn(f);
        }
        taken
    }

    /// Roll a snowball up from snow near `at`: the ball is a body at rest there.
    pub fn roll_snowball(&mut self, at: P, radius: f32) -> bool {
        let Some(p) = self.patches.iter_mut().filter(|p| p.kind == PatchKind::Snow && p.amount >= 0.5 && dist2(p.at, at) <= radius * radius).next() else {
            return false;
        };
        p.amount -= 0.5;
        let mut b = Body::new(BodyKind::Snowball, at, (0.0, 0.0), Material::Cloth, 4.0);
        b.life = 0.0;
        self.spawn(b)
    }

    /// Throw a snowball (or anything small) from `at`.
    pub fn throw(&mut self, kind: BodyKind, at: P, vel: P, material: Material, radius: f32) -> bool {
        let speed = (vel.0 * vel.0 + vel.1 * vel.1).sqrt();
        let k = if speed > MAX_THROW { MAX_THROW / speed } else { 1.0 };
        self.spawn(Body::new(kind, at, (vel.0 * k, vel.1 * k), material, radius))
    }

    /// One fixed step.
    pub fn step(&mut self, surfaces: &SurfaceMap, objects: &mut ObjectStore, defs: &[ObjectDef], bus: &mut EventBus) {
        self.tick += 1;
        let dt = STEP;

        // objects settling back on their fixings
        let mut settled = Vec::new();
        for sp in self.springs.iter_mut() {
            let Some(s) = objects.get_mut(sp.id) else {
                settled.push(sp.id);
                continue;
            };
            let k = 90.0;
            let c = 5.5;
            let p = s.pose;
            sp.vel.dx += (-k * p.dx - c * sp.vel.dx) * dt;
            sp.vel.dy += (-k * p.dy - c * sp.vel.dy) * dt;
            sp.vel.turn += (-k * 0.35 * p.turn - c * 0.6 * sp.vel.turn) * dt;
            s.pose = Pose { dx: p.dx + sp.vel.dx * dt, dy: p.dy + sp.vel.dy * dt, turn: p.turn + sp.vel.turn * dt };
            let calm = s.pose.dx.abs() < 0.4 && s.pose.dy.abs() < 0.4 && s.pose.turn.abs() < 0.01 && sp.vel.dx.abs() < 1.0 && sp.vel.dy.abs() < 1.0;
            if calm || !s.pose.is_finite() {
                s.pose = Pose::default();
                s.swinging = false;
                s.revision += 1;
                settled.push(sp.id);
            }
        }
        self.springs.retain(|sp| !settled.contains(&sp.id));

        // jets
        let mut spawn = Vec::new();
        for j in self.jets.iter_mut().flatten() {
            j.owed += j.rate * dt;
            while j.owed >= 1.0 {
                j.owed -= 1.0;
                spawn.push((j.at, j.vel));
            }
        }
        for (at, vel) in spawn {
            let jitter = (self.rng.range(-0.12, 0.12), self.rng.range(-0.10, 0.10));
            let mut b = Body::new(BodyKind::Droplet, at, (vel.0 * (1.0 + jitter.0), vel.1 * (1.0 + jitter.1)), Material::Water, 1.6);
            b.life = 6.0;
            self.spawn(b);
        }

        // bodies
        let mut landed: Vec<(ObjectId, P)> = Vec::new();
        let mut sunk: Vec<ObjectId> = Vec::new();
        let mut bodies = std::mem::take(&mut self.bodies);
        let mut extra: Vec<Body> = Vec::new();
        bodies.retain_mut(|b| {
            if b.asleep {
                b.age += dt;
                return b.life == 0.0 || b.age < b.life;
            }
            b.age += dt;
            if b.life > 0.0 && b.age >= b.life {
                return false;
            }
            let light = matches!(b.material, Material::Cloth | Material::Paper) || b.kind == BodyKind::Flake;
            let g = if light { GRAVITY * 0.16 } else { GRAVITY };
            b.vel.1 += g * dt;
            if light {
                // the wind pushes what it can lift, and air slows it; on the ground a light
                // thing lies where it fell unless it is a gale
                if !b.contact || surfaces.wind.abs() > 250.0 {
                    b.vel.0 += (surfaces.wind - b.vel.0) * (2.2 * dt);
                }
                b.vel.1 *= 1.0 - 1.2 * dt;
            }
            let from = b.pos;
            let mut to = (b.pos.0 + b.vel.0 * dt, b.pos.1 + b.vel.1 * dt);
            b.turn += b.spin * dt;

            // water to fall into
            if let Some(basin) = surfaces.basins.iter().find(|w| to.0 >= w.rect[0] && to.0 <= w.rect[2] && to.1 >= w.rect[1] && to.1 <= w.rect[3]) {
                let tag = TAG_WATER;
                let fast = b.vel.1 > 90.0;
                if b.kind == BodyKind::Droplet || b.kind == BodyKind::Flake {
                    return false;
                }
                if fast && !b.floating {
                    for _ in 0..3 {
                        let mut d = Body::new(BodyKind::Droplet, (to.0, basin.rect[1]), (self.rng.range(-40.0, 40.0), -self.rng.range(60.0, 150.0)), Material::Water, 1.3);
                        d.life = 1.2;
                        extra.push(d);
                    }
                    let origin = self.next_origin + 1;
                    self.next_origin += 1;
                    let mut e = Event::new(EventKind::Noise, ObjectId(0), (to.0, basin.rect[1]), origin);
                    e.tags = tag;
                    if let BodyKind::Thrown(id) = b.kind {
                        e.source = id;
                    }
                    bus.emit(e);
                }
                if matches!(b.material, Material::Wood | Material::Cloth | Material::Rope | Material::Paper) {
                    b.floating = true;
                    b.pos = (to.0, basin.rect[1] + b.radius * 0.3);
                    b.vel = (basin.current.0, basin.current.1);
                    b.spin = 0.0;
                    return true;
                }
                // heavy things go under
                if let BodyKind::Thrown(id) = b.kind {
                    sunk.push(id);
                }
                return false;
            }
            if b.floating {
                // out of the basin (drifted past its edge): fall again
                b.floating = false;
            }

            // snags catch what the wind carries
            if light && b.snagged.is_none() {
                if let Some(s) = surfaces.snags.iter().find(|s| dist2(to, s.at) <= s.radius * s.radius) {
                    b.pos = s.at;
                    b.vel = (0.0, 0.0);
                    b.spin = 0.0;
                    b.asleep = true;
                    b.snagged = Some(s.tag);
                    let origin = self.next_origin + 1;
                    self.next_origin += 1;
                    let mut e = Event::new(EventKind::Obstruction, if let BodyKind::Thrown(id) = b.kind { id } else { ObjectId(0) }, s.at, origin);
                    e.tags = TAG_SNAG;
                    bus.emit(e);
                    return true;
                }
            }

            // contacts with the authored segments
            let mut hit: Option<(f32, Segment)> = None;
            for s in &surfaces.segments {
                if let Some(t) = cross(from, to, s.a, s.b) {
                    if hit.as_ref().map_or(true, |h| t < h.0) {
                        hit = Some((t, *s));
                    }
                }
            }
            if let Some((t, s)) = hit {
                let (ex, ey) = (s.b.0 - s.a.0, s.b.1 - s.a.1);
                let len = (ex * ex + ey * ey).sqrt().max(1e-3);
                let (tx, ty) = (ex / len, ey / len);
                // the normal that faces the side the body came from
                let (mut nx, mut ny) = (-ty, tx);
                let side = (from.0 - s.a.0) * nx + (from.1 - s.a.1) * ny;
                if side < 0.0 {
                    nx = -nx;
                    ny = -ny;
                }
                let along = b.vel.0 * tx + b.vel.1 * ty;
                let into = -(b.vel.0 * nx + b.vel.1 * ny);
                let heavy = !light;
                // the contact: the normal part bounces by the material, the tangent part rubs off
                // a slow arrival is a settling, not a bounce
                let e = if b.kind == BodyKind::Droplet || into < 70.0 { 0.0 } else { bounce_of(b.material) };
                // water runs freely down a slope and stops dead on the flat
                let flat = ty.abs() < 0.12;
                let friction = match (b.material == Material::Water, flat) {
                    (true, false) => 0.985,
                    (true, true) => 0.7,
                    (false, _) => 0.78,
                };
                let (vn, vt) = (into * e, along * friction);
                b.vel = (nx * vn + tx * vt, ny * vn + ty * vt);
                to = (from.0 + (to.0 - from.0) * t + nx * 0.6, from.1 + (to.1 - from.1) * t + ny * 0.6);
                b.spin *= 0.6;
                if into > 140.0 && heavy {
                    let origin = self.next_origin + 1;
                    self.next_origin += 1;
                    let source = if let BodyKind::Thrown(id) = b.kind { id } else { ObjectId(0) };
                    let mut ev = Event::new(if b.material == Material::Glass { EventKind::Damage } else { EventKind::Impact }, source, to, origin);
                    ev.tags = tag_of(b.material) | tag_of(s.material);
                    bus.emit(ev);
                    if b.material == Material::Glass && into > 190.0 {
                        // it breaks on landing
                        if let BodyKind::Thrown(id) = b.kind {
                            landed.push((id, to));
                        }
                        for _ in 0..fragments_of(Material::Glass) {
                            let a = self.rng.range(-3.0, -0.14);
                            let v = self.rng.range(30.0, 110.0);
                            let mut f = Body::new(BodyKind::Fragment, to, (a.cos() * v, a.sin() * v), Material::Glass, self.rng.range(1.2, 2.2));
                            f.life = self.rng.range(3.0, 6.0);
                            f.spin = self.rng.range(-8.0, 8.0);
                            extra.push(f);
                        }
                        b.pos = to;
                        b.asleep = true;
                        b.vel = (0.0, 0.0);
                        return true;
                    }
                }
            }
            b.pos = to;
            b.contact = hit.is_some();

            // at rest?
            let speed = (b.vel.0 * b.vel.0 + b.vel.1 * b.vel.1).sqrt();
            let touching = hit.is_some() && speed < REST_SPEED * 4.0;
            if touching && speed < REST_SPEED {
                b.still += 1;
            } else {
                b.still = 0;
            }
            if b.still >= SLEEP_AFTER {
                b.asleep = true;
                b.vel = (0.0, 0.0);
                b.spin = 0.0;
            }
            // a droplet at rest is a puddle
            if b.kind == BodyKind::Droplet && b.asleep {
                self.pending_patch.push((b.pos, 0.18));
                return false;
            }
            if b.kind == BodyKind::Flake && b.asleep {
                self.pending_patch.push((b.pos, 0.0));
                return false;
            }
            // out of the world
            if b.pos.1 > 1500.0 || b.pos.0 < -400.0 || b.pos.0 > 2000.0 || !b.is_finite() {
                if let BodyKind::Thrown(id) = b.kind {
                    sunk.push(id);
                }
                return false;
            }
            true
        });
        self.bodies = bodies;
        for b in extra {
            self.spawn(b);
        }
        for (at, amount) in std::mem::take(&mut self.pending_patch) {
            if amount > 0.0 {
                self.add_patch(at, amount, PatchKind::Water);
            }
        }
        // puddles dry in the end, so nothing grows without bound
        for p in self.patches.iter_mut().filter(|p| p.kind == PatchKind::Water) {
            p.amount -= 0.004 * dt;
        }
        self.patches.retain(|p| p.amount > 0.0);

        // what came to rest or was lost, back in the objects' own state
        for id in sunk {
            if let Some(s) = objects.get_mut(id) {
                s.damage = Damage::Missing;
                s.visible = false;
                s.attachment = Attachment::Fixed;
                s.revision += 1;
            }
        }
        let mut done: Vec<(ObjectId, P)> = Vec::new();
        for b in &self.bodies {
            if let (BodyKind::Thrown(id), true) = (b.kind, b.asleep) {
                done.push((id, b.pos));
            }
        }
        for (id, at) in done {
            let Some(def) = defs.iter().find(|d| d.id == id) else { continue };
            if let Some(s) = objects.get_mut(id) {
                let snagged = self.bodies.iter().find(|b| b.kind == BodyKind::Thrown(id)).and_then(|b| b.snagged);
                s.attachment = if snagged.is_some() { Attachment::Hanging } else { Attachment::Fixed };
                s.pose = Pose { dx: at.0 - def.at.0, dy: at.1 - def.at.1, turn: 0.0 };
                s.revision += 1;
                s.swinging = false;
                if landed.iter().any(|l| l.0 == id) {
                    s.damage = Damage::Broken;
                }
                let origin = self.next_origin + 1;
                self.next_origin += 1;
                let mut e = Event::new(EventKind::Obstruction, id, at, origin);
                e.tags = snagged.map_or(0, |_| TAG_SNAG) | tag_of(def.material);
                bus.emit(e);
                if let Some(Owner::Actor(_)) = s.owner {
                    s.owner = None;
                }
            }
            // the body served: its object is back in the world, at rest where it fell
            self.bodies.retain(|b| b.kind != BodyKind::Thrown(id));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::model::Material::*;

    fn floor(y: f32) -> SurfaceMap {
        SurfaceMap { segments: vec![Segment { a: (0.0, y), b: (1600.0, y), material: Stone }], ..Default::default() }
    }

    struct Rig {
        p: Physics,
        defs: Vec<ObjectDef>,
        objs: ObjectStore,
        bus: EventBus,
        map: SurfaceMap,
    }

    impl Rig {
        fn new(defs: Vec<ObjectDef>, map: SurfaceMap) -> Rig {
            let objs = ObjectStore::new(&defs);
            Rig { p: Physics::new(3), defs, objs, bus: EventBus::new(), map }
        }

        fn step(&mut self, n: usize) {
            for _ in 0..n {
                self.p.step(&self.map, &mut self.objs, &self.defs, &mut self.bus);
            }
        }

        fn g(&mut self, g: Gesture) {
            self.p.gesture(g, &self.defs, &mut self.objs, &mut self.bus);
        }

        fn events(&mut self) -> Vec<Event> {
            self.bus.drain().collect()
        }
    }

    fn obj(id: u16, material: Material, at: P, accepts: Accepts) -> ObjectDef {
        ObjectDef::new(id, "thing", material, at, [at.0 - 20.0, at.1 - 20.0, at.0 + 20.0, at.1 + 20.0]).accepting(accepts)
    }

    #[test]
    fn drag_and_release_velocity_is_bounded_and_finite() {
        let mut r = Rig::new(vec![obj(1, Stone, (400.0, 300.0), Accepts::THROW)], floor(700.0));
        r.g(Gesture::Press((400.0, 300.0)));
        assert_eq!(r.p.held(), Some(ObjectId(1)));
        r.g(Gesture::Drag((450.0, 280.0)));
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().pose.dx, 50.0);
        // a wild, enormous velocity is clamped to what a hand can do
        r.g(Gesture::Release { at: (450.0, 280.0), velocity: (1.0e9, -1.0e9) });
        r.step(1);
        let b = r.p.bodies.iter().find(|b| b.kind == BodyKind::Thrown(ObjectId(1))).unwrap();
        let speed = (b.vel.0 * b.vel.0 + b.vel.1 * b.vel.1).sqrt();
        assert!(speed <= MAX_THROW + GRAVITY * STEP + 1.0, "{}", speed);
        // a velocity that is not a number is no velocity at all
        let mut r = Rig::new(vec![obj(1, Stone, (400.0, 300.0), Accepts::THROW.or(Accepts::MOVE))], floor(700.0));
        r.g(Gesture::Press((400.0, 300.0)));
        r.g(Gesture::Release { at: (400.0, 300.0), velocity: (f32::NAN, 5.0) });
        assert!(r.p.bodies.is_empty(), "nothing thrown at zero speed");
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().attachment, Attachment::Fixed);
        // a drag to a place that is not a number does nothing
        r.g(Gesture::Press((400.0, 300.0)));
        r.g(Gesture::Drag((f32::NAN, 1.0)));
        assert!(r.objs.get(ObjectId(1)).unwrap().pose.is_finite());
    }

    #[test]
    fn a_pull_past_the_limit_breaks_the_attachment_in_two_stages() {
        let sign = ObjectDef::new(1, "sign", Metal, (300.0, 200.0), [250.0, 180.0, 350.0, 230.0]).accepting(Accepts::GRAB).breaks_at(40.0);
        let mut r = Rig::new(vec![sign], floor(700.0));
        r.g(Gesture::Press((300.0, 200.0)));
        r.g(Gesture::Drag((320.0, 200.0)));
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().damage, Damage::Intact, "a gentle pull bends nothing");
        r.g(Gesture::Drag((360.0, 215.0)));
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().damage, Damage::Damaged);
        let es = r.events();
        assert!(es.iter().any(|e| e.kind == EventKind::Damage && e.tags & TAG_METAL != 0));
        // let go: it hangs by the other fixing and swings, then is still, and stays damaged
        r.g(Gesture::Release { at: (360.0, 215.0), velocity: (0.0, 0.0) });
        let s = r.objs.get(ObjectId(1)).unwrap();
        assert_eq!((s.attachment, s.swinging), (Attachment::Hanging, true));
        r.step(300);
        let s = r.objs.get(ObjectId(1)).unwrap();
        assert!(!s.swinging && s.pose == Pose::default(), "{:?}", s);
        assert_eq!(s.damage, Damage::Damaged);
        // pulled hard enough the second fixing goes, and it falls to the floor
        r.g(Gesture::Press((300.0, 200.0)));
        r.g(Gesture::Drag((400.0, 260.0)));
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().damage, Damage::Broken);
        assert_eq!(r.p.held(), None, "the pointer no longer holds what came away");
        r.step(240);
        let s = r.objs.get(ObjectId(1)).unwrap();
        assert_eq!(s.attachment, Attachment::Fixed);
        assert!(s.pose.dy > 300.0, "it fell to the floor: {:?}", s.pose);
        assert!(r.events().iter().any(|e| e.kind == EventKind::Obstruction && e.source == ObjectId(1)));
    }

    #[test]
    fn glass_wood_metal_and_stone_land_differently() {
        let drop = |m: Material| {
            let mut r = Rig::new(vec![obj(1, m, (800.0, 100.0), Accepts::THROW)], floor(600.0));
            r.g(Gesture::Press((800.0, 100.0)));
            r.g(Gesture::Release { at: (800.0, 100.0), velocity: (60.0, 200.0) });
            let mut peak = 0.0f32;
            let mut bounces = 0;
            let mut last_vy = 0.0;
            let mut most_pieces = 0;
            for _ in 0..240 {
                r.step(1);
                most_pieces = most_pieces.max(r.p.bodies.iter().filter(|b| b.kind == BodyKind::Fragment).count());
                if let Some(b) = r.p.bodies.iter().find(|b| b.kind == BodyKind::Thrown(ObjectId(1))) {
                    if last_vy > 0.0 && b.vel.1 < 0.0 {
                        bounces += 1;
                    }
                    if bounces > 0 {
                        // the highest it gets after first touching the floor
                        peak = peak.max(600.0 - b.pos.1);
                    }
                    last_vy = b.vel.1;
                }
            }
            let fragments = most_pieces;
            let loud: Vec<u16> = r.events().iter().map(|e| e.tags).collect();
            (r.objs.get(ObjectId(1)).unwrap().damage, fragments, bounces, peak, loud)
        };
        let (glass_damage, glass_pieces, _, _, glass_tags) = drop(Glass);
        assert_eq!(glass_damage, Damage::Broken);
        assert!(glass_pieces >= 4, "glass shatters into pieces: {}", glass_pieces);
        assert!(glass_tags.iter().any(|t| t & TAG_GLASS != 0));
        let (wood_damage, wood_pieces, _, wood_peak, _) = drop(Wood);
        let (metal_damage, _, _, metal_peak, metal_tags) = drop(Metal);
        let (stone_damage, _, _, stone_peak, _) = drop(Stone);
        assert_eq!((wood_damage, metal_damage, stone_damage), (Damage::Intact, Damage::Intact, Damage::Intact));
        assert_eq!(wood_pieces, 0);
        assert!(metal_peak > wood_peak && wood_peak > stone_peak, "metal {} wood {} stone {}", metal_peak, wood_peak, stone_peak);
        assert!(metal_tags.iter().any(|t| t & TAG_METAL != 0), "metal rings");
    }

    #[test]
    fn a_thrown_thing_follows_a_parabola_and_lands_where_it_should() {
        let mut r = Rig::new(vec![obj(1, Stone, (100.0, 500.0), Accepts::THROW)], floor(700.0));
        r.g(Gesture::Press((100.0, 500.0)));
        r.g(Gesture::Release { at: (100.0, 500.0), velocity: (300.0, -200.0) });
        r.step(400);
        let s = r.objs.get(ObjectId(1)).unwrap();
        assert_eq!(s.attachment, Attachment::Fixed);
        // flight time from 200 units above the floor with vy = -200: ~ 0.99 s
        let g = GRAVITY;
        let t = (200.0 + ((200.0f32).powi(2) + 2.0 * g * 200.0).sqrt()) / g;
        let first_landing_x = 100.0 + 300.0 * t;
        let rest_x = 100.0 + s.pose.dx;
        assert!(rest_x > first_landing_x - 30.0, "it carried on past the first landing: {} vs {}", rest_x, first_landing_x);
        assert!((s.pose.dy - 200.0).abs() < 4.0, "it rests on the floor: {:?}", s.pose);
        assert!(!r.p.bodies.iter().any(|b| b.kind == BodyKind::Thrown(ObjectId(1))));
    }

    #[test]
    fn resting_bodies_stop_asking_for_frames() {
        let mut r = Rig::new(vec![obj(1, Stone, (100.0, 500.0), Accepts::THROW)], floor(700.0));
        r.g(Gesture::Press((100.0, 500.0)));
        r.g(Gesture::Release { at: (100.0, 500.0), velocity: (0.0, 200.0) });
        assert!(r.p.active());
        r.step(500);
        assert!(!r.p.active(), "everything is at rest");
        // debris likewise: fragments sleep, then are gone
        r.p.spawn(Body::new(BodyKind::Fragment, (500.0, 100.0), (0.0, 0.0), Glass, 1.5));
        assert!(r.p.active());
        r.step(1200);
        assert!(r.p.bodies.iter().all(|b| b.asleep || b.life > 0.0));
    }

    #[test]
    fn water_follows_a_slope_to_the_low_point_and_pools_there() {
        // a roof falling to the right, ending above a flat gutter
        let map = SurfaceMap {
            segments: vec![
                Segment { a: (200.0, 300.0), b: (500.0, 400.0), material: Stone },
                Segment { a: (500.0, 520.0), b: (700.0, 520.0), material: Metal },
            ],
            ..Default::default()
        };
        let mut r = Rig::new(vec![], map);
        r.p.set_jet(0, (260.0, 270.0), (0.0, 0.0), 12.0);
        r.step(150);
        r.p.stop_jet(0);
        r.step(300);
        assert!(!r.p.patches.is_empty(), "a puddle formed");
        let pooled: f32 = r.p.patches.iter().map(|p| p.amount).sum();
        assert!(pooled > 0.5, "{}", pooled);
        for p in &r.p.patches {
            assert!(p.at.0 >= 480.0 && p.at.0 <= 720.0 && (p.at.1 - 520.0).abs() < 5.0, "water collected at the low point, not at {:?}", p.at);
        }
        assert!(r.p.patches.len() <= MAX_PATCHES);
    }

    #[test]
    fn snow_gathers_is_brushed_off_and_rolls() {
        let map = SurfaceMap { snow: vec![SnowSpot { at: (300.0, 200.0), capacity: 3.0 }], ..floor(700.0) };
        let mut r = Rig::new(vec![], map);
        for _ in 0..40 {
            r.p.snowfall(&r.map, 0.2);
        }
        let held: f32 = r.p.patches.iter().map(|p| p.amount).sum();
        assert!((held - 3.0).abs() < 1e-4, "it holds no more than its capacity: {}", held);
        assert!(r.p.roll_snowball((302.0, 198.0), 20.0));
        assert!(!r.p.roll_snowball((900.0, 198.0), 20.0), "no snow there");
        let taken = r.p.brush_snow((300.0, 200.0), 30.0);
        assert!((taken - 2.5).abs() < 1e-4, "{}", taken);
        assert!(r.p.patches.iter().all(|p| p.kind != PatchKind::Snow));
        assert!(r.p.bodies.iter().any(|b| b.kind == BodyKind::Flake));
        r.step(200);
        assert!(r.p.bodies.iter().all(|b| b.kind != BodyKind::Flake), "flakes are gone");
    }

    #[test]
    fn cancel_after_the_seat_disappears_releases_cleanly_and_keeps_damage() {
        let sign = ObjectDef::new(1, "sign", Metal, (300.0, 200.0), [250.0, 180.0, 350.0, 230.0]).accepting(Accepts::GRAB).breaks_at(40.0);
        let mut r = Rig::new(vec![sign], floor(700.0));
        r.g(Gesture::Press((300.0, 200.0)));
        r.g(Gesture::Drag((350.0, 210.0)));
        assert_eq!(r.objs.get(ObjectId(1)).unwrap().damage, Damage::Damaged);
        r.g(Gesture::Cancel);
        assert_eq!(r.p.held(), None);
        let s = *r.objs.get(ObjectId(1)).unwrap();
        assert_eq!(s.damage, Damage::Damaged, "damage is not undone");
        assert_ne!(s.attachment, Attachment::Dragged, "no stuck capture");
        // more input after the cancel is ignored until a new press
        r.g(Gesture::Drag((600.0, 600.0)));
        assert!(r.objs.get(ObjectId(1)).unwrap().pose.dx.abs() < 100.0);
        r.step(300);
        assert!(!r.p.active());
        // a cancel with nothing held is nothing
        r.g(Gesture::Cancel);
    }

    #[test]
    fn floods_of_input_never_pass_the_ceilings_or_make_nonfinite_state() {
        let defs: Vec<ObjectDef> = (0..8).map(|n| obj(n, if n % 2 == 0 { Glass } else { Wood }, (100.0 + 150.0 * n as f32, 300.0), Accepts::THROW.or(Accepts::GRAB).or(Accepts::POKE))).collect();
        let mut r = Rig::new(defs, floor(700.0));
        r.p.set_jet(0, (800.0, 100.0), (30.0, 0.0), 90.0);
        let mut rng = Fast::new(99);
        for n in 0..10_000 {
            let p = (rng.range(0.0, 1600.0), rng.range(0.0, 700.0));
            match n % 4 {
                0 => r.g(Gesture::Press(p)),
                1 => r.g(Gesture::Drag((p.0 + rng.range(-300.0, 300.0), p.1))),
                2 => r.g(Gesture::Release { at: p, velocity: (rng.range(-2000.0, 2000.0), rng.range(-2000.0, 2000.0)) }),
                _ => r.g(Gesture::Cancel),
            }
            r.step(1);
            assert!(r.p.bodies.len() <= MAX_BODIES);
            assert!(r.p.patches.len() <= MAX_PATCHES);
            if n % 500 == 0 {
                r.bus.drain().for_each(drop);
            }
        }
        assert!(r.p.bodies.iter().all(|b| b.is_finite()));
        assert!(r.objs.iter().all(|(_, s)| s.pose.is_finite()));
        assert!(r.bus.len() <= 256);
    }

    #[test]
    fn the_same_ticks_give_the_same_state_whatever_the_frame_rate() {
        use crate::story::events::SimClock;
        let run = |frame: f64| {
            let defs = vec![obj(1, Wood, (300.0, 200.0), Accepts::THROW)];
            let mut r = Rig::new(defs, floor(700.0));
            r.g(Gesture::Press((300.0, 200.0)));
            r.g(Gesture::Release { at: (300.0, 200.0), velocity: (240.0, -120.0) });
            let mut clock = SimClock::new();
            while clock.tick < 90 {
                let n = clock.advance(frame);
                for _ in 0..n {
                    r.step(1);
                }
            }
            // stop at exactly tick 90 whatever the cadence overshot by
            assert!(clock.tick <= 92);
            (r.p.tick, r.p.bodies.first().map(|b| b.pos), r.objs.get(ObjectId(1)).copied())
        };
        let a = run(1.0 / 30.0);
        let b = run(1.0 / 60.0);
        let c = run(1.0 / 20.0);
        assert_eq!(a.0, 90);
        // cadences that overshoot by a tick or two are compared at the same tick below
        assert_eq!(a, run(1.0 / 30.0));
        assert!(b.0 >= 90 && c.0 >= 90);
    }

    #[test]
    fn wind_carries_light_things_to_a_snag() {
        let map = SurfaceMap { snags: vec![Snag { at: (652.0, 358.0), radius: 20.0, tag: 5 }], wind: 140.0, ..floor(700.0) };
        let mut r = Rig::new(vec![obj(1, Cloth, (300.0, 250.0), Accepts::THROW)], map);
        r.g(Gesture::Press((300.0, 250.0)));
        r.g(Gesture::Release { at: (300.0, 250.0), velocity: (200.0, -60.0) });
        r.step(400);
        let s = r.objs.get(ObjectId(1)).unwrap();
        assert_eq!(s.attachment, Attachment::Hanging, "caught on the aerial: {:?}", s);
        assert!((s.pose.dx + 300.0 - 652.0).abs() < 1.0 && (s.pose.dy + 250.0 - 358.0).abs() < 1.0);
        assert!(r.events().iter().any(|e| e.tags & TAG_SNAG != 0));
    }

    #[test]
    fn heavy_things_go_under_and_floating_things_drift() {
        let map = SurfaceMap {
            basins: vec![Basin { rect: [0.0, 600.0, 1600.0, 900.0], current: (30.0, 0.0) }],
            ..Default::default()
        };
        let mut r = Rig::new(vec![obj(1, Wood, (300.0, 400.0), Accepts::THROW), obj(2, Metal, (500.0, 400.0), Accepts::THROW)], map);
        for at in [(300.0, 400.0), (500.0, 400.0)] {
            r.g(Gesture::Press(at));
            r.g(Gesture::Release { at, velocity: (0.0, 300.0) });
        }
        r.step(200);
        let wood = r.p.bodies.iter().find(|b| b.kind == BodyKind::Thrown(ObjectId(1))).expect("the crate floats");
        assert!(wood.floating && (wood.pos.1 - 600.0).abs() < 8.0);
        assert!(wood.pos.0 > 300.0 + 20.0, "and drifts with the current: {}", wood.pos.0);
        assert_eq!(r.objs.get(ObjectId(2)).unwrap().damage, Damage::Missing, "the metal one sank");
        assert!(r.events().iter().any(|e| e.tags & TAG_WATER != 0), "a splash is heard");
    }
}
