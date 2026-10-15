//! What happens in each developed view: its objects, its routes and its stories.
//!
//! A view module keeps the geometry and the scenery; the file for it here
//! turns that geometry into a `story::Definition`. The shared mechanisms
//! (physics, actors, the director, birds, cats, people at windows, repair
//! crews) live in `story/`, so none of this is simulation code: it says where
//! things are and what happens when they are disturbed.

pub mod city;
pub mod rooftops;
pub mod street;
