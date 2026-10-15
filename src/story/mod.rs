//! The living scene: what in a view can change, who is about, what happens.
//!
//! One `StoryWorld` belongs to one output. It holds typed state for the view's
//! objects, a bounded event bus, the local physics, a few actors and a
//! sequence director, and nothing about pixels or Wayland. A view says what is
//! in it with a `Definition`; the shared behaviors and sequences here do the
//! rest, so a new incident is data and a small sequence, not new plumbing.

//! The pieces are wired in one at a time; until the last of them is, some are not yet used.
#![allow(dead_code)]

pub mod events;
pub mod model;

pub use model::*;

/// The world of one output.
pub struct StoryWorld {
    definition: Definition,
    limits: Limits,
    objects: ObjectStore,
    seed: u64,
}

impl StoryWorld {
    pub fn new(definition: Definition, seed: u64) -> Result<StoryWorld, DefinitionError> {
        let limits = Limits::default();
        definition.validate(&limits)?;
        Ok(StoryWorld { objects: ObjectStore::new(&definition.objects), definition, limits, seed })
    }

    pub fn snapshot(&self) -> Snapshot<'_> {
        Snapshot::new(&self.objects)
    }

    pub fn definition(&self) -> &Definition {
        &self.definition
    }

    pub fn limits(&self) -> &Limits {
        &self.limits
    }

    pub fn seed(&self) -> u64 {
        self.seed
    }

    /// The view laid itself out again (a resize): keep the state of every object whose ID
    /// is still there. On error nothing changes.
    pub fn rebind(&mut self, definition: Definition) -> Result<(), DefinitionError> {
        definition.validate(&self.limits)?;
        self.objects.rebind(&definition.objects);
        self.definition = definition;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn definition(x: f32) -> Definition {
        Definition { view: "coast", objects: vec![ObjectDef::new(1, "boat", Material::Wood, (x, 700.0), [x - 40.0, 680.0, x + 40.0, 720.0])] }
    }

    #[test]
    fn rebind_keeps_state_and_refuses_bad_definitions_without_losing_it() {
        let mut w = StoryWorld::new(definition(400.0), 5).unwrap();
        assert!(w.snapshot().is_intact(ObjectId(1)));
        w.objects.get_mut(ObjectId(1)).unwrap().damage = Damage::Damaged;
        w.rebind(definition(900.0)).unwrap();
        assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Damaged);
        assert_eq!(w.definition().objects[0].at.0, 900.0);
        let mut bad = definition(10.0);
        bad.objects[0].at.1 = f32::NAN;
        assert!(w.rebind(bad).is_err());
        assert_eq!(w.definition().objects[0].at.0, 900.0, "a refused definition changes nothing");
        assert_eq!(w.snapshot().damage(ObjectId(1)), Damage::Damaged);
    }
}
