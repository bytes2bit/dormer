//! Moon: a station on the Lake of Dreams, with the earth overhead.
//!
//! Nothing here has weather, and a day lasts a month. What changes is real
//! all the same: the sun crosses this sky once for each turn of the moon's
//! phases, so the station has its noon near full moon and its midnight at
//! new; the earth hangs where it always hangs, turning once a day, lit the
//! opposite way to the moon you see from home.
//!
//!   the earth          its true face and phase; cloud over home as the weather reading says
//!   the sun            up for a fortnight, down for a fortnight: long shadows at each end
//!   the cupola         yours: the lamp in it is lit while an agent is running
//!   the cargo yard     a pod lit blue for each running container
//!   the vent           blows while the processor is busy
//!   the garden         its lamps are on through the day at home, off for the night
//!   the lander         comes in on the hour and leaves at half past

use std::cell::RefCell;
use std::f32::consts::PI;

use crate::color::{mix, shade, smooth, Rgb, AMBER, BEACON, BLUE, CREAM, ROSE, SAGE, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, Surface, P};
use crate::paint::{milky_way, starfield, Starlight};
use crate::props::{self, Layout};
use crate::rng::hash2;
use crate::scene::Scene;
use crate::sky::Star;
use crate::views::globe::{self, add, cross, dot, toward, unit, Globe, V3};
use crate::views::kit::{curve, to_px};
use crate::views::{lamp_colours, yours, Anchors, Blink, Breath, Knob, Lights, Sky, Stage, View, Yours};

pub struct Moonbase;

const HORIZON: f64 = 0.640;
const HZ: f32 = 640.0;

/// Lacus Somniorum: degrees north of the moon's equator, and east of the middle of its face.
const SITE_LAT: f32 = 38.0;
const SITE_LON: f32 = 29.2;
/// Days from one new moon to the next.
const MONTH: f32 = 29.53;

/// Where the earth hangs, and how big. It is drawn a little large: the real one is three and a
/// half suns across.
const EARTH: P = (1098.0, 238.0);
const EARTH_R: f32 = 90.0;
const SUN_R: f32 = 21.0;

/// The hills on the horizon: old, round, and never worn by anything.
const FAR: &[P] = &[
    (-40.0, 614.0),
    (60.0, 598.0),
    (150.0, 568.0),
    (240.0, 550.0),
    (330.0, 557.0),
    (410.0, 585.0),
    (480.0, 612.0),
    (560.0, 629.0),
    (650.0, 636.0),
    (760.0, 631.0),
    (850.0, 623.0),
    (930.0, 630.0),
    (1010.0, 636.0),
    (1090.0, 627.0),
    (1170.0, 605.0),
    (1250.0, 573.0),
    (1330.0, 549.0),
    (1410.0, 541.0),
    (1480.0, 553.0),
    (1550.0, 579.0),
    (1640.0, 601.0),
];

/// The bank in front of everything.
const NEAR: &[P] = &[
    (-40.0, 906.0),
    (120.0, 893.0),
    (280.0, 898.0),
    (430.0, 916.0),
    (560.0, 938.0),
    (700.0, 950.0),
    (860.0, 946.0),
    (1000.0, 932.0),
    (1140.0, 908.0),
    (1280.0, 884.0),
    (1420.0, 870.0),
    (1540.0, 876.0),
    (1640.0, 888.0),
];

/// Craters on the plain: x, y, half-width.
const CRATERS: &[(f32, f32, f32)] = &[
    (300.0, 656.0, 28.0),
    (522.0, 663.0, 13.0),
    (706.0, 654.0, 36.0),
    (906.0, 668.0, 17.0),
    (1176.0, 660.0, 46.0),
    (1300.0, 676.0, 12.0),
    (1546.0, 668.0, 20.0),
    (392.0, 694.0, 13.0),
    (176.0, 872.0, 50.0),
    (470.0, 886.0, 17.0),
    (760.0, 898.0, 13.0),
    (994.0, 880.0, 30.0),
    (1250.0, 866.0, 11.0),
    (1560.0, 866.0, 34.0),
];

/// Loose rock: x, y of its foot, half-width, and whether it is turned round.
const ROCKS: &[(f32, f32, f32, bool)] = &[
    (40.0, 772.0, 7.0, false),
    (300.0, 842.0, 6.0, true),
    (330.0, 847.0, 3.4, false),
    (616.0, 872.0, 7.0, false),
    (846.0, 842.0, 4.0, true),
    (1126.0, 868.0, 9.0, false),
    (1154.0, 874.0, 4.4, true),
    (1408.0, 858.0, 5.0, false),
    (1584.0, 786.0, 7.0, true),
    (640.0, 684.0, 3.0, false),
    (1010.0, 690.0, 2.6, true),
    (424.0, 700.0, 3.2, true),
];
/// The big ones on the bank in front. Someone sits on the first.
const BOULDERS: &[(f32, f32, f32, bool)] = &[(150.0, 968.0, 50.0, false), (232.0, 978.0, 20.0, true), (1486.0, 944.0, 28.0, true), (1432.0, 956.0, 11.0, false), (640.0, 990.0, 8.0, false), (1040.0, 978.0, 6.0, true)];
const SEAT: P = (160.0, 921.0);

/// The station is drawn in its own units, this much bigger than they say, with `PLOT.0` put at `PLOT.1`.
const BIG: f32 = 1.42;
const PLOT: (P, P) = ((526.0, 786.0), (372.0, 814.0));
/// Where the buildings stand, in those units.
const GROUND: f32 = 786.0;
const MAST: f32 = 452.0;
const CUPOLA: P = (770.0, 706.0);
const VENT: P = (668.0, 726.0);
const DOME: (f32, f32) = (938.0, 62.0);
/// The landing pad, out on the plain.
const PAD: P = (176.0, 716.0);
/// The cargo pods, in the order they light.
const PODS: [P; 8] = [(1036.0, 786.0), (1088.0, 786.0), (1140.0, 786.0), (1192.0, 786.0), (1036.0, 762.0), (1088.0, 762.0), (1140.0, 762.0), (1192.0, 762.0)];
const SHED: f32 = 1272.0;

const REGOLITH: Rgb = [184.0, 175.0, 167.0];
const HULL: Rgb = [232.0, 228.0, 220.0];
const METAL: Rgb = [118.0, 124.0, 140.0];

/// Where the sun is in the station's sky, and how the earth is lit.
struct Day {
    /// the moon's age as an angle: 0 at new, 180 at full
    elong: f32,
    /// the sun's hour angle at the station, degrees: 0 at lunar noon, 90 either side at the horizon
    hour: f32,
    el: f32,
    /// its place on the picture, in design units
    at: P,
    /// 0 with the sun down, 1 with it clear of the hills
    up: f32,
    /// how much of the earth's face is lit
    earth: f32,
    /// the sun is behind the earth
    eclipse: f32,
}

fn day_of(sc: &Scene) -> Day {
    let elong = sc.knob("lunar").map_or(sc.eph.moon.elong as f32, |d| (d / MONTH * 360.0).rem_euclid(360.0));
    let hour = (elong - 180.0 + SITE_LON + 180.0).rem_euclid(360.0) - 180.0;
    let el = (SITE_LAT.to_radians().cos() * hour.to_radians().cos()).asin().to_degrees();
    // The moon's path is tilted to the sun's, so the sun passes over or under the earth at full
    // moon. Only when the moon is crossing the sun's path does it go behind: an eclipse.
    let beta = sc.eph.moon.beta as f32;
    let near = (-((hour - SITE_LON) / 25.0).powi(2)).exp();
    let lift = beta.signum() * (beta.abs() / 5.15).sqrt() * 170.0 * near;
    let at = (800.0 + hour * 9.955, HZ - 580.0 * el.to_radians().sin() - lift);
    let apart = ((at.0 - EARTH.0).powi(2) + (at.1 - EARTH.1).powi(2)).sqrt();
    Day {
        elong,
        hour,
        el,
        at,
        up: smooth(-1.2, 1.8, el),
        earth: (1.0 + elong.to_radians().cos()) / 2.0,
        eclipse: if el > 0.0 { smooth(EARTH_R + SUN_R, EARTH_R - SUN_R, apart) } else { 0.0 },
    }
}

/// The part of the lunar day, in a word.
fn part(hour: f32) -> &'static str {
    match hour {
        h if h < -90.0 || h > 90.0 => "night",
        h if h < -66.0 => "sunrise",
        h if h < -22.0 => "morning",
        h if h < 22.0 => "noon",
        h if h < 66.0 => "afternoon",
        _ => "sunset",
    }
}

/// The light there is to see by: the sun for a fortnight, then the earth.
struct Lamp {
    /// where it comes from, in pixels across
    x: f32,
    key: [f32; 3],
    amb: [f32; 3],
    /// light mode lifts everything towards paper
    lift: f32,
    /// how brightly level ground is lit, against a slope turned to face the light
    flat: f32,
    /// how far a shadow reaches, as a multiple of what casts it
    reach: f32,
    /// 1 when the sun is low: shadows long, slopes modelled
    low: f32,
    sun: f32,
}

impl Lamp {
    fn of(sc: &Scene, day: &Day, lay: &Layout) -> Lamp {
        let up = day.up * (1.0 - 0.92 * day.eclipse);
        let sin = day.el.max(0.0).to_radians().sin();
        let glow = 0.45 + 0.55 * day.earth;
        let m = |a: [f32; 3], b: [f32; 3], t: f32| [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t];
        let light = sc.mode == "light";
        // a low sun is given a little gold, though nothing in this sky would redden it
        let gold = smooth(34.0, 4.0, day.el);
        let mut key = m([0.21 * glow, 0.28 * glow, 0.46 * glow], [0.96, 0.91 - 0.04 * gold, 0.85 - 0.12 * gold], up);
        let mut amb = m([0.10, 0.13, 0.23], [0.19, 0.20, 0.29], up);
        if day.eclipse > 0.0 {
            // the only light is what comes round the earth through all its sunsets
            key = m(key, [0.42, 0.17, 0.10], day.eclipse * day.up);
        }
        if light {
            amb = [amb[0] + 0.16, amb[1] + 0.17, amb[2] + 0.18];
            key = [key[0] * 0.8, key[1] * 0.8, key[2] * 0.8];
        }
        let by_sun = up > 0.5;
        Lamp {
            x: lay.dx(if by_sun { day.at.0 } else { EARTH.0 }),
            key,
            amb,
            lift: if light { 0.30 } else { 0.0 },
            flat: if by_sun { 0.24 + 0.76 * sin.powf(0.75) } else { 0.62 },
            reach: if by_sun { (1.0 / day.el.max(1.0).to_radians().tan()).clamp(0.5, 7.0) } else { 1.0 },
            low: if by_sun { smooth(30.0, 4.0, day.el) } else { 0.25 },
            sun: up,
        }
    }

    /// A surface of this colour, turned `facing` the light (0 in shadow, 1 square on).
    fn tone(&self, base: Rgb, facing: f32) -> Rgb {
        let f = facing.clamp(0.0, 1.0);
        let c = |i: usize| {
            let v = (base[i] * (self.amb[i] + self.key[i] * f)).min(255.0);
            v + (255.0 - v) * self.lift
        };
        [c(0), c(1), c(2)]
    }

    /// Which side of something at pixel `x` the light falls on: -1 its left, 1 its right.
    fn side(&self, x: f32) -> f32 {
        if self.x < x {
            -1.0
        } else {
            1.0
        }
    }
}

struct Station {
    lay: Layout,
    /// the layout the buildings are drawn in
    st: Layout,
    masks: Vec<(usize, Vec<u8>)>,
    depth: Depth,
    far: Vec<P>,
    /// the earth as it was last painted
    earth: RefCell<Option<globe::Kept>>,
}

impl Station {
    fn new(w: usize, h: usize) -> Station {
        let lay = Layout::new(w, h, HORIZON);
        let mut sh = Shaper::new();
        let far = curve(FAR, 8);
        let near = curve(NEAR, 8);
        let mut masks = Vec::new();
        for (line, top) in [(&far, 530.0), (&near, 860.0)] {
            if sh.begin(0.0, lay.dy(top).max(0.0), lay.w, lay.h, w, h) {
                let mut p = to_px(&lay, line);
                p.extend([(lay.w + 2.0, lay.h + 2.0), (-2.0, lay.h + 2.0)]);
                sh.poly(&p);
                masks.push(sh.mask(w));
            } else {
                masks.push((h, Vec::new()));
            }
        }
        let depth = Depth::stack(&masks, w, h);
        Station { lay, st: lay.scaled(BIG, PLOT.0, PLOT.1), masks, depth, far, earth: RefCell::new(None) }
    }
}

/// Are the crew up? They keep the hours of home.
fn awake(sc: &Scene) -> bool {
    (6.5..23.5).contains(&sc.t)
}

/// Seconds the lander takes to come down, and to be gone again.
const DESCENT: f64 = 80.0;
const ASCENT: f64 = 70.0;

/// What the lander is doing: on its way down (0 to 1), standing on the pad, or on its way up (0 to 1).
enum Flight {
    Down(f32),
    Parked,
    Up(f32),
    Away,
}

fn flight(now: f64) -> Flight {
    let s = now.rem_euclid(3600.0);
    if s < DESCENT {
        Flight::Down((s / DESCENT) as f32)
    } else if s < 1800.0 {
        Flight::Parked
    } else if s < 1800.0 + ASCENT {
        Flight::Up(((s - 1800.0) / ASCENT) as f32)
    } else {
        Flight::Away
    }
}

/// Is the lander standing still for long enough to be part of the still picture?
fn settled(ts: f64) -> bool {
    (DESCENT..1778.0).contains(&ts.rem_euclid(3600.0))
}

/// The rover goes out to the far craters and back on working days.
fn rover_at(sc: &Scene) -> Option<(f32, f32)> {
    if sc.dow < 5 && (8.0..17.75).contains(&sc.t) {
        let f = ((sc.t - 8.0) / 9.75) as f32;
        let out = (PI * f).sin().powf(0.7);
        Some((1440.0 - 1140.0 * out, 838.0 + 10.0 * out))
    } else {
        None
    }
}

/// Is somebody off duty with daylight at home to sit in? Evenings in the week, most of the weekend.
fn resting(sc: &Scene) -> bool {
    if sc.dow >= 5 {
        (10.0..21.5).contains(&sc.t)
    } else {
        (18.25..22.0).contains(&sc.t)
    }
}

fn capsule<S: Surface>(d: &mut Draw<S>, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb) {
    let r = (y1 - y0) / 2.0;
    d.rect(x0 + r, y0, x1 - r, y1, col, 1.0);
    d.ellipse(x0 + r, y0 + r, r, r, col, 1.0);
    d.ellipse(x1 - r, y0 + r, r, r, col, 1.0);
}

/// Half an ellipse, standing on its flat side.
fn arch(cx: f32, base: f32, rx: f32, ry: f32) -> Vec<P> {
    (0..=24).map(|i| PI * i as f32 / 24.0).map(|a| (cx - rx * a.cos(), base - ry * a.sin())).collect()
}

/// The shadow something throws along the ground, away from the light.
fn shadow(d: &mut Draw<Canvas>, lay: &Layout, lamp: &Lamp, x0: f32, x1: f32, y: f32, tall: f32) {
    let side = lamp.side(lay.dx((x0 + x1) / 2.0));
    let len = (tall * lamp.reach * 0.8).min(260.0);
    let from = if side > 0.0 { x0 } else { x1 };
    let to = from - side * len;
    let col = lamp.tone(REGOLITH, 0.0);
    let a = 0.50 + 0.36 * lamp.sun;
    d.poly(&[(lay.dx(from), lay.dy(y - 1.5)), (lay.dx(from - side * len * 0.35), lay.dy(y - 0.6)), (lay.dx(from - side * len * 0.35), lay.dy(y + 4.2)), (lay.dx(from + side * (x1 - x0) * 0.9), lay.dy(y + 4.5))], col, a);
    // it thins out along its length
    for (k0, k1, fade) in [(0.35, 0.6, 0.62), (0.6, 0.82, 0.34), (0.82, 1.0, 0.15)] {
        let (xa, xb) = (from - side * len * k0, from - side * len * k1);
        d.poly(&[(lay.dx(xa), lay.dy(y - 0.6)), (lay.dx(xb), lay.dy(y + 0.2)), (lay.dx(xb), lay.dy(y + 4.0)), (lay.dx(xa), lay.dy(y + 4.2))], col, a * fade);
    }
    let _ = to;
}

impl Station {
    /// A height in the sky, in pixels. On a screen too short for the whole design the sky is
    /// squeezed towards the horizon, so that the earth stays in the picture.
    fn sy(&self, y: f32) -> f32 {
        let lay = &self.lay;
        let squeeze = ((lay.horizon() - 0.03 * lay.h) / lay.d(HZ - 110.0)).min(1.0);
        lay.horizon() - lay.d(HZ - y) * squeeze
    }

    /// The hills along the horizon: the flank turned from the light is dark.
    fn hills(&self, d: &mut Draw<Canvas>, lamp: &Lamp, plain: Rgb) {
        let lay = &self.lay;
        let (top, bottom) = (lay.dy(528.0), lay.dy(HZ) + 1.0);
        let outline = |dx: f32, dy: f32| {
            let mut p: Vec<P> = self.far.iter().map(|p| (lay.dx(p.0) + dx, lay.dy(p.1) + dy)).collect();
            p.extend([(lay.w + 400.0, lay.h), (-400.0, lay.h)]);
            p
        };
        if !d.sh.begin(0.0, top, lay.w, bottom, d.s.w, d.s.h) {
            return;
        }
        d.sh.poly(&outline(0.0, 0.0));
        let body = d.sh.take();
        let dark = lamp.tone(REGOLITH, 0.0);
        let (y0, y1) = (lay.dy(560.0), lay.dy(HZ));
        body.paint(d.s, Blend::Over, |y| (mix(dark, plain, 0.85 * smooth(y0, y1, y).powi(2)), 1.0));
        // the same outline slid towards the light: where the two overlap, the slope faces it
        let reach = lay.d(16.0 + 58.0 * lamp.low);
        let side = lamp.side(lay.dx(800.0));
        if d.sh.begin(0.0, top, lay.w, bottom, d.s.w, d.s.h) {
            d.sh.poly(&outline(side * reach, -reach * 0.16 * (1.0 - lamp.low)));
            let mut lit = d.sh.take();
            lit.clip(&body);
            let high = lamp.tone(REGOLITH, 1.0);
            let (y0, y1) = (lay.dy(548.0), lay.dy(HZ));
            lit.paint(d.s, Blend::Over, |y| (mix(high, plain, smooth(y0, y1, y)), 1.0));
        }
    }

    fn crater(&self, d: &mut Draw<Canvas>, lamp: &Lamp, (x, y, half): (f32, f32, f32)) {
        let lay = &self.lay;
        let ry = half * (0.12 + 0.20 * ((y - HZ) / 280.0).clamp(0.0, 1.0));
        let (cx, cy, ax, ay) = (lay.dx(x), lay.dy(y), lay.d(half), lay.d(ry));
        let side = lamp.side(cx);
        // the rim, standing a little proud of the plain
        d.ellipse(cx, cy - ay * 0.10, ax * 1.10, ay * 1.16, lamp.tone(REGOLITH, (lamp.flat + 0.22).min(1.0)), 0.75);
        d.ellipse(cx, cy, ax, ay, lamp.tone(shade(REGOLITH, -0.08), lamp.flat * 0.9), 1.0);
        let crescent = |sign: f32, thick: f32| -> Vec<P> {
            let arc = |k: f32| (0..=20).map(move |i| -PI / 2.0 + PI * i as f32 / 20.0).map(move |a| (cx + sign * ax * k * a.cos(), cy + ay * a.sin()));
            arc(1.0).chain(arc(1.0 - thick).collect::<Vec<P>>().into_iter().rev()).collect()
        };
        // the wall under the light is in shadow, and the far wall catches it
        d.poly(&crescent(side, 0.34 + 0.50 * lamp.low), lamp.tone(REGOLITH, 0.0), 0.92);
        d.poly(&crescent(-side, 0.20), lamp.tone(REGOLITH, 1.0), 0.50 + 0.3 * lamp.low);
    }

    fn rock(&self, d: &mut Draw<Canvas>, lamp: &Lamp, lay: &Layout, (x, y, s, turned): (f32, f32, f32, bool)) {
        const SHAPE: [P; 6] = [(-1.0, 0.0), (-0.84, -0.56), (-0.30, -0.96), (0.34, -0.80), (0.86, -0.42), (1.0, 0.0)];
        let flip = if turned { -1.0 } else { 1.0 };
        let side = lamp.side(lay.dx(x));
        let at = |p: P| (lay.dx(x + p.0 * s * flip), lay.dy(y + p.1 * s));
        let len = s * (1.0 + 1.5 * lamp.reach).min(9.0);
        d.poly(&[at((-side * 0.7 * flip, -0.10)), (lay.dx(x - side * len), lay.dy(y + 0.5)), at((-side * 0.2 * flip, 0.22))], lamp.tone(REGOLITH, 0.0), 0.45 + 0.4 * lamp.sun);
        let body: Vec<P> = SHAPE.iter().map(|&p| at(p)).collect();
        d.poly(&body, lamp.tone(shade(REGOLITH, -0.10), 0.0), 1.0);
        // the faces turned to the light
        let lit: Vec<P> = SHAPE.iter().filter(|p| p.0 * flip * side > -0.4).map(|&p| at(p)).chain([at((-0.1 * side * flip, -0.04))]).collect();
        if lit.len() > 2 {
            d.poly(&lit, lamp.tone(REGOLITH, 0.95), 1.0);
        }
    }

    /// The panels that follow the sun, in three rows going back.
    fn solar(&self, d: &mut Draw<Canvas>, lamp: &Lamp, day: &Day) {
        let lay = &self.lay;
        let u = lay.u;
        let post = lamp.tone(METAL, 0.4);
        for (row, (y, s)) in [(708.0f32, 0.80f32), (724.0, 1.0), (744.0, 1.25)].into_iter().enumerate() {
            for k in 0..4 {
                let x = 1318.0 + (62.0 + 12.0 * row as f32) * k as f32 - 16.0 * row as f32;
                // they lie flat through the night
                let lean = if day.up > 0.5 { ((day.at.0 - x) / 620.0).clamp(-1.0, 1.0) } else { 0.0 };
                let (hw, t, yc) = (19.0 * s, -lean * 7.0 * s, y - 11.0 * s);
                d.line((lay.dx(x), lay.dy(y)), (lay.dx(x), lay.dy(yc)), (1.4 * u * s).max(0.8), post, 1.0);
                let face = lamp.tone([54.0, 84.0, 152.0], 0.30 + 0.70 * lean.abs().max(0.3) * day.up);
                let quad = [(lay.dx(x - hw), lay.dy(yc + t)), (lay.dx(x + hw), lay.dy(yc - t)), (lay.dx(x + hw), lay.dy(yc - t + 6.5 * s)), (lay.dx(x - hw), lay.dy(yc + t + 6.5 * s))];
                d.poly(&quad, face, 1.0);
                d.line(quad[0], quad[1], (1.1 * u * s).max(0.8), lamp.tone(HULL, 0.9), 0.9);
                for c in 1..4 {
                    let f = c as f32 / 4.0;
                    let (cx, cy) = (lay.dx(x - hw + 2.0 * hw * f), lay.dy(yc + t - 2.0 * t * f));
                    d.line((cx, cy), (cx, cy + lay.d(6.5 * s)), 0.8 * u, lamp.tone([30.0, 44.0, 84.0], 0.2), 0.8);
                }
            }
        }
    }

    /// The mast, the dish on it that looks at the earth, and the light on top.
    fn mast(&self, d: &mut Draw<Canvas>, lamp: &Lamp, out: &mut Anchors) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let metal = lamp.tone(METAL, 0.55);
        let (x, top) = (lay.dx(MAST), lay.dy(612.0));
        shadow(d, lay, lamp, MAST - 4.0, MAST + 4.0, GROUND, 60.0);
        props::crane_mast(d, x, lay.dy(GROUND), top, u * 0.9, metal);
        // the dish: a shallow bowl turned up towards the earth
        let (cx, cy) = (lay.dx(MAST + 4.0), lay.dy(662.0));
        let aim = (self.sy(EARTH.1) - cy).atan2(self.lay.dx(EARTH.0) - cx);
        let (ax, ay) = (aim.cos(), aim.sin());
        let at = |along: f32, across: f32| (cx + lay.d(ax * along - ay * across), cy + lay.d(ay * along + ax * across));
        let bowl: Vec<P> = (0..=16).map(|i| -1.0 + 2.0 * i as f32 / 16.0).map(|k| at(6.0 - 13.0 * (1.0 - k * k), 23.0 * k)).collect();
        d.poly(&bowl, lamp.tone(HULL, 0.75), 1.0);
        let inner: Vec<P> = (0..=16).map(|i| -1.0 + 2.0 * i as f32 / 16.0).map(|k| at(6.0 - 7.0 * (1.0 - k * k), 21.0 * k)).collect();
        d.poly(&inner, lamp.tone(shade(HULL, -0.25), 0.35), 1.0);
        d.line(at(-5.0, 0.0), at(17.0, 0.0), 1.2 * u, metal, 1.0);
        d.circle(at(17.0, 0.0).0, at(17.0, 0.0).1, 2.0 * u, lamp.tone(HULL, 0.9), 1.0);
        let tip = (x, top - 27.0 * u);
        d.circle(tip.0, tip.1, 1.8 * u, BEACON, 0.9);
        out.blink.push(Blink { x: tip.0, y: tip.1, r: 13.0 * u, col: BEACON, every: 2.6, phase: 0.0, behind: None });
    }

    /// A round window: lit while the crew are up, a night light otherwise.
    fn port(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, bloom: &mut Bloom, x: f32, y: f32, r: f32, k: u32) {
        let lay = &self.st;
        let (px, py, pr) = (lay.dx(x), lay.dy(y), lay.d(r));
        d.circle(px, py, pr * 1.45, lamp.tone(METAL, 0.3), 1.0);
        let on = awake(sc) && hash2(k, (sc.t * 2.0) as u32 + sc.day_id as u32) < 0.86;
        let watch = !awake(sc) && k % 5 == 2;
        let dim = 0.55 + 0.45 * (1.0 - lamp.sun);
        if on {
            d.circle(px, py, pr, mix(lamp.tone(shade(HULL, -0.5), 0.2), WARM[k as usize % 4], 0.62 + 0.38 * dim), 1.0);
            bloom.light(px - pr * 1.6, py - pr * 1.6, px + pr * 1.6, py + pr * 1.6, WARM[1], 0.85 * dim * sc.p.bloom.max(0.5));
        } else if watch {
            d.circle(px, py, pr, mix([30.0, 40.0, 62.0], crate::color::SCREEN, 0.75), 1.0);
            bloom.light(px - pr * 1.4, py - pr * 1.4, px + pr * 1.4, py + pr * 1.4, crate::color::SCREEN, 0.5 * dim * sc.p.bloom.max(0.5));
        } else {
            d.circle(px, py, pr, lamp.tone([40.0, 50.0, 78.0], 0.5), 1.0);
        }
    }

    /// The long hut the crew sleep in, half buried, and the vent on its roof.
    fn hut(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, bloom: &mut Bloom) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let side = lamp.side(lay.dx(610.0));
        let (lit, dark) = (lamp.tone(mix(HULL, AMBER, 0.10), 0.9), lamp.tone(HULL, 0.12));
        shadow(d, lay, lamp, 526.0, 700.0, GROUND, 40.0);
        // the hut behind, then the one in front
        capsule(d, lay.dx(574.0), lay.dy(738.0), lay.dx(712.0), lay.dy(770.0), lamp.tone(shade(HULL, -0.08), 0.7));
        // its underside is in its own shadow, and so is the end turned from the light
        capsule(d, lay.dx(526.0), lay.dy(750.0), lay.dx(690.0), lay.dy(GROUND + 2.0), mix(lit, dark, 0.45));
        capsule(d, lay.dx(528.0), lay.dy(750.0), lay.dx(688.0), lay.dy(777.0), lit);
        let end = if side > 0.0 { 526.0 } else { 664.0 };
        capsule(d, lay.dx(end), lay.dy(750.0), lay.dx(end + 26.0), lay.dy(GROUND + 2.0), mix(lit, dark, 0.34));
        for (k, x) in [562.0f32, 602.0, 642.0].into_iter().enumerate() {
            d.rect(lay.dx(x - 14.0) - 0.6 * u, lay.dy(751.0), lay.dx(x - 14.0) + 0.6 * u, lay.dy(GROUND), lamp.tone(METAL, 0.4), 0.55);
            self.port(d, sc, lamp, bloom, x, 765.0, 4.4, k as u32);
        }
        d.rect(lay.dx(540.0), lay.dy(757.0), lay.dx(676.0), lay.dy(759.2), mix(lit, AMBER, 0.55), 0.85);
        // soil heaped against it, to keep the sun and the cold out
        let heap = lamp.tone(REGOLITH, lamp.flat);
        d.poly(&[(lay.dx(506.0), lay.dy(GROUND + 3.0)), (lay.dx(530.0), lay.dy(774.0)), (lay.dx(556.0), lay.dy(GROUND + 3.0))], heap, 1.0);
        d.poly(&[(lay.dx(664.0), lay.dy(GROUND + 3.0)), (lay.dx(696.0), lay.dy(771.0)), (lay.dx(732.0), lay.dy(GROUND + 3.0))], heap, 1.0);
        // the vent
        let (vx, vy) = (lay.dx(VENT.0), lay.dy(VENT.1));
        d.rect(vx - 2.2 * u, vy, vx + 2.2 * u, lay.dy(742.0), lamp.tone(METAL, 0.5), 1.0);
        d.rect(vx - 3.4 * u, vy - 1.6 * u, vx + 3.4 * u, vy + 1.0 * u, lamp.tone(METAL, 0.8), 1.0);
    }

    /// The tower with the cupola on it: yours.
    fn tower(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let (x0, x1, top) = (728.0, 812.0, 720.0);
        let side = lamp.side(lay.dx(770.0));
        let (lit, dark) = (lamp.tone(mix(HULL, ROSE, 0.07), 0.95), lamp.tone(HULL, 0.10));
        shadow(d, lay, lamp, x0, x1, GROUND, 84.0);
        // the radiators: two fins that glow when the machine runs hot
        let hot = smooth(0.30, 0.95, sc.machine.load);
        for fx in [x1 + 5.0, x1 + 13.0] {
            let fin = mix(lamp.tone(METAL, 0.5), [236.0, 108.0, 70.0], 0.80 * hot);
            d.rect(lay.dx(fx), lay.dy(726.0), lay.dx(fx + 4.0), lay.dy(772.0), fin, 1.0);
            if hot > 0.05 {
                bloom.light(lay.dx(fx - 2.0), lay.dy(726.0), lay.dx(fx + 6.0), lay.dy(772.0), [236.0, 108.0, 70.0], 0.6 * hot);
            }
        }
        d.rect(lay.dx(x1 - 2.0), lay.dy(744.0), lay.dx(x1 + 17.0), lay.dy(747.0), lamp.tone(METAL, 0.4), 1.0);
        // the drum and its domed roof
        d.rect(lay.dx(x0), lay.dy(top), lay.dx(x1), lay.dy(GROUND + 2.0), lit, 1.0);
        d.poly(&arch(lay.dx(770.0), lay.dy(top) + 0.5, lay.d(42.0), lay.d(13.0)), lit, 1.0);
        // round things shade off towards the side away from the light
        for (k, a) in [(0.34, 0.26), (0.20, 0.30), (0.09, 0.34)] {
            let wide = (x1 - x0) * k;
            let from = if side > 0.0 { x0 } else { x1 - wide };
            d.rect(lay.dx(from), lay.dy(top), lay.dx(from + wide), lay.dy(GROUND + 2.0), dark, a);
        }
        d.rect(lay.dx(x0), lay.dy(733.0), lay.dx(x1), lay.dy(737.0), mix(lit, ROSE, 0.62), 0.9);
        d.rect(lay.dx(x0), lay.dy(GROUND - 9.0), lay.dx(x1), lay.dy(GROUND - 7.0), lamp.tone(METAL, 0.4), 0.6);
        for (k, x) in [748.0f32, 770.0, 792.0].into_iter().enumerate() {
            self.port(d, sc, lamp, bloom, x, 752.0, 4.0, 10 + k as u32);
        }
        // the hatch, and the steps up to it
        d.rect(lay.dx(762.0), lay.dy(766.0), lay.dx(778.0), lay.dy(GROUND), lamp.tone(METAL, 0.25), 1.0);
        d.rect(lay.dx(764.5), lay.dy(769.0), lay.dx(775.5), lay.dy(774.0), lamp.tone([40.0, 50.0, 78.0], 0.5), 1.0);
        // the cupola
        let (cx, cy, r) = (lay.dx(CUPOLA.0), lay.dy(CUPOLA.1), lay.d(13.0));
        d.rect(cx - r * 1.15, cy - 1.0 * u, cx + r * 1.15, lay.dy(top - 9.0), lamp.tone(METAL, 0.5), 1.0);
        let glass = arch(cx, cy, r, r * 0.95);
        if sc.machine.agents > 0 {
            let (lamp_col, glow, low) = lamp_colours(sc);
            d.poly(&glass, lamp_col, 1.0);
            bloom.light(cx - r * 1.4, cy - r * 1.4, cx + r * 1.4, cy + r * 0.3, glow, (1.2 + 0.7 * (1.0 - lamp.sun)) * sc.p.bloom.max(0.6));
            out.breath.push(Breath { x: cx, y: cy - r * 0.4, rx: lay.d(54.0), ry: lay.d(42.0), col: glow, strength: 0.55 + 0.45 * (1.0 - lamp.sun), low, behind: None });
        } else {
            d.poly(&glass, lamp.tone([52.0, 66.0, 100.0], 0.55), 1.0);
            d.line((cx - r * 0.5, cy - r * 0.62), (cx - r * 0.1, cy - r * 0.86), 1.2 * u, WHITE, 0.25 + 0.3 * lamp.sun);
        }
        for k in [-0.5f32, 0.0, 0.5] {
            d.line((cx + r * k * 1.2, cy), (cx + r * k * 0.7, cy - r * (0.95 - 0.5 * k.abs())), 0.9 * u, lamp.tone(METAL, 0.5), 0.9);
        }
        d.line((lay.dx(798.0), lay.dy(top - 6.0)), (lay.dx(798.0), lay.dy(top - 38.0)), 1.0 * u, lamp.tone(METAL, 0.6), 1.0);
        out.music = Some((cx + lay.d(8.0), cy - r));
        // a flag on a rod, since nothing here will ever lift it
        let (fx, fy) = (lay.dx(706.0), lay.dy(734.0));
        d.line((fx, lay.dy(GROUND)), (fx, fy), 1.2 * u, lamp.tone(METAL, 0.7), 1.0);
        d.rect(fx, fy, fx + lay.d(19.0), fy + lay.d(11.0), lamp.tone(CREAM, 0.9), 1.0);
        d.rect(fx, fy + lay.d(3.7), fx + lay.d(19.0), fy + lay.d(7.3), lamp.tone(ROSE, 0.9), 1.0);
    }

    /// The garden under glass. Its tree keeps the season of home.
    fn garden(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, bloom: &mut Bloom) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let (x, r) = DOME;
        let (cx, base, pr) = (lay.dx(x), lay.dy(GROUND), lay.d(r));
        shadow(d, lay, lamp, x - r, x + r, GROUND, r * 0.8);
        let on = awake(sc);
        let dim = 0.50 + 0.50 * (1.0 - lamp.sun);
        let shell = arch(cx, base, pr, pr * 0.96);
        // the air inside, pink under the growing lamps
        let inside = if on { mix(lamp.tone([70.0, 96.0, 110.0], 0.5), [232.0, 150.0, 190.0], 0.34 + 0.30 * dim) } else { lamp.tone([44.0, 62.0, 84.0], 0.4) };
        d.poly(&shell, inside, 0.92);
        // beds along the floor, a row of shelves, and the tree
        let leaf = crate::color::hex(sc.p.seasons[sc.season][1]);
        let green = if on { mix(leaf, [96.0, 176.0, 110.0], 0.5) } else { lamp.tone(mix(leaf, [70.0, 120.0, 86.0], 0.5), 0.5) };
        for (bx, by, half) in [(x - 34.0, GROUND - 3.0, 15.0), (x + 4.0, GROUND - 3.0, 18.0), (x + 37.0, GROUND - 3.0, 12.0)] {
            d.poly(&arch(lay.dx(bx), lay.dy(by), lay.d(half), lay.d(7.0)), green, 1.0);
        }
        let trunk = lamp.tone([96.0, 78.0, 64.0], 0.5);
        d.line((lay.dx(x - 6.0), lay.dy(GROUND - 4.0)), (lay.dx(x - 5.0), lay.dy(GROUND - 30.0)), 2.2 * u, trunk, 1.0);
        d.line((lay.dx(x - 5.0), lay.dy(GROUND - 22.0)), (lay.dx(x + 5.0), lay.dy(GROUND - 33.0)), 1.4 * u, trunk, 1.0);
        let dress = |c: Rgb| if on { c } else { lamp.tone(c, 0.5) };
        let crown = match sc.season {
            0 => None,
            1 => Some(mix(green, dress([244.0, 190.0, 206.0]), 0.6)),
            3 => Some(mix(green, dress([226.0, 150.0, 70.0]), 0.7)),
            _ => Some(green),
        };
        if let Some(c) = crown {
            for (ox, oy, rr) in [(-12.0, -33.0, 10.0), (-2.0, -40.0, 11.0), (8.0, -34.0, 9.0), (-5.0, -30.0, 9.0)] {
                d.circle(lay.dx(x + ox), lay.dy(GROUND + oy), lay.d(rr), c, 1.0);
            }
        } else {
            for (ox, oy) in [(-16.0, -40.0), (-9.0, -45.0), (2.0, -43.0), (12.0, -40.0)] {
                d.line((lay.dx(x - 5.0), lay.dy(GROUND - 28.0)), (lay.dx(x + ox), lay.dy(GROUND + oy)), 1.0 * u, trunk, 1.0);
            }
        }
        if on {
            for k in 0..3 {
                let lx = lay.dx(x - 26.0 + 26.0 * k as f32);
                d.rect(lx - lay.d(7.0), lay.dy(GROUND - 51.0), lx + lay.d(7.0), lay.dy(GROUND - 49.4), [255.0, 196.0, 226.0], 0.95);
            }
            bloom.light(cx - pr * 0.8, base - pr * 0.8, cx + pr * 0.8, base, [236.0, 140.0, 190.0], 0.42 * dim * sc.p.bloom.max(0.5));
        }
        // the frame: ribs over the top, hoops round it
        let rib = lamp.tone(HULL, 0.8);
        for k in [-0.72f32, -0.36, 0.0, 0.36, 0.72] {
            let line: Vec<P> = (0..=16).map(|i| PI * i as f32 / 32.0).map(|a| (cx + pr * k * a.cos(), base - pr * 0.96 * a.sin())).collect();
            d.path(&line, 1.1 * u, rib, 0.80);
        }
        for f in [0.36f32, 0.68] {
            let half = pr * (1.0 - f * f).sqrt();
            d.line((cx - half, base - pr * 0.96 * f), (cx + half, base - pr * 0.96 * f), 1.0 * u, rib, 0.70);
        }
        d.path(&shell, 1.6 * u, rib, 1.0);
        // where the light catches the glass
        let side = lamp.side(lay.dx(x));
        let shine: Vec<P> = (0..=10).map(|i| 0.25 + 0.5 * i as f32 / 10.0).map(|a| (cx + side * pr * 0.88 * a.cos(), base - pr * 0.86 * a.sin())).collect();
        d.path(&shine, 2.4 * u, WHITE, 0.10 + 0.26 * lamp.sun);
        d.rect(cx - pr * 1.04, base - 3.0 * u, cx + pr * 1.04, base + 2.0 * u, mix(lamp.tone(HULL, 0.7), SAGE, 0.5), 1.0);
    }

    /// The cargo yard: eight pods on a rack, one lit blue for each running container.
    fn yard(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, bloom: &mut Bloom) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        shadow(d, lay, lamp, 1014.0, 1214.0, GROUND, 46.0);
        let frame = lamp.tone(METAL, 0.5);
        for x in [1012.0f32, 1064.0, 1116.0, 1168.0, 1218.0] {
            d.rect(lay.dx(x) - 1.0 * u, lay.dy(736.0), lay.dx(x) + 1.0 * u, lay.dy(GROUND + 1.0), frame, 1.0);
        }
        d.rect(lay.dx(1012.0), lay.dy(762.0), lay.dx(1218.0), lay.dy(764.0), frame, 1.0);
        let side = lamp.side(lay.dx(1116.0));
        for (k, (x, y)) in PODS.into_iter().enumerate() {
            let tint = [ROSE, SAGE, AMBER, BLUE][(k + k / 4) % 4];
            let body = lamp.tone(mix(HULL, tint, 0.30), 0.85);
            capsule(d, lay.dx(x - 21.0), lay.dy(y - 21.0), lay.dx(x + 21.0), lay.dy(y - 1.0), body);
            d.rect(lay.dx(x - 15.0), lay.dy(y - 21.0), lay.dx(x - 9.0), lay.dy(y - 1.0), lamp.tone(tint, 0.85), 1.0);
            let far = if side > 0.0 { x - 21.0 } else { x + 9.0 };
            capsule(d, lay.dx(far), lay.dy(y - 21.0), lay.dx(far + 12.0), lay.dy(y - 1.0), mix(body, lamp.tone(HULL, 0.1), 0.42));
            d.rect(lay.dx(x - 14.0), lay.dy(y - 6.0), lay.dx(x + 14.0), lay.dy(y - 1.0), lamp.tone(HULL, 0.1), 0.30);
            let (lx, ly) = (lay.dx(x + 10.0), lay.dy(y - 12.0));
            if k < sc.machine.containers as usize {
                d.glow(lx, ly, 9.0 * u, 9.0 * u, BLUE, 0.80, 2, Blend::Screen);
                d.rect(lx - 3.2 * u, ly - 2.2 * u, lx + 3.2 * u, ly + 2.2 * u, mix(BLUE, WHITE, 0.35), 1.0);
                bloom.light(lx - 5.0 * u, ly - 4.0 * u, lx + 5.0 * u, ly + 4.0 * u, BLUE, 0.9 * sc.p.bloom.max(0.6));
            } else {
                d.rect(lx - 3.2 * u, ly - 2.2 * u, lx + 3.2 * u, ly + 2.2 * u, lamp.tone([40.0, 50.0, 78.0], 0.4), 1.0);
            }
        }
    }

    /// The pad out on the plain, and the lamps along the way to it.
    fn pad(&self, d: &mut Draw<Canvas>, lamp: &Lamp, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let (px, py) = (lay.dx(PAD.0), lay.dy(PAD.1));
        // a bank thrown up round it, to stop the blast
        d.ellipse(px, py - lay.d(2.0), lay.d(88.0), lay.d(12.0), lamp.tone(REGOLITH, (lamp.flat + 0.2).min(1.0)), 1.0);
        d.ellipse(px, py, lay.d(74.0), lay.d(8.6), lamp.tone(shade(REGOLITH, -0.30), lamp.flat), 1.0);
        d.ellipse(px, py, lay.d(36.0), lay.d(4.0), lamp.tone(HULL, lamp.flat * 0.9), 0.55);
        d.ellipse(px, py, lay.d(29.0), lay.d(3.1), lamp.tone(shade(REGOLITH, -0.30), lamp.flat), 1.0);
        for k in 0..6 {
            let a = PI * (0.08 + 0.84 * k as f32 / 5.0);
            let (lx, ly) = (px - lay.d(74.0) * a.cos(), py + lay.d(8.6) * a.sin());
            d.dot(lx, ly, 1.4 * u, 1.4 * u, AMBER, 0.95, Blend::Over);
            out.blink.push(Blink { x: lx, y: ly, r: 9.0 * u, col: AMBER, every: 3.0, phase: k as f32 / 6.0, behind: Some(1) });
        }
        // lamps on short posts, from the station out to the pad: nearer ones are bigger
        let night = 1.0 - lamp.sun;
        for (k, (x, y)) in [(1010.0f32, 826.0f32), (820.0, 828.0), (640.0, 827.0), (500.0, 824.0), (396.0, 812.0), (330.0, 788.0), (286.0, 762.0), (252.0, 740.0)].into_iter().enumerate() {
            let s = 0.55 + 0.65 * ((y - 730.0) / 96.0).clamp(0.0, 1.0);
            let (lx, ly) = (lay.dx(x), lay.dy(y));
            d.line((lx, ly), (lx, ly - 8.0 * u * s), (1.1 * u * s).max(0.8), lamp.tone(METAL, 0.5), 1.0);
            d.dot(lx, ly - 9.0 * u * s, 1.7 * u * s, 1.7 * u * s, WARM[k % 4], 0.5 + 0.5 * night, Blend::Over);
            d.glow(lx, ly - 9.0 * u * s, 15.0 * u * s, 11.0 * u * s, WARM[1], 0.45 * night, 2, Blend::Screen);
        }
    }

    /// The shed the rover lives in.
    fn shed(&self, d: &mut Draw<Canvas>, sc: &Scene, lamp: &Lamp, out: &mut Anchors) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let x = SHED;
        shadow(d, lay, lamp, x - 36.0, x + 36.0, GROUND, 34.0);
        d.poly(&arch(lay.dx(x), lay.dy(GROUND + 2.0), lay.d(36.0), lay.d(34.0)), lamp.tone(mix(HULL, AMBER, 0.14), 0.85), 1.0);
        let side = lamp.side(lay.dx(x));
        d.poly(&arch(lay.dx(x - side * 9.0), lay.dy(GROUND + 2.0), lay.d(27.0), lay.d(30.0)), lamp.tone(HULL, 0.1), 0.30);
        d.poly(&arch(lay.dx(x), lay.dy(GROUND + 2.0), lay.d(20.0), lay.d(22.0)), lamp.tone([34.0, 40.0, 60.0], 0.4), 1.0);
        d.rect(lay.dx(x - 37.0), lay.dy(GROUND - 2.0), lay.dx(x + 37.0), lay.dy(GROUND + 2.0), lamp.tone(AMBER, 0.8), 1.0);
        if sc.festoon {
            // lights strung out for Friday night
            let bulbs = props::festoon(d, lay.dx(826.0), lay.dx(1010.0), lay.dy(GROUND + 10.0), u * 0.8, lamp.tone(METAL, 0.5));
            out.twinkle.extend(bulbs.into_iter().map(|b| (b.0, b.1, b.2, 1)));
        }
    }

    /// The rover at pixel (`x`, `y`), `s` pixels to one of its own units, and the two who went out with it.
    fn rover<S: Surface>(&self, d: &mut Draw<S>, sc: &Scene, lamp: &Lamp, (x, y, s): (f32, f32, f32), way: f32, crew: bool) {
        let u = self.lay.u * 1.25;
        let at = |a: f32, b: f32| (x + a * way * s, y + b * s);
        let dark = lamp.tone([46.0, 52.0, 70.0], 0.3);
        let away = -lamp.side(x);
        d.poly(&[(x - away * 14.0 * s, y), (x + away * (16.0 + 22.0 * lamp.reach.min(4.0)) * s, y + 1.4 * s), (x - away * 10.0 * s, y + 3.0 * s)], lamp.tone(REGOLITH, 0.0), 0.3 + 0.4 * lamp.sun);
        for wx in [-12.0, 0.0, 12.0] {
            let (cx, cy) = at(wx, -3.2);
            d.circle(cx, cy, 3.4 * s, dark, 1.0);
            d.circle(cx, cy, 1.2 * s, lamp.tone(METAL, 0.8), 1.0);
        }
        let hull = lamp.tone(HULL, 0.9);
        d.poly(&[at(-16.0, -6.0), at(-15.0, -15.0), at(4.0, -17.0), at(13.0, -13.0), at(17.0, -6.0)], hull, 1.0);
        d.poly(&[at(1.0, -15.4), at(4.6, -15.6), at(11.6, -12.4), at(13.0, -9.0), at(1.0, -9.0)], mix(lamp.tone([52.0, 66.0, 100.0], 0.6), WARM[1], if awake(sc) { 0.45 } else { 0.0 }), 1.0);
        d.line(at(-15.0, -10.5), at(0.0, -10.5), 1.4 * u, lamp.tone(AMBER, 0.9), 1.0);
        d.line(at(-10.0, -16.0), at(-10.0, -24.0), 0.9 * u, lamp.tone(METAL, 0.7), 1.0);
        let night = 1.0 - lamp.sun;
        if night > 0.2 {
            let (hx, hy) = at(17.0, -8.0);
            d.poly_mode(&[(hx, hy - 1.0 * u), at(70.0, 0.0), at(34.0, 4.0)], [255.0, 240.0, 204.0], 0.20 * night, Blend::Screen);
            d.dot(hx, hy, 1.6 * u, 1.6 * u, [255.0, 244.0, 214.0], night, Blend::Over);
        }
        if crew {
            for (k, ox) in [-36.0f32, 32.0].into_iter().enumerate() {
                let (fx, fy) = at(ox, 2.0);
                self.figure(d, lamp, fx, fy, s * 0.9, way, [ROSE, BLUE][k], false);
            }
        }
    }

    /// Someone in a suit, standing, or sitting with their legs over the edge of something.
    fn figure<S: Surface>(&self, d: &mut Draw<S>, lamp: &Lamp, x: f32, y: f32, s: f32, way: f32, band: Rgb, sitting: bool) {
        let suit = lamp.tone(HULL, 0.9);
        let pack = lamp.tone(METAL, 0.6);
        let top = if sitting { y } else { y - 5.0 * s };
        if sitting {
            // thighs along the seat, shins hanging
            d.rect(x - 1.0 * s, y - 2.6 * s, x + way * 5.0 * s, y, suit, 1.0);
            d.rect(x + way * 3.2 * s, y - 1.0 * s, x + way * 5.2 * s, y + 5.0 * s, suit, 1.0);
        } else {
            d.rect(x - 2.6 * s, y - 5.4 * s, x - 0.5 * s, y, suit, 1.0);
            d.rect(x + 0.5 * s, y - 5.4 * s, x + 2.6 * s, y, suit, 1.0);
        }
        d.rect(x - 2.8 * s, top - 9.0 * s, x + 2.8 * s, top, suit, 1.0);
        d.rect(x - way * 5.4 * s, top - 9.4 * s, x - way * 2.6 * s, top - 2.0 * s, pack, 1.0);
        d.circle(x, top - 12.2 * s, 3.7 * s, suit, 1.0);
        d.circle(x + way * 1.1 * s, top - 12.2 * s, 2.3 * s, mix(lamp.tone([60.0, 70.0, 90.0], 0.5), AMBER, 0.6), 1.0);
        d.rect(x - 2.8 * s, top - 6.0 * s, x + 2.8 * s, top - 4.6 * s, lamp.tone(band, 0.9), 1.0);
    }

    /// The lander: `height` above the pad in design units, and how hard its engine is burning.
    fn lander<S: Surface>(&self, d: &mut Draw<S>, lamp: &Lamp, height: f32, burn: f32, t: f32) {
        let lay = &self.lay;
        let u = lay.u;
        let (x, y) = (lay.dx(PAD.0), lay.dy(PAD.1 - height));
        let s = lay.d(1.0);
        if burn > 0.02 {
            let flick = 0.85 + 0.15 * (t * 31.0).sin();
            let len = (26.0 + 30.0 * burn) * flick * s;
            d.glow(x, y + 2.0 * s, 34.0 * s * burn.sqrt(), 34.0 * s * burn.sqrt(), [255.0, 214.0, 160.0], 0.50 * burn, 2, Blend::Screen);
            d.poly_mode(&[(x - 3.4 * s, y - 2.0 * s), (x + 3.4 * s, y - 2.0 * s), (x, y - 2.0 * s + len)], [255.0, 232.0, 190.0], 0.85 * burn, Blend::Screen);
            d.poly_mode(&[(x - 1.6 * s, y - 2.0 * s), (x + 1.6 * s, y - 2.0 * s), (x, y - 2.0 * s + len * 0.6)], WHITE, 0.9 * burn, Blend::Screen);
            // close to the ground it throws the dust out flat, and with no air the dust just flies
            if height < 46.0 {
                let near = 1.0 - height / 46.0;
                let gy = lay.dy(PAD.1);
                for i in 0..16u32 {
                    let f = (t * (0.9 + 0.5 * hash2(i, 5)) + hash2(i, 9)).fract();
                    let way = if i % 2 == 0 { 1.0 } else { -1.0 };
                    let gx = x + way * (8.0 + 82.0 * f) * s;
                    d.dot(gx, gy - (4.0 * f * (1.0 - f)) * 16.0 * s * hash2(i, 13), 1.2 * u, 1.2 * u, lamp.tone(REGOLITH, 1.0), 0.7 * near * (1.0 - f), Blend::Over);
                }
            }
        }
        let (hull, dark) = (lamp.tone(HULL, 0.9), lamp.tone(METAL, 0.3));
        for way in [-1.0f32, 1.0] {
            d.line((x + way * 5.0 * s, y - 6.0 * s), (x + way * 12.0 * s, y), 1.2 * u, dark, 1.0);
            d.line((x + way * 9.5 * s, y), (x + way * 14.5 * s, y), 1.4 * u, dark, 1.0);
        }
        d.poly(&[(x - 3.0 * s, y - 6.0 * s), (x + 3.0 * s, y - 6.0 * s), (x + 4.4 * s, y - 2.0 * s), (x - 4.4 * s, y - 2.0 * s)], dark, 1.0);
        d.poly(&[(x - 9.0 * s, y - 6.0 * s), (x - 9.0 * s, y - 14.0 * s), (x - 5.0 * s, y - 21.0 * s), (x + 5.0 * s, y - 21.0 * s), (x + 9.0 * s, y - 14.0 * s), (x + 9.0 * s, y - 6.0 * s)], hull, 1.0);
        let side = lamp.side(x);
        d.poly(&[(x - side * 9.0 * s, y - 6.0 * s), (x - side * 9.0 * s, y - 14.0 * s), (x - side * 5.0 * s, y - 21.0 * s), (x - side * 1.0 * s, y - 21.0 * s), (x - side * 3.0 * s, y - 6.0 * s)], lamp.tone(HULL, 0.1), 0.40);
        d.rect(x - 9.0 * s, y - 11.6 * s, x + 9.0 * s, y - 9.8 * s, mix(hull, AMBER, 0.75), 1.0);
        d.rect(x - 2.6 * s, y - 18.6 * s, x + 2.6 * s, y - 15.0 * s, lamp.tone([52.0, 66.0, 100.0], 0.6), 1.0);
    }

    /// Where the lander is on its way down or up: how high above the pad, and how hard it burns.
    fn course(f: f32, down: bool) -> (f32, f32) {
        if down {
            // falling fast, then slower and slower
            let k = (1.0 - f).powf(2.2);
            (760.0 * k, 0.35 + 0.65 * smooth(0.15, 0.9, f))
        } else {
            (760.0 * f.powf(2.4), 1.0 - 0.5 * f)
        }
    }

    /// The sun, a hard disc in a black sky.
    fn sun(&self, d: &mut Draw<Canvas>, day: &Day) {
        if day.el < -1.5 {
            return;
        }
        let lay = &self.lay;
        let (x, y, r) = (lay.dx(day.at.0), self.sy(day.at.1), lay.d(SUN_R));
        let col = [255.0, 246.0, 226.0];
        d.glow(x, y, r * 9.0, r * 9.0, col, 0.20, 2, Blend::Screen);
        d.glow(x, y, r * 3.2, r * 3.2, col, 0.48, 2, Blend::Screen);
        // what a lens makes of something that bright
        for (dx, dy, reach) in [(1.0, 0.0, 7.5), (0.0, 1.0, 5.5), (0.71, 0.71, 3.4), (0.71, -0.71, 3.4)] {
            d.path_mode(&[(x - dx * r * reach, y - dy * r * reach), (x + dx * r * reach, y + dy * r * reach)], 1.2 * lay.u, col, 0.13, Blend::Screen);
        }
        d.circle(x, y, r, WHITE, 1.0);
    }
}

/// From where the earth is seen, and how it is lit.
fn globe_of(sc: &Scene, day: &Day) -> Globe {
    let (m, s) = (&sc.eph.moon.body, &sc.eph.sun);
    let out = toward(m.ra, m.dec);
    // the pole of the sun's path: the moon stands very nearly upright on it
    let tilt = 23.44f32.to_radians();
    let pole: V3 = [0.0, -tilt.sin(), tilt.cos()];
    let mut sun = toward(s.ra, s.dec);
    if sc.knob("lunar").is_some() {
        // the dev panel has moved the month on: swing the sun round to where that puts it
        let by = (sc.eph.moon.elong as f32 - day.elong).to_radians();
        let (c, sn) = (by.cos(), by.sin());
        let k = dot(pole, sun) * (1.0 - c);
        let x = cross(pole, sun);
        sun = unit([sun[0] * c + x[0] * sn + pole[0] * k, sun[1] * c + x[1] * sn + pole[1] * k, sun[2] * c + x[2] * sn + pole[2] * k]);
    }
    // from this far east and north on the moon, the earth's north leans over to the right
    let north = unit(add(pole, out, -dot(pole, out)));
    let east = cross(north, out);
    let lean = 32.0f32.to_radians();
    let up = unit(add([north[0] * lean.cos(), north[1] * lean.cos(), north[2] * lean.cos()], east, -lean.sin()));
    let right = cross(up, out);
    Globe {
        out,
        right,
        up,
        sun,
        turn: (sc.eph.lst - sc.place.lon.to_radians()).rem_euclid(std::f64::consts::TAU) as f32,
        days: (sc.ts / 86400.0).rem_euclid(4096.0) as f32,
        home: (sc.place.lon as f32, sc.place.lat as f32, sc.cover.max(sc.rain).max(sc.snow).max(if sc.storm { 1.0 } else { 0.0 })),
        eclipse: day.eclipse,
        // on paper the night side is slate, not black
        dark: if sc.mode == "light" { [62.0, 70.0, 110.0] } else { [9.0, 13.0, 28.0] },
    }
}

impl Stage for Station {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        Lights { share: if awake(sc) { 0.12 } else { 0.02 }, on: Vec::new() }
    }

    fn heavens(&self, d: &mut Draw<Canvas>, sc: &Scene, stars: &[Star], out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let day = day_of(sc);
        // no air: the sky is dark at noon. It keeps the colour of the bar along the top.
        let (low, mid) = if sc.mode == "light" { ([92.0, 100.0, 142.0], [118.0, 126.0, 166.0]) } else { ([9.0, 11.0, 23.0], [12.0, 15.0, 29.0]) };
        let hy = lay.horizon().max(1.0);
        let rows: Vec<Rgb> = (0..d.s.h).map(|y| y as f32 / hy).map(|f| mix(mix(sc.sky[0], mid, smooth(0.0, 0.34, f)), low, smooth(0.30, 1.0, f))).collect();
        d.s.rows(&rows);
        // The stars turn once in a month here, and never twinkle. The glare of the lit ground hides the faint ones.
        let mut eph = sc.eph.clone();
        eph.lat = (SITE_LAT as f64).to_radians();
        eph.lst = (sc.ts / (27.3217 * 86400.0)).fract() * std::f64::consts::TAU;
        let glare = day.up * (1.0 - day.eclipse);
        milky_way(d.s, sc, lay, &eph, 1.0 - 0.72 * glare);
        starfield(d, sc, lay, stars, &Starlight { eph: &eph, lim: 5.9 - 2.1 * glare, clear: 1.0, air: false }, out);
        self.sun(d, &day);
        let g = globe_of(sc, &day);
        // it is worked out afresh every two minutes, or when the month, the eclipse or the weather at home moves on
        let key = ((sc.ts / 120.0) as u64) << 20 ^ ((day.elong * 8.0) as u64) << 8 ^ ((g.home.2 * 15.0) as u64) << 4 ^ (g.eclipse * 15.0) as u64 ^ (g.dark[0] as u64) << 52;
        let seen = globe::paint(d, &g, (lay.dx(EARTH.0), self.sy(EARTH.1)), lay.d(EARTH_R), u, &mut self.earth.borrow_mut(), key);
        if let Some((p, _)) = seen.home {
            // home: a ring round the spot
            let ring: Vec<P> = (0..=20).map(|i| i as f32 / 20.0 * 2.0 * PI).map(|a| (p.0 + a.cos() * 4.2 * u, p.1 + a.sin() * 4.2 * u)).collect();
            d.path(&ring, 1.1 * u, CREAM, 0.85);
        }
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, _lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let w = lay.w as usize;
        let day = day_of(sc);
        let lamp = Lamp::of(sc, &day, &self.lay);

        // ── the plain, and the hills beyond it ──
        // level ground is brightest far off, where it is seen at a glancing angle
        let (far, near) = (lamp.tone(REGOLITH, (lamp.flat + 0.14).min(1.0)), lamp.tone(shade(REGOLITH, -0.05), lamp.flat));
        let (y0, y1) = (lay.dy(HZ), lay.dy(900.0));
        d.s.stamp(&self.masks[0].1, self.masks[0].0 as i32, 0, |y| (mix(far, near, smooth(y0, y1, y as f32)), 1.0));
        bloom.hide(&self.masks[0].1, w, self.masks[0].0);
        self.hills(d, &lamp, far);
        // old lava, a shade darker, in broad patches; and small stones everywhere
        let lava = lamp.tone(shade(REGOLITH, -0.22), lamp.flat);
        for (x, y, rx, ry) in [(260.0f32, 676.0f32, 250.0f32, 16.0f32), (1010.0, 690.0, 330.0, 22.0), (620.0, 852.0, 420.0, 30.0), (1420.0, 780.0, 260.0, 26.0), (60.0, 800.0, 200.0, 24.0)] {
            d.glow(lay.dx(x), lay.dy(y), lay.d(rx), lay.d(ry), lava, 0.34, 1, Blend::Over);
        }
        let (stone, lee) = (lamp.tone(REGOLITH, 1.0), lamp.tone(REGOLITH, 0.0));
        for i in 0..170u32 {
            let f = hash2(i, 601).powf(1.6);
            let (x, y) = (lay.w * hash2(i, 607), lay.dy(HZ + 8.0 + 250.0 * f));
            let r = (0.7 + 1.5 * f) * u;
            let away = -lamp.side(x);
            d.dot(x + away * r * 1.2, y + r * 0.3, r * 1.3, r * 0.5, lee, 0.55, Blend::Over);
            d.dot(x, y, r, r * 0.7, stone, 0.50, Blend::Over);
        }
        for &c in CRATERS {
            self.crater(d, &lamp, c);
        }
        // the tracks the rover has worn, out to the far craters and back
        let rut = lamp.tone(shade(REGOLITH, -0.22), lamp.flat * 0.8);
        for off in [0.0f32, 7.0] {
            let way = curve(&[(1500.0, 834.0 + off), (1200.0, 846.0 + off), (860.0, 852.0 + off), (560.0, 852.0 + off), (300.0, 848.0 + off)], 6);
            d.path(&to_px(lay, &way), 1.5 * u, rut, 0.55);
        }
        for &r in ROCKS {
            self.rock(d, &lamp, lay, r);
        }

        // ── the station ──
        self.pad(d, &lamp, out);
        if settled(sc.ts) {
            self.lander(d, &lamp, 0.0, 0.0, 0.0);
        }
        self.solar(d, &lamp, &day);
        self.mast(d, &lamp, out);
        // the tubes that join everything up
        let st = &self.st;
        let tube = lamp.tone(shade(HULL, -0.12), 0.6);
        for (x0, x1) in [(690.0f32, 730.0f32), (812.0, 880.0), (996.0, 1014.0), (1216.0, 1240.0)] {
            d.rect(st.dx(x0), st.dy(770.0), st.dx(x1), st.dy(781.0), tube, 1.0);
            d.rect(st.dx(x0), st.dy(777.0), st.dx(x1), st.dy(781.0), lamp.tone(HULL, 0.1), 0.35);
        }
        self.hut(d, sc, &lamp, bloom);
        self.tower(d, sc, &lamp, bloom, out);
        self.garden(d, sc, &lamp, bloom);
        self.yard(d, sc, &lamp, bloom);
        self.shed(d, sc, &lamp, out);
        let size = lay.d(BIG);
        match rover_at(sc) {
            Some((x, y)) => self.rover(d, sc, &lamp, (lay.dx(x), lay.dy(y), size), if sc.t < 12.9 { -1.0 } else { 1.0 }, (10.5..15.2).contains(&sc.t)),
            None => self.rover(d, sc, &lamp, (lay.dx(1520.0), lay.dy(826.0), size), -1.0, false),
        }
        if sc.machine.load > 0.35 && !sc.live {
            self.vent(d, sc, &lamp, (sc.ts % 60.0) as f32);
        }
        if sc.machine.audio && !sc.live {
            let ink = mix(crate::color::hex(sc.p.ink), AMBER, 0.4);
            let (cx, cy) = (st.dx(CUPOLA.0), st.dy(CUPOLA.1));
            props::note(d, cx + 26.0 * u, cy - 26.0 * u, u, ink, 0.8, false);
            props::note(d, cx + 10.0 * u, cy - 46.0 * u, u, ink, 0.55, true);
        }

        // ── the bank in front ──
        let bank = lamp.tone(shade(REGOLITH, -0.16), lamp.flat * 0.85);
        let deep = lamp.tone(shade(REGOLITH, -0.30), lamp.flat * 0.6);
        let (b0, b1) = (lay.dy(880.0), lay.dy(1000.0));
        d.s.stamp(&self.masks[1].1, self.masks[1].0 as i32, 0, |y| (mix(bank, deep, smooth(b0, b1, y as f32)), 1.0));
        bloom.hide(&self.masks[1].1, w, self.masks[1].0);
        // its edge catches the light
        let edge = to_px(lay, &curve(NEAR, 8));
        d.path(&edge, 1.2 * u, lamp.tone(REGOLITH, 1.0), 0.10 + 0.22 * lamp.low);
        for &r in BOULDERS {
            self.rock(d, &lamp, lay, r);
        }
        // bootprints, from the edge of the picture up towards the station
        let print = lamp.tone(shade(REGOLITH, -0.36), lamp.flat * 0.5);
        for k in 0..9 {
            let f = k as f32 / 8.0;
            let (x, y) = (810.0 + 46.0 * f + if k % 2 == 0 { 5.0 } else { -5.0 }, 996.0 - 44.0 * f);
            d.ellipse(lay.dx(x), lay.dy(y), lay.d(3.6 - 1.2 * f), lay.d(1.6 - 0.5 * f), print, 0.8);
        }
        if resting(sc) {
            // off duty: someone has come out to sit on the big rock and look at home
            self.figure(d, &lamp, lay.dx(SEAT.0), lay.dy(SEAT.1), lay.d(2.5), 1.0, SAGE, true);
        }
    }

    fn targets(&self, sc: &Scene, _a: &Anchors) -> Vec<crate::interaction::Target> {
        use crate::interaction::{Action, Target};
        vec![
            Target::new(100, [280.0, 850.0, 1350.0, 975.0], (650.0, 935.0), Action::Dust, mix(sc.layers[4], REGOLITH, 0.45), None),
            Target::near(101, self.lay.back(self.st.dx(CUPOLA.0), self.st.dy(CUPOLA.1)), 38.0, Action::Light, WARM[1], None),
        ]
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let day = day_of(sc);
        let lamp = Lamp::of(sc, &day, &self.lay);
        if sc.machine.load > 0.35 {
            self.vent(d, sc, &lamp, t);
        }
        d.s.behind = None;
        match flight(now) {
            Flight::Down(f) => {
                let (height, burn) = Station::course(f, true);
                self.lander(d, &lamp, height, burn, t);
            }
            Flight::Up(f) => {
                let (height, burn) = Station::course(f, false);
                self.lander(d, &lamp, height, burn, t);
            }
            // it is down, but the still picture was painted before it landed, or will outlast its leaving
            Flight::Parked if !settled(sc.ts) => self.lander(d, &lamp, 0.0, 0.0, t),
            _ => {}
        }
    }

    fn lively(&self, sc: &Scene, now: f64) -> bool {
        sc.machine.load > 0.35 || matches!(flight(now), Flight::Down(_) | Flight::Up(_))
    }
}

impl Station {
    /// What the vent lets go: with no air to hang in, it goes up in a fan and falls back as frost.
    fn vent<S: Surface>(&self, d: &mut Draw<S>, sc: &Scene, lamp: &Lamp, t: f32) {
        let lay = &self.st;
        let u = lay.u * 1.25;
        let (x, y) = (lay.dx(VENT.0), lay.dy(VENT.1) - 2.0 * u);
        let push = 0.5 + 0.5 * sc.machine.load;
        let col = lamp.tone([236.0, 242.0, 255.0], 1.0);
        let n = (14.0 + 26.0 * sc.machine.load) as u32;
        for i in 0..n {
            let f = (t * (0.34 + 0.22 * hash2(i, 3)) + hash2(i, 7)).fract();
            let away = (hash2(i, 11) - 0.5) * 2.0;
            let (px, py) = (x + away * lay.d(26.0) * f * push, y - lay.d(96.0) * push * (1.0 - away * away * 0.35) * 4.0 * f * (1.0 - f * 0.86));
            let a = (1.0 - f).powf(0.8) * (0.45 + 0.45 * hash2(i, 17));
            d.dot(px, py, (0.9 + 0.8 * hash2(i, 19)) * u, (0.9 + 0.8 * hash2(i, 19)) * u, col, a, Blend::Over);
        }
    }
}

fn show_lunar(v: f64) -> String {
    let hour = ((v as f32 / MONTH * 360.0) - 180.0 + SITE_LON + 180.0).rem_euclid(360.0) - 180.0;
    format!("day {:>4.1}  {}", v, part(hour))
}

const KNOBS: &[Knob] = &[Knob { key: "lunar", label: "lunar day", lo: 0.0, hi: 29.5, step: 0.25, big: 2.0, show: show_lunar }];

/// What is turned towards the moon, by the longitude in the middle of the earth's disc.
fn facing(lon: f32) -> &'static str {
    match lon {
        l if l < -140.0 => "the Pacific",
        l if l < -52.0 => "the Americas",
        l if l < -22.0 => "the Atlantic",
        l if l < 56.0 => "Africa and Europe",
        l if l < 152.0 => "Asia",
        _ => "the Pacific",
    }
}

impl View for Moonbase {
    fn id(&self) -> &'static str {
        "moon"
    }

    fn about(&self) -> &'static str {
        "a station on the Lake of Dreams, the earth overhead"
    }

    fn horizon(&self) -> f64 {
        HORIZON
    }

    fn sky(&self) -> Sky {
        Sky::Void
    }

    fn knobs(&self) -> &'static [Knob] {
        KNOBS
    }

    fn real(&self, key: &str, sc: &Scene) -> f64 {
        match key {
            "lunar" => sc.eph.moon.elong / 360.0 * MONTH as f64,
            _ => 0.0,
        }
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Station::new(w, h))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let day = day_of(sc);
        let per_day = 360.0 / MONTH;
        let sun = if day.eclipse > 0.5 {
            "behind the earth: an eclipse, the ground lit red".to_string()
        } else if day.el > 0.0 {
            format!("lunar {}, the sun {}° up, {:.1} days to sunset", part(day.hour), day.el.round(), (90.0 - day.hour) / per_day)
        } else {
            format!("lunar night, {:.1} days to sunrise; the ground lit by the earth", (-90.0 - day.hour).rem_euclid(360.0) / per_day)
        };
        // the same sums as the painter, without the paint
        let g = globe_of(sc, &day);
        let middle = ((g.out[1].atan2(g.out[0]) - g.turn).to_degrees() + 180.0).rem_euclid(360.0) - 180.0;
        let at_home = {
            let (ra, dec) = ((sc.place.lon as f32).to_radians() + g.turn, (sc.place.lat as f32).to_radians());
            let n = [dec.cos() * ra.cos(), dec.cos() * ra.sin(), dec.sin()];
            if dot(n, g.out) > 0.06 {
                format!("home in sight, in the {}", if dot(n, g.sun) > 0.0 { "daylight" } else { "dark" })
            } else {
                "home round the far side".to_string()
            }
        };
        let earth = format!("{}% lit, {} turned this way, {}", (day.earth * 100.0).round(), facing(middle), at_home);
        let mut base = vec![if awake(sc) { "the crew up, the garden lamps on" } else { "the crew asleep, one on watch" }.to_string()];
        for (flag, text) in [
            (matches!(flight(sc.ts), Flight::Down(_)), "the lander coming in"),
            (matches!(flight(sc.ts), Flight::Parked), "the lander on the pad until half past"),
            (matches!(flight(sc.ts), Flight::Up(_)), "the lander leaving"),
            (rover_at(sc).is_some(), "the rover out"),
            (sc.festoon, "lights strung out for the night off"),
        ] {
            if flag {
                base.push(text.to_string());
            }
        }
        let house = Yours { lamp: "the lamp in the cupola", containers: ("cargo pod lit blue", "cargo pods lit blue"), smoke: "the vent blowing" };
        vec![("sun", sun), ("earth", earth), ("base", base.join(", ")), ("yours", yours(sc, &house))]
    }
}
