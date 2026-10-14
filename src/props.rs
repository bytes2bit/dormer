//! The things that live on the roofs and in the air.
//!
//! Each is drawn by one function that works on any surface, so the still
//! picture and the live layer draw exactly the same cat, kite and crane; the
//! live layer just passes a clock that keeps moving.

use std::f32::consts::PI;

use crate::color::{mix, shade, Rgb, AMBER, BEACON, BLUE, CREAM, ROSE, SAGE, WARM, WHITE};
use crate::gfx::{Blend, Draw, Surface, P};
use crate::rng::PyRng;
use crate::scene::Scene;
use crate::sky::HORIZON;

/// The size every view is designed at. Views are drawn in these units and scaled to the screen.
pub const DW: f32 = 1600.0;
pub const DH: f32 = 1000.0;

/// How the 16:10 design sits on a screen of any shape: the ground keeps its
/// proportions and stands on the bottom edge, and the sky takes what is left.
#[derive(Clone, Copy, Debug)]
pub struct Layout {
    pub w: f32,
    pub h: f32,
    /// the old town's reference pixels to screen pixels
    pub s: f32,
    /// one pixel of a 1200-line screen
    pub u: f32,
    /// where the top of the 16:10 design falls (above the screen when negative)
    pub oy: f32,
    /// the height of the 16:10 design
    pub hv: f32,
    /// where this view's horizon lies, as a fraction of the design's height
    pub hz: f32,
    /// how far this view's sky is stretched from the one the sky was first drawn for
    pub sk: f32,
    /// pixels that design units are slid sideways by: see `moved`
    pub ox: f32,
}

impl Layout {
    pub fn new(w: usize, h: usize, horizon: f64) -> Layout {
        let s = w as f32 / crate::town::REF_W as f32;
        let hv = crate::town::REF_H as f32 * s;
        Layout { w: w as f32, h: h as f32, s, u: hv / 1200.0, oy: h as f32 - hv, hv, hz: horizon as f32, sk: (horizon / HORIZON) as f32, ox: 0.0 }
    }

    /// The same layout with everything drawn in design units slid sideways by `by` of them: a way
    /// to move one part of a view without rewriting every number in it.
    pub fn moved(&self, by: f32) -> Layout {
        Layout { ox: self.ox + by / DW * self.w, ..*self }
    }

    /// The same layout with everything drawn in design units made `k` times as big, so that the
    /// point `from` lands where `to` was: a way to resize one part of a view as a piece.
    pub fn scaled(&self, k: f32, from: (f32, f32), to: (f32, f32)) -> Layout {
        let (w, hv) = (self.w * k, self.hv * k);
        Layout { w, hv, ox: self.dx(to.0) - from.0 / DW * w, oy: self.dy(to.1) - from.1 / DH * hv, ..*self }
    }

    /// A height in the sky, as a fraction of the picture the sky was first drawn for: clouds,
    /// birds and planes keep their place above the horizon wherever a view puts it.
    pub fn sy(&self, f: f32) -> f32 {
        self.oy + f * self.sk * self.hv
    }

    /// Design units across.
    pub fn dx(&self, v: f32) -> f32 {
        v / DW * self.w + self.ox
    }

    /// Design units down.
    pub fn dy(&self, v: f32) -> f32 {
        self.oy + v / DH * self.hv
    }

    /// A length in design units.
    pub fn d(&self, v: f32) -> f32 {
        v / DH * self.hv
    }

    /// Screen pixels back to design units.
    pub fn back(&self, x: f32, y: f32) -> (f32, f32) {
        ((x - self.ox) / self.w * DW, (y - self.oy) / self.hv * DH)
    }

    /// A fraction of the width.
    pub fn x(&self, f: f32) -> f32 {
        f * self.w
    }

    /// A fraction of the design's height.
    pub fn y(&self, f: f32) -> f32 {
        self.oy + f * self.hv
    }

    /// Reference pixels across.
    pub fn cx(&self, v: f64) -> f32 {
        v as f32 * self.s
    }

    /// Reference pixels down.
    pub fn cy(&self, v: f64) -> f32 {
        self.oy + v as f32 * self.s
    }

    pub fn horizon(&self) -> f32 {
        self.y(self.hz)
    }

    /// Does the city fit the screen the way it was designed to?
    pub fn snug(&self) -> bool {
        (self.oy / self.hv).abs() < 0.12
    }
}

/// A mast from `foot` up to `top`, and the pennant on it that shows the wind.
pub fn pennant<S: Surface>(d: &mut Draw<S>, sc: &Scene, mx: f32, foot: f32, top: f32, u: f32, col: Rgb, phase: f32) {
    d.line((mx, foot), (mx, top), 1.7 * u, col, 1.0);
    let lean = if sc.wind_x.abs() > 0.12 { sc.wind_x } else { 0.0 };
    let strength = (0.35 + lean.abs()).min(1.0);
    let reach = 19.0 * u * if lean >= 0.0 { 1.0 } else { -1.0 } * strength;
    let droop = (1.0 - (lean.abs() * 1.2).min(1.0)) * 9.0 * u;
    let gust = (lean.abs() + 0.12).min(1.0);
    let (w1, w2) = (phase.sin() * 1.3 * u * gust, (phase - 1.3).sin() * 2.4 * u * gust);
    let flag = mix(col, AMBER, 0.42 + 0.58 * sc.day);
    d.poly(
        &[
            (mx, top),
            (mx + reach * 0.5, top + 1.6 * u + droop * 0.45 + w1),
            (mx + reach, top + 5.0 * u + droop + w2),
            (mx + reach * 0.5, top + 8.0 * u + droop * 0.6 + w1),
            (mx, top + 10.0 * u),
        ],
        flag,
        1.0,
    );
}

/// Washing on a line between two posts, from `x0` to `x1` on ground at `y`.
pub fn laundry<S: Surface>(d: &mut Draw<S>, sc: &Scene, x0: f32, x1: f32, y: f32, u: f32, col: Rgb, k: usize, phase: f32) {
    let inset = (x1 - x0) * 0.10;
    let (a, b, ph) = (x0 + inset, x1 - inset, 25.0 * u);
    let lw = 1.3 * u;
    d.line((a, y), (a, y - ph), lw, col, 1.0);
    d.line((b, y), (b, y - ph), lw, col, 1.0);
    let sag = |f: f32| y - ph + 4.0 * 5.0 * u * f * (1.0 - f);
    let line: Vec<P> = (0..13).map(|i| (a + (b - a) * i as f32 / 12.0, sag(i as f32 / 12.0))).collect();
    d.path(&line, (lw * 0.7).max(0.8), col, 0.9);
    let mut r = PyRng::new(77 + k as u64);
    let cloths = [CREAM, SAGE, BLUE, AMBER, ROSE, CREAM];
    let mut x = a + 6.0 * u;
    while x < b - 16.0 * u {
        let (w, h) = (r.uniform(8.0, 15.0) as f32 * u, r.uniform(10.0, 19.0) as f32 * u);
        if r.random() < 0.82 {
            let (f0, f1) = ((x - a) / (b - a), (x + w - a) / (b - a));
            let lean = sc.wind_x * 5.0 * u * (0.6 + 0.4 * (phase + x * 0.21).sin());
            let flap = (phase * 1.7 + x * 0.4).sin() * 0.8 * u * sc.wind_x.abs().min(1.0);
            let tint = mix(col, r.pick(&cloths), 0.30 + 0.62 * sc.day);
            d.poly(
                &[(x, sag(f0)), (x + w, sag(f1)), (x + w + lean, sag(f1) + h + flap), (x + lean, sag(f0) + h - flap)],
                tint,
                1.0,
            );
        }
        x += w + r.uniform(2.0, 7.0) as f32 * u;
    }
}

/// A kite flown from (`ax`, `ay`) on a weekend afternoon.
pub fn kite<S: Surface>(d: &mut Draw<S>, sc: &Scene, ax: f32, ay: f32, u: f32, sway: f32, bob: f32, wiggle: f32) {
    let lean = if sc.wind_x.abs() > 0.2 { sc.wind_x } else { 0.4 };
    let x = ax + lean * 110.0 * u + sway.sin() * 22.0 * u;
    let y = ay - (215.0 + 26.0 * bob.sin()) * u;
    let ink = crate::color::hex(sc.p.ink);
    d.path(&[(ax, ay), ((ax + x) / 2.0 + lean * 14.0 * u, (ay + y) / 2.0 + 26.0 * u), (x, y + 13.0 * u)], u, ink, 0.43);
    let col = mix(sc.sky[3], ROSE, 0.9);
    d.poly(&[(x, y - 14.0 * u), (x + 9.0 * u, y), (x, y + 14.0 * u), (x - 9.0 * u, y)], col, 1.0);
    d.poly(&[(x, y - 14.0 * u), (x + 9.0 * u, y), (x, y + 14.0 * u)], shade(col, -0.14), 1.0);
    let tail: Vec<P> = (0..6)
        .map(|k| {
            let k = k as f32;
            (x - lean * k * 6.0 * u + (k * 1.4 + wiggle).sin() * 4.0 * u, y + 14.0 * u + k * 8.0 * u)
        })
        .collect();
    d.path(&tail, 1.5 * u, mix(sc.sky[3], AMBER, 0.9), 1.0);
}

/// Where the crane's jib points right now: (turn -1..1, reach along it, how far the hook hangs).
pub fn crane_pose(sc: &Scene, ts: f64) -> (f32, f32, f32) {
    if !sc.crane_works {
        return (-1.0, 0.25, 14.0);
    }
    let turn = (PI * (0.5 + 0.47 * (ts / 53.0).sin() as f32)).cos();
    let turn = if turn.abs() < 0.18 { 0.18f32.copysign(turn) } else { turn };
    (turn, 0.65 + 0.30 * (ts / 31.0).sin() as f32, 90.0 + 60.0 * (ts / 19.0).sin() as f32)
}

/// The crane's tower, which never moves: it stands at `x` from `y` up to `top`.
pub fn crane_mast<S: Surface>(d: &mut Draw<S>, x: f32, y: f32, top: f32, u: f32, col: Rgb) {
    let (lw, mw) = (1.5 * u, 4.5 * u);
    for sx in [-mw, mw] {
        d.line((x + sx, y), (x + sx, top), lw, col, 1.0);
    }
    let (mut yy, mut flip) = (y, 1.0);
    while yy > top {
        d.line((x - mw * flip, yy), (x + mw * flip, (yy - 13.0 * u).max(top)), lw * 0.7, col, 1.0);
        yy -= 13.0 * u;
        flip = -flip;
    }
    d.rect(x - 7.0 * u, top - 9.0 * u, x + 7.0 * u, top, col, 1.0);
    d.line((x, top), (x, top - 30.0 * u), lw, col, 1.0);
}

/// The crane's jib, its counterweight and the hook. Turning shortens and lengthens it, seen from here.
pub fn crane_jib<S: Surface>(d: &mut Draw<S>, x: f32, top: f32, u: f32, col: Rgb, pose: (f32, f32, f32)) {
    let (turn, reach, drop) = pose;
    let lw = 1.5 * u;
    let apex = (x, top - 30.0 * u);
    let (jl, cj, jy) = (128.0 * u * turn, -40.0 * u * turn, top - 9.0 * u);
    for off in [0.0, 4.0 * u] {
        d.line((x + cj, jy - off), (x + jl, jy - off), lw, col, 1.0);
    }
    let n = (jl.abs() / (9.0 * u)) as usize;
    for k in 0..n {
        let (a, b) = (x + jl * k as f32 / n as f32, x + jl * (k + 1) as f32 / n as f32);
        d.line((a, jy), (b, jy - 4.0 * u), lw * 0.5, col, 0.9);
    }
    d.line(apex, (x + jl * 0.72, jy - 4.0 * u), lw * 0.7, col, 1.0);
    d.line(apex, (x + cj, jy - 4.0 * u), lw * 0.7, col, 1.0);
    d.rect(x + cj, jy, x + cj * 0.62, jy + 9.0 * u, col, 1.0);
    let hx = x + jl * reach;
    d.line((hx, jy), (hx, jy + drop * u), lw * 0.6, col, 1.0);
    d.rect(hx - 2.5 * u, jy + drop * u, hx + 2.5 * u, jy + (drop + 5.0) * u, col, 1.0);
}

/// The cat. `tail` swings its tail; `face` is the way it looks, -1 or 1.
pub fn cat<S: Surface>(d: &mut Draw<S>, x: f32, y: f32, u: f32, col: Rgb, asleep: bool, face: f32, tail: f32) {
    if asleep {
        let breath = 1.0 + 0.05 * tail.sin();
        d.ellipse(x, y - 3.5 * u, 12.0 * u, 5.5 * u * breath, col, 1.0);
        let (hx, hy) = (x + face * 10.0 * u, y - 7.0 * u);
        d.ellipse(hx, hy, 5.0 * u, 4.5 * u, col, 1.0);
        for e in [-1.0, 1.0] {
            d.poly(&[(hx + e * 1.2 * u, hy - 3.0 * u), (hx + e * 4.6 * u, hy - 8.5 * u), (hx + e * 5.0 * u, hy - 1.5 * u)], col, 1.0);
        }
        d.path(&[(x - face * 11.0 * u, y - 3.0 * u), (x - face * 15.0 * u, y - 0.5 * u), (x - face * 6.0 * u, y + 0.5 * u)], 2.6 * u, col, 1.0);
        return;
    }
    d.ellipse(x, y - 7.5 * u, 6.5 * u, 9.5 * u, col, 1.0);
    d.poly(&[(x - 6.5 * u, y - 6.0 * u), (x + 6.5 * u, y - 6.0 * u), (x + 7.5 * u, y + u), (x - 7.5 * u, y + u)], col, 1.0);
    let (hx, hy) = (x + face * 2.0 * u, y - 20.5 * u);
    d.ellipse(hx, hy, 5.4 * u, 4.8 * u, col, 1.0);
    for e in [-1.0, 1.0] {
        d.poly(&[(hx + e * 1.0 * u, hy - 3.2 * u), (hx + e * 4.8 * u, hy - 9.5 * u), (hx + e * 5.4 * u, hy - 1.5 * u)], col, 1.0);
    }
    let swing = tail.sin();
    d.path(
        &[
            (x - face * 5.0 * u, y - u),
            (x - face * 13.0 * u, y - u),
            (x - face * (16.0 + 1.5 * swing) * u, y - (6.0 + swing) * u),
            (x - face * (14.0 + 4.0 * swing) * u, y - (10.0 + 1.5 * swing.abs()) * u),
        ],
        2.4 * u,
        col,
        1.0,
    );
}

/// A hot-air balloon drifting over on a weekend morning.
pub fn balloon<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, bob: f32) {
    let u = lay.u;
    let f = ((sc.t - 7.5) / 4.0) as f32;
    let (x, y) = (lay.x(0.26 + 0.34 * f), lay.sy(0.56 - 0.10 * (f * PI).sin() + 0.01 * bob.sin()));
    let far = |c: Rgb| mix(sc.sky[4], c, 0.80);
    let (bw, bh) = (27.0 * u, 33.0 * u);
    d.ellipse(x, y, bw / 2.0, bh / 2.0, far(AMBER), 1.0);
    d.poly(&[(x - bw * 0.43, y + bh * 0.20), (x + bw * 0.43, y + bh * 0.20), (x + 3.5 * u, y + bh * 0.74), (x - 3.5 * u, y + bh * 0.74)], far(AMBER), 1.0);
    d.ellipse(x, y + bh * 0.01, bw * 0.19, bh * 0.51, far(CREAM), 1.0);
    d.poly(&[(x - bw * 0.17, y + bh * 0.2), (x + bw * 0.17, y + bh * 0.2), (x + 1.5 * u, y + bh * 0.74), (x - 1.5 * u, y + bh * 0.74)], far(CREAM), 1.0);
    d.ellipse(x - bw * 0.36, y + bh * 0.02, bw * 0.10, bh * 0.44, far(shade(AMBER, -0.16)), 0.8);
    let basket = far(sc.layers[3]);
    d.line((x - 3.5 * u, y + bh * 0.74), (x - 3.0 * u, y + bh * 0.90), u, basket, 1.0);
    d.line((x + 3.5 * u, y + bh * 0.74), (x + 3.0 * u, y + bh * 0.90), u, basket, 1.0);
    d.rect(x - 3.5 * u, y + bh * 0.88, x + 3.5 * u, y + bh * 0.88 + 5.0 * u, basket, 1.0);
}

/// z z z over a roof where someone is sleeping in.
pub fn sleep_z<S: Surface>(d: &mut Draw<S>, sc: &Scene, x: f32, y: f32, u: f32, phase: f32) {
    let ink = crate::color::hex(sc.p.ink);
    let sway = (phase + x).sin() * 3.0 * u;
    for (k, (size, dx, dy, a)) in [(7.0, 0.0, 0.0, 0.84), (10.5, 13.0, -17.0, 0.67), (15.0, 31.0, -40.0, 0.47)].into_iter().enumerate() {
        let rise = ((phase * 0.6 + k as f32 * 0.9).sin() * 0.5 + 0.5) * 3.0 * u;
        let (zx, zy, z) = (x + dx * u + sway * (dy / -40.0), y + dy * u - rise, size * u);
        d.path(&[(zx, zy - z), (zx + z, zy - z), (zx, zy), (zx + z, zy)], 1.7 * u, ink, a);
    }
}

/// String lights from `x0` to `x1` over ground at `y`, on Friday and Saturday nights.
/// Returns the bulbs, for the live layer.
pub fn festoon<S: Surface>(d: &mut Draw<S>, x0: f32, x1: f32, y: f32, u: f32, col: Rgb) -> Vec<(f32, f32, Rgb)> {
    let posts = [x0 + (x1 - x0) * 0.06, x0 + (x1 - x0) * 0.5, x0 + (x1 - x0) * 0.94];
    let (ph, lw) = (30.0 * u, 1.3 * u);
    for px in posts {
        d.line((px, y), (px, y - ph), lw, col, 1.0);
    }
    let tints = [AMBER, ROSE, WARM[0], SAGE, WARM[1], BLUE];
    let mut bulbs = Vec::new();
    for (i, pair) in posts.windows(2).enumerate() {
        let (a, b) = (pair[0], pair[1]);
        let n = (((b - a) / (9.0 * u)) as usize).max(4);
        let line: Vec<P> = (0..=n)
            .map(|k| {
                let f = k as f32 / n as f32;
                (a + (b - a) * f, y - ph + 4.0 * 8.0 * u * f * (1.0 - f))
            })
            .collect();
        d.path(&line, (lw * 0.7).max(0.8), col, 0.9);
        for (k, p) in line[1..line.len() - 1].iter().enumerate() {
            bulbs.push((p.0, p.1 + 2.0 * u, tints[(k + i * 3) % tints.len()]));
        }
    }
    for &(px, py, tint) in &bulbs {
        d.glow(px, py, 7.0 * u, 7.0 * u, mix(tint, WARM[1], 0.5), 0.55, 2, Blend::Screen);
        d.circle(px, py, 1.9 * u, mix(tint, WHITE, 0.25), 1.0);
    }
    bulbs
}

/// A plume of smoke caught in one frame: what the still picture shows from a chimney.
pub fn smoke_still<S: Surface>(d: &mut Draw<S>, sc: &Scene, x: f32, y: f32, u: f32, col: Rgb, phase: f32) {
    let lean = sc.wind_x;
    for k in 0..9 {
        let kf = k as f32;
        let f = kf / 8.0;
        let px = x + lean * kf.powf(1.3) * 5.0 * u + (phase + kf * 0.9).sin() * 2.4 * u;
        let py = y - kf * (11.5 - 3.5 * lean.abs().min(1.5)) * u;
        let r = (4.5 + kf * 2.9) * u;
        d.puff(px, py, r, r * 0.8, col, 0.30 * (1.0 - f).powf(1.2), 0.15);
    }
}

/// One bird, wings at `lift` (-0.55 below the body to 0.95 well above it).
pub fn bird<S: Surface>(d: &mut Draw<S>, x: f32, y: f32, size: f32, lift: f32, col: Rgb, a: f32) {
    let (w, h) = (size, size * 0.636);
    let (cx, cy, span) = (x, y + h * 0.06, w * 0.46);
    let mut pts: Vec<P> = Vec::with_capacity(13);
    for side in [-1.0f32, 1.0] {
        let (tx, ty) = (cx + side * span * (lift * 0.6).cos(), cy - span * lift.sin() * 0.72);
        let (c1, c2) = ((cx + side * span * 0.22, cy - h * 0.12), (cx + side * span * 0.5, cy - span * lift.sin() * 0.5 - h * 0.07));
        let wing = |t: f32| -> P {
            let m = 1.0 - t;
            (
                m * m * m * cx + 3.0 * m * m * t * c1.0 + 3.0 * m * t * t * c2.0 + t * t * t * tx,
                m * m * m * cy + 3.0 * m * m * t * c1.1 + 3.0 * m * t * t * c2.1 + t * t * t * ty,
            )
        };
        if side < 0.0 {
            for k in (1..=6).rev() {
                pts.push(wing(k as f32 / 6.0));
            }
            pts.push((cx, cy));
        } else {
            for k in 1..=6 {
                pts.push(wing(k as f32 / 6.0));
            }
        }
    }
    d.path(&pts, (h * 0.10).max(1.0), col, a);
}

/// A plane and what it leaves behind: a trail by day, lights by night.
pub fn plane<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, x: f32, y: f32, way: f32, slope: f32, strobe: f32) {
    let u = lay.u;
    let hx = crate::color::hex;
    if sc.day > 0.25 {
        let col = mix(hx(sc.p.trail), sc.glow, 0.6 * sc.twilight);
        // one long wedge, fading away from the plane
        let length = 0.26 * lay.w;
        let (xt, yt) = (x - way * length, y - slope * lay.hv * 0.26);
        let (sw, shh) = (d.s.w(), d.s.h());
        if d.sh.begin(x.min(xt), y.min(yt) - 4.0 * u, x.max(xt), y.max(yt) + 4.0 * u, sw, shh) {
            d.sh.poly(&[(x, y - 0.9 * u), (xt, yt - 3.5 * u), (xt, yt + 3.5 * u), (x, y + 0.9 * u)]);
            let mut trail = d.sh.take();
            for c in 0..trail.w {
                let f = (((trail.x + c as i32) as f32 + 0.5 - x).abs() / length).min(1.0);
                let k = (1.0 - f).powf(1.5);
                for r in 0..trail.h {
                    trail.data[r * trail.w + c] *= k;
                }
            }
            trail.paint(d.s, Blend::Over, |_| (col, 0.59 * sc.day));
        }
        d.ellipse(x, y, 2.2 * u, 1.5 * u, col, 0.92);
    } else {
        d.dot(x, y, 1.5 * u, 1.5 * u, hx(sc.p.star), 0.45 + 0.5 * strobe, Blend::Over);
        d.glow(x - way * 5.0 * u, y, 4.0 * u, 4.0 * u, BEACON, 0.9 - 0.4 * strobe, 2, Blend::Screen);
        d.dot(x - way * 5.0 * u, y, 1.2 * u, 1.2 * u, BEACON, 0.9, Blend::Over);
    }
}

/// The face of a public clock, showing the real time. It is lit from behind after dark.
pub fn clock_face(d: &mut Draw<crate::gfx::Canvas>, sc: &Scene, fx: f32, fy: f32, r: f32, u: f32, col: Rgb, bloom: &mut crate::gfx::Bloom) {
    let slate = shade(col, -0.10);
    let unlit = mix(col, CREAM, 0.30 + 0.48 * sc.day);
    let face = mix(unlit, [255.0, 232.0, 184.0], 0.92 * sc.dark);
    d.circle(fx, fy, r + 1.8 * u, slate, 1.0);
    d.circle(fx, fy, r, face, 1.0);
    let ink = mix(shade(col, -0.45), [38.0, 34.0, 30.0], sc.dark);
    for k in 0..12 {
        let a = k as f32 / 12.0 * 2.0 * PI;
        let (inner, w) = if k % 3 == 0 { (0.74, 1.3) } else { (0.84, 0.9) };
        d.line((fx + a.sin() * r * inner, fy - a.cos() * r * inner), (fx + a.sin() * r * 0.93, fy - a.cos() * r * 0.93), w * u, ink, 0.8);
    }
    clock_hands(d, fx, fy, r, sc.t as f32, ink);
    if sc.dark > 0.2 {
        bloom.light(fx - r, fy - r, fx + r, fy + r, [255.0, 226.0, 170.0], 0.55 * sc.dark * sc.p.bloom);
    }
}

/// The hands of the tower clock.
pub fn clock_hands<S: Surface>(d: &mut Draw<S>, cx: f32, cy: f32, r: f32, hours: f32, col: Rgb) {
    let hour = (hours % 12.0) / 12.0 * 2.0 * PI;
    let minute = hours.fract() * 2.0 * PI;
    let w = (r * 0.17).max(1.2);
    d.line((cx, cy), (cx + hour.sin() * r * 0.52, cy - hour.cos() * r * 0.52), w * 1.25, col, 1.0);
    d.line((cx, cy), (cx + minute.sin() * r * 0.82, cy - minute.cos() * r * 0.82), w, col, 1.0);
    d.circle(cx, cy, w * 0.9, col, 1.0);
}

/// A note of music.
pub fn note<S: Surface>(d: &mut Draw<S>, x: f32, y: f32, u: f32, col: Rgb, a: f32, pair: bool) {
    let stem = 9.0 * u;
    d.ellipse(x, y, 2.6 * u, 2.0 * u, col, a);
    d.line((x + 2.2 * u, y), (x + 2.2 * u, y - stem), 1.1 * u, col, a);
    if pair {
        d.ellipse(x + 7.0 * u, y - 1.5 * u, 2.6 * u, 2.0 * u, col, a);
        d.line((x + 9.2 * u, y - 1.5 * u), (x + 9.2 * u, y - stem - 1.5 * u), 1.1 * u, col, a);
        d.line((x + 2.2 * u, y - stem), (x + 9.2 * u, y - stem - 1.5 * u), 2.0 * u, col, a);
    } else {
        d.path(&[(x + 2.2 * u, y - stem), (x + 5.5 * u, y - stem + 2.5 * u), (x + 5.0 * u, y - stem + 5.0 * u)], 1.3 * u, col, a);
    }
}
