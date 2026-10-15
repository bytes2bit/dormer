//! How things in a scene hear about each other, and when jobs fall due.
//!
//! An `Event` is something that happened at a place: an impact, a noise, a
//! worker arriving. Consumers subscribe by distance, layer and tag. Three
//! rules keep a storm of them finite: the queue has a fixed size (with a
//! little kept back for events that finish a job), an event is delivered at
//! most once to each subscriber for the same origin, and the depth of the
//! chain a disturbance can set off is capped, so a reaction can never
//! provoke itself forever.
//!
//! Deadlines run in simulation ticks, which only advance when the world is
//! stepped: a hidden or held output does not age its jobs.

use super::model::ObjectId;
use crate::gfx::P;

/// Pending events an output can hold.
pub const QUEUE: usize = 256;
/// Of those, places that only events which complete or hand over work may use.
pub const RESERVED: usize = 32;
/// How many hops a chain of reactions may make.
pub const MAX_DEPTH: u8 = 4;
/// Remembered (origin, subscriber) pairs, so one origin is delivered once.
const REMEMBERED: usize = 512;
/// Two disturbances of one kind this near count as the same one when the queue is full.
const NEAR: f32 = 24.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum EventKind {
    Impact,
    Damage,
    Noise,
    Obstruction,
    Arrival,
    RepairProgress,
    WeatherChanged,
    Assistance,
}

impl EventKind {
    /// Events that finish or hand over a job are never crowded out by disturbances.
    pub fn is_critical(self) -> bool {
        matches!(self, EventKind::Arrival | EventKind::RepairProgress | EventKind::Assistance)
    }

    fn bit(self) -> u16 {
        1 << (self as u16)
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Event {
    pub kind: EventKind,
    pub source: ObjectId,
    pub at: P,
    pub layer: u8,
    /// what set the chain off: events with one origin are one disturbance
    pub origin: u64,
    /// hops from the origin
    pub depth: u8,
    /// what sort of thing it concerns (a subscriber can listen for just some)
    pub tags: u16,
}

impl Event {
    pub fn new(kind: EventKind, source: ObjectId, at: P, origin: u64) -> Event {
        Event { kind, source, at, layer: 0, origin, depth: 0, tags: 0 }
    }

    /// What a reaction to this event emits: the same origin, one hop further.
    pub fn follow(&self, kind: EventKind, source: ObjectId, at: P) -> Event {
        Event { kind, source, at, layer: self.layer, origin: self.origin, depth: self.depth.saturating_add(1), tags: self.tags }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum EmitResult {
    Queued,
    /// the same disturbance was already waiting
    Coalesced,
    /// the queue is full of things that matter more
    Dropped,
    /// the chain has gone far enough
    Capped,
    /// a position that is not a number
    Refused,
}

/// Who wants to hear about what.
#[derive(Clone, Copy, Debug)]
pub struct Subscription {
    pub id: u16,
    /// kinds heard, as a mask built with `listens`
    pub kinds: u16,
    pub at: P,
    pub radius: f32,
    /// the layers heard, as a mask of bits (0: every layer)
    pub layers: u32,
    /// the tags heard (0: every tag)
    pub tags: u16,
    /// seconds before it hears the same kind again
    pub cooldown: f32,
}

impl Subscription {
    pub fn new(id: u16, at: P, radius: f32) -> Subscription {
        Subscription { id, kinds: 0xFFFF, at, radius, layers: 0, tags: 0, cooldown: 0.0 }
    }

    pub fn listens(mut self, kinds: &[EventKind]) -> Subscription {
        self.kinds = kinds.iter().fold(0, |m, k| m | k.bit());
        self
    }

    pub fn on_layers(mut self, layers: u32) -> Subscription {
        self.layers = layers;
        self
    }

    pub fn tagged(mut self, tags: u16) -> Subscription {
        self.tags = tags;
        self
    }

    pub fn matches(&self, e: &Event) -> bool {
        if self.kinds & e.kind.bit() == 0 {
            return false;
        }
        if self.layers != 0 && (e.layer >= 32 || self.layers & (1 << e.layer) == 0) {
            return false;
        }
        if self.tags != 0 && self.tags & e.tags == 0 {
            return false;
        }
        let (dx, dy) = (e.at.0 - self.at.0, e.at.1 - self.at.1);
        dx * dx + dy * dy <= self.radius * self.radius
    }
}

pub struct EventBus {
    queue: Vec<Event>,
    /// (origin, subscriber): one disturbance reaches one subscriber once
    delivered: Vec<(u64, u16)>,
    next_slot: usize,
    /// last time each (subscriber, kind) was served, for cooldowns
    served: Vec<(u16, EventKind, f64)>,
    pub dropped: u64,
    pub coalesced: u64,
}

impl Default for EventBus {
    fn default() -> Self {
        EventBus::new()
    }
}

impl EventBus {
    pub fn new() -> EventBus {
        EventBus {
            queue: Vec::with_capacity(QUEUE),
            delivered: Vec::with_capacity(REMEMBERED),
            next_slot: 0,
            served: Vec::with_capacity(64),
            dropped: 0,
            coalesced: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.queue.len()
    }

    pub fn is_empty(&self) -> bool {
        self.queue.is_empty()
    }

    pub fn emit(&mut self, event: Event) -> EmitResult {
        if !event.at.0.is_finite() || !event.at.1.is_finite() {
            return EmitResult::Refused;
        }
        if event.depth > MAX_DEPTH {
            return EmitResult::Capped;
        }
        // the same disturbance, already waiting
        if self.queue.iter().any(|q| q.kind == event.kind && q.source == event.source && q.origin == event.origin) {
            self.coalesced += 1;
            return EmitResult::Coalesced;
        }
        let limit = if event.kind.is_critical() { QUEUE } else { QUEUE - RESERVED };
        if self.queue.len() >= limit {
            // fold into a neighbour of the same kind before giving up on it
            let (x, y) = event.at;
            if !event.kind.is_critical()
                && self.queue.iter().any(|q| q.kind == event.kind && (q.at.0 - x).abs() < NEAR && (q.at.1 - y).abs() < NEAR)
            {
                self.coalesced += 1;
                return EmitResult::Coalesced;
            }
            if self.queue.len() >= QUEUE {
                // a full queue: a critical event takes the place of the oldest disturbance
                if event.kind.is_critical() {
                    if let Some(at) = self.queue.iter().position(|q| !q.kind.is_critical()) {
                        self.queue.remove(at);
                        self.dropped += 1;
                        self.queue.push(event);
                        return EmitResult::Queued;
                    }
                }
            }
            self.dropped += 1;
            return EmitResult::Dropped;
        }
        self.queue.push(event);
        EmitResult::Queued
    }

    /// Take everything waiting, in the order it was emitted. Events emitted while this is
    /// being walked wait for the next drain.
    pub fn drain(&mut self) -> std::vec::IntoIter<Event> {
        let taken = std::mem::replace(&mut self.queue, Vec::with_capacity(QUEUE));
        taken.into_iter()
    }

    /// Which subscribers should be told of `event` at time `now`: each at most once for this
    /// origin, and not again within its cooldown. Fills `out` with indexes into `subs`.
    pub fn route(&mut self, event: &Event, subs: &[Subscription], now: f64, out: &mut Vec<usize>) {
        out.clear();
        for (i, s) in subs.iter().enumerate() {
            if !s.matches(event) {
                continue;
            }
            if self.delivered.iter().any(|d| d.0 == event.origin && d.1 == s.id) {
                continue;
            }
            if s.cooldown > 0.0 && self.served.iter().any(|v| v.0 == s.id && v.1 == event.kind && now - v.2 < s.cooldown as f64) {
                continue;
            }
            self.remember(event.origin, s.id);
            if s.cooldown > 0.0 {
                match self.served.iter().position(|v| v.0 == s.id && v.1 == event.kind) {
                    Some(at) => self.served[at].2 = now,
                    None if self.served.len() < 256 => self.served.push((s.id, event.kind, now)),
                    None => {}
                }
            }
            out.push(i);
        }
    }

    fn remember(&mut self, origin: u64, id: u16) {
        if self.delivered.len() < REMEMBERED {
            self.delivered.push((origin, id));
        } else {
            self.delivered[self.next_slot] = (origin, id);
            self.next_slot = (self.next_slot + 1) % REMEMBERED;
        }
    }

    pub fn clear(&mut self) {
        self.queue.clear();
        self.delivered.clear();
        self.served.clear();
        self.next_slot = 0;
    }
}

/// Jobs that fall due at a tick, in the order they were scheduled for it.
pub struct DeadlineQueue<T> {
    /// kept sorted by (due, order)
    items: Vec<(u64, u64, T)>,
    order: u64,
    capacity: usize,
}

impl<T> DeadlineQueue<T> {
    pub fn new(capacity: usize) -> DeadlineQueue<T> {
        DeadlineQueue { items: Vec::with_capacity(capacity), order: 0, capacity }
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Schedule `job` for tick `due`. Err gives the job back when the queue is full.
    pub fn schedule(&mut self, due: u64, job: T) -> Result<(), T> {
        if self.items.len() >= self.capacity {
            return Err(job);
        }
        self.order += 1;
        let key = (due, self.order);
        let at = self.items.partition_point(|i| (i.0, i.1) <= key);
        self.items.insert(at, (due, self.order, job));
        Ok(())
    }

    /// The next job that is due at `tick`, if any. Call until it returns None.
    pub fn pop_due(&mut self, tick: u64) -> Option<T> {
        if self.items.first().is_some_and(|i| i.0 <= tick) {
            Some(self.items.remove(0).2)
        } else {
            None
        }
    }

    /// The tick the next job falls due.
    pub fn next_due(&self) -> Option<u64> {
        self.items.first().map(|i| i.0)
    }

    pub fn retain(&mut self, mut keep: impl FnMut(&T) -> bool) {
        self.items.retain(|i| keep(&i.2));
    }

    pub fn clear(&mut self) {
        self.items.clear();
    }
}

/// Fixed simulation steps from presentation time, with no catching up after a gap.
#[derive(Clone, Copy, Debug)]
pub struct SimClock {
    pub tick: u64,
    carry: f64,
    pub paused: bool,
}

/// Seconds per simulation tick.
pub const STEP: f64 = 1.0 / 30.0;
/// The most time one presentation frame is allowed to stand for.
pub const MAX_FRAME: f64 = 0.1;

impl SimClock {
    pub fn new() -> SimClock {
        SimClock { tick: 0, carry: 0.0, paused: false }
    }

    /// Time has passed on the wall clock: how many simulation ticks does the world take now.
    /// A pause takes none, and a long gap (a sleep) counts as one frame's worth, not all of it.
    pub fn advance(&mut self, dt: f64) -> u32 {
        if self.paused || !dt.is_finite() || dt <= 0.0 {
            return 0;
        }
        self.carry += dt.min(MAX_FRAME);
        let mut n = 0;
        while self.carry >= STEP {
            self.carry -= STEP;
            self.tick += 1;
            n += 1;
        }
        n
    }

    /// Simulation time in seconds.
    pub fn seconds(&self) -> f64 {
        self.tick as f64 * STEP
    }

    /// Where a tick lies on the simulation's own clock, in seconds.
    pub fn at(tick: u64) -> f64 {
        tick as f64 * STEP
    }

    /// The tick that is `seconds` of simulation time from now (rounded up, at least one).
    pub fn after(&self, seconds: f64) -> u64 {
        self.tick + ((seconds / STEP).ceil().max(1.0) as u64)
    }
}

impl Default for SimClock {
    fn default() -> Self {
        SimClock::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(kind: EventKind, x: f32, origin: u64) -> Event {
        Event::new(kind, ObjectId(1), (x, 100.0), origin)
    }

    #[test]
    fn event_storm_is_bounded_and_deduplicated() {
        let mut bus = EventBus::new();
        // one disturbance reported again and again is one event
        assert_eq!(bus.emit(ev(EventKind::Impact, 10.0, 7)), EmitResult::Queued);
        for _ in 0..50 {
            assert_eq!(bus.emit(ev(EventKind::Impact, 10.0, 7)), EmitResult::Coalesced);
        }
        assert_eq!(bus.len(), 1);
        // ten thousand different ones, far apart: the queue never passes its size
        for n in 0..10_000u64 {
            bus.emit(Event::new(EventKind::Noise, ObjectId((n % 60) as u16), ((n % 500) as f32 * 40.0, (n % 77) as f32 * 40.0), 1000 + n));
            assert!(bus.len() <= QUEUE - RESERVED, "disturbances leave the reserve alone: {}", bus.len());
        }
        assert!(bus.dropped > 0);
        // the reserve is there for work that must be noticed
        for n in 0..RESERVED as u64 {
            assert_eq!(bus.emit(Event::new(EventKind::Arrival, ObjectId(1), (5.0, 5.0), 90_000 + n)), EmitResult::Queued, "{}", n);
        }
        assert_eq!(bus.len(), QUEUE);
        // and a full queue still takes one more of them, by giving up an old disturbance
        assert_eq!(bus.emit(Event::new(EventKind::RepairProgress, ObjectId(2), (6.0, 6.0), 99_000)), EmitResult::Queued);
        assert_eq!(bus.len(), QUEUE);
        // neighbours of one kind fold together when it is full
        let before = bus.coalesced;
        let mut next = Event::new(EventKind::Noise, ObjectId(9), (0.0, 0.0), 555_555);
        next.at = bus.queue.iter().find(|q| q.kind == EventKind::Noise).unwrap().at;
        assert_eq!(bus.emit(next), EmitResult::Coalesced);
        assert_eq!(bus.coalesced, before + 1);
        assert_eq!(bus.emit(Event::new(EventKind::Impact, ObjectId(1), (f32::NAN, 0.0), 1)), EmitResult::Refused);
        assert_eq!(bus.drain().count(), QUEUE);
        assert!(bus.is_empty());
    }

    #[test]
    fn distance_layer_and_tags_filter_subscribers() {
        let mut bus = EventBus::new();
        let glass = 0b01;
        let mut e = ev(EventKind::Impact, 100.0, 1);
        e.layer = 2;
        e.tags = glass;
        let near = Subscription::new(1, (110.0, 100.0), 30.0);
        let far = Subscription::new(2, (400.0, 100.0), 30.0);
        let wrong_layer = Subscription::new(3, (100.0, 100.0), 30.0).on_layers(1 << 5);
        let right_layer = Subscription::new(4, (100.0, 100.0), 30.0).on_layers(1 << 2);
        let wrong_tag = Subscription::new(5, (100.0, 100.0), 30.0).tagged(0b10);
        let right_tag = Subscription::new(6, (100.0, 100.0), 30.0).tagged(glass);
        let deaf = Subscription::new(7, (100.0, 100.0), 30.0).listens(&[EventKind::Noise]);
        let subs = [near, far, wrong_layer, right_layer, wrong_tag, right_tag, deaf];
        let mut out = Vec::new();
        bus.route(&e, &subs, 0.0, &mut out);
        assert_eq!(out.iter().map(|i| subs[*i].id).collect::<Vec<_>>(), vec![1, 4, 6]);
        // the same origin again reaches nobody: each hears of a disturbance once
        bus.route(&e, &subs, 0.1, &mut out);
        assert!(out.is_empty());
        // a new origin does; a cooldown holds back a second one that comes too soon
        let mut quick = Subscription::new(8, (100.0, 100.0), 30.0);
        quick.cooldown = 2.0;
        let subs = [quick];
        bus.route(&ev(EventKind::Impact, 100.0, 10), &subs, 10.0, &mut out);
        assert_eq!(out, vec![0]);
        bus.route(&ev(EventKind::Impact, 100.0, 11), &subs, 10.5, &mut out);
        assert!(out.is_empty());
        bus.route(&ev(EventKind::Impact, 100.0, 12), &subs, 12.5, &mut out);
        assert_eq!(out, vec![0]);
    }

    #[test]
    fn deadlines_preserve_order_and_pause() {
        let mut q: DeadlineQueue<&str> = DeadlineQueue::new(8);
        q.schedule(30, "c").unwrap();
        q.schedule(10, "a").unwrap();
        q.schedule(10, "b").unwrap(); // later in order, same tick
        q.schedule(20, "mid").unwrap();
        assert_eq!(q.next_due(), Some(10));
        assert_eq!(q.pop_due(9), None, "nothing is due early");
        assert_eq!(q.pop_due(10), Some("a"));
        assert_eq!(q.pop_due(10), Some("b"));
        assert_eq!(q.pop_due(10), None);
        assert_eq!(q.pop_due(100), Some("mid"));
        assert_eq!(q.pop_due(100), Some("c"));
        // full: the job comes back
        for n in 0..8 {
            q.schedule(n, "x").unwrap();
        }
        assert_eq!(q.schedule(1, "late"), Err("late"));

        // time: a pause (hidden, held) takes no ticks however long it lasts
        let mut clock = SimClock::new();
        let due = clock.after(1.0);
        clock.paused = true;
        for _ in 0..1000 {
            assert_eq!(clock.advance(0.1), 0);
        }
        assert_eq!(clock.tick, 0);
        clock.paused = false;
        // resuming after a long sleep is one frame's worth, not a catch-up of everything missed
        assert!(clock.advance(3600.0) <= 3);
        assert!(clock.tick < due);
        // while ordinary frames add up to the right amount
        let mut c = SimClock::new();
        for _ in 0..300 {
            c.advance(1.0 / 60.0);
        }
        assert!((c.seconds() - 5.0).abs() < 0.1, "{}", c.seconds());
        assert_eq!(c.advance(f64::NAN), 0);
        assert_eq!(c.advance(-1.0), 0);
    }

    #[test]
    fn propagation_stops_feedback() {
        // two things that answer each other: an impact makes a noise, a noise makes an impact
        let mut bus = EventBus::new();
        let subs = [
            Subscription::new(1, (0.0, 0.0), 1e6).listens(&[EventKind::Impact]),
            Subscription::new(2, (0.0, 0.0), 1e6).listens(&[EventKind::Noise]),
        ];
        bus.emit(ev(EventKind::Impact, 0.0, 42));
        let (mut handled, mut max_depth, mut rounds) = (0, 0, 0);
        let mut out = Vec::new();
        while !bus.is_empty() {
            rounds += 1;
            assert!(rounds < 100, "feedback loop");
            for e in bus.drain().collect::<Vec<_>>() {
                handled += 1;
                max_depth = max_depth.max(e.depth);
                bus.route(&e, &subs, 0.0, &mut out);
                for i in out.clone() {
                    let answer = if subs[i].id == 1 { EventKind::Noise } else { EventKind::Impact };
                    // each reaction emits a follow-on from a different object each time
                    bus.emit(e.follow(answer, ObjectId(10 + e.depth as u16), e.at));
                }
            }
        }
        // one origin reaches each subscriber once, so the chain is two hops at most here
        assert!(handled <= 3, "{} events handled", handled);
        assert!(max_depth <= MAX_DEPTH);
        // and with fresh origins every time, the depth cap is what stops a runaway
        let mut e = ev(EventKind::Impact, 0.0, 1);
        let mut hops = 0;
        loop {
            e = e.follow(EventKind::Noise, ObjectId(1), (0.0, 0.0));
            if bus.emit(e) == EmitResult::Capped {
                break;
            }
            hops += 1;
            bus.clear();
            assert!(hops < 20);
        }
        assert_eq!(hops, MAX_DEPTH as usize);
    }
}
