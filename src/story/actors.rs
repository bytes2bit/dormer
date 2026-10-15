//! The people, animals, vehicles and boats of a scene, and the authored
//! routes they keep to.
//!
//! Nobody here finds a way across the picture's pixels. A view authors a
//! `RouteGraph`: named places joined by edges of a mode (walk, road, roof,
//! ladder, water), each possibly guarded by an area a job can reserve or by
//! an object that blocks it when it has been moved there. An actor asked to
//! go somewhere takes the shortest path its kind may use that is clear, and
//! if none is, waits a while and then reports that it could not.

use super::events::{Event, EventBus, EventKind};
use super::model::{ActorId, AreaId, Attachment, Damage, ObjectDef, ObjectId, ObjectStore, Owner, Pose, RouteId};
use crate::gfx::P;

pub const MAX_ACTORS: usize = 24;
pub const MAX_NODES: usize = 96;
pub const MAX_EDGES: usize = 256;
/// seconds an actor waits for a blocked way before it gives up
pub const PATIENCE: f32 = 9.0;
/// Walking pace in design units a second at scale 1.
const WALK: f32 = 34.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Mode {
    Walk,
    Road,
    Roof,
    Ladder,
    Water,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Kind {
    Person,
    Cat,
    Bird,
    Car,
    Van,
    Truck,
    Boat,
    Train,
}

impl Kind {
    /// May this sort of actor use an edge of this mode?
    pub fn may_use(self, mode: Mode) -> bool {
        match self {
            Kind::Person => matches!(mode, Mode::Walk | Mode::Roof | Mode::Ladder),
            Kind::Cat => matches!(mode, Mode::Walk | Mode::Roof),
            Kind::Bird => true,
            Kind::Car | Kind::Van | Kind::Truck | Kind::Train => mode == Mode::Road,
            Kind::Boat => mode == Mode::Water,
        }
    }

    fn pace(self) -> f32 {
        match self {
            Kind::Person => WALK,
            Kind::Cat => 52.0,
            Kind::Bird => 120.0,
            Kind::Car | Kind::Van | Kind::Truck => 140.0,
            Kind::Train => 210.0,
            Kind::Boat => 30.0,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Role {
    Resident,
    Worker,
    Roofer,
    Plumber,
    Electrician,
    Police,
    Courier,
    Customer,
    Owner,
    Keeper,
    Dockhand,
    Passenger,
    Staff,
    Technician,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActorPose {
    Idle,
    Look,
    Walk,
    Run,
    Carry,
    Operate,
    Work,
    Climb,
    React,
    Wave,
    Sit,
}

#[derive(Clone, Copy, Debug)]
pub struct Node {
    pub id: RouteId,
    pub at: P,
    pub mode: Mode,
    /// how big a person is here, as a fraction of the nearest pavement's: perspective
    pub scale: f32,
    /// the layer of the view that hides someone standing here (see `Depth`); None: nothing does
    pub layer: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct Edge {
    pub a: RouteId,
    pub b: RouteId,
    pub mode: Mode,
    /// the way is shut while another job holds this area
    pub area: Option<AreaId>,
    /// the way is shut while one of these objects lies displaced
    pub blockers: Vec<ObjectId>,
}

#[derive(Clone, Debug, Default)]
pub struct RouteGraph {
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
}

impl RouteGraph {
    pub fn node(&self, id: RouteId) -> Option<&Node> {
        self.nodes.iter().find(|n| n.id == id)
    }

    pub fn add_node(&mut self, id: u16, at: P, mode: Mode, scale: f32) -> &mut RouteGraph {
        self.nodes.push(Node { id: RouteId(id), at, mode, scale, layer: None });
        self
    }

    /// Say what hides someone at this place.
    pub fn behind(&mut self, id: u16, layer: u8) -> &mut RouteGraph {
        if let Some(n) = self.nodes.iter_mut().find(|n| n.id.0 == id) {
            n.layer = Some(layer);
        }
        self
    }

    /// Both ways, unless the mode makes it one-way (it does not: ladders go up and down).
    pub fn connect(&mut self, a: u16, b: u16, mode: Mode) -> &mut RouteGraph {
        self.edges.push(Edge { a: RouteId(a), b: RouteId(b), mode, area: None, blockers: Vec::new() });
        self
    }

    pub fn guard(&mut self, a: u16, b: u16, area: Option<AreaId>, blockers: &[ObjectId]) -> &mut RouteGraph {
        if let Some(e) = self.edges.iter_mut().find(|e| (e.a.0, e.b.0) == (a, b) || (e.a.0, e.b.0) == (b, a)) {
            e.area = area;
            e.blockers = blockers.to_vec();
        }
        self
    }

    pub fn validate(&self) -> Result<(), String> {
        if self.nodes.len() > MAX_NODES || self.edges.len() > MAX_EDGES {
            return Err(format!("{} places and {} edges is more than the {} and {} allowed", self.nodes.len(), self.edges.len(), MAX_NODES, MAX_EDGES));
        }
        for (n, node) in self.nodes.iter().enumerate() {
            if self.nodes[..n].iter().any(|o| o.id == node.id) {
                return Err(format!("route place {} is defined twice", node.id.0));
            }
            if !(node.at.0.is_finite() && node.at.1.is_finite() && node.scale.is_finite() && node.scale > 0.0) {
                return Err(format!("route place {} has no finite position and size", node.id.0));
            }
        }
        for e in &self.edges {
            if self.node(e.a).is_none() || self.node(e.b).is_none() {
                return Err(format!("a route edge joins {} and {}, and one of them is not a place", e.a.0, e.b.0));
            }
            if e.a == e.b {
                return Err(format!("a route edge joins place {} to itself", e.a.0));
            }
        }
        Ok(())
    }

    fn edge(&self, a: RouteId, b: RouteId) -> Option<&Edge> {
        self.edges.iter().find(|e| (e.a == a && e.b == b) || (e.a == b && e.b == a))
    }

    /// The shortest path for `kind` from `from` to `to`, over edges `open` accepts. Includes
    /// both ends. None when there is none.
    pub fn path(&self, kind: Kind, from: RouteId, to: RouteId, open: &dyn Fn(&Edge) -> bool) -> Option<Vec<RouteId>> {
        if kind == Kind::Bird {
            return self.node(from).and(self.node(to)).map(|_| vec![from, to]);
        }
        let n = self.nodes.len();
        let index = |id: RouteId| self.nodes.iter().position(|x| x.id == id);
        let (s, t) = (index(from)?, index(to)?);
        let mut dist = vec![f32::INFINITY; n];
        let mut prev = vec![usize::MAX; n];
        let mut done = vec![false; n];
        dist[s] = 0.0;
        for _ in 0..n {
            let Some(u) = (0..n).filter(|i| !done[*i] && dist[*i].is_finite()).min_by(|a, b| dist[*a].partial_cmp(&dist[*b]).unwrap_or(std::cmp::Ordering::Equal)) else { break };
            done[u] = true;
            if u == t {
                break;
            }
            for e in &self.edges {
                let other = if e.a == self.nodes[u].id {
                    e.b
                } else if e.b == self.nodes[u].id {
                    e.a
                } else {
                    continue;
                };
                if !kind.may_use(e.mode) || !open(e) {
                    continue;
                }
                let Some(v) = index(other) else { continue };
                let (p, q) = (self.nodes[u].at, self.nodes[v].at);
                let len = ((p.0 - q.0).powi(2) + (p.1 - q.1).powi(2)).sqrt();
                // ladders are slow and the roofs a little wary: prefer the flat way round
                let cost = len * if e.mode == Mode::Ladder { 2.2 } else { 1.0 };
                if dist[u] + cost < dist[v] {
                    dist[v] = dist[u] + cost;
                    prev[v] = u;
                }
            }
        }
        if !dist[t].is_finite() {
            return None;
        }
        let mut path = vec![self.nodes[t].id];
        let mut at = t;
        while at != s {
            at = prev[at];
            path.push(self.nodes[at].id);
        }
        path.reverse();
        Some(path)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActorSpec {
    pub kind: Kind,
    pub role: Role,
    pub at: RouteId,
    /// picks the clothes, the coat of the cat, the paint of the van
    pub look: u32,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Failure {
    /// no clear way exists, and the wait for one ran out
    Timeout,
    /// there is no way at all for this sort of actor
    NoRoute,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActorState {
    Idle,
    Moving,
    /// the way ahead is shut; will try again
    Waiting,
    Arrived,
    Failed(Failure),
}

#[derive(Clone, Debug)]
pub struct Actor {
    pub id: ActorId,
    pub kind: Kind,
    pub role: Role,
    pub look: u32,
    pub pos: P,
    pub scale: f32,
    /// -1 faces left, 1 faces right
    pub facing: f32,
    pub pose: ActorPose,
    pub state: ActorState,
    pub carrying: Option<ObjectId>,
    pub gaze: Option<P>,
    /// the layer that hides it where it stands
    pub layer: Option<u8>,
    node: RouteId,
    path: Vec<RouteId>,
    goal: Option<RouteId>,
    hurry: bool,
    waited: f32,
    /// the stationary pose asked for
    held_pose: ActorPose,
    token: u64,
}

impl Actor {
    /// The place it last reached.
    pub fn node_id(&self) -> RouteId {
        self.node
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub enum ActorCommand {
    Move(RouteId),
    Run(RouteId),
    Look(P),
    Pose(ActorPose),
    Carry(ObjectId),
    Drop(P),
    Reserve(AreaId),
    Release,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ActorError {
    /// the command was for an actor that has gone, and whose slot may now be someone else's
    Stale,
    /// somebody else carries it or has it in hand
    Owned(ObjectId),
    /// no such object or place
    Unknown,
    /// another job holds the area
    Reserved(ActorId),
    /// not carrying anything
    Empty,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Capacity;

pub struct ActorSystem {
    slots: Vec<Option<Actor>>,
    generations: Vec<u16>,
    reservations: Vec<(AreaId, ActorId)>,
    /// things put down this tick that the world should hand to the physics: object, where
    drops: Vec<(ObjectId, P)>,
    counter: u64,
}

impl Default for ActorSystem {
    fn default() -> Self {
        ActorSystem::new()
    }
}

/// The ObjectId events from an actor carry as their source: the top of the range, by slot.
pub fn actor_source(id: ActorId) -> ObjectId {
    ObjectId(u16::MAX - id.slot as u16)
}

impl ActorSystem {
    pub fn new() -> ActorSystem {
        ActorSystem { slots: (0..MAX_ACTORS).map(|_| None).collect(), generations: vec![0; MAX_ACTORS], reservations: Vec::new(), drops: Vec::new(), counter: 0 }
    }

    pub fn count(&self) -> usize {
        self.slots.iter().flatten().count()
    }

    pub fn get(&self, id: ActorId) -> Option<&Actor> {
        self.slots.get(id.slot as usize).and_then(|s| s.as_ref()).filter(|a| a.id == id)
    }

    fn get_mut(&mut self, id: ActorId) -> Option<&mut Actor> {
        self.slots.get_mut(id.slot as usize).and_then(|s| s.as_mut()).filter(|a| a.id == id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &Actor> {
        self.slots.iter().flatten()
    }

    /// Anybody about that is doing something?
    pub fn busy(&self) -> bool {
        self.iter().any(|a| matches!(a.state, ActorState::Moving | ActorState::Waiting))
    }

    pub fn spawn(&mut self, spec: ActorSpec, graph: &RouteGraph) -> Result<ActorId, Capacity> {
        let Some(slot) = self.slots.iter().position(Option::is_none) else { return Err(Capacity) };
        let node = graph.node(spec.at).ok_or(Capacity)?;
        self.generations[slot] = self.generations[slot].wrapping_add(1);
        let id = ActorId { slot: slot as u8, generation: self.generations[slot] };
        self.counter += 1;
        self.slots[slot] = Some(Actor {
            id,
            kind: spec.kind,
            role: spec.role,
            look: spec.look,
            pos: node.at,
            scale: node.scale,
            facing: 1.0,
            pose: ActorPose::Idle,
            state: ActorState::Idle,
            carrying: None,
            gaze: None,
            layer: node.layer,
            node: spec.at,
            path: Vec::new(),
            goal: None,
            hurry: false,
            waited: 0.0,
            held_pose: ActorPose::Idle,
            token: ((slot as u64) << 48) | (self.generations[slot] as u64) << 32 | self.counter,
        });
        Ok(id)
    }

    /// Take an actor out of the scene. Its reservations are released and what it carried is put down.
    pub fn remove(&mut self, id: ActorId, objects: &mut ObjectStore, defs: &[ObjectDef]) {
        let Some(a) = self.get(id) else { return };
        let carried = a.carrying;
        let pos = a.pos;
        self.reservations.retain(|r| r.1 != id);
        if let Some(obj) = carried {
            self.put_down(obj, pos, objects, defs);
        }
        self.slots[id.slot as usize] = None;
    }

    fn put_down(&mut self, obj: ObjectId, at: P, objects: &mut ObjectStore, defs: &[ObjectDef]) {
        if let (Some(s), Some(def)) = (objects.get_mut(obj), defs.iter().find(|d| d.id == obj)) {
            s.owner = None;
            s.attachment = Attachment::Fixed;
            s.pose = Pose { dx: at.0 - def.at.0, dy: at.1 - def.at.1, turn: 0.0 };
            s.revision += 1;
        }
    }

    pub fn command(&mut self, id: ActorId, command: ActorCommand, graph: &RouteGraph, objects: &mut ObjectStore) -> Result<(), ActorError> {
        if self.get(id).is_none() {
            return Err(ActorError::Stale);
        }
        match command {
            ActorCommand::Move(to) | ActorCommand::Run(to) => {
                if graph.node(to).is_none() {
                    return Err(ActorError::Unknown);
                }
                let a = self.get_mut(id).unwrap();
                a.goal = Some(to);
                a.hurry = matches!(command, ActorCommand::Run(_));
                a.path.clear();
                a.waited = 0.0;
                a.state = ActorState::Moving;
                a.gaze = None;
                Ok(())
            }
            ActorCommand::Look(at) => {
                let a = self.get_mut(id).unwrap();
                a.gaze = Some(at);
                if (at.0 - a.pos.0).abs() > 1.0 {
                    a.facing = (at.0 - a.pos.0).signum();
                }
                a.held_pose = ActorPose::Look;
                Ok(())
            }
            ActorCommand::Pose(p) => {
                self.get_mut(id).unwrap().held_pose = p;
                Ok(())
            }
            ActorCommand::Carry(obj) => {
                let Some(s) = objects.get_mut(obj) else { return Err(ActorError::Unknown) };
                if let Some(Owner::Actor(o)) = s.owner {
                    if o != id {
                        return Err(ActorError::Owned(obj));
                    }
                }
                if matches!(s.owner, Some(Owner::Story(_))) || matches!(s.attachment, Attachment::Dragged | Attachment::Loose) {
                    return Err(ActorError::Owned(obj));
                }
                s.owner = Some(Owner::Actor(id));
                s.attachment = Attachment::Carried(id);
                s.revision += 1;
                let a = self.get_mut(id).unwrap();
                if let Some(old) = a.carrying.replace(obj) {
                    // one thing at a time: the old one stays where it is
                    if let Some(o) = objects.get_mut(old) {
                        o.owner = None;
                        o.attachment = Attachment::Fixed;
                    }
                }
                Ok(())
            }
            ActorCommand::Drop(at) => {
                let a = self.get_mut(id).unwrap();
                let Some(obj) = a.carrying.take() else { return Err(ActorError::Empty) };
                if let Some(s) = objects.get_mut(obj) {
                    s.owner = None;
                    s.attachment = Attachment::Loose;
                    s.revision += 1;
                }
                self.drops.push((obj, at));
                Ok(())
            }
            ActorCommand::Reserve(area) => {
                match self.reservations.iter().find(|r| r.0 == area) {
                    Some(r) if r.1 != id => return Err(ActorError::Reserved(r.1)),
                    Some(_) => {}
                    None => self.reservations.push((area, id)),
                }
                Ok(())
            }
            ActorCommand::Release => {
                self.reservations.retain(|r| r.1 != id);
                Ok(())
            }
        }
    }

    /// Nobody carries this any more (it was put back by the story).
    pub fn release_object(&mut self, obj: ObjectId) {
        for a in self.slots.iter_mut().flatten() {
            if a.carrying == Some(obj) {
                a.carrying = None;
            }
        }
    }

    pub fn reserved_by(&self, area: AreaId) -> Option<ActorId> {
        self.reservations.iter().find(|r| r.0 == area).map(|r| r.1)
    }

    pub fn reservation_count(&self) -> usize {
        self.reservations.len()
    }

    /// Objects put down this tick (and where), for the physics to take over.
    pub fn take_drops(&mut self) -> Vec<(ObjectId, P)> {
        std::mem::take(&mut self.drops)
    }

    fn open_for<'a>(&'a self, mover: ActorId, objects: &'a ObjectStore) -> impl Fn(&Edge) -> bool + 'a {
        move |e: &Edge| {
            let held = e.area.and_then(|a| self.reserved_by(a)).is_some_and(|by| by != mover);
            let moved = e.blockers.iter().any(|b| {
                objects.get(*b).is_some_and(|s| s.visible && s.damage != Damage::Missing && (s.pose.dx.abs() > 6.0 || s.pose.dy.abs() > 6.0) && s.attachment == Attachment::Fixed)
            });
            !held && !moved
        }
    }

    /// One fixed step of everyone's movement.
    pub fn step(&mut self, dt: f32, graph: &RouteGraph, objects: &mut ObjectStore, defs: &[ObjectDef], bus: &mut EventBus) {
        for slot in 0..self.slots.len() {
            let Some(mut a) = self.slots[slot].take() else { continue };
            self.step_one(&mut a, dt, graph, objects, bus);
            // what is carried goes where the hands go
            if let Some(obj) = a.carrying {
                if let (Some(s), Some(def)) = (objects.get_mut(obj), defs.iter().find(|d| d.id == obj)) {
                    let (hx, hy) = (a.pos.0 + a.facing * 7.0 * a.scale, a.pos.1 - 16.0 * a.scale);
                    s.pose = Pose { dx: hx - def.at.0, dy: hy - def.at.1, turn: 0.0 };
                }
            }
            self.slots[slot] = Some(a);
        }
    }

    fn step_one(&mut self, a: &mut Actor, dt: f32, graph: &RouteGraph, objects: &ObjectStore, bus: &mut EventBus) {
        if !matches!(a.state, ActorState::Moving | ActorState::Waiting) {
            a.pose = a.held_pose;
            return;
        }
        let Some(goal) = a.goal else {
            a.state = ActorState::Idle;
            return;
        };
        // plan (or re-plan) when there is no path, or the next edge has been shut
        let blocked_ahead = |a: &Actor, me: &ActorSystem| {
            let open = me.open_for(a.id, objects);
            a.path.first().is_some_and(|n| graph.edge(a.node, *n).is_some_and(|e| !open(e)))
        };
        if (a.path.is_empty() && a.node != goal) || blocked_ahead(a, self) {
            let planned = {
                let open = self.open_for(a.id, objects);
                graph.path(a.kind, a.node, goal, &open)
            };
            match planned {
                Some(mut p) => {
                    p.remove(0);
                    a.path = p;
                    a.waited = 0.0;
                    a.state = ActorState::Moving;
                }
                None => {
                    if a.path.is_empty() {
                        // nothing is clear just now: set off along the way there is, and
                        // stop where it is shut
                        match graph.path(a.kind, a.node, goal, &|_| true) {
                            Some(mut p) => {
                                p.remove(0);
                                a.path = p;
                            }
                            None => {
                                a.state = ActorState::Failed(Failure::NoRoute);
                                a.pose = ActorPose::Idle;
                                self.report(a, EventKind::Obstruction, bus);
                                return;
                            }
                        }
                    }
                    if blocked_ahead(a, self) {
                        a.waited += dt;
                        a.state = ActorState::Waiting;
                        a.pose = ActorPose::Idle;
                        if a.waited >= PATIENCE {
                            a.state = ActorState::Failed(Failure::Timeout);
                            self.report(a, EventKind::Obstruction, bus);
                        }
                        return;
                    }
                }
            }
        }
        // told to go back to the place it last left, from between two places: walk back to it
        if a.node == goal && a.path.is_empty() {
            let at = graph.node(a.node).map_or(a.pos, |n| n.at);
            if (at.0 - a.pos.0).abs() + (at.1 - a.pos.1).abs() > 0.5 {
                a.path.push(goal);
            }
        }
        if a.node == goal && a.path.is_empty() {
            a.state = ActorState::Arrived;
            a.pose = a.held_pose;
            self.report(a, EventKind::Arrival, bus);
            return;
        }
        let Some(&next) = a.path.first() else { return };
        // a bird flies straight to where it is going; everyone else keeps to an authored edge
        let edge_mode = match graph.edge(a.node, next) {
            Some(e) => Some(e.mode),
            None if a.kind == Kind::Bird || next == a.node => graph.node(a.node).map(|n| n.mode),
            None => None,
        };
        let (Some(to), Some(edge_mode)) = (graph.node(next), edge_mode) else {
            a.path.clear();
            return;
        };
        let to_at = to.at;
        let (dx, dy) = (to_at.0 - a.pos.0, to_at.1 - a.pos.1);
        let dist = (dx * dx + dy * dy).sqrt();
        let mut speed = a.kind.pace() * a.scale.max(0.2);
        let mut pose = ActorPose::Walk;
        match edge_mode {
            Mode::Ladder => {
                speed *= 0.45;
                pose = ActorPose::Climb;
            }
            _ if a.hurry && a.kind == Kind::Person => {
                speed *= 2.3;
                pose = ActorPose::Run;
            }
            _ if a.carrying.is_some() && a.kind == Kind::Person => {
                speed *= 0.85;
                pose = ActorPose::Carry;
            }
            _ => {}
        }
        a.pose = pose;
        if dx.abs() > 0.5 {
            a.facing = dx.signum();
        }
        let travel = speed * dt;
        if dist <= travel.max(0.01) {
            a.pos = to_at;
            a.node = next;
            a.scale = to.scale;
            a.layer = to.layer;
            a.path.remove(0);
        } else {
            let k = travel / dist;
            a.pos = (a.pos.0 + dx * k, a.pos.1 + dy * k);
            // perspective eases from one place's size to the next
            let from_scale = graph.node(a.node).map_or(a.scale, |n| n.scale);
            let total = {
                let n = graph.node(a.node).map_or(a.pos, |n| n.at);
                ((to_at.0 - n.0).powi(2) + (to_at.1 - n.1).powi(2)).sqrt().max(1.0)
            };
            let f = (1.0 - dist / total).clamp(0.0, 1.0);
            a.scale = from_scale + (to.scale - from_scale) * f;
        }
    }

    fn report(&mut self, a: &Actor, kind: EventKind, bus: &mut EventBus) {
        let mut e = Event::new(kind, actor_source(a.id), a.pos, a.token);
        // tags: bit 7 marks an actor's own report
        e.tags = 1 << 7;
        bus.emit(e);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::story::model::{Accepts, Material};

    /// A street with a pavement, a way up a ladder to a roof, a road, and a water edge.
    fn graph() -> RouteGraph {
        let mut g = RouteGraph::default();
        g.add_node(1, (100.0, 800.0), Mode::Walk, 1.0)
            .add_node(2, (400.0, 800.0), Mode::Walk, 1.0)
            .add_node(3, (700.0, 800.0), Mode::Walk, 1.0)
            .add_node(4, (400.0, 700.0), Mode::Walk, 1.0) // an alley round the middle
            .add_node(5, (700.0, 700.0), Mode::Ladder, 0.6)
            .add_node(6, (900.0, 700.0), Mode::Roof, 0.6)
            .add_node(7, (100.0, 900.0), Mode::Road, 1.2)
            .add_node(8, (1300.0, 900.0), Mode::Road, 1.2)
            .add_node(9, (1000.0, 820.0), Mode::Water, 0.7)
            .add_node(10, (1400.0, 820.0), Mode::Water, 0.7);
        g.connect(1, 2, Mode::Walk).connect(2, 3, Mode::Walk).connect(1, 4, Mode::Walk).connect(4, 3, Mode::Walk);
        g.connect(3, 5, Mode::Ladder).connect(5, 6, Mode::Roof);
        g.connect(7, 8, Mode::Road).connect(9, 10, Mode::Water);
        g
    }

    fn rig() -> (ActorSystem, RouteGraph, ObjectStore, Vec<ObjectDef>, EventBus) {
        let defs = vec![
            ObjectDef::new(1, "bucket", Material::Metal, (400.0, 800.0), [380.0, 780.0, 420.0, 820.0]).accepting(Accepts::MOVE),
            ObjectDef::new(2, "wrench", Material::Metal, (150.0, 790.0), [140.0, 780.0, 160.0, 800.0]),
        ];
        let objects = ObjectStore::new(&defs);
        (ActorSystem::new(), graph(), objects, defs, EventBus::new())
    }

    fn run(sys: &mut ActorSystem, g: &RouteGraph, o: &mut ObjectStore, d: &[ObjectDef], bus: &mut EventBus, seconds: f32) {
        for _ in 0..(seconds * 30.0) as usize {
            sys.step(1.0 / 30.0, g, o, d, bus);
        }
    }

    fn person(sys: &mut ActorSystem, g: &RouteGraph, at: u16) -> ActorId {
        sys.spawn(ActorSpec { kind: Kind::Person, role: Role::Worker, at: RouteId(at), look: 0 }, g).unwrap()
    }

    #[test]
    fn route_modes_and_obstacles_choose_valid_paths() {
        let (mut sys, mut g, mut o, d, mut bus) = rig();
        // a person goes up the ladder to the roof; a van cannot, and has no way to the pavement
        let p = person(&mut sys, &g, 1);
        sys.command(p, ActorCommand::Move(RouteId(6)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 40.0);
        let a = sys.get(p).unwrap();
        assert_eq!(a.state, ActorState::Arrived);
        assert!((a.pos.0 - 900.0).abs() < 0.1 && (a.pos.1 - 700.0).abs() < 0.1);
        let van = sys.spawn(ActorSpec { kind: Kind::Van, role: Role::Worker, at: RouteId(7), look: 1 }, &g).unwrap();
        sys.command(van, ActorCommand::Move(RouteId(2)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 2.0);
        assert_eq!(sys.get(van).unwrap().state, ActorState::Failed(Failure::NoRoute));
        sys.command(van, ActorCommand::Move(RouteId(8)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 20.0);
        assert_eq!(sys.get(van).unwrap().state, ActorState::Arrived);
        // a boat stays on the water
        let boat = sys.spawn(ActorSpec { kind: Kind::Boat, role: Role::Dockhand, at: RouteId(9), look: 0 }, &g).unwrap();
        sys.command(boat, ActorCommand::Move(RouteId(10)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 30.0);
        assert_eq!(sys.get(boat).unwrap().state, ActorState::Arrived);

        // an obstacle in the direct way sends the next walker round by the alley
        g.guard(1, 2, None, &[ObjectId(1)]);
        o.get_mut(ObjectId(1)).unwrap().pose = Pose { dx: 0.0, dy: 30.0, turn: 0.0 };
        let q = person(&mut sys, &g, 1);
        sys.command(q, ActorCommand::Move(RouteId(3)), &g, &mut o).unwrap();
        let mut seen_alley = false;
        for _ in 0..(40.0 * 30.0) as usize {
            sys.step(1.0 / 30.0, &g, &mut o, &d, &mut bus);
            let a = sys.get(q).unwrap();
            if (a.pos.0 - 400.0).abs() < 1.0 && (a.pos.1 - 700.0).abs() < 1.0 {
                seen_alley = true;
            }
        }
        assert!(seen_alley, "went by the alley");
        assert_eq!(sys.get(q).unwrap().state, ActorState::Arrived);
    }

    #[test]
    fn actor_and_prop_have_one_owner() {
        let (mut sys, g, mut o, d, mut bus) = rig();
        let (a, b) = (person(&mut sys, &g, 1), person(&mut sys, &g, 2));
        sys.command(a, ActorCommand::Carry(ObjectId(2)), &g, &mut o).unwrap();
        assert_eq!(o.get(ObjectId(2)).unwrap().owner, Some(Owner::Actor(a)));
        assert_eq!(sys.command(b, ActorCommand::Carry(ObjectId(2)), &g, &mut o), Err(ActorError::Owned(ObjectId(2))));
        // a carried tool goes where its carrier goes
        sys.command(a, ActorCommand::Move(RouteId(3)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 6.0);
        let (carrier, tool) = (sys.get(a).unwrap().pos, o.get(ObjectId(2)).unwrap().pose);
        let at = (d[1].at.0 + tool.dx, d[1].at.1 + tool.dy);
        assert!((at.0 - carrier.0).abs() < 12.0 && (at.1 - carrier.1).abs() < 20.0, "{:?} vs {:?}", at, carrier);
        // set down, it is a physical thing, free for anyone
        sys.command(a, ActorCommand::Drop(carrier), &g, &mut o).unwrap();
        assert_eq!(o.get(ObjectId(2)).unwrap().owner, None);
        assert_eq!(o.get(ObjectId(2)).unwrap().attachment, Attachment::Loose);
        assert_eq!(sys.take_drops(), vec![(ObjectId(2), carrier)]);
        assert_eq!(sys.command(a, ActorCommand::Drop(carrier), &g, &mut o), Err(ActorError::Empty));
    }

    #[test]
    fn unavailable_route_has_timeout_event() {
        let (mut sys, _, mut o, d, mut bus) = rig();
        // the only way along is a work zone someone else holds
        let mut g = RouteGraph::default();
        g.add_node(1, (0.0, 0.0), Mode::Walk, 1.0).add_node(2, (200.0, 0.0), Mode::Walk, 1.0).add_node(3, (400.0, 0.0), Mode::Walk, 1.0);
        g.connect(1, 2, Mode::Walk).connect(2, 3, Mode::Walk);
        g.guard(2, 3, Some(AreaId(7)), &[]);
        let (holder, walker) = (person(&mut sys, &g, 3), person(&mut sys, &g, 1));
        sys.command(holder, ActorCommand::Reserve(AreaId(7)), &g, &mut o).unwrap();
        assert_eq!(sys.command(walker, ActorCommand::Reserve(AreaId(7)), &g, &mut o), Err(ActorError::Reserved(holder)));
        sys.command(walker, ActorCommand::Move(RouteId(3)), &g, &mut o).unwrap();
        // it walks up to the zone, waits, and when the patience runs out says so
        run(&mut sys, &g, &mut o, &d, &mut bus, 8.0);
        assert_eq!(sys.get(walker).unwrap().state, ActorState::Waiting);
        assert!((sys.get(walker).unwrap().pos.0 - 200.0).abs() < 1.0, "stopped at the edge of the zone");
        run(&mut sys, &g, &mut o, &d, &mut bus, 9.0);
        assert_eq!(sys.get(walker).unwrap().state, ActorState::Failed(Failure::Timeout));
        let events: Vec<Event> = bus.drain().collect();
        let report = events.iter().find(|e| e.kind == EventKind::Obstruction && e.source == actor_source(walker)).expect("a timeout event");
        assert!((report.at.0 - 200.0).abs() < 1.0);
        // when the zone is released, a new instruction goes through
        sys.command(holder, ActorCommand::Release, &g, &mut o).unwrap();
        sys.command(walker, ActorCommand::Move(RouteId(3)), &g, &mut o).unwrap();
        run(&mut sys, &g, &mut o, &d, &mut bus, 10.0);
        assert_eq!(sys.get(walker).unwrap().state, ActorState::Arrived);
        assert!(bus.drain().any(|e| e.kind == EventKind::Arrival && e.source == actor_source(walker)));
    }

    #[test]
    fn removal_releases_reservations() {
        let (mut sys, g, mut o, d, _) = rig();
        let a = person(&mut sys, &g, 1);
        sys.command(a, ActorCommand::Reserve(AreaId(1)), &g, &mut o).unwrap();
        sys.command(a, ActorCommand::Reserve(AreaId(2)), &g, &mut o).unwrap();
        sys.command(a, ActorCommand::Carry(ObjectId(2)), &g, &mut o).unwrap();
        assert_eq!(sys.reservation_count(), 2);
        sys.remove(a, &mut o, &d);
        assert_eq!(sys.reservation_count(), 0);
        assert_eq!(sys.count(), 0);
        // and what it carried is left where it stood, owned by nobody
        let s = o.get(ObjectId(2)).unwrap();
        assert_eq!((s.owner, s.attachment), (None, Attachment::Fixed));
    }

    #[test]
    fn population_is_capped() {
        let (mut sys, g, _, _, _) = rig();
        let ids: Vec<ActorId> = (0..MAX_ACTORS).map(|_| person(&mut sys, &g, 1)).collect();
        assert_eq!(ids.len(), 24);
        assert_eq!(sys.count(), 24);
        assert_eq!(sys.spawn(ActorSpec { kind: Kind::Cat, role: Role::Resident, at: RouteId(1), look: 0 }, &g), Err(Capacity));
        // and a place that does not exist cannot be started from
        let mut sys = ActorSystem::new();
        assert_eq!(sys.spawn(ActorSpec { kind: Kind::Person, role: Role::Worker, at: RouteId(99), look: 0 }, &g), Err(Capacity));
    }

    #[test]
    fn stale_actor_command_cannot_control_reused_slot() {
        let (mut sys, g, mut o, d, _) = rig();
        let first = person(&mut sys, &g, 1);
        sys.remove(first, &mut o, &d);
        let second = person(&mut sys, &g, 2);
        assert_eq!(first.slot, second.slot, "the slot was reused");
        assert_ne!(first.generation, second.generation);
        // a command that was meant for the first actor does nothing to the second
        assert_eq!(sys.command(first, ActorCommand::Move(RouteId(3)), &g, &mut o), Err(ActorError::Stale));
        assert_eq!(sys.get(second).unwrap().state, ActorState::Idle);
        assert!(sys.get(first).is_none());
        // and the same for reservations and carrying
        assert_eq!(sys.command(first, ActorCommand::Reserve(AreaId(1)), &g, &mut o), Err(ActorError::Stale));
        assert_eq!(sys.reservation_count(), 0);
    }

    #[test]
    fn an_unknown_place_is_refused() {
        let (mut sys, g, mut o, _, _) = rig();
        let a = person(&mut sys, &g, 1);
        assert_eq!(sys.command(a, ActorCommand::Move(RouteId(404)), &g, &mut o), Err(ActorError::Unknown));
        assert_eq!(sys.command(a, ActorCommand::Carry(ObjectId(404)), &g, &mut o), Err(ActorError::Unknown));
    }
}
