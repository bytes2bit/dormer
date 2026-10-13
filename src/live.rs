//! The part of the wallpaper that moves.
//!
//! `World` is a small simulation of everything that should not stand still:
//!
//!   by day     birds crossing, planes drawing their trails, leaves on the wind,
//!              the crane turning, washing and the pennant moving in the wind
//!   by night   stars that twinkle, shooting stars, a blinking crane light,
//!              televisions flickering behind a few windows, fireflies in summer
//!   weather    rain and snow that fall, lightning, smoke that leans with the wind
//!   yours      the lamp in your roof room breathes while an agent is working
//!
//! It lives in design coordinates (1600 by 1000, the 16:10 picture) and is
//! drawn straight onto screen pixels, over the still frame, every tick.
//!
//! What is in the air is the same in every view. What stands on the ground
//! is the view's: the still painter leaves `Anchors` saying where the
//! chimneys, lamps and beacons are, and this animates them; anything a view
//! wants to move by itself it draws in its own `alive`.

use std::f32::consts::{PI, TAU};

use crate::color::{hex, mix, smooth, Rgb, AMBER, WARM};
use crate::gfx::{Blend, Draw, Frame, Rect};
use crate::paint::Cache;
use crate::props::{self, Layout, DH as VH, DW as VW};
use crate::rng::Fast;
use crate::scene::Scene;
use crate::views::Anchors;

const GLOWWORM: Rgb = [216.0, 226.0, 122.0];
const LEAVES: [Rgb; 3] = [[160.0, 86.0, 44.0], [176.0, 138.0, 51.0], [201.0, 122.0, 60.0]];
const PETALS: [Rgb; 3] = [[232.0, 178.0, 192.0], [240.0, 204.0, 212.0], [222.0, 160.0, 180.0]];

struct Bird {
    dx: f32,
    dy: f32,
    ph: f32,
    rate: f32,
    /// its own heading, for birds scattering from the tower
    vx: f32,
    vy: f32,
}

struct Flock {
    way: f32,
    x: f32,
    y: f32,
    speed: f32,
    depth: f32,
    age: f32,
    bob: f32,
    climb: f32,
    birds: Vec<Bird>,
    scatter: bool,
}

struct Meteor {
    x: f32,
    y: f32,
    dx: f32,
    dy: f32,
    dist: f32,
    dur: f32,
    age: f32,
    len: f32,
    thick: f32,
}

struct Plane {
    way: f32,
    x: f32,
    y: f32,
    slope: f32,
    speed: f32,
}

struct Puff {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    size: f32,
    grow: f32,
    col: Rgb,
    /// which mask hides it: 5 for nothing
    behind: usize,
}

struct Leaf {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    spin: f32,
    turn: f32,
    ph: f32,
    kind: usize,
    size: f32,
    age: f32,
}

struct Fly {
    /// where it lives inside its glade, 0 to 1 each way
    x: f32,
    y: f32,
    a: f32,
    b: f32,
    c: f32,
}

struct Note {
    x: f32,
    y: f32,
    age: f32,
    life: f32,
    sway: f32,
    pair: bool,
}

struct Drop {
    x: f32,
    y: f32,
    len: f32,
    a: f32,
    speed: f32,
}

struct Flake {
    x: f32,
    y: f32,
    r: f32,
    a: f32,
    speed: f32,
    ph: f32,
}

/// A firework: where it burst, its colour, how big it gets, and the angles of its sparks.
struct Burst {
    x: f32,
    y: f32,
    col: Rgb,
    size: f32,
    age: f32,
    rays: Vec<f32>,
}

/// A satellite catching the sun after dusk: a steady point crossing the sky.
struct Satellite {
    x: f32,
    y: f32,
    vx: f32,
    vy: f32,
    age: f32,
    life: f32,
    bright: f32,
}

struct Flash {
    age: f32,
    pts: Vec<(f32, f32)>,
}

/// Where the things this simulates start from, in design coordinates.
#[derive(Default)]
pub struct Sources {
    /// x, y, scale, colour, the layer that hides it
    pub chimneys: Vec<(f32, f32, f32, Rgb, usize)>,
    /// the one that smokes with the processor
    pub stove: Option<(f32, f32, f32, Rgb, usize)>,
    pub perch: Option<(f32, f32)>,
    pub music: Option<(f32, f32)>,
    pub leaves: bool,
}

impl Sources {
    pub fn from(a: &Anchors, lay: &Layout) -> Sources {
        let smoke = |c: &crate::views::Smoke| {
            let (x, y) = lay.back(c.x, c.y);
            (x, y, c.scale, c.col, c.behind)
        };
        Sources {
            chimneys: a.smoke.iter().map(smoke).collect(),
            stove: a.stove.as_ref().map(smoke),
            perch: a.perch.map(|p| lay.back(p.0, p.1)),
            music: a.music.map(|p| lay.back(p.0, p.1)),
            leaves: a.leaves,
        }
    }
}

pub struct World {
    r: Fast,
    pub t: f64,
    flocks: Vec<Flock>,
    meteors: Vec<Meteor>,
    planes: Vec<Plane>,
    puffs: Vec<Puff>,
    leaves: Vec<Leaf>,
    flies: Vec<Fly>,
    notes: Vec<Note>,
    flash: Option<Flash>,
    bursts: Vec<Burst>,
    satellites: Vec<Satellite>,
    due_burst: f64,
    due_satellite: f64,
    due_flock: f64,
    due_meteor: f64,
    due_plane: f64,
    due_gust: f64,
    due_flash: f64,
    due_note: f64,
    drops: Vec<Drop>,
    flakes: Vec<Flake>,
    tv: Vec<(f32, f64)>,
    acc: Vec<f32>,
    pipe_acc: f32,
    last_hour: i64,
    rates: crate::scene::Rates,
}

impl World {
    pub fn new(seed: u64) -> World {
        let mut r = Fast::new(seed);
        let flies = (0..12)
            .map(|_| Fly {
                x: r.f(),
                y: r.f(),
                a: r.range(0.0, 6.28),
                b: r.range(0.2, 0.6),
                c: r.range(0.6, 1.6),
            })
            .collect();
        World {
            r,
            t: 0.0,
            flocks: Vec::new(),
            meteors: Vec::new(),
            planes: Vec::new(),
            puffs: Vec::new(),
            leaves: Vec::new(),
            flies,
            notes: Vec::new(),
            flash: None,
            bursts: Vec::new(),
            satellites: Vec::new(),
            due_burst: 1.0,
            due_satellite: 20.0,
            due_flock: 2.0,
            due_meteor: 5.0,
            due_plane: 12.0,
            due_gust: 6.0,
            due_flash: 3.0,
            due_note: 1.0,
            drops: Vec::new(),
            flakes: Vec::new(),
            tv: Vec::new(),
            acc: Vec::new(),
            pipe_acc: 0.0,
            last_hour: -1,
            rates: Default::default(),
        }
    }

    /// The scene has been worked out afresh: take note of what changed.
    pub fn retune(&mut self, sc: &Scene, tvs: usize) {
        if self.rates != sc.rates {
            self.rates = sc.rates;
            // the dev panel turned something up: do not make it wait
            let soon = self.t + 1.5;
            self.due_flock = self.due_flock.min(soon);
            self.due_meteor = self.due_meteor.min(soon);
            self.due_plane = self.due_plane.min(soon);
        }
        self.tv.resize(tvs, (0.0, 0.0));
        let want = if sc.rain > 0.0 { ((50.0 + 230.0 * sc.rain) * 6.5) as usize } else { 0 };
        if self.drops.len() != want {
            let r = &mut self.r;
            self.drops = (0..want)
                .map(|k| {
                    let near = k % 9 < 5;
                    Drop {
                        x: r.range(0.0, 2048.0),
                        y: r.range(0.0, 1280.0),
                        len: r.range(16.0, 44.0) * (0.6 + 0.5 * sc.rain) * if near { 1.0 } else { 0.75 },
                        a: r.range(0.13, 0.42) * if near { 0.95 } else { 0.6 },
                        speed: if near { 1050.0 } else { 720.0 },
                    }
                })
                .collect();
        }
        let want = if sc.snow > 0.0 { ((30.0 + 90.0 * sc.snow) * 15.0) as usize } else { 0 };
        if self.flakes.len() != want {
            let r = &mut self.r;
            self.flakes = (0..want)
                .map(|k| {
                    let (big, speed, a) = [(2.2, 72.0, 0.95), (1.5, 48.0, 0.8), (1.0, 30.0, 0.65)][k % 3];
                    Flake { x: r.range(0.0, 2048.0), y: r.range(0.0, 1280.0), r: r.range(0.8, 1.6) * big, a: r.range(0.45, 0.92) * a, speed, ph: r.range(0.0, 6.28) }
                })
                .collect();
        }
    }

    /// The view has changed: what was rising from the old one's chimneys has nowhere to be.
    pub fn leave(&mut self) {
        self.puffs.clear();
        self.notes.clear();
        self.flocks.retain(|f| !f.scatter);
    }

    /// Is anything moving fast enough to want every frame? (birds, rain, falling stars)
    pub fn lively(&self) -> bool {
        !self.flocks.is_empty()
            || !self.meteors.is_empty()
            || !self.leaves.is_empty()
            || !self.drops.is_empty()
            || !self.flakes.is_empty()
            || self.flash.is_some()
            || !self.bursts.is_empty()
    }

    pub fn raining(&self) -> bool {
        !self.drops.is_empty() || self.flash.is_some()
    }

    /// Does the whole picture change every frame? (rain, snow, a lightning flash)
    pub fn everywhere(&self) -> bool {
        !self.drops.is_empty() || !self.flakes.is_empty() || self.flash.is_some()
    }

    pub fn step(&mut self, dt: f32, sc: &Scene, src: &Sources, now: f64) {
        self.t += dt as f64;
        let t = self.t;
        let rates = sc.rates;

        if sc.eph.sun.el > -3.0 && !sc.storm && sc.snow < 0.6 && sc.fog < 0.6 && rates.birds > 0.0 {
            if t >= self.due_flock && self.flocks.len() < 3 + rates.birds as usize {
                let wait = self.r.range(5.0, 17.0) * if sc.eph.sun.el > 20.0 { 1.6 } else { 1.0 } * if sc.rain > 0.3 { 3.0 } else { 1.0 };
                self.due_flock = t + (wait / rates.birds) as f64;
                self.spawn_flock(None);
            }
        }
        // on the hour the bell rings, and the birds on the tower take off
        let hour = (now / 3600.0).floor() as i64;
        if self.last_hour < 0 {
            self.last_hour = hour;
        } else if hour != self.last_hour {
            self.last_hour = hour;
            if let (Some(b), true) = (src.perch, sc.eph.sun.el > -4.0 && !sc.storm && sc.rain < 0.5) {
                self.scatter(b);
            }
        }
        for f in self.flocks.iter_mut() {
            f.age += dt;
            if f.scatter {
                let ease = (1.0 - f.age / 9.0).max(0.25);
                for b in f.birds.iter_mut() {
                    b.dx += b.vx * dt * ease;
                    b.dy += b.vy * dt * ease - 6.0 * dt;
                }
            } else {
                f.x += f.way * f.speed * dt;
                f.y += f.climb * dt;
            }
        }
        self.flocks.retain(|f| if f.scatter { f.age < 14.0 } else { -260.0 < f.x && f.x < VW + 260.0 });

        let shower = sc.shower.map_or(1.0, |s| s.1);
        if sc.night > 0.4 && sc.cover < 0.7 && rates.meteors > 0.0 && t >= self.due_meteor {
            let r = &mut self.r;
            let big = if r.chance(0.18) { 1.5 } else { 1.0 };
            let (ang, way) = (r.range(16.0, 42.0).to_radians(), if r.chance(0.5) { -1.0 } else { 1.0 });
            self.meteors.push(Meteor {
                x: r.range(220.0, 1380.0),
                y: r.range(60.0, 380.0),
                dx: ang.cos() * way,
                dy: ang.sin(),
                dist: r.range(260.0, 500.0) * big,
                dur: r.range(0.6, 1.0) * big,
                age: 0.0,
                len: r.range(110.0, 190.0) * big,
                thick: 4.5 * big,
            });
            self.due_meteor = t + (r.range(14.0, 55.0) / rates.meteors / shower) as f64;
        }
        for m in self.meteors.iter_mut() {
            m.age += dt;
        }
        self.meteors.retain(|m| m.age < m.dur);

        if sc.cover < 0.8 && sc.fog < 0.5 && rates.planes > 0.0 && t >= self.due_plane && self.planes.len() < 2 + rates.planes as usize {
            let r = &mut self.r;
            let way = if r.chance(0.5) { -1.0 } else { 1.0 };
            self.planes.push(Plane { way, x: if way > 0.0 { -40.0 } else { VW + 40.0 }, y: r.range(80.0, 330.0), slope: r.range(-0.05, 0.05), speed: r.range(15.0, 23.0) });
            self.due_plane = t + (r.range(55.0, 150.0) / sc.busy.max(0.2) / rates.planes) as f64;
        }
        for p in self.planes.iter_mut() {
            p.x += p.way * p.speed * dt;
            p.y += p.slope * p.speed * dt;
        }
        self.planes.retain(|p| -460.0 < p.x && p.x < VW + 460.0);

        // smoke
        self.acc.resize(src.chimneys.len(), 0.0);
        let r = &mut self.r;
        for (k, &(x, y, scale, col, behind)) in src.chimneys.iter().enumerate() {
            self.acc[k] += dt * 1.3;
            while self.acc[k] >= 1.0 {
                self.acc[k] -= 1.0;
                if self.puffs.len() < 170 {
                    self.puffs.push(Puff {
                        x: x + r.range(-1.5, 1.5),
                        y,
                        vx: sc.wind_x * r.range(3.0, 8.0),
                        vy: -r.range(11.0, 17.0) * scale,
                        age: 0.0,
                        life: r.range(4.2, 6.4),
                        size: 5.0 * scale,
                        grow: r.range(2.6, 3.6) * scale,
                        col,
                        behind,
                    });
                }
            }
        }
        if let (Some((x, y, scale, col, behind)), true) = (src.stove, sc.machine.load > 0.35) {
            // the harder the processor works, the more the stovepipe smokes
            self.pipe_acc += dt * (1.2 + 3.6 * sc.machine.load);
            while self.pipe_acc >= 1.0 {
                self.pipe_acc -= 1.0;
                if self.puffs.len() < 190 {
                    self.puffs.push(Puff {
                        x: x + r.range(-1.0, 1.0),
                        y,
                        vx: sc.wind_x * r.range(3.0, 8.0),
                        vy: -r.range(10.0, 15.0),
                        age: 0.0,
                        life: r.range(3.5, 5.5),
                        size: 4.9 * scale,
                        grow: r.range(2.0, 3.0),
                        col,
                        behind,
                    });
                }
            }
        }
        for p in self.puffs.iter_mut() {
            p.vx += sc.wind_x * 8.0 * dt;
            p.vy *= 1.0 - 0.10 * dt;
            p.x += p.vx * dt;
            p.y += p.vy * dt;
            p.age += dt;
        }
        self.puffs.retain(|p| p.age < p.life);

        // leaves in autumn, petals in spring
        if src.leaves && (sc.season == 1 || sc.season == 3) && sc.wind >= 5.0 && sc.rain < 0.5 && sc.snow == 0.0 && t >= self.due_gust {
            let side = if sc.wind_x >= 0.0 { 1.0 } else { -1.0 };
            for k in 0..2 + r.below(6) {
                self.leaves.push(Leaf {
                    x: (if side > 0.0 { -30.0 } else { VW + 30.0 }) - side * k as f32 * r.range(30.0, 90.0),
                    y: r.range(380.0, 930.0),
                    vx: side * r.range(70.0, 150.0) * (0.6 + sc.wind_x.abs().min(1.5)),
                    vy: r.range(8.0, 34.0),
                    spin: r.range(-4.5, 4.5),
                    turn: r.range(0.0, TAU),
                    ph: r.range(0.0, 6.28),
                    kind: r.below(3),
                    size: r.range(9.0, 15.0),
                    age: 0.0,
                });
            }
            self.due_gust = t + r.range(8.0, 28.0) as f64;
        }
        for l in self.leaves.iter_mut() {
            l.age += dt;
            l.x += l.vx * dt;
            l.y += (l.vy + (l.age * 3.0 + l.ph).sin() * 24.0) * dt;
            l.turn += l.spin * dt;
        }
        self.leaves.retain(|l| -400.0 < l.x && l.x < VW + 400.0 && l.y < VH + 30.0);

        // music
        if sc.machine.audio && sc.machine.agents + 1 > 0 {
            if let (Some((x, y)), true) = (src.music, t >= self.due_note) {
                self.notes.push(Note { x: x + r.range(14.0, 34.0), y: y + 6.0, age: 0.0, life: r.range(3.2, 4.6), sway: r.range(0.0, 6.28), pair: r.chance(0.4) });
                self.due_note = t + r.range(0.9, 2.2) as f64;
            }
        }
        for n in self.notes.iter_mut() {
            n.age += dt;
            n.y -= 11.0 * dt;
            n.x += ((n.age * 1.6 + n.sway).sin() * 5.0 + sc.wind_x * 4.0) * dt;
        }
        self.notes.retain(|n| n.age < n.life);

        if sc.storm {
            match self.flash.as_mut() {
                Some(f) => {
                    f.age += dt;
                    if f.age > 0.55 {
                        self.flash = None;
                    }
                }
                None if t >= self.due_flash => {
                    let (mut x, mut y, mut pts) = (r.range(240.0, 1360.0), 170.0, Vec::new());
                    while y < 790.0 {
                        pts.push((x, y));
                        x += r.range(-30.0, 30.0);
                        y += r.range(20.0, 55.0);
                    }
                    self.flash = Some(Flash { age: 0.0, pts });
                    self.due_flash = t + r.range(4.0, 15.0) as f64;
                }
                None => {}
            }
        } else {
            self.flash = None;
        }

        // fireworks at the turn of the year
        if sc.fireworks && t >= self.due_burst {
            let col = r.pick(&[AMBER, crate::color::ROSE, crate::color::BLUE, crate::color::SAGE, WARM[0], crate::color::CREAM]);
            self.bursts.push(Burst {
                x: r.range(260.0, 1340.0),
                y: r.range(200.0, 520.0),
                col,
                size: r.range(55.0, 120.0),
                age: 0.0,
                rays: (0..26).map(|k| k as f32 / 26.0 * TAU + r.range(-0.09, 0.09)).collect(),
            });
            self.due_burst = t + r.range(0.5, 2.6) as f64;
        }
        for b in self.bursts.iter_mut() {
            b.age += dt;
        }
        self.bursts.retain(|b| b.age < 2.2);

        // in the hour or two after dusk and before dawn, satellites are still in sunlight
        let el = sc.eph.sun.el as f32;
        if (-24.0..-7.0).contains(&el) && sc.cover < 0.6 && t >= self.due_satellite {
            let way = if r.chance(0.5) { -1.0 } else { 1.0 };
            let life = r.range(34.0, 60.0);
            self.satellites.push(Satellite {
                x: if way > 0.0 { r.range(100.0, 700.0) } else { r.range(900.0, 1500.0) },
                y: r.range(90.0, 520.0),
                vx: way * r.range(9.0, 17.0),
                vy: r.range(-5.0, 2.0),
                age: 0.0,
                life,
                bright: r.range(0.45, 0.95),
            });
            self.due_satellite = t + r.range(150.0, 520.0) as f64;
        }
        for s in self.satellites.iter_mut() {
            s.age += dt;
            s.x += s.vx * dt;
            s.y += s.vy * dt;
        }
        self.satellites.retain(|s| s.age < s.life);

        for tv in self.tv.iter_mut() {
            if t >= tv.1 {
                *tv = (r.pick(&[0.0, 0.0, 0.10, 0.20, 0.32, 0.46]), t + r.range(0.07, 0.6) as f64);
            }
        }
    }

    pub fn spawn_flock(&mut self, x: Option<f32>) {
        let r = &mut self.r;
        let (way, depth) = (if r.chance(0.5) { -1.0 } else { 1.0 }, r.range(0.7, 1.3));
        let mut birds = vec![Bird { dx: 0.0, dy: 0.0, ph: r.range(0.0, 6.28), rate: r.range(9.0, 13.0), vx: 0.0, vy: 0.0 }];
        for k in 1..r.pick(&[1usize, 1, 2, 3, 3, 5, 7, 9]) {
            let (rank, side) = (((k + 1) / 2) as f32, if k % 2 == 1 { 1.0 } else { -1.0 });
            birds.push(Bird { dx: -rank * r.range(17.0, 23.0) * depth, dy: side * rank * r.range(7.0, 11.0) * depth, ph: r.range(0.0, 6.28), rate: r.range(9.0, 13.0), vx: 0.0, vy: 0.0 });
        }
        self.flocks.push(Flock {
            way,
            x: x.unwrap_or(if way > 0.0 { -60.0 } else { VW + 60.0 }),
            y: r.range(410.0, 690.0),
            speed: r.range(52.0, 88.0) * depth,
            depth,
            age: 0.0,
            bob: r.range(0.0, 6.28),
            climb: r.range(-5.0, 5.0),
            birds,
            scatter: false,
        });
    }

    /// Birds bursting from one place in every direction, then drifting off.
    pub fn scatter(&mut self, from: (f32, f32)) {
        let r = &mut self.r;
        let birds = (0..9 + r.below(6))
            .map(|_| {
                let a = r.range(-PI, 0.0);
                let v = r.range(34.0, 78.0);
                Bird { dx: r.range(-8.0, 8.0), dy: r.range(-4.0, 6.0), ph: r.range(0.0, 6.28), rate: r.range(11.0, 15.0), vx: a.cos() * v * 1.5, vy: a.sin() * v * 0.55 }
            })
            .collect();
        self.flocks.push(Flock { way: 1.0, x: from.0, y: from.1, speed: 0.0, depth: 0.8, age: 0.0, bob: 0.0, climb: 0.0, birds, scatter: true });
    }

    pub fn spawn_meteor(&mut self) {
        self.due_meteor = self.t;
    }

    /// Draw everything that moves onto one screen.
    pub fn draw(&self, d: &mut Draw<Frame>, sc: &Scene, cache: &Cache, a: &Anchors, now: f64) {
        let lay = &cache.lay;
        let t = self.t as f32;
        let k = lay.w / VW;
        let px = |x: f32| x / VW * lay.w;
        let py = |y: f32| lay.oy + y / VH * lay.hv;
        // heights in the sky keep their place above the horizon, wherever the view puts it
        let psy = |y: f32| lay.sy(y / VH);
        let u = lay.u;
        let snug = lay.snug();
        let star = hex(sc.p.star);

        d.s.behind = Some(0);
        if sc.night > 0.05 {
            for (i, s) in a.stars.iter().enumerate() {
                let i = i as f32;
                let pulse = (t * (0.9 + (i * 0.37) % 1.9) + i * 2.4).sin().max(0.0).powi(6);
                let amount = pulse * (0.45 + 0.55 * s.low) * s.a;
                if amount > 0.04 {
                    d.dot(s.x, s.y, s.r * 1.25, s.r * 1.25, s.col, amount, Blend::Screen);
                    if s.r > 1.2 * u && amount > 0.3 {
                        let reach = s.r * 3.4;
                        d.path_mode(&[(s.x - reach, s.y), (s.x + reach, s.y)], 0.9 * u, s.col, 0.35 * amount, Blend::Screen);
                        d.path_mode(&[(s.x, s.y - reach), (s.x, s.y + reach)], 0.9 * u, s.col, 0.35 * amount, Blend::Screen);
                    }
                }
            }
        }
        for m in &self.meteors {
            let p = m.age / m.dur;
            let ease = 1.0 - (1.0 - p) * (1.0 - p);
            let tail = m.len * (p * 3.5).min(1.0) * k;
            let (hx, hy) = (px(m.x + m.dx * m.dist * ease), psy(m.y + m.dy * m.dist * ease));
            let alpha = (PI * p).sin().powf(0.6) * sc.night;
            for i in 0..14 {
                let (f0, f1) = (i as f32 / 14.0, (i + 1) as f32 / 14.0);
                d.path_mode(
                    &[(hx - m.dx * tail * f0, hy - m.dy * tail * f0), (hx - m.dx * tail * f1, hy - m.dy * tail * f1)],
                    (m.thick * 0.42 * (1.0 - f0 * 0.75) * k).max(0.8),
                    star,
                    alpha * (1.0 - f0).powf(1.6) * 0.8,
                    Blend::Screen,
                );
            }
            d.glow(hx, hy, m.thick * 1.5 * k, m.thick * 1.5 * k, [255.0; 3], alpha, 2, Blend::Screen);
        }
        for s in &self.satellites {
            // it brightens as it rises into the sunlight and winks out as it enters the earth's shadow
            let f = s.age / s.life;
            let a = s.bright * smooth(0.0, 0.2, f) * smooth(1.0, 0.86, f) * sc.night.max(0.3);
            d.dot(px(s.x), psy(s.y), 1.25 * u, 1.25 * u, star, a, Blend::Screen);
        }
        for b in &self.bursts {
            let f = b.age / 2.2;
            let reach = b.size * k * (1.0 - (1.0 - f).powi(3));
            let fade = (1.0 - f).powf(1.4);
            let (x, y) = (px(b.x), psy(b.y));
            d.glow(x, y, reach * 1.5, reach * 1.5, b.col, 0.22 * fade, 2, Blend::Screen);
            for (i, a) in b.rays.iter().enumerate() {
                let far = reach * (0.82 + 0.18 * ((i * 7) % 5) as f32 / 4.0);
                let droop = 26.0 * k * f * f; // sparks fall as they burn out
                let tip = (x + a.cos() * far, y + a.sin() * far + droop);
                let tail = (x + a.cos() * far * (0.35 + 0.5 * f), y + a.sin() * far * (0.35 + 0.5 * f) + droop * 0.6);
                d.path_mode(&[tail, tip], 1.3 * u, b.col, 0.75 * fade, Blend::Screen);
                d.dot(tip.0, tip.1, 1.7 * u, 1.7 * u, mix(b.col, [255.0; 3], 0.55), fade, Blend::Screen);
            }
        }
        for p in &self.planes {
            let strobe = if (t * 0.9).fract() < 0.10 { 1.0 } else { 0.0 };
            props::plane(d, sc, lay, px(p.x), psy(p.y), p.way, p.slope, strobe);
        }
        let ink = mix(hex(sc.p.bird), sc.sky[3], 0.25);
        for f in &self.flocks {
            let size = 24.0 * f.depth * k;
            let drift = (f.age * 0.13 + f.bob).sin() * 18.0;
            let fade = if f.scatter { smooth(14.0, 9.0, f.age) } else { 1.0 };
            for b in &f.birds {
                let mut beat = (f.age * b.rate + b.ph).sin();
                if !f.scatter && (f.age * 0.7 + b.ph * 1.7).sin() > 0.55 {
                    beat = 0.3; // gliding for a moment
                }
                let x = px(f.x + f.way * b.dx);
                let at = f.y + b.dy + drift - beat * 1.4 * f.depth;
                // a flock crossing keeps to the sky; birds leaving a perch start from the ground
                let y = if f.scatter { py(at) } else { psy(at) };
                let shrink = if f.scatter { 1.0 - 0.35 * (f.age / 14.0) } else { 1.0 };
                props::bird(d, x, y, size * shrink, 0.2 + 0.75 * beat, ink, 0.92 * fade);
            }
        }

        if snug {
            for p in &self.puffs {
                let age = p.age / p.life;
                let s = (p.size + p.grow * p.age) * k;
                d.s.behind = Some(p.behind);
                d.puff(px(p.x), py(p.y), s, s * 0.85, p.col, 0.44 * (p.age / 0.6).min(1.0) * (1.0 - age).powf(1.4), 0.1);
            }
            for b in &a.blink {
                d.s.behind = b.behind;
                let pulse = (TAU * (t / b.every + b.phase)).sin().max(0.0).powi(6);
                d.glow(b.x, b.y, b.r, b.r, b.col, 0.9 * pulse * sc.dark, 2, Blend::Screen);
            }
            for l in &a.breath {
                d.s.behind = l.behind;
                let breath = (0.20 + 0.16 * (t * 1.15).sin()) * l.strength;
                // a lamp that is nearly out of battery gutters
                let breath = if l.low { breath * (0.5 + 0.5 * (t * 9.0).sin() * (t * 2.3).sin()).max(0.1) } else { breath };
                d.glow(l.x, l.y, l.rx, l.ry, l.col, breath, 2, Blend::Screen);
            }
            if sc.dark > 0.5 {
                for (tv, state) in a.flicker.iter().zip(&self.tv) {
                    if state.0 > 0.0 {
                        d.s.behind = Some(tv.1);
                        let [x, y, w, h] = tv.0;
                        d.rect(x, y, x + w, y + h, [150.0, 196.0, 236.0], state.0);
                    }
                }
            }
            for (i, b) in a.twinkle.iter().enumerate() {
                let tw = (t * (1.3 + (i % 5) as f32 * 0.35) + i as f32 * 1.9).sin().max(0.0).powi(3);
                if tw > 0.05 {
                    d.s.behind = Some(b.3);
                    d.glow(b.0, b.1, 9.0 * u, 9.0 * u, mix(b.2, WARM[1], 0.4), 0.55 * tw, 2, Blend::Screen);
                }
            }
            cache.stage.alive(d, sc, a, t, now);
            d.s.behind = None;
            let ink = mix(hex(sc.p.ink), AMBER, 0.4);
            for n in &self.notes {
                let f = n.age / n.life;
                props::note(d, px(n.x), py(n.y), u * (0.85 + 0.25 * f), ink, 0.85 * smooth(0.0, 0.12, f) * smooth(1.0, 0.55, f), n.pair);
            }
        }

        d.s.behind = None;
        let tints = if sc.season == 1 { PETALS } else { LEAVES };
        for l in &self.leaves {
            let (x, y, s) = (px(l.x), py(l.y), l.size * k);
            let (c, sn) = (l.turn.cos(), l.turn.sin());
            let squash = 0.35 + 0.65 * (l.age * 2.3 + l.ph).sin().abs();
            let at = |a: f32, b: f32| (x + (a * c - b * sn * squash) * s * 0.5, y + (a * sn + b * c * squash) * s * 0.5);
            let pts: Vec<(f32, f32)> = if sc.season == 1 {
                (0..10).map(|i| i as f32 / 10.0 * TAU).map(|a| at(a.cos() * 0.72, a.sin() * 0.5)).collect()
            } else {
                vec![at(-1.0, 0.0), at(-0.4, -0.5), at(0.4, -0.5), at(1.0, 0.0), at(0.4, 0.5), at(-0.4, 0.5)]
            };
            d.poly(&pts, tints[l.kind], 0.22 + 0.73 * sc.day);
        }
        if sc.season == 2 && sc.dark > 0.6 && sc.rain == 0.0 && !a.glades.is_empty() {
            for (i, f) in self.flies.iter().enumerate() {
                let g = a.glades[i % a.glades.len()];
                let x = g[0] + (g[2] - g[0]) * f.x + (t * f.b + f.a).sin() * 34.0 * k;
                let y = g[1] + (g[3] - g[1]) * f.y + (t * f.b * 0.8 + f.a * 2.0).cos() * 16.0 * k;
                let on = (t * f.c + f.a).sin().max(0.0).powi(2);
                d.glow(x, y, 6.0 * k, 6.0 * k, GLOWWORM, 0.9 * on, 2, Blend::Screen);
            }
        }

        if !self.flakes.is_empty() {
            let col = hex(sc.p.snow);
            let (bw, bh) = (2048.0, 1280.0);
            for (i, f) in self.flakes.iter().enumerate() {
                let sway = (t * (0.55 - 0.1 * (i % 3) as f32) + f.ph).sin() * 16.0;
                let x = (f.x + sc.wind_x * 26.0 * t + sway).rem_euclid(bw) - 224.0;
                let y = (f.y + f.speed * t).rem_euclid(bh) - 140.0;
                d.dot(px(x), py(y), f.r * k, f.r * k, col, f.a, Blend::Over);
            }
        }
        if !self.drops.is_empty() {
            let col = hex(sc.p.rain);
            let lean = (sc.wind_x * 0.45).clamp(-0.7, 0.7);
            let vis = 0.6 + 0.4 * sc.day;
            let (bw, bh) = (2048.0, 1280.0);
            for dr in &self.drops {
                let x = (dr.x + lean * dr.speed * t).rem_euclid(bw) - 224.0;
                let y = (dr.y + dr.speed * t).rem_euclid(bh) - 140.0;
                streak(d.s, px(x), py(y), lean, dr.len * k, col, dr.a * vis);
            }
            d.s.dirty.push(Rect { x0: 0, y0: 0, x1: d.s.w as i32, y1: d.s.h as i32 });
        }
        if let Some(f) = &self.flash {
            let lit = (1.0 - f.age / 0.5).max(0.0) * if f.age < 0.07 || (0.13..0.21).contains(&f.age) { 1.0 } else { 0.35 };
            d.rect_mode(0.0, 0.0, lay.w, lay.sy(0.86), [214.0, 224.0, 242.0], 0.22 * lit, Blend::Screen);
            if f.age < 0.21 {
                let pts: Vec<(f32, f32)> = f.pts.iter().map(|p| (px(p.0), psy(p.1))).collect();
                d.path_mode(&pts, 6.0 * u, [190.0, 205.0, 240.0], 0.3, Blend::Screen);
                d.path(&pts, 2.0 * u, [238.0, 242.0, 252.0], 0.95);
            }
        }
    }
}

/// A raindrop: a thin, nearly upright line, written straight into the pixels.
fn streak(f: &mut Frame, x: f32, y: f32, lean: f32, len: f32, col: Rgb, a: f32) {
    let (y0, y1) = ((y.max(0.0)) as i32, ((y + len).min(f.h as f32)) as i32);
    let k = (a * 256.0) as i32;
    if y1 <= y0 || k <= 0 {
        return;
    }
    let (cr, cg, cb) = (col[0] as i32, col[1] as i32, col[2] as i32);
    let w = f.w as i32;
    for yy in y0..y1 {
        let xf = x + lean * (yy as f32 + 0.5 - y);
        let xi = xf.floor();
        let frac = ((xf - xi) * 256.0) as i32;
        for (xx, part) in [(xi as i32, 256 - frac), (xi as i32 + 1, frac)] {
            if xx < 0 || xx >= w {
                continue;
            }
            let kk = (k * part) >> 8;
            let p = &mut f.px[yy as usize * f.w + xx as usize];
            let (r, g, b) = (((*p >> 16) & 255) as i32, ((*p >> 8) & 255) as i32, (*p & 255) as i32);
            *p = 0xFF00_0000
                | (((r + (((cr - r) * kk) >> 8)) as u32) << 16)
                | (((g + (((cg - g) * kk) >> 8)) as u32) << 8)
                | (b + (((cb - b) * kk) >> 8)) as u32;
        }
    }
}

/// Tidy a list of touched rectangles into fewer that cover the same pixels: neighbours are joined,
/// and when there are still a great many they are snapped to a coarse grid and merged into runs.
/// It never answers with one box round everything, because that would repaint the whole sky for
/// the sake of a few stars.
pub fn coalesce(mut rects: Vec<Rect>, w: i32, h: i32) -> Vec<Rect> {
    if rects.is_empty() {
        return rects;
    }
    let full = Rect { x0: 0, y0: 0, x1: w, y1: h };
    if rects.iter().any(|r| r.area() * 10 > full.area() * 7) {
        return vec![full];
    }
    if rects.len() <= 400 {
        for _ in 0..4 {
            let mut merged = false;
            let mut i = 0;
            while i < rects.len() {
                let mut j = i + 1;
                while j < rects.len() {
                    if rects[i].touches(&rects[j], 4) {
                        rects[i] = rects[i].join(&rects[j]);
                        rects.swap_remove(j);
                        merged = true;
                    } else {
                        j += 1;
                    }
                }
                i += 1;
            }
            if !merged {
                break;
            }
        }
        if rects.len() <= 160 {
            return rects;
        }
    }
    // too many to list one by one: mark the cells of a grid they touch, and report runs of cells
    const CELL: i32 = 32;
    let (gw, gh) = (((w + CELL - 1) / CELL) as usize, ((h + CELL - 1) / CELL) as usize);
    let mut grid = vec![false; gw * gh];
    for r in &rects {
        let (cx0, cx1) = ((r.x0.max(0) / CELL) as usize, (((r.x1.min(w) + CELL - 1) / CELL) as usize).min(gw));
        let (cy0, cy1) = ((r.y0.max(0) / CELL) as usize, (((r.y1.min(h) + CELL - 1) / CELL) as usize).min(gh));
        for cy in cy0..cy1 {
            grid[cy * gw + cx0..cy * gw + cx1.max(cx0)].fill(true);
        }
    }
    let mut out: Vec<Rect> = Vec::new();
    let mut open: Vec<usize> = Vec::new(); // runs from the row above that can still grow downwards
    for cy in 0..gh {
        let mut next = Vec::new();
        let mut cx = 0;
        while cx < gw {
            if !grid[cy * gw + cx] {
                cx += 1;
                continue;
            }
            let start = cx;
            while cx < gw && grid[cy * gw + cx] {
                cx += 1;
            }
            let (x0, x1) = (start as i32 * CELL, (cx as i32 * CELL).min(w));
            let (y0, y1) = (cy as i32 * CELL, ((cy as i32 + 1) * CELL).min(h));
            match open.iter().find(|&&k| out[k].x0 == x0 && out[k].x1 == x1 && out[k].y1 == y0) {
                Some(&k) => {
                    out[k].y1 = y1;
                    next.push(k);
                }
                None => {
                    out.push(Rect { x0, y0, x1, y1 });
                    next.push(out.len() - 1);
                }
            }
        }
        open = next;
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn damage_is_joined_up() {
        let r = |x0, y0, x1, y1| Rect { x0, y0, x1, y1 };
        let out = coalesce(vec![r(0, 0, 10, 10), r(8, 8, 20, 20), r(500, 500, 510, 510)], 1920, 1200);
        assert_eq!(out.len(), 2);
        assert!(out.contains(&r(0, 0, 20, 20)));
        assert_eq!(coalesce(vec![r(0, 0, 1900, 1100)], 1920, 1200), vec![r(0, 0, 1920, 1200)]);
        // a sky full of separate stars stays a sky full of small rectangles
        let stars: Vec<Rect> = (0..500).map(|k| r((k * 37) % 1900, (k * 91) % 1100, (k * 37) % 1900 + 4, (k * 91) % 1100 + 4)).collect();
        let out = coalesce(stars.clone(), 1920, 1200);
        let area: i64 = out.iter().map(|r| r.area()).sum();
        assert!(area < 1920 * 1200 / 3, "{} rectangles covering {}", out.len(), area);
        let covered = |x: i32, y: i32| out.iter().any(|o| o.x0 <= x && x < o.x1 && o.y0 <= y && y < o.y1);
        for s in &stars {
            assert!(covered(s.x0, s.y0) && covered(s.x1 - 1, s.y0) && covered(s.x0, s.y1 - 1) && covered(s.x1 - 1, s.y1 - 1));
        }
    }
}
