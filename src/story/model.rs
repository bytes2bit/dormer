//! The vocabulary of a living scene: typed IDs, what an object can be, what a
//! view's `Definition` has to say, and the bounded store that holds the state.
//!
//! Everything here is plain data in the 1600x1000 design space. Nothing holds
//! pixels: a resize rebuilds a view's `Definition` for the new layout and
//! `StoryWorld::rebind` keeps the state of every object whose ID survives.

use std::fmt;

use crate::gfx::P;

/// Hard ceilings per output. They are limits, not targets.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Limits {
    pub incidents: usize,
    pub actors: usize,
    pub bodies: usize,
    pub objects: usize,
    pub events: usize,
    /// director instructions executed in one simulation tick
    pub instructions: usize,
    /// escalations of one incident before the protected cleanup
    pub escalations: usize,
}

impl Default for Limits {
    fn default() -> Self {
        Limits { incidents: 4, actors: 24, bodies: 128, objects: 64, events: 256, instructions: 256, escalations: 3 }
    }
}

macro_rules! id {
    ($(#[$m:meta])* $name:ident($t:ty)) => {
        $(#[$m])*
        #[derive(Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Debug)]
        pub struct $name(pub $t);
    };
}

id!(
    /// A part of the view that can change: a sign, a hydrant, a mooring rope. Chosen by the
    /// view's author and stable across refreshes, resizes and view switches.
    ObjectId(u16)
);
id!(RouteId(u16));
id!(StoryId(u16));
id!(
    /// A piece of ground (or water) a job keeps to itself while it works there.
    AreaId(u16)
);

/// A reference an actor can hold: its slot, and how many times that slot has been reused.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct ActorId {
    pub slot: u8,
    pub generation: u16,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Material {
    Glass,
    Wood,
    Metal,
    Stone,
    Cloth,
    Rope,
    Water,
    Paper,
    Flesh,
}

/// What state the object's own drawing is in.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Damage {
    #[default]
    Intact,
    /// bent, cracked, hanging by one fixing
    Damaged,
    Broken,
    /// not there: fallen away, taken, or carried off
    Missing,
}

/// How an object is held to the world.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub enum Attachment {
    /// where the view put it
    #[default]
    Fixed,
    /// swinging on one fixing, or hung from somewhere else
    Hanging,
    /// held by an actor
    Carried(ActorId),
    /// being dragged by the pointer
    Dragged,
    /// a free body under the physics
    Loose,
}

/// Displacement from the place the view authored the object at.
#[derive(Clone, Copy, PartialEq, Debug, Default)]
pub struct Pose {
    pub dx: f32,
    pub dy: f32,
    pub turn: f32,
}

impl Pose {
    pub fn is_finite(&self) -> bool {
        self.dx.is_finite() && self.dy.is_finite() && self.turn.is_finite()
    }
}

/// Who has a claim on an object: only one at a time.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owner {
    Actor(ActorId),
    Story(StoryId),
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct ObjectState {
    pub pose: Pose,
    pub damage: Damage,
    pub attachment: Attachment,
    pub owner: Option<Owner>,
    pub visible: bool,
    /// a small switch the view's drawing reads: a valve open, a lamp on
    pub open: bool,
    /// still swinging or settling: the live layer draws it until it is still
    pub swinging: bool,
    /// counts every change of what the base picture shows
    pub revision: u32,
}

impl Default for ObjectState {
    fn default() -> Self {
        ObjectState { pose: Pose::default(), damage: Damage::Intact, attachment: Attachment::Fixed, owner: None, visible: true, open: false, swinging: false, revision: 0 }
    }
}

impl ObjectState {
    /// Is it standing still where the base picture can show it?
    pub fn at_rest(&self) -> bool {
        matches!(self.attachment, Attachment::Fixed | Attachment::Hanging) && !self.swinging
    }
}

/// Gestures an object accepts; an empty set is scenery.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Accepts(pub u16);

impl Accepts {
    pub const NONE: Accepts = Accepts(0);
    pub const GRAB: Accepts = Accepts(1);
    pub const KNOCK: Accepts = Accepts(1 << 1);
    pub const THROW: Accepts = Accepts(1 << 2);
    pub const POKE: Accepts = Accepts(1 << 3);
    pub const BRUSH: Accepts = Accepts(1 << 4);
    pub const MOVE: Accepts = Accepts(1 << 5);

    pub const fn or(self, o: Accepts) -> Accepts {
        Accepts(self.0 | o.0)
    }

    pub fn has(self, o: Accepts) -> bool {
        self.0 & o.0 == o.0 && o.0 != 0
    }
}

/// One thing a view lets the scene change.
#[derive(Clone, Debug)]
pub struct ObjectDef {
    pub id: ObjectId,
    pub name: &'static str,
    pub material: Material,
    /// where it rests, in design units
    pub at: P,
    /// the most of the picture its drawing can cover in any of its forms: x0, y0, x1, y1
    pub bounds: [f32; 4],
    pub accepts: Accepts,
    /// the layer that hides it (see `Depth`), if any
    pub layer: Option<u8>,
    /// how hard a pull must be, in design units dragged, before an attachment lets go
    pub break_pull: f32,
}

impl ObjectDef {
    pub fn new(id: u16, name: &'static str, material: Material, at: P, bounds: [f32; 4]) -> ObjectDef {
        ObjectDef { id: ObjectId(id), name, material, at, bounds, accepts: Accepts::NONE, layer: None, break_pull: 0.0 }
    }

    pub fn accepting(mut self, a: Accepts) -> ObjectDef {
        self.accepts = a;
        self
    }

    pub fn behind(mut self, layer: u8) -> ObjectDef {
        self.layer = Some(layer);
        self
    }

    pub fn breaks_at(mut self, pull: f32) -> ObjectDef {
        self.break_pull = pull;
        self
    }
}

/// Everything a view says about what in it can live. Built by `views/episodes`.
#[derive(Clone, Debug, Default)]
pub struct Definition {
    /// the view's id, for diagnostics
    pub view: &'static str,
    pub objects: Vec<ObjectDef>,
}

/// Why a definition was refused, naming the view and the binding at fault.
#[derive(Clone, Debug, PartialEq)]
pub struct DefinitionError {
    pub view: &'static str,
    /// the story or section the problem is in, when it is in one
    pub story: Option<String>,
    pub binding: String,
    pub why: String,
}

impl fmt::Display for DefinitionError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match &self.story {
            Some(s) => write!(f, "view '{}', story '{}', {}: {}", self.view, s, self.binding, self.why),
            None => write!(f, "view '{}', {}: {}", self.view, self.binding, self.why),
        }
    }
}

impl std::error::Error for DefinitionError {}

impl Definition {
    pub fn error(&self, story: Option<&str>, binding: impl Into<String>, why: impl Into<String>) -> DefinitionError {
        DefinitionError { view: self.view, story: story.map(str::to_string), binding: binding.into(), why: why.into() }
    }

    pub fn object(&self, id: ObjectId) -> Option<&ObjectDef> {
        self.objects.iter().find(|o| o.id == id)
    }

    /// Check what can be checked without running anything: unique IDs, finite geometry,
    /// capacity. Later tasks add references, routes and sequence control flow.
    pub fn validate(&self, limits: &Limits) -> Result<(), DefinitionError> {
        if self.objects.len() > limits.objects {
            return Err(self.error(None, "objects", format!("{} objects is more than the {} an output can hold", self.objects.len(), limits.objects)));
        }
        for (n, o) in self.objects.iter().enumerate() {
            let at = format!("object '{}' ({})", o.name, o.id.0);
            if self.objects[..n].iter().any(|p| p.id == o.id) {
                return Err(self.error(None, at, "its ID is used by another object"));
            }
            let finite = o.at.0.is_finite() && o.at.1.is_finite() && o.bounds.iter().all(|v| v.is_finite()) && o.break_pull.is_finite();
            if !finite {
                return Err(self.error(None, at, "has a position or bounds that is not a finite number"));
            }
            if o.bounds[2] < o.bounds[0] || o.bounds[3] < o.bounds[1] {
                return Err(self.error(None, at, "has bounds that end before they begin"));
            }
        }
        Ok(())
    }
}

/// The state of every object in play, found by its stable ID. Capacity is fixed by `Limits`.
#[derive(Clone, Debug, Default)]
pub struct ObjectStore {
    slots: Vec<(ObjectId, ObjectState)>,
}

impl ObjectStore {
    pub fn new(defs: &[ObjectDef]) -> ObjectStore {
        ObjectStore { slots: defs.iter().map(|d| (d.id, ObjectState::default())).collect() }
    }

    pub fn get(&self, id: ObjectId) -> Option<&ObjectState> {
        self.slots.iter().find(|s| s.0 == id).map(|s| &s.1)
    }

    pub fn get_mut(&mut self, id: ObjectId) -> Option<&mut ObjectState> {
        self.slots.iter_mut().find(|s| s.0 == id).map(|s| &mut s.1)
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (ObjectId, &ObjectState)> {
        self.slots.iter().map(|s| (s.0, &s.1))
    }

    /// Carry state over to a new set of definitions: what still exists keeps its state, what is
    /// new starts intact, what is gone is forgotten.
    pub fn rebind(&mut self, defs: &[ObjectDef]) {
        let old = std::mem::take(&mut self.slots);
        self.slots = defs
            .iter()
            .map(|d| (d.id, old.iter().find(|s| s.0 == d.id).map(|s| s.1).unwrap_or_default()))
            .collect();
    }
}

/// A read-only look at the objects, for painting. An ID that is not there reads as intact.
#[derive(Clone, Copy)]
pub struct Snapshot<'a> {
    store: Option<&'a ObjectStore>,
}

static INTACT: ObjectState = ObjectState {
    pose: Pose { dx: 0.0, dy: 0.0, turn: 0.0 },
    damage: Damage::Intact,
    attachment: Attachment::Fixed,
    owner: None,
    visible: true,
    open: false,
    swinging: false,
    revision: 0,
};

impl<'a> Snapshot<'a> {
    pub fn new(store: &'a ObjectStore) -> Snapshot<'a> {
        Snapshot { store: Some(store) }
    }

    /// Nothing has happened to anything.
    pub fn intact() -> Snapshot<'static> {
        Snapshot { store: None }
    }

    pub fn get(&self, id: ObjectId) -> &ObjectState {
        self.store.and_then(|s| s.get(id)).unwrap_or(&INTACT)
    }

    pub fn damage(&self, id: ObjectId) -> Damage {
        self.get(id).damage
    }

    /// Should the base picture draw it at all: not missing, visible, and at rest. Something
    /// that is moving is drawn by the live layer instead.
    pub fn in_base(&self, id: ObjectId) -> bool {
        let s = self.get(id);
        s.visible && s.damage != Damage::Missing && s.at_rest()
    }

    pub fn is_intact(&self, id: ObjectId) -> bool {
        let s = self.get(id);
        s.damage == Damage::Intact && s.at_rest() && s.visible && s.pose == Pose::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn def() -> Definition {
        Definition {
            view: "street",
            objects: vec![
                ObjectDef::new(1, "hydrant", Material::Metal, (600.0, 880.0), [580.0, 840.0, 620.0, 890.0]).accepting(Accepts::GRAB.or(Accepts::POKE)),
                ObjectDef::new(2, "sign", Material::Metal, (190.0, 630.0), [140.0, 600.0, 240.0, 660.0]).breaks_at(40.0),
            ],
        }
    }

    #[test]
    fn stable_ids_survive_rebind() {
        let d = def();
        let mut store = ObjectStore::new(&d.objects);
        {
            let s = store.get_mut(ObjectId(2)).unwrap();
            s.damage = Damage::Broken;
            s.pose = Pose { dx: 3.0, dy: 40.0, turn: 1.2 };
            s.revision = 7;
        }
        // a resize: the view lays itself out again, at other places, with a new object too
        let mut moved = def();
        moved.objects[1].at = (300.0, 500.0);
        moved.objects.push(ObjectDef::new(3, "bin", Material::Metal, (10.0, 20.0), [0.0, 0.0, 30.0, 40.0]));
        store.rebind(&moved.objects);
        let s = store.get(ObjectId(2)).unwrap();
        assert_eq!((s.damage, s.revision, s.pose.dy), (Damage::Broken, 7, 40.0));
        assert_eq!(store.get(ObjectId(3)).unwrap(), &ObjectState::default());
        // an object the view no longer has is forgotten, and an unknown ID reads as intact
        let fewer = Definition { view: "street", objects: vec![moved.objects[0].clone()] };
        store.rebind(&fewer.objects);
        assert!(store.get(ObjectId(2)).is_none());
        assert_eq!(Snapshot::new(&store).damage(ObjectId(2)), Damage::Intact);
        assert!(Snapshot::new(&store).is_intact(ObjectId(99)));
    }

    #[test]
    fn invalid_bindings_are_diagnostic() {
        let limits = Limits::default();
        assert!(def().validate(&limits).is_ok());
        let mut dup = def();
        dup.objects[1].id = ObjectId(1);
        let e = dup.validate(&limits).unwrap_err();
        assert_eq!((e.view, e.binding.as_str()), ("street", "object 'sign' (1)"));
        assert!(e.to_string().contains("view 'street'") && e.to_string().contains("sign"), "{}", e);
        let mut nan = def();
        nan.objects[0].at = (f32::NAN, 0.0);
        assert!(nan.validate(&limits).unwrap_err().why.contains("finite"));
        let mut inf = def();
        inf.objects[0].bounds[2] = f32::INFINITY;
        assert!(inf.validate(&limits).is_err());
        let mut inside_out = def();
        inside_out.objects[0].bounds = [10.0, 10.0, 0.0, 20.0];
        assert!(inside_out.validate(&limits).unwrap_err().why.contains("bounds"));
    }

    #[test]
    fn definition_limits_are_enforced() {
        let limits = Limits::default();
        assert_eq!((limits.incidents, limits.actors, limits.bodies, limits.objects, limits.events, limits.instructions, limits.escalations), (4, 24, 128, 64, 256, 256, 3));
        let mut d = Definition { view: "city", objects: Vec::new() };
        for n in 0..64u16 {
            d.objects.push(ObjectDef::new(n, "thing", Material::Wood, (n as f32, 0.0), [0.0, 0.0, 1.0, 1.0]));
        }
        assert!(d.validate(&limits).is_ok());
        d.objects.push(ObjectDef::new(64, "one too many", Material::Wood, (0.0, 0.0), [0.0, 0.0, 1.0, 1.0]));
        let e = d.validate(&limits).unwrap_err();
        assert!(e.why.contains("65") && e.why.contains("64"), "{}", e);
    }

    #[test]
    fn a_moving_or_missing_object_is_not_in_the_base_picture() {
        let d = def();
        let mut store = ObjectStore::new(&d.objects);
        assert!(Snapshot::new(&store).in_base(ObjectId(1)));
        store.get_mut(ObjectId(1)).unwrap().attachment = Attachment::Dragged;
        assert!(!Snapshot::new(&store).in_base(ObjectId(1)), "dragged: the live layer draws it");
        store.get_mut(ObjectId(1)).unwrap().attachment = Attachment::Hanging;
        assert!(Snapshot::new(&store).in_base(ObjectId(1)), "hanging at rest is part of the base");
        store.get_mut(ObjectId(1)).unwrap().damage = Damage::Missing;
        assert!(!Snapshot::new(&store).in_base(ObjectId(1)));
    }
}
