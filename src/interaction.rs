//! Local, finite responses to touching the wallpaper.

use std::f32::consts::{PI, TAU};

use crate::color::{hex, mix, Rgb};
use crate::gfx::{Blend, Draw, Frame, P};
use crate::props::{self, Layout};
use crate::rng::hash2;
use crate::scene::Scene;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Action {
    Birds,
    Leaves,
    Ripple,
    Dust,
    Sparks,
    Light,
    Cat,
}

/// Ordered far to near. The last matching target receives the input.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Target {
    pub id: usize,
    pub rect: [f32; 4],
    pub at: P,
    pub action: Action,
    pub col: Rgb,
    pub behind: Option<usize>,
}

impl Target {
    pub fn new(id: usize, rect: [f32; 4], at: P, action: Action, col: Rgb, behind: Option<usize>) -> Self {
        Self { id, rect, at, action, col, behind }
    }

    pub fn near(id: usize, at: P, radius: f32, action: Action, col: Rgb, behind: Option<usize>) -> Self {
        Self::new(id, [at.0 - radius, at.1 - radius, at.0 + radius, at.1 + radius], at, action, col, behind)
    }
}

pub fn hit(targets: &[Target], p: P) -> Option<Target> {
    targets.iter().rev().copied().find(|t| {
        let [x0, y0, x1, y1] = t.rect;
        p.0 >= x0 && p.0 < x1 && p.1 >= y0 && p.1 < y1
    })
}

pub fn parse_point(text: &str) -> Option<P> {
    let (x, y) = text.split_once(',')?;
    let p: P = (x.trim().parse().ok()?, y.trim().parse().ok()?);
    (p.0.is_finite() && p.1.is_finite()).then_some(p)
}

pub fn common_targets(sc: &Scene, lay: &Layout, a: &crate::views::Anchors) -> Vec<Target> {
    let mut out = Vec::new();
    if !sc.void && sc.eph.sun.el > -4.0 && !sc.storm && sc.rain < 0.5 && sc.snow < 0.6 {
        if let Some(p) = a.perch {
            out.push(Target::near(1, lay.back(p.0, p.1), 32.0, Action::Birds, hex(sc.p.bird), Some(0)));
        }
        if sc.season != 0 {
            for (n, g) in a.glades.iter().enumerate() {
                let p0 = lay.back(g[0], g[1]);
                let p1 = lay.back(g[2], g[3]);
                let col = if sc.season == 1 { [232.0, 178.0, 192.0] } else { [186.0, 126.0, 74.0] };
                out.push(Target::new(10 + n, [p0.0, p0.1, p1.0, p1.1], ((p0.0 + p1.0) / 2.0, (p0.1 + p1.1) / 2.0), Action::Leaves, col, None));
            }
        }
    }
    out
}

/// Wayland coordinates are logical, not buffer pixels (including rotated outputs).
pub fn pointer_position(p: (f64, f64), logical: (u32, u32), lay: &Layout) -> Option<P> {
    if logical.0 == 0 || logical.1 == 0 || !p.0.is_finite() || !p.1.is_finite()
        || p.0 < 0.0 || p.1 < 0.0 || p.0 >= logical.0 as f64 || p.1 >= logical.1 as f64 {
        return None;
    }
    Some(lay.back(p.0 as f32 * lay.w / logical.0 as f32, p.1 as f32 * lay.h / logical.1 as f32))
}

#[derive(Clone, Copy)]
struct Reaction {
    target: Target,
    at: P,
    age: f32,
    salt: u32,
}

impl Reaction {
    fn life(&self) -> f32 {
        match self.target.action {
            Action::Birds => 8.0,
            Action::Leaves => 4.5,
            Action::Ripple => 2.4,
            Action::Dust => 2.8,
            Action::Sparks => 3.2,
            Action::Light | Action::Cat => 1.4,
        }
    }
}

pub struct Interaction {
    pointer: Option<P>,
    hover: Option<Target>,
    strength: f32,
    transition: bool,
    pending_motion: bool,
    reactions: [Option<Reaction>; 8],
    next: usize,
    serial: u32,
    last_click: Option<f64>,
}

impl Interaction {
    pub fn new() -> Self {
        Self { pointer: None, hover: None, strength: 0.0, transition: false, pending_motion: false,
            reactions: [None; 8], next: 0, serial: 0, last_click: None }
    }

    pub fn reset(&mut self) { *self = Self::new(); }

    pub fn motion(&mut self, p: Option<P>) {
        let p = p.filter(|p| p.0.is_finite() && p.1.is_finite());
        if self.pointer != p {
            self.pointer = p;
            self.pending_motion = true;
        }
    }

    /// Record a bounded action, without painting or rebuilding the scene.
    pub fn click(&mut self, p: P, now: f64, targets: &[Target]) -> bool {
        if !now.is_finite() || self.last_click.is_some_and(|last| now - last + 1e-9 < 0.12) { return false; }
        let Some(target) = hit(targets, p) else { return false };
        self.last_click = Some(now);
        self.serial = self.serial.wrapping_add(1);
        let at = if matches!(target.action, Action::Ripple | Action::Dust | Action::Leaves | Action::Sparks) { p } else { target.at };
        self.reactions[self.next] = Some(Reaction { target, at, age: 0.0, salt: self.serial });
        self.next = (self.next + 1) % self.reactions.len();
        true
    }

    pub fn step(&mut self, dt: f32, targets: &[Target]) {
        let dt = if dt.is_finite() { dt.clamp(0.0, 0.12) } else { 0.0 };
        let wanted = self.pointer.and_then(|p| hit(targets, p));
        if wanted.is_some() && wanted.map(|t| t.id) != self.hover.map(|t| t.id) {
            self.strength = 0.0;
            self.hover = wanted;
        } else if let Some(t) = wanted { self.hover = Some(t); }
        let goal = if wanted.is_some() { 1.0 } else { 0.0 };
        self.strength += (goal - self.strength) * (1.0 - (-dt * 14.0).exp());
        if (self.strength - goal).abs() < 0.002 { self.strength = goal; }
        if self.strength == 0.0 && goal == 0.0 { self.hover = None; }
        self.transition = self.strength != goal;
        self.pending_motion = false;
        for slot in &mut self.reactions {
            if let Some(r) = slot { r.age += dt; if r.age >= r.life() { *slot = None; } }
        }
    }

    pub fn active(&self) -> bool { self.pending_motion || self.transition || self.reactions.iter().any(Option::is_some) }

    #[cfg(test)]
    fn hover_strength(&self) -> f32 { self.strength }
    #[cfg(test)]
    fn reaction_count(&self) -> usize { self.reactions.iter().flatten().count() }
    #[cfg(test)]
    fn particle_count(&self) -> usize {
        self.reactions.iter().flatten().filter(|r| matches!(r.target.action, Action::Birds | Action::Leaves | Action::Dust | Action::Sparks)).count() * 12
    }

    pub fn draw(&self, d: &mut Draw<Frame>, sc: &Scene, lay: &Layout) {
        if let Some(t) = self.hover {
            d.s.behind = t.behind;
            let a = self.strength;
            if t.action == Action::Cat {
                cat_ears(d, lay, t.at, t.col, a, 0.0);
            } else if t.action != Action::Birds && t.action != Action::Leaves {
                let radius = if t.action == Action::Ripple { 18.0 } else { 22.0 };
                d.glow(lay.dx(t.at.0), lay.dy(t.at.1), lay.d(radius), lay.d(radius * 0.65), t.col, a * (0.04 + 0.10 * sc.dark), 2, Blend::Screen);
            }
        }
        for r in self.reactions.iter().flatten() {
            d.s.behind = r.target.behind;
            let a = r.age;
            let fade = (1.0 - a / r.life()).max(0.0);
            let col = r.target.col;
            match r.target.action {
                Action::Ripple => {
                    for n in 0..3 {
                        let age = a - n as f32 * 0.22;
                        if age <= 0.0 { continue; }
                        let rx = 5.0 + age * 31.0;
                        let pts: [P; 33] = std::array::from_fn(|k| {
                            let ang = k as f32 / 32.0 * TAU;
                            (lay.dx(r.at.0 + rx * ang.cos()), lay.dy(r.at.1 + rx * 0.20 * ang.sin()))
                        });
                        d.path_mode(&pts, lay.d(0.8).max(0.65), col, fade * 0.26 * (age * 5.0).min(1.0), Blend::Screen);
                    }
                }
                Action::Light => {
                    let pulse = (a * PI / r.life()).sin().max(0.0);
                    d.glow(lay.dx(r.at.0), lay.dy(r.at.1), lay.d(36.0), lay.d(24.0), col, pulse * (0.13 + 0.16 * sc.dark), 2, Blend::Screen);
                }
                Action::Cat => cat_ears(d, lay, r.at, col, fade, a * 5.0),
                action => {
                    for n in 0..12 {
                        let h = |salt| hash2(n, r.salt.wrapping_mul(31).wrapping_add(salt));
                        let angle = -PI + h(1) * PI;
                        let speed = 24.0 + h(2) * 54.0;
                        let (x, y) = match action {
                            Action::Birds => {
                                // Same decelerating scatter and climbing motion as the ambient flock.
                                let travel = a - a * a / 18.0;
                                (r.at.0 + angle.cos() * speed * 1.5 * travel, r.at.1 + angle.sin() * speed * 0.55 * travel - a * 6.0)
                            }
                            Action::Dust => (r.at.0 + angle.cos() * speed * a * 0.6, r.at.1 + angle.sin() * speed * a + 17.0 * a * a),
                            Action::Leaves => (r.at.0 + (sc.wind_x * 22.0 + angle.cos() * 15.0) * a + (a * 2.0 + h(3) * TAU).sin() * 9.0, r.at.1 - speed * a * 0.4 + 6.0 * a * a),
                            _ => (r.at.0 + angle.cos() * speed * a * 0.35 + (a * 2.0 + h(3) * TAU).sin() * 5.0, r.at.1 - speed * a * 0.35),
                        };
                        let (x, y) = (lay.dx(x), lay.dy(y));
                        match action {
                            Action::Birds => props::bird(d, x, y, lay.d(6.0 + h(4) * 3.0), 0.2 + 0.75 * (a * (11.0 + h(5) * 4.0) + h(6) * TAU).sin(), mix(hex(sc.p.bird), sc.sky[3], 0.25), fade),
                            Action::Dust => d.dot(x, y, lay.d(1.2 + h(4)), lay.d(0.8 + h(4)), col, fade * 0.45, Blend::Over),
                            Action::Leaves => {
                                let v = lay.d(2.0 + h(4) * 1.5);
                                d.ellipse(x, y, v, v * (0.3 + 0.5 * (a * 3.0 + h(6) * TAU).sin().abs()), col, fade * 0.7);
                            }
                            _ => d.glow(x, y, lay.d(4.0), lay.d(4.0), col, fade * (0.35 + 0.35 * (a * 4.0 + h(6) * TAU).sin()), 2, Blend::Screen),
                        }
                    }
                }
            }
        }
        d.s.behind = None;
    }
}

/// A small ear movement, drawn in the cat's own fur, rather than a UI halo.
fn cat_ears(d: &mut Draw<Frame>, lay: &Layout, at: P, col: Rgb, attention: f32, phase: f32) {
    let lean = attention * (0.8 + 0.4 * phase.sin());
    let (x, y, u) = (lay.dx(at.0), lay.dy(at.1), lay.u * 2.5);
    for side in [-1.0, 1.0] {
        let ex = x + side * 3.0 * u;
        d.poly(&[(ex - 1.8 * u, y + 1.5 * u), (ex + side * lean * 1.5 * u, y - (3.0 + lean) * u), (ex + 2.0 * u, y + u)], col, attention * 0.75);
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::props::Layout;

    fn target(id: usize, action: Action) -> Target {
        Target::new(id, [700.0, 700.0, 900.0, 900.0], (800.0, 800.0), action, [200.0, 180.0, 140.0], None)
    }

    #[test]
    fn pointer_matches_fractional_and_portrait_objects() {
        let lay = Layout::new(2000, 1250, 0.7);
        assert_eq!(pointer_position((800.0, 800.0), (1600, 1000), &lay), Some((800.0, 800.0)));
        let portrait = Layout::new(540, 960, 0.7);
        let p = pointer_position((270.0, 900.0), (540, 960), &portrait).unwrap();
        assert!((p.0 - 800.0).abs() < 0.01);
        assert!((p.1 - 822.2222).abs() < 0.01);
        assert!(pointer_position((f64::NAN, 0.0), (540, 960), &portrait).is_none());
        assert!(pointer_position((-1.0, 0.0), (540, 960), &portrait).is_none());
        assert!(pointer_position((0.0, 0.0), (0, 0), &portrait).is_none());
    }

    #[test]
    fn nearer_targets_win_and_empty_space_does_nothing() {
        let targets = [target(1, Action::Ripple), target(2, Action::Light)];
        assert_eq!(hit(&targets, (800.0, 800.0)).unwrap().id, 2);
        assert!(hit(&targets, (600.0, 600.0)).is_none());
        assert!(hit(&targets, (f32::NAN, 800.0)).is_none());
    }

    #[test]
    fn preview_coordinates_require_two_finite_numbers() {
        assert_eq!(parse_point("850,920"), Some((850.0, 920.0)));
        assert_eq!(parse_point(" 1.5, -12 "), Some((1.5, -12.0)));
        for input in ["1", "1,2,3", "NaN,2", "1,inf", "x,4"] { assert!(parse_point(input).is_none()); }
    }

    #[test]
    fn hover_settles_and_leave_releases_it() {
        let targets = [target(1, Action::Light)];
        let mut i = Interaction::new();
        i.motion(Some((800.0, 800.0)));
        i.step(0.05, &targets);
        assert!(i.active());
        for _ in 0..100 { i.step(0.05, &targets); }
        assert!(!i.active(), "stationary hover must settle");
        assert!(i.hover_strength() > 0.99);
        i.motion(None);
        for _ in 0..100 { i.step(0.05, &targets); }
        assert_eq!(i.hover_strength(), 0.0);
        assert!(!i.active());
    }

    #[test]
    fn clicks_are_bounded_ordered_and_expire() {
        let targets = [target(1, Action::Dust)];
        let mut i = Interaction::new();
        assert!(i.click((800.0, 800.0), 0.0, &targets));
        assert!(!i.click((800.0, 800.0), 0.119, &targets));
        assert!(i.click((800.0, 800.0), 0.12, &targets));
        for n in 2..1000 { i.click((800.0, 800.0), n as f64 * 0.13, &targets); }
        i.step(0.01, &targets);
        assert_eq!(i.reaction_count(), 8);
        assert!(i.particle_count() <= 96);
        assert!(i.active());
        for _ in 0..400 { i.step(0.05, &targets); }
        assert_eq!(i.reaction_count(), 0);
        assert!(!i.active());
    }

    #[test]
    fn reset_drops_focus_and_other_monitors_are_independent() {
        let targets = [target(1, Action::Birds)];
        let mut first = Interaction::new();
        let second = Interaction::new();
        first.motion(Some((800.0, 800.0)));
        first.click((800.0, 800.0), 0.0, &targets);
        first.step(0.05, &targets);
        assert!(first.active());
        assert!(!second.active());
        first.reset();
        assert!(!first.active());
        assert_eq!(first.reaction_count(), 0);
        assert_eq!(first.hover_strength(), 0.0);
    }

    pub fn scene(view: &dyn crate::views::View, rain: bool) -> Scene {
        let mut wx = crate::sky::from_wmo(if rain { 61 } else { 0 });
        wx.temp = 18.0;
        crate::scene::scene(crate::scene::Inputs {
            ts: 1791925200.0, wx: &wx, mode: "dark", machine: Default::default(),
            place: &crate::scene::Place { lat: 35.7758, lon: -5.7909, name: String::new() },
            drift: None, top: None, rates: Default::default(), horizon: view.horizon(),
            knobs: &[], void: view.sky() == crate::views::Sky::Void,
        })
    }

    #[test]
    fn every_view_has_local_targets_and_expired_effects_touch_nothing() {
        for view in crate::views::ALL {
            let cache = crate::paint::Cache::new(view, 12, 640, 400);
            let mut sc = scene(view, true);
            sc.live = true;
            let (cv, anchors) = crate::paint::render(&mut sc, &cache);
            let targets = cache.stage.targets(&sc, &anchors);
            assert!(!targets.is_empty(), "{} has no targets", view.id());
            let t = targets[0];
            let mut i = Interaction::new();
            assert!(i.click(t.at, 0.0, &targets), "{} target cannot be clicked", view.id());
            i.step(0.12, &targets);
            let mut pixels = cv.finish(sc.p.grain);
            let original = pixels.clone();
            let mut dirty = Vec::new();
            let mut shaper = crate::gfx::Shaper::new();
            let mut f = Frame { w: 640, h: 400, px: &mut pixels, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None };
            i.draw(&mut Draw::new(&mut f, &mut shaper), &sc, &cache.lay);
            assert!(!dirty.is_empty(), "{} response is invisible", view.id());
            assert!(pixels != original, "{} response must change pixels", view.id());
            assert!(dirty.iter().map(|r| r.area()).sum::<i64>() < 640 * 400 / 2);
            for _ in 0..100 { i.step(0.12, &targets); }
            dirty.clear();
            let before = pixels.clone();
            let mut f = Frame { w: 640, h: 400, px: &mut pixels, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None };
            i.draw(&mut Draw::new(&mut f, &mut shaper), &sc, &cache.lay);
            assert!(dirty.is_empty());
            assert_eq!(pixels, before);
        }
    }

    #[test]
    fn wet_street_and_void_targets_follow_the_scene() {
        let street = crate::views::find("street").unwrap();
        let cache = crate::paint::Cache::new(street, 12, 640, 400);
        let mut sc = scene(street, false);
        sc.live = true;
        sc.cat_out = true;
        sc.cat_asleep = false;
        let (_, a) = crate::paint::render(&mut sc, &cache);
        let dry = cache.stage.targets(&sc, &a);
        assert!(!dry.iter().any(|t| t.action == Action::Cat || t.action == Action::Ripple));
        sc.rain = 0.8;
        sc.cat_out = false;
        let wet = cache.stage.targets(&sc, &a);
        assert!(wet.iter().any(|t| t.action == Action::Cat));
        assert!(wet.iter().any(|t| t.action == Action::Ripple));
        let moon = crate::views::find("moon").unwrap();
        let cache = crate::paint::Cache::new(moon, 12, 640, 400);
        let sc = scene(moon, true);
        assert!(!cache.stage.targets(&sc, &crate::views::Anchors::default()).iter().any(|t| matches!(t.action, Action::Birds | Action::Leaves)));
    }
}
