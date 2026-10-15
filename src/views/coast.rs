//! Coast: a headland with a lighthouse, looking out over the strait.
//!
//! The land is on the left, so the sun goes down into the sea and the water
//! carries its light all the way to the shore. The sea is a mirror of
//! whatever sky was painted above it, broken up by the wind.
//!
//!   the keeper's cottage     yours: the lamp in its window, the stovepipe on its lean-to
//!   boats at the jetty       a blue lantern lit for each running container
//!   the tide                 follows the moon: skerries and a sandbar show at low water
//!   the ferry                leaves on the hour and takes twelve minutes to cross
//!   small boats out          fishing before sunrise on working days, sails at the weekend
//!   a fire on the dune       Friday and Saturday nights

use std::f32::consts::{PI, TAU};

use crate::color::{hex, mix, shade, smooth, Rgb, AMBER, BLUE, CREAM, ROSE, SAGE, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, Surface, P};
use crate::paint::{band, Light};
use crate::props::{self, Layout};
use crate::rng::hash2;
use crate::scene::Scene;
use crate::views::kit::{curve, grass, height_at, to_px};
use crate::story::draw::Tone;
use crate::story::{Damage, Snapshot};
use crate::views::episodes::coast::{self as episode, CoastBindings};
use crate::views::{lamp_colours, yours, Anchors, Blink, Breath, Knob, Lights, Smoke, Stage, View, Yours};
use std::cell::Cell;

pub struct Coast;

/// The sea meets the sky here, as a fraction of the height.
const HORIZON: f64 = 0.700;
const SEA: f32 = 700.0;

/// The far shore, across the water: x, and how high it stands above the sea. It is lowest where
/// the sun goes down.
const FAR: &[P] = &[
    (400.0, 0.0),
    (500.0, 12.0),
    (630.0, 30.0),
    (750.0, 20.0),
    (860.0, 36.0),
    (975.0, 46.0),
    (1085.0, 30.0),
    (1190.0, 17.0),
    (1300.0, 9.0),
    (1380.0, 5.0),
    (1470.0, 10.0),
    (1560.0, 15.0),
    (1660.0, 8.0),
];

/// A long low cape behind the headland, running out into the sea.
const CAPE: &[P] = &[(-190.0, 626.0), (-20.0, 622.0), (110.0, 612.0), (250.0, 628.0), (380.0, 650.0), (500.0, 667.0), (620.0, 684.0), (705.0, 694.0), (770.0, 700.0)];

/// The headland: along its top from the left edge to the point, then back along the foot of the
/// cliff, where it meets the water.
const TOP: &[P] = &[
    (-190.0, 532.0),
    (-20.0, 524.0),
    (90.0, 514.0),
    (200.0, 517.0),
    (300.0, 521.0),
    (372.0, 528.0),
    (430.0, 543.0),
    (462.0, 566.0),
    (478.0, 598.0),
    (500.0, 618.0),
    (512.0, 648.0),
    (540.0, 670.0),
    (566.0, 688.0),
    (604.0, 699.0),
    (640.0, 705.0),
];
const FOOT: &[P] = &[(640.0, 705.0), (598.0, 722.0), (560.0, 752.0), (522.0, 790.0), (486.0, 834.0), (440.0, 884.0), (386.0, 940.0), (330.0, 1010.0)];

/// The dune in the corner of the picture, where the grass is.
const DUNE: &[P] = &[(820.0, 1012.0), (930.0, 988.0), (1040.0, 962.0), (1160.0, 944.0), (1290.0, 926.0), (1420.0, 912.0), (1530.0, 896.0), (1640.0, 884.0)];

/// The bank under the window on the left, dark and close, with grass on it.
const BLUFF: &[P] = &[(-20.0, 778.0), (80.0, 802.0), (190.0, 846.0), (290.0, 900.0), (380.0, 954.0), (455.0, 1014.0)];

const LIGHT_X: f32 = 318.0;
const LIGHT_FOOT: f32 = 522.0;
const LAMP_Y: f32 = 372.0;
const JETTY: (f32, f32, f32) = (508.0, 706.0, 802.0);
/// Where the boats lie: x, y of the waterline, and whether there is a cabin on it.
const BOATS: [(f32, f32, bool); 8] =
    [(566.0, 811.0, false), (622.0, 811.0, true), (676.0, 811.0, false), (752.0, 800.0, false), (806.0, 822.0, true), (742.0, 846.0, false), (850.0, 792.0, false), (902.0, 834.0, true)];
/// Rocks off the point: x, width, how high they stand at mean tide, y of the water there.
const SKERRIES: [(f32, f32, f32, f32); 5] = [(690.0, 44.0, 9.0, 712.0), (742.0, 30.0, 6.0, 716.0), (668.0, 26.0, 4.0, 726.0), (790.0, 22.0, 3.5, 709.0), (612.0, 34.0, 7.0, 742.0)];
const FIRE: P = (1392.0, 913.0);

/// The headland, and everything on it and moored under it, is drawn this far to the right of
/// where its numbers say: the light then stands a third of the way across the picture.
const LAND: f32 = 150.0;

/// The coast laid out for one screen.
struct Shore {
    lay: Layout,
    /// the layout the headland is drawn in
    land: Layout,
    masks: Vec<(usize, Vec<u8>)>,
    depth: Depth,
    far: Vec<P>,
    top: Vec<P>,
    foot: Vec<P>,
    dune: Vec<P>,
    bluff: Vec<P>,
    /// what the living scene works with
    bind: CoastBindings,
    /// the lantern is not turning (a fault, or a test of the sweep), as of the last frame
    lantern: Cell<u8>,
    /// boats that have gone from their moorings, as of the last frame: bit 0 the first, bit 1 the second
    gone: Cell<u8>,
}

/// Where the living scene's objects stand, in the numbers of this view.
fn bindings() -> CoastBindings {
    CoastBindings { land: LAND, light: (LIGHT_X, LIGHT_FOOT, LAMP_Y), jetty: JETTY, door: (213.0, 518.0), boats: BOATS, crate_at: (640.0, 793.5) }
}

/// The outline of the lighthouse and the cottage, so that birds pass behind them.
fn buildings(sh: &mut Shaper, lay: &Layout) {
    let r = |sh: &mut Shaper, x0: f32, y0: f32, x1: f32, y1: f32| {
        sh.poly(&[(lay.dx(x0), lay.dy(y0)), (lay.dx(x1), lay.dy(y0)), (lay.dx(x1), lay.dy(y1) + 1.0), (lay.dx(x0), lay.dy(y1) + 1.0)]);
    };
    let x = LIGHT_X;
    sh.poly(&to_px(lay, &[(x - 15.0, LIGHT_FOOT + 2.0), (x - 9.5, 387.0), (x + 9.5, 387.0), (x + 15.0, LIGHT_FOOT + 2.0)]));
    r(sh, x - 15.0, 382.0, x + 15.0, 387.0);
    r(sh, x - 8.0, 361.0, x + 8.0, 383.0);
    sh.ellipse(lay.dx(x), lay.dy(361.5), lay.d(10.5), lay.d(9.5));
    r(sh, x - 36.0, 503.0, x - 13.0, LIGHT_FOOT);
    // the cottage, its lean-to and its chimney
    r(sh, 160.0, 488.0, 232.0, 518.0);
    sh.poly(&to_px(lay, &[(153.0, 489.0), (196.0, 467.0), (239.0, 489.0)]));
    r(sh, 142.0, 499.0, 161.0, 518.0);
    r(sh, 214.0, 461.0, 222.0, 482.0);
}

impl Shore {
    fn new(w: usize, h: usize) -> Shore {
        let lay = Layout::new(w, h, HORIZON);
        let land = lay.moved(LAND);
        let mut sh = Shaper::new();
        let far: Vec<P> = curve(FAR, 10).into_iter().map(|p| (p.0, SEA - p.1)).collect();
        let (top, foot, dune, bluff) = (curve(TOP, 8), curve(FOOT, 8), curve(DUNE, 8), curve(BLUFF, 8));
        let mut masks = Vec::new();
        let mut mask = |sh: &mut Shaper, top: f32, f: &mut dyn FnMut(&mut Shaper)| {
            if sh.begin(0.0, lay.dy(top).max(0.0), lay.w, lay.h, w, h) {
                f(sh);
                masks.push(sh.mask(w));
            } else {
                masks.push((h, Vec::new()));
            }
        };
        mask(&mut sh, SEA - 52.0, &mut |sh| {
            let mut p = to_px(&lay, &far);
            p.extend([(lay.w + 2.0, lay.dy(SEA)), (lay.dx(400.0), lay.dy(SEA))]);
            sh.poly(&p);
        });
        mask(&mut sh, 604.0, &mut |sh| {
            let mut p = to_px(&land, &curve(CAPE, 8));
            p.extend([(-2.0, lay.dy(SEA))]);
            sh.poly(&p);
        });
        mask(&mut sh, 336.0, &mut |sh| {
            let mut p = to_px(&land, &top);
            p.extend(to_px(&land, &foot));
            p.extend([(-2.0, lay.h + 2.0)]);
            sh.poly(&p);
            buildings(sh, &land);
        });
        mask(&mut sh, 690.0, &mut |sh| {
            let (x0, x1, y) = JETTY;
            sh.poly(&to_px(&land, &[(x0, y - 2.5), (x1, y - 2.5), (x1, y + 1.5), (x0, y + 1.5)]));
            for (x, wide, tall, y) in SKERRIES {
                sh.ellipse(land.dx(x), land.dy(y), land.d(wide / 2.0), land.d(tall));
            }
        });
        mask(&mut sh, 760.0, &mut |sh| {
            let mut p = to_px(&lay, &dune);
            p.extend([(lay.w + 2.0, lay.h + 2.0), (lay.dx(820.0), lay.h + 2.0)]);
            sh.poly(&p);
            let mut p = to_px(&lay, &bluff);
            p.extend([(-2.0, lay.h + 2.0)]);
            sh.poly(&p);
        });
        let depth = Depth::stack(&masks, w, h);
        Shore { lay, land, masks, depth, far, top, foot, dune, bluff, bind: bindings(), lantern: Cell::new(0), gone: Cell::new(0) }
    }
}

fn tide(sc: &Scene) -> f32 {
    sc.knob("tide").unwrap_or(sc.tide).clamp(-1.0, 1.0)
}

/// Is the water still coming in?
fn rising(sc: &Scene) -> bool {
    (2.0 * (sc.eph.moon.body.ha as f32 - 0.45)).sin() < 0.0
}

/// How broken the water is, 0 for a mirror to 1 for a gale.
fn chop(sc: &Scene) -> f32 {
    smooth(6.0, 42.0, sc.wind).max(if sc.storm { 0.8 } else { 0.0 })
}

/// The water takes the colour of the sky just over the horizon, deepening towards the shore.
fn water(sc: &Scene) -> (Rgb, Rgb) {
    let deep = mix(mix(sc.layers[2], sc.sky[3], 0.50), sc.sky[5], 0.16);
    (deep, mix(shade(deep, -0.34), sc.layers[4], 0.25))
}

/// Paint the sea from the horizon down: the sky above it turned over, row by row, pushed about
/// by ripples and sunk into the colour of deep water as it nears the shore.
fn sea(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout) {
    let (w, h) = (d.s.w, d.s.h);
    let hy = (lay.horizon().round().max(0.0) as usize).min(h);
    if hy >= h || hy < 2 {
        return;
    }
    let (deep, near) = water(sc);
    let rough = chop(sc);
    let mirror = (0.72 - 0.30 * rough) * (1.0 - 0.45 * sc.gloom);
    let span = (h - hy) as f32;
    let mut src = vec![0.0f32; w * 3];
    for y in hy..h {
        let t = (y - hy) as f32 / span;
        // the far water shows the sky low down; near the shore it shows what is overhead
        // seen at a glancing angle, far water squeezes what it reflects into a thin band
        let from = (hy as f32 - 1.0 - (y - hy) as f32 * (1.7 - 0.8 * t)).max(0.0) as usize;
        // moving water smears what it shows: each row is three rows of sky, further apart the nearer it is
        let apart = (1.0 + (6.0 + 18.0 * rough) * t * lay.u) as usize;
        let (a, b) = (from.saturating_sub(apart), (from + apart).min(hy - 1));
        for (k, v) in src.iter_mut().enumerate() {
            *v = 0.25 * d.s.px[a * w * 3 + k] + 0.5 * d.s.px[from * w * 3 + k] + 0.25 * d.s.px[b * w * 3 + k];
        }
        let yy = y as f32 / lay.u;
        let shift = (((yy * 0.71).sin() + 0.6 * (yy * 0.23 + 1.3).sin()) * (0.6 + 9.0 * t) * (0.5 + 1.3 * rough) * lay.u) as isize;
        let tone = mix(deep, near, t.powf(0.75));
        let k = mirror * (1.0 - 0.62 * t);
        let dst = &mut d.s.px[y * w * 3..(y + 1) * w * 3];
        for (x, p) in dst.chunks_exact_mut(3).enumerate() {
            let sx = (x as isize + shift).clamp(0, w as isize - 1) as usize * 3;
            p[0] = tone[0] + (src[sx] - tone[0]) * k;
            p[1] = tone[1] + (src[sx + 1] - tone[1]) * k;
            p[2] = tone[2] + (src[sx + 2] - tone[2]) * k;
        }
    }
    // a line where the sea ends, and the haze that lies along it
    d.rect(0.0, hy as f32, lay.w, hy as f32 + 1.4 * lay.u, shade(deep, -0.10), 0.55);
    band(d.s, hy as f32 - 26.0 * lay.u, hy as f32 + 34.0 * lay.u, 18.0 * lay.u, sc.haze, (0.10 + 0.5 * sc.fog + 0.3 * sc.mist).min(0.8));
}

/// The light a low sun or a bright moon lays on the water, from the horizon to the shore.
/// `flicker` picks which of the glints are lit: the still picture shows them all a little.
fn glitter<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, flicker: Option<f32>) {
    let (sun, moon) = (&sc.eph.sun, &sc.eph.moon);
    let el = sun.el as f32;
    let sun_k = sc.sun_vis * smooth(-1.5, 2.5, el) * (1.0 - 0.72 * smooth(14.0, 48.0, el));
    let moon_k = sc.moonlight * (1.0 - sc.day) * smooth(0.0, 6.0, moon.body.el as f32) * 0.8;
    let (x, col, k) = if sun_k > moon_k { (sun.x as f32, mix(sc.sun_col, sc.glow, 0.35), sun_k) } else { (moon.body.x as f32, hex(sc.p.moon), moon_k) };
    if k < 0.04 {
        return;
    }
    let (hy, bottom) = (lay.horizon(), lay.h);
    let cx = lay.x(x);
    let rough = chop(sc);
    let n = 150;
    for i in 0..n as u32 {
        let t = hash2(i, 31).powf(1.5);
        let y = hy + 2.0 * lay.u + (bottom - hy) * t;
        // the path is narrow at the horizon and spreads as it comes nearer
        let spread = lay.d(5.0 + 110.0 * t) * (0.7 + 0.8 * rough);
        let off = (hash2(i, 37) + hash2(i, 41) + hash2(i, 43) - 1.5) / 1.5;
        let (gx, len) = (cx + off * spread, lay.d(3.0 + 30.0 * t) * (0.6 + 0.8 * hash2(i, 47)));
        let centre = 1.0 - off.abs().powf(1.6);
        let a = match flicker {
            None => 0.50,
            Some(time) => (time * (1.6 + 2.4 * hash2(i, 53)) + TAU * hash2(i, 59)).sin().max(0.0).powi(2) * 0.55,
        };
        let a = a * k * centre * (0.95 - 0.45 * t);
        if a > 0.02 {
            d.rect_mode(gx - len / 2.0, y, gx + len / 2.0, y + (1.0 + 1.4 * t) * lay.u, col, a, Blend::Screen);
        }
    }
}

/// The lines the swell draws on the water, and white horses when it blows.
fn swell(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout) {
    let (hy, bottom) = (lay.horizon(), lay.h);
    let rough = chop(sc);
    let (deep, _) = water(sc);
    let crest = mix(deep, sc.sky[5], 0.55);
    let trough = shade(deep, -0.22);
    let n = (90.0 + 150.0 * rough) as u32;
    for i in 0..n {
        let t = hash2(i, 71).powf(1.7);
        let y = hy + 3.0 * lay.u + (bottom - hy) * t;
        let (x, len) = (lay.w * (hash2(i, 73) * 1.2 - 0.1), lay.d(14.0 + 120.0 * t) * (0.5 + hash2(i, 79)));
        let thick = (0.9 + 1.5 * t) * lay.u;
        let dark = i % 3 == 0;
        d.rect(x, y, x + len, y + thick, if dark { trough } else { crest }, (0.10 + 0.14 * rough) * if dark { 1.3 } else { 1.0 });
    }
    if rough > 0.35 {
        let foam = mix(hex(sc.p.snow), sc.sky[5], 0.25 + 0.4 * (1.0 - sc.day));
        for i in 0..(140.0 * (rough - 0.35)) as u32 {
            let t = hash2(i, 83).powf(1.4);
            let y = hy + 5.0 * lay.u + (bottom - hy) * t;
            let (x, len) = (lay.w * hash2(i, 89), lay.d(4.0 + 16.0 * t));
            d.rect(x, y, x + len, y + (1.0 + 1.2 * t) * lay.u, foam, 0.55);
        }
    }
}

/// A boat at its mooring: `k` picks its colour, `lit` hangs a blue lantern on it.
fn boat<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, x: f32, y: f32, cabin: bool, k: usize, lit: bool, bob: f32) {
    let u = lay.u;
    let hull = mix(sc.layers[3], CREAM, 0.06 + 0.26 * sc.day);
    let band_col = mix(hull, [ROSE, SAGE, AMBER, BLUE][k % 4], 0.62);
    let (px, py) = (lay.dx(x), lay.dy(y) + bob);
    let s = lay.d(1.0);
    d.poly(&[(px - 15.0 * s, py - 5.4 * s), (px + 17.0 * s, py - 6.4 * s), (px + 12.0 * s, py), (px - 11.0 * s, py)], hull, 1.0);
    d.rect(px - 14.0 * s, py - 5.6 * s, px + 15.6 * s, py - 4.0 * s, band_col, 0.95);
    if cabin {
        d.rect(px - 9.0 * s, py - 11.0 * s, px - 1.0 * s, py - 5.4 * s, hull, 1.0);
        d.rect(px - 7.4 * s, py - 9.6 * s, px - 2.6 * s, py - 7.4 * s, mix(hull, sc.sky[5], 0.5), 0.9);
    }
    let (mx, top) = (px + 3.0 * s, py - 22.0 * s);
    d.line((mx, py - 5.0 * s), (mx, top), (1.0 * u).max(0.8), hull, 1.0);
    if lit {
        let glow = 0.45 + 0.55 * sc.dark;
        d.glow(mx, top, 9.0 * u, 9.0 * u, BLUE, 0.75 * glow, 2, Blend::Screen);
        d.circle(mx, top, 2.3 * u, mix(BLUE, WHITE, 0.3), 1.0);
        // and what the water makes of it
        d.rect_mode(mx - 1.2 * u, py + 2.0 * s, mx + 1.2 * u, py + 13.0 * s, BLUE, 0.30 * glow, Blend::Screen);
    }
    // its shadow on the water
    d.rect(px - 11.0 * s, py, px + 12.0 * s, py + 1.6 * s, shade(water(sc).1, -0.2), 0.5);
}

/// Seconds the ferry takes to cross.
const FERRY: f64 = 720.0;

/// What is out on the water just now: the ferry on the hour, and small boats by the day of the week.
fn ships<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, now: f64, t: f32) {
    let u = lay.u;
    let far = mix(sc.layers[1], sc.sky[5], 0.20);
    let night = smooth(1.0, -5.0, sc.el_eff);
    // the ferry
    let since = now.rem_euclid(3600.0);
    if since < FERRY && sc.fog < 0.7 {
        let f = (since / FERRY) as f32;
        let way = if (now / 3600.0).floor() as i64 % 2 == 0 { 1.0 } else { -1.0 };
        let x = lay.dx(if way > 0.0 { 560.0 + 1100.0 * f } else { 1660.0 - 1100.0 * f });
        let y = lay.dy(SEA + 3.2);
        let s = lay.d(1.0);
        d.poly(&[(x - 27.0 * s, y - 5.0 * s), (x + 27.0 * s, y - 5.0 * s), (x + 24.0 * s, y), (x - 24.0 * s, y)], far, 1.0);
        d.rect(x - 17.0 * s, y - 10.0 * s, x + 13.0 * s, y - 5.0 * s, far, 1.0);
        d.rect(x - 3.0 * s - way * 5.0 * s, y - 15.0 * s, x + 3.0 * s - way * 5.0 * s, y - 10.0 * s, far, 1.0);
        if night > 0.1 {
            for i in 0..9 {
                d.dot(x - 15.0 * s + i as f32 * 3.4 * s, y - 7.4 * s, 0.9 * u, 0.9 * u, WARM[0], night, Blend::Over);
            }
            d.rect_mode(x - 16.0 * s, y + 0.5 * s, x + 14.0 * s, y + 3.6 * s, WARM[1], 0.20 * night, Blend::Screen);
        }
    }
    let calm = sc.rain < 0.3 && !sc.storm && sc.fog < 0.6 && sc.wind < 46.0;
    // fishing boats go out before sunrise on working days, each with a lamp
    if sc.dow < 5 && (4.4..8.6).contains(&sc.t) && calm {
        for (k, (x, y)) in [(960.0f32, 734.0f32), (1150.0, 768.0), (1290.0, 748.0)].into_iter().enumerate() {
            let s = lay.d(0.55 + 0.9 * (y - SEA) / 100.0);
            let (px, py) = (lay.dx(x), lay.dy(y) + (t * 0.9 + k as f32 * 2.1).sin() * 0.7 * u);
            d.poly(&[(px - 13.0 * s, py - 5.0 * s), (px + 14.0 * s, py - 5.0 * s), (px + 10.0 * s, py), (px - 10.0 * s, py)], far, 1.0);
            d.rect(px - 6.0 * s, py - 10.0 * s, px + 1.0 * s, py - 5.0 * s, far, 1.0);
            let lamp = 0.35 + 0.65 * sc.dark;
            d.glow(px + 4.0 * s, py - 11.0 * s, 8.0 * u, 8.0 * u, WARM[1], 0.7 * lamp, 2, Blend::Screen);
            d.dot(px + 4.0 * s, py - 11.0 * s, 1.4 * u, 1.4 * u, WARM[0], 1.0, Blend::Over);
            d.rect_mode(px + 4.0 * s - 1.0 * u, py + 1.0 * s, px + 4.0 * s + 1.0 * u, py + 10.0 * s, WARM[1], 0.26 * lamp, Blend::Screen);
        }
    }
    // sails at the weekend, when there is wind to fill them and light to see by
    if sc.dow >= 5 && sc.day > 0.4 && calm && sc.wind > 5.0 {
        let lean = sc.wind_x.clamp(-1.0, 1.0);
        for (k, (x, y, tint)) in [(1010.0f32, 742.0f32, CREAM), (1236.0, 724.0, mix(CREAM, ROSE, 0.35)), (1120.0, 786.0, mix(CREAM, SAGE, 0.3))].into_iter().enumerate() {
            let drift = (((now / 60.0) as f32 * (0.7 + 0.2 * k as f32) * lean * 4.0 + x).rem_euclid(1500.0)) + 80.0;
            let s = lay.d(0.6 + 1.0 * (y - SEA) / 100.0);
            let (px, py) = (lay.dx(drift), lay.dy(y) + (t * 0.8 + k as f32).sin() * 0.6 * u);
            let sail = mix(far, tint, 0.35 + 0.6 * sc.day);
            d.poly(&[(px - 8.0 * s, py - 3.0 * s), (px + 9.0 * s, py - 3.0 * s), (px + 6.0 * s, py), (px - 6.0 * s, py)], far, 1.0);
            d.poly(&[(px + 0.5 * s, py - 4.0 * s), (px + 0.5 * s - lean * 3.0 * s, py - 27.0 * s), (px + 10.0 * s, py - 4.0 * s)], sail, 1.0);
            d.poly(&[(px - 1.0 * s, py - 4.0 * s), (px - 1.0 * s - lean * 2.5 * s, py - 22.0 * s), (px - 8.0 * s, py - 4.0 * s)], shade(sail, -0.08), 1.0);
        }
    }
}

/// The beam of the light, at one moment of its turn. `turn` runs 0 to 1.
fn beam<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, turn: f32) {
    let night = smooth(2.0, -4.0, sc.el_eff);
    if night < 0.05 {
        return;
    }
    let (x, y) = (lay.dx(LIGHT_X), lay.dy(LAMP_Y));
    let a = TAU * turn;
    let (across, toward) = (a.cos(), a.sin());
    let air = 0.10 + 0.34 * sc.fog.max(sc.mist).max(sc.rain * 0.7);
    let col = [255.0, 240.0, 204.0];
    for side in [1.0, -1.0] {
        let reach = lay.d(620.0) * across * side;
        if reach.abs() > lay.d(40.0) {
            let wide = lay.d(30.0) * (0.35 + 0.65 * (reach.abs() / lay.d(620.0)));
            let rise = -lay.d(16.0) * (reach.abs() / lay.d(620.0));
            // in slices, each fainter than the last, so that it thins out into the dark
            let at = |k: f32, side: f32| (x + reach * k, y + rise * k + side * (1.5 * lay.u + wide * k / 2.0));
            const N: usize = 12;
            for i in 0..N {
                let (k0, k1) = (i as f32 / N as f32, (i + 1) as f32 / N as f32);
                let fade = (1.0 - (k0 + k1) / 2.0).powf(1.6);
                d.poly_mode(&[at(k0, -1.0), at(k1, -1.0), at(k1, 1.0), at(k0, 1.0)], col, air * night * fade, Blend::Screen);
            }
        }
    }
    // as it swings past it looks straight at you
    let flash = toward.max(0.0).powi(8);
    d.glow(x, y, lay.d(30.0 + 50.0 * flash), lay.d(30.0 + 50.0 * flash), col, (0.40 + 0.6 * flash) * night, 2, Blend::Screen);
    d.glow(x, y, lay.d(10.0), lay.d(10.0), WHITE, (0.6 + 0.4 * flash) * night, 2, Blend::Screen);
}

const DUNE_GRASS: (f32, f32, u32) = (846.0, 1606.0, 110);
const BLUFF_GRASS: (f32, f32, u32) = (-10.0, 440.0, 64);

fn cat_place(sc: &Scene) -> (f32, f32, f32, bool) {
    // on the wall, on the lean-to, by the tower door, on the dune
    const SPOTS: [(f32, f32, bool); 4] = [(266.0, 512.5, false), (151.0, 499.0, false), (346.0, 524.0, false), (1216.0, 936.0, true)];
    let k = (hash2((sc.day_id % 100000) as u32, (sc.t / 1.5) as u32 + 5) * SPOTS.len() as f32) as usize % SPOTS.len();
    let face = if hash2(k as u32 + 3, (sc.t / 1.5) as u32) < 0.5 { -1.0 } else { 1.0 };
    (SPOTS[k].0, SPOTS[k].1, face, SPOTS[k].2)
}

impl Stage for Shore {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        // the only windows are across the water: a village that keeps village hours
        let awake = smooth(1.0, -4.0, sc.el_eff) * if sc.t >= 16.0 || sc.t < 0.2 { 1.0 } else if sc.t < 1.5 { 0.45 } else if (5.6..8.5).contains(&sc.t) { 0.4 } else { 0.12 };
        Lights { share: 0.22 * awake, on: Vec::new() }
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        self.ground_state(d, sc, lights, bloom, out, &Snapshot::intact())
    }

    fn story(&self) -> Option<crate::story::Definition> {
        Some(episode::define(&self.bind))
    }

    fn story_tone(&self, sc: &Scene, layer: Option<u8>) -> Tone {
        let near = match layer {
            Some(l) if l <= 2 => shade(mix(sc.layers[3], sc.layers[2], 0.42), -0.25),
            Some(3) => sc.layers[3],
            _ => sc.layers[4],
        };
        Tone::new(near, 0.22 + 0.66 * sc.day, sc.sky[5])
    }

    fn story_actor(&self, d: &mut Draw<Frame>, sc: &Scene, a: &crate::story::actors::Actor, t: f32) -> bool {
        use crate::story::actors::{ActorState, Kind, Role};
        if a.kind != Kind::Boat {
            return false;
        }
        let u = self.lay.u;
        let moving = matches!(a.state, ActorState::Moving);
        let bob = ((t * 1.3 + a.id.slot as f32 * 1.9).sin() * (0.5 + 1.4 * chop(sc))) * u;
        let (x, y) = (a.pos.0, a.pos.1);
        // the owner's boat is the cabin-less one; the work boat has the cabin
        boat(d, sc, &self.lay, x, y, a.role == Role::Dockhand, a.id.slot as usize + 1, false, bob);
        if moving {
            // a wake behind it
            let way = a.facing;
            d.rect_mode(self.lay.dx(x) - way * 20.0 * u, self.lay.dy(y) + 1.0 * u, self.lay.dx(x) - way * 4.0 * u, self.lay.dy(y) + 2.2 * u, hex(sc.p.snow), 0.30, Blend::Screen);
        }
        true
    }

    fn story_alive(&self, d: &mut Draw<Frame>, sc: &Scene, state: &Snapshot, t: f32, indoors: bool) {
        if indoors {
            return;
        }
        let (lay, land) = (&self.lay, &self.land);
        let b = &self.bind;
        let gone = (!(state.get(episode::BOAT_A).visible && state.in_base(episode::BOAT_A))) as u8 | (((!state.in_base(episode::BOAT_B)) as u8) << 1);
        self.gone.set(gone);
        let fault = state.damage(episode::LANTERN) != Damage::Intact;
        let test = state.get(episode::LANTERN).open;
        self.lantern.set(fault as u8 | ((test as u8) << 1));
        let tone = self.story_tone(sc, Some(3));
        d.s.behind = Some(3);
        // the boat while somebody has it in hand, swinging on its rope
        let s = state.get(episode::BOAT_A);
        if s.visible && !state.in_base(episode::BOAT_A) && s.damage != Damage::Missing {
            let (x, y, cabin) = b.boats[2];
            boat(d, sc, lay, x + LAND + s.pose.dx, y + s.pose.dy, cabin, 2, false, 0.0);
        }
        let at = |x: f32, y: f32| (x + LAND, y);
        let s = state.get(episode::CRATE);
        if s.visible && !state.in_base(episode::CRATE) && s.damage != Damage::Missing {
            let c = at(b.crate_at.0, b.crate_at.1);
            crate::story::draw::crate_box(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), 0.8, s.pose.turn, &tone, 1.0);
        }
        let s = state.get(episode::LINE);
        if s.visible && !state.in_base(episode::LINE) {
            let c = at(JETTY.1 - 14.0, JETTY.2 - 2.0);
            episode::coil(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), &tone, 1.0);
        }
        let s = state.get(episode::PART);
        if s.visible && !state.in_base(episode::PART) {
            let c = at(LIGHT_X + 12.0, 380.0);
            episode::part(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), s.pose.turn, &tone, 1.0);
        }
        d.s.behind = None;
        if test {
            // the sweep under test: quick and plainly lit, by day as well
            let (x, y) = (land.dx(LIGHT_X), lay.dy(LAMP_Y));
            beam(d, sc, land, (t * 0.7).fract());
            d.glow(x, y, lay.d(16.0), lay.d(16.0), [255.0, 240.0, 204.0], 0.55, 2, Blend::Screen);
        }
    }

    fn ground_state(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let (lay, land) = (&self.lay, &self.land);
        let u = lay.u;
        let w = lay.w as usize;
        let light = Light::of(sc, lay);
        let level = tide(sc);
        let dusk = smooth(2.0, -4.0, sc.el_eff);

        // ── the far shore, and the village on it ──
        let far_col = mix(sc.layers[0], sc.sky[5], 0.22);
        d.s.stamp(&self.masks[0].1, self.masks[0].0 as i32, 0, |_| (far_col, 1.0));
        bloom.hide(&self.masks[0].1, w, self.masks[0].0);
        let village = lights.share / 0.22;
        if village > 0.04 {
            for i in 0..26u32 {
                if hash2(i, 211) > village {
                    continue;
                }
                let x = 880.0 + 210.0 * (hash2(i, 213) + hash2(i, 217)) / 2.0;
                let y = SEA - 1.5 - (height_at(&self.far, x).min(SEA) - SEA).abs() * 0.5 * hash2(i, 219);
                let (px, py) = (lay.dx(x), lay.dy(y));
                d.dot(px, py, 0.95 * u, 0.95 * u, WARM[i as usize % 4], 0.95, Blend::Over);
                bloom.light(px - 1.5 * u, py - 1.5 * u, px + 1.5 * u, py + 1.5 * u, WARM[1], 0.55 * sc.p.bloom);
            }
        }

        // ── the sea ──
        sea(d, sc, lay);
        swell(d, sc, lay);
        // the live layer makes the path of light dance; this is what it dances on
        glitter(d, sc, lay, None);
        if !sc.live {
            ships(d, sc, lay, sc.ts, (sc.ts % 600.0) as f32);
        }

        // ── the cape ──
        let cape = sc.layers[1];
        band(d.s, lay.dy(600.0), lay.dy(SEA + 30.0), 30.0 * u, sc.haze, sc.haze_alpha[1] / 255.0 * 0.8);
        d.s.stamp(&self.masks[1].1, self.masks[1].0 as i32, 0, |_| (cape, 1.0));
        bloom.hide(&self.masks[1].1, w, self.masks[1].0);
        // it darkens the water it stands in
        for (k, a) in [(1.0, 0.30), (0.6, 0.22), (0.3, 0.16)] {
            d.poly(&[(0.0, lay.dy(SEA)), (land.dx(770.0 * k + 200.0 * (1.0 - k)), lay.dy(SEA)), (land.dx(560.0 * k), lay.dy(SEA + 16.0 * k)), (0.0, lay.dy(SEA + 22.0 * k))], shade(cape, -0.1), a);
        }
        if sc.fog > 0.0 {
            band(d.s, lay.dy(560.0), lay.h * 1.3, 70.0 * u, mix(sc.sky[4], sc.haze, 0.5), 0.5 * sc.fog);
        }
        if sc.mist > 0.02 {
            band(d.s, lay.dy(640.0), lay.dy(760.0), 30.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.34 * sc.mist);
        }

        // ── the headland ──
        let head = mix(sc.layers[3], sc.layers[2], 0.42);
        // the land darkens as it falls away from the light towards the water
        let (hy0, hy1) = (lay.dy(520.0), lay.dy(960.0));
        let low = mix(head, sc.layers[4], 0.55);
        d.s.stamp(&self.masks[2].1, self.masks[2].0 as i32, 0, |y| (mix(head, low, smooth(hy0, hy1, y as f32)), 1.0));
        bloom.hide(&self.masks[2].1, w, self.masks[2].0);
        self.headland(d, sc, head, &light, level);
        self.lighthouse(d, sc, head, &light, bloom, out, state.damage(episode::LANTERN) == Damage::Intact);
        self.cottage(d, sc, head, &light, bloom, out);
        let turning = state.damage(episode::LANTERN) == Damage::Intact;
        if !sc.live {
            if turning {
                beam(d, sc, land, 0.07);
            }
        } else if turning {
            // the lamp itself never goes dark between sweeps
            let night = smooth(2.0, -4.0, sc.el_eff);
            d.glow(land.dx(LIGHT_X), lay.dy(LAMP_Y), lay.d(22.0), lay.d(22.0), [255.0, 240.0, 204.0], 0.30 * night, 2, Blend::Screen);
        }

        // ── rocks, the sandbar and the jetty ──
        let rock = sc.layers[3];
        let foam = mix(hex(sc.p.snow), sc.sky[5], 0.20 + 0.45 * (1.0 - sc.day));
        let bar = smooth(-0.10, -0.62, level);
        if bar > 0.02 {
            let sand = mix(sc.layers[3], [214.0, 198.0, 168.0], 0.16 + 0.26 * sc.day);
            let (bx, by, half) = (lay.dx(1040.0), lay.dy(880.0), lay.d(150.0) * (0.45 + 0.55 * bar));
            // wet sand round the edge of it, then the dry top
            d.ellipse(bx, by + lay.d(1.0), half * 1.06, lay.d(5.2 * bar), shade(sand, -0.14), 0.75);
            d.ellipse(bx - half * 0.04, by, half * 0.93, lay.d(3.6 * bar), sand, 0.9);
            d.ellipse(bx, by + lay.d(4.2 * bar), half * 1.04, lay.d(2.4 * bar), foam, 0.14);
        }
        for (x, wide, tall, y) in SKERRIES {
            let show = tall - 6.0 * level;
            if show > 0.8 {
                let (px, py) = (land.dx(x), lay.dy(y));
                let half = lay.d(wide / 2.0) * (0.6 + 0.4 * (show / (tall + 6.0)));
                let pts: Vec<P> = (0..=16).map(|i| PI * i as f32 / 16.0).map(|a| (px - half * a.cos(), py - lay.d(show) * a.sin().max(0.0).powf(0.8))).collect();
                d.poly(&pts, rock, 1.0);
                d.rect(px - half * 1.2, py, px + half * 1.2, py + 1.5 * u, foam, 0.42);
                d.rect(px - half, py + 1.5 * u, px + half, py + lay.d(show * 0.7), shade(water(sc).1, -0.1), 0.34);
            }
        }
        let (jx0, jx1, jy) = JETTY;
        let timber = mix(rock, sc.layers[2], 0.2);
        let mut px = jx0 + 10.0;
        while px < jx1 {
            d.rect(land.dx(px) - 1.2 * u, lay.dy(jy), land.dx(px) + 1.2 * u, lay.dy(jy + 9.0 + 3.0 * level.min(0.0).abs()), timber, 1.0);
            d.rect_mode(land.dx(px) - 1.0 * u, lay.dy(jy + 10.0), land.dx(px) + 1.0 * u, lay.dy(jy + 22.0), shade(timber, -0.3), 0.22, Blend::Over);
            px += 22.0;
        }
        d.rect(land.dx(jx0), lay.dy(jy - 2.5), land.dx(jx1), lay.dy(jy + 1.5), timber, 1.0);
        d.rect(land.dx(jx0), lay.dy(jy - 2.5), land.dx(jx1), lay.dy(jy - 2.5) + 1.1 * u, mix(timber, light.col, 0.5), (light.k * 0.9).min(0.8));
        d.line((land.dx(jx1 - 4.0), lay.dy(jy - 2.5)), (land.dx(jx1 - 4.0), lay.dy(jy - 13.0)), 1.2 * u, timber, 1.0);
        if dusk > 0.05 {
            // a lamp at the end of it, to steer by
            let (lx, ly) = (land.dx(jx1 - 4.0), lay.dy(jy - 14.0));
            d.glow(lx, ly, 8.0 * u, 8.0 * u, WARM[1], 0.6 * dusk, 2, Blend::Screen);
            d.dot(lx, ly, 1.6 * u, 1.6 * u, WARM[0], dusk, Blend::Over);
            d.rect_mode(lx - u, lay.dy(jy + 4.0), lx + u, lay.dy(jy + 20.0), WARM[1], 0.22 * dusk, Blend::Screen);
        }
        if !sc.live {
            for (k, (x, y, cabin)) in BOATS.into_iter().enumerate() {
                let away = (k == 2 && !(state.get(episode::BOAT_A).visible && state.in_base(episode::BOAT_A))) || (k == 3 && !state.in_base(episode::BOAT_B));
                if !away {
                    boat(d, sc, land, x, y, cabin, k, k < sc.machine.containers as usize, 0.0);
                }
            }
        }
        // what the scene has put on the jetty and the gallery, where it lies at rest
        let tone = self.story_tone(sc, Some(3));
        let at = |x: f32, y: f32| (x + LAND, y);
        let s = state.get(episode::CRATE);
        if s.visible && state.in_base(episode::CRATE) && s.damage != Damage::Missing {
            let c = at(self.bind.crate_at.0, self.bind.crate_at.1);
            crate::story::draw::crate_box(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), 0.8, s.pose.turn, &tone, 1.0);
        }
        let s = state.get(episode::LINE);
        if s.visible && state.in_base(episode::LINE) {
            let c = at(JETTY.1 - 14.0, JETTY.2 - 2.0);
            episode::coil(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), &tone, 1.0);
        }
        let s = state.get(episode::PART);
        if s.visible && state.in_base(episode::PART) {
            let c = at(LIGHT_X + 12.0, 380.0);
            episode::part(d, lay, (c.0 + s.pose.dx, c.1 + s.pose.dy), s.pose.turn, &tone, 1.0);
        }

        // ── the dune ──
        let dune = sc.layers[4];
        d.s.stamp(&self.masks[4].1, self.masks[4].0 as i32, 0, |_| (dune, 1.0));
        bloom.hide(&self.masks[4].1, w, self.masks[4].0);
        if !sc.live {
            grass(d, sc, lay, &self.dune, DUNE_GRASS, (sc.ts / 60.0) as f32, sc.layers[4]);
            grass(d, sc, lay, &self.bluff, BLUFF_GRASS, (sc.ts / 60.0) as f32, sc.layers[4]);
            if sc.kite {
                props::kite(d, sc, lay.dx(1290.0), lay.dy(924.0), u, (sc.ts / 420.0) as f32, (sc.ts / 700.0) as f32, (sc.ts / 300.0) as f32);
            }
        }
        if sc.festoon {
            // a fire on the dune: Friday and Saturday nights, and the long nights at the turn of the year
            let (fx, fy) = (lay.dx(FIRE.0), lay.dy(FIRE.1));
            for (dx, lean) in [(-7.0, 1.0), (7.0, -1.0), (0.0, 0.2)] {
                d.line((fx + dx * u, fy + 1.0 * u), (fx + (dx + lean * 9.0) * u, fy - 7.0 * u), 2.2 * u, shade(dune, -0.2), 1.0);
            }
            d.glow(fx, fy - 6.0 * u, 62.0 * u, 40.0 * u, [255.0, 170.0, 90.0], 0.26, 2, Blend::Screen);
            d.poly(&[(fx - 6.0 * u, fy), (fx - 2.0 * u, fy - 15.0 * u), (fx + 1.0 * u, fy - 8.0 * u), (fx + 4.0 * u, fy - 19.0 * u), (fx + 7.0 * u, fy)], [255.0, 196.0, 110.0], 0.96);
            d.poly(&[(fx - 3.0 * u, fy), (fx, fy - 9.0 * u), (fx + 3.5 * u, fy)], [255.0, 236.0, 190.0], 0.96);
            bloom.light(fx - 8.0 * u, fy - 20.0 * u, fx + 8.0 * u, fy, [255.0, 170.0, 90.0], 1.0);
            out.breath.push(Breath { x: fx, y: fy - 8.0 * u, rx: 90.0 * u, ry: 56.0 * u, col: [255.0, 170.0, 90.0], strength: 0.9, low: true, behind: None });
            for k in 0..5 {
                out.twinkle.push((fx + (k as f32 * 7.0 - 14.0) * u, fy - (24.0 + 9.0 * ((k * 3) % 5) as f32) * u, [255.0, 190.0, 110.0], 9));
            }
        }
        if sc.cat_out && !sc.live {
            let (x, y, face, big) = cat_place(sc);
            if !sc.cat_asleep {
                props::cat(d, if big { lay.dx(x) } else { land.dx(x) }, lay.dy(y), u * if big { 1.25 } else { 0.9 }, if big { dune } else { shade(head, -0.25) }, false, face, (sc.ts / 37.0) as f32);
            }
        }
        if sc.cat_out && sc.cat_asleep {
            props::cat(d, land.dx(248.0), lay.dy(512.5), u * 0.9, shade(head, -0.25), true, 1.0, 0.0);
        }
        out.glades.push([lay.dx(900.0), lay.dy(880.0), lay.dx(1580.0), lay.dy(950.0)]);
        out.glades.push([lay.dx(20.0), lay.dy(770.0), lay.dx(380.0), lay.dy(900.0)]);
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let (lay, land) = (&self.lay, &self.land);
        let u = lay.u;
        d.s.behind = Some(1);
        glitter(d, sc, lay, Some(t));
        ships(d, sc, lay, now, t);
        d.s.behind = None;
        // the light turns once in twelve seconds, unless it has failed or is being tried
        if self.lantern.get() == 0 {
            beam(d, sc, land, (now / 12.0).fract() as f32);
        }
        d.s.behind = Some(4);
        for (k, (x, y, cabin)) in BOATS.into_iter().enumerate() {
            let bit = match k {
                2 => 1,
                3 => 2,
                _ => 0,
            };
            if self.gone.get() & bit != 0 {
                continue;
            }
            let bob = ((t * (0.7 + 0.08 * k as f32) + k as f32 * 1.7).sin() * (0.5 + 1.4 * chop(sc))) * u;
            boat(d, sc, land, x, y, cabin, k, k < sc.machine.containers as usize, bob);
        }
        d.s.behind = None;
        let head = shade(mix(sc.layers[3], sc.layers[2], 0.42), -0.25);
        props::pennant(d, sc, land.dx(136.0), lay.dy(516.0), lay.dy(470.0), u * 0.9, head, t * (2.0 + 3.0 * sc.wind_x.abs().min(1.5)));
        if sc.laundry > 0 {
            props::laundry(d, sc, land.dx(244.0), land.dx(296.0), lay.dy(519.0), u * 0.8, head, 0, t * 1.5);
        }
        if sc.sleepers > 0 {
            props::sleep_z(d, sc, land.dx(206.0), lay.dy(458.0), u * 0.9, t * 0.8);
        }
        grass(d, sc, lay, &self.dune, DUNE_GRASS, t, sc.layers[4]);
        grass(d, sc, lay, &self.bluff, BLUFF_GRASS, t, sc.layers[4]);
        if sc.kite {
            props::kite(d, sc, lay.dx(1290.0), lay.dy(924.0), u, t * 0.35, t * 0.23, t * 3.0);
        }
        if sc.cat_out && !sc.cat_asleep {
            let (x, y, face, big) = cat_place(sc);
            let tail = if (t * 0.11).sin() > -0.3 { t * 1.4 } else { 0.0 };
            props::cat(d, if big { lay.dx(x) } else { land.dx(x) }, lay.dy(y), u * if big { 1.25 } else { 0.9 }, if big { sc.layers[4] } else { head }, false, face, tail);
        }
    }

    fn lively(&self, sc: &Scene, _now: f64) -> bool {
        sc.kite
    }
}

impl Shore {
    /// The turf on top, the strata of the cliff, and the line of foam at its foot.
    fn headland(&self, d: &mut Draw<Canvas>, sc: &Scene, head: Rgb, light: &Light, level: f32) {
        let lay = &self.land;
        let u = lay.u;
        // turf lies along the top as far as the cliff edge
        let turf = mix(head, hex(sc.p.seasons[sc.season][1]), 0.10 + 0.34 * sc.day);
        let edge: Vec<P> = self.top.iter().copied().filter(|p| p.0 <= 452.0).collect();
        let mut sod = to_px(lay, &edge);
        sod.extend(edge.iter().rev().map(|p| (lay.dx(p.0), lay.dy(p.1 + 7.0 + 4.0 * (p.0 * 0.05).sin()))));
        d.poly(&sod, turf, 0.9);
        // the face of the cliff catches a low sun in the west
        let face: Vec<P> = self.top.iter().copied().filter(|p| p.0 >= 430.0).chain(self.foot.iter().copied().take_while(|p| p.1 < 800.0)).collect();
        let glow = light.k * if light.x > lay.dx(480.0) { 1.0 } else { 0.25 };
        if glow > 0.03 {
            d.poly(&to_px(lay, &face), mix(head, light.col, 0.55), (0.42 * glow).min(0.5));
        }
        // beds of rock, tilted: broad soft bands rather than lines, with a gully between two buttresses
        let dark = shade(head, -0.16);
        for (a, b, wide) in [((462.0, 574.0), (520.0, 706.0), 9.0), ((432.0, 556.0), (478.0, 700.0), 7.0), ((396.0, 548.0), (446.0, 760.0), 12.0), ((350.0, 560.0), (410.0, 820.0), 8.0), ((296.0, 580.0), (366.0, 880.0), 13.0), ((500.0, 640.0), (548.0, 742.0), 6.0)] {
            d.line((lay.dx(a.0), lay.dy(a.1)), (lay.dx(b.0), lay.dy(b.1)), lay.d(wide), dark, 0.22);
        }
        d.poly(&to_px(lay, &[(428.0, 548.0), (446.0, 596.0), (432.0, 668.0), (446.0, 742.0), (420.0, 700.0), (410.0, 620.0), (414.0, 566.0)]), dark, 0.30);
        let ledge = mix(shade(head, 0.07), light.col, 0.25 * light.k);
        for (a, b) in [((456.0, 604.0), (486.0, 611.0)), ((476.0, 662.0), (520.0, 671.0)), ((506.0, 712.0), (560.0, 720.0))] {
            d.line((lay.dx(a.0), lay.dy(a.1)), (lay.dx(b.0), lay.dy(b.1)), 2.4 * u, ledge, 0.22 + 0.2 * sc.day);
        }
        // the path down from the cottage, widening as it comes nearer
        let track = mix(head, CREAM, 0.10 + 0.22 * sc.day);
        let way = curve(&[(213.0, 520.0), (236.0, 548.0), (214.0, 590.0), (160.0, 640.0), (96.0, 700.0), (20.0, 776.0), (-70.0, 860.0)], 8);
        let n = way.len();
        let side = |k: usize, sign: f32| {
            let (a, b) = (way[k.saturating_sub(1)], way[(k + 1).min(n - 1)]);
            let (dx, dy) = (b.0 - a.0, b.1 - a.1);
            let len = (dx * dx + dy * dy).sqrt().max(1e-3);
            let half = (1.0 + 5.5 * k as f32 / n as f32) * sign;
            (lay.dx(way[k].0 - dy / len * half), lay.dy(way[k].1 + dx / len * half))
        };
        let outline: Vec<P> = (0..n).map(|k| side(k, 1.0)).chain((0..n).rev().map(|k| side(k, -1.0))).collect();
        d.poly(&outline, track, 0.42);
        // a fence along the edge of the cliff, and gorse in the lee of it
        let post = shade(head, -0.28);
        let mut fx = 346.0;
        let mut last: Option<P> = None;
        while fx < 452.0 {
            let (px, py) = (lay.dx(fx), lay.dy(height_at(&self.top, fx) + 1.0));
            d.rect(px - 0.6 * u, py - lay.d(6.5), px + 0.6 * u, py, post, 1.0);
            if let Some(p) = last {
                d.line((p.0, p.1 - lay.d(4.4)), (px, py - lay.d(4.4)), 0.8 * u, post, 0.9);
            }
            last = Some((px, py));
            fx += 13.0;
        }
        let gorse = mix(shade(head, -0.20), hex(sc.p.seasons[sc.season][0]), 0.20 * sc.day);
        for (x, wide, tall) in [(72.0, 20.0, 8.0), (96.0, 13.0, 6.0), (262.0, 11.0, 5.0), (404.0, 15.0, 6.5), (424.0, 9.0, 4.5)] {
            d.ellipse(lay.dx(x), lay.dy(height_at(&self.top, x) + 1.0), lay.d(wide / 2.0), lay.d(tall), gorse, 1.0);
        }
        // wet rock below the tide mark, and the surf
        let foot = to_px(lay, &self.foot);
        let wet = lay.d(3.0 + 5.0 * (1.0 - level) / 2.0);
        let up: Vec<P> = foot.iter().map(|p| (p.0 - wet * 0.8, p.1 - wet * 0.4)).collect();
        d.path(&up, wet * 1.4, shade(head, -0.16), 0.7);
        let foam = mix(hex(sc.p.snow), sc.sky[5], 0.20 + 0.45 * (1.0 - sc.day));
        let rough = chop(sc);
        d.path(&foot, (2.0 + 3.5 * rough) * u, foam, 0.42 + 0.3 * rough);
        let off: Vec<P> = foot.iter().enumerate().map(|(i, p)| (p.0 + (9.0 + 4.0 * (i as f32 * 0.9).sin()) * u, p.1 + 3.0 * u)).collect();
        d.path(&off, 1.4 * u, foam, 0.16 + 0.2 * rough);
    }

    fn lighthouse(&self, d: &mut Draw<Canvas>, sc: &Scene, head: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors, working: bool) {
        let lay = &self.land;
        let u = lay.u;
        let x = LIGHT_X;
        let p = |a: f32, b: f32| (lay.dx(a), lay.dy(b));
        let white = mix(head, CREAM, 0.08 + 0.70 * sc.day);
        let red = mix(head, ROSE, 0.10 + 0.52 * sc.day);
        let dark = shade(head, -0.22);
        // the tower, tapering, with one broad band
        let half = |y: f32| 9.5 + 5.5 * (y - 387.0) / (LIGHT_FOOT - 387.0);
        d.poly(&[p(x - half(LIGHT_FOOT), LIGHT_FOOT + 2.0), p(x - 9.5, 387.0), p(x + 9.5, 387.0), p(x + half(LIGHT_FOOT), LIGHT_FOOT + 2.0)], white, 1.0);
        d.poly(&[p(x - half(470.0), 470.0), p(x - half(432.0), 432.0), p(x + half(432.0), 432.0), p(x + half(470.0), 470.0)], red, 1.0);
        // the side away from the light is in shade
        let side = if light.x < lay.dx(x) { 1.0 } else { -1.0 };
        let shadow = shade(head, -0.1);
        d.poly(&[p(x + side * half(LIGHT_FOOT), LIGHT_FOOT + 2.0), p(x + side * 9.5, 387.0), p(x + side * 4.0, 387.0), p(x + side * (half(LIGHT_FOOT) - 6.0), LIGHT_FOOT + 2.0)], shadow, 0.20 + 0.25 * sc.day);
        if light.k > 0.03 {
            d.poly(&[p(x - side * half(LIGHT_FOOT), LIGHT_FOOT + 2.0), p(x - side * 9.5, 387.0), p(x - side * 7.6, 387.0), p(x - side * (half(LIGHT_FOOT) - 2.2), LIGHT_FOOT + 2.0)], mix(white, light.col, 0.7), (light.k * 0.9).min(0.85));
        }
        for y in [410.0, 452.0, 490.0] {
            d.rect(lay.dx(x - 1.6), lay.dy(y), lay.dx(x + 1.6), lay.dy(y + 6.0), dark, 0.85);
        }
        d.rect(lay.dx(x - 3.4), lay.dy(508.0), lay.dx(x + 3.4), lay.dy(LIGHT_FOOT + 1.0), dark, 0.9);
        // the gallery and its rail
        d.rect(lay.dx(x - 15.0), lay.dy(382.0), lay.dx(x + 15.0), lay.dy(387.0), dark, 1.0);
        d.rect(lay.dx(x - 15.0), lay.dy(374.5), lay.dx(x + 15.0), lay.dy(374.5) + 1.0 * u, dark, 1.0);
        for k in 0..7 {
            let px = lay.dx(x - 15.0 + 5.0 * k as f32);
            d.rect(px - 0.5 * u, lay.dy(374.5), px + 0.5 * u, lay.dy(382.0), dark, 1.0);
        }
        // the lantern: glass by day, the lamp behind it by night
        let night = smooth(2.0, -4.0, sc.el_eff) * if working { 1.0 } else { 0.0 };
        let glass = mix(mix(head, sc.sky[5], 0.62), [255.0, 244.0, 214.0], night);
        d.rect(lay.dx(x - 8.0), lay.dy(361.0), lay.dx(x + 8.0), lay.dy(383.0), dark, 1.0);
        d.rect(lay.dx(x - 6.4), lay.dy(363.0), lay.dx(x + 6.4), lay.dy(381.0), glass, 1.0);
        for dx in [-2.2, 2.2] {
            d.rect(lay.dx(x + dx) - 0.6 * u, lay.dy(363.0), lay.dx(x + dx) + 0.6 * u, lay.dy(381.0), dark, 0.9);
        }
        if night > 0.05 {
            bloom.light(lay.dx(x - 8.0), lay.dy(361.0), lay.dx(x + 8.0), lay.dy(383.0), [255.0, 232.0, 180.0], 1.2 * night);
        }
        // its cap, and the vane on top
        let cap: Vec<P> = (0..=14).map(|i| PI * i as f32 / 14.0).map(|a| p(x - 10.5 * a.cos(), 361.5 - 9.5 * a.sin())).collect();
        d.poly(&cap, red, 1.0);
        d.poly(&cap, dark, 0.35);
        d.line(p(x, 353.0), p(x, 340.0), 1.1 * u, dark, 1.0);
        d.circle(lay.dx(x), lay.dy(351.0), 1.9 * u, dark, 1.0);
        // the store beside it
        d.rect(lay.dx(x - 36.0), lay.dy(504.0), lay.dx(x - 13.0), lay.dy(LIGHT_FOOT + 1.0), mix(white, head, 0.25), 1.0);
        d.poly(&[p(x - 38.0, 505.0), p(x - 13.0, 498.0), p(x - 13.0, 505.0)], dark, 1.0);
        out.perch = Some(p(x, 374.0));
    }

    fn cottage(&self, d: &mut Draw<Canvas>, sc: &Scene, head: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.land;
        let u = lay.u;
        let p = |a: f32, b: f32| (lay.dx(a), lay.dy(b));
        let r = |d: &mut Draw<Canvas>, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, a: f32| d.rect(lay.dx(x0), lay.dy(y0), lay.dx(x1), lay.dy(y1), col, a);
        let wall = mix(head, CREAM, 0.06 + 0.56 * sc.day);
        let slate = mix(shade(head, -0.20), ROSE, 0.20 * sc.day);
        let dark = shade(head, -0.25);
        // a dry-stone wall from the cottage to the tower
        r(d, 232.0, 512.5, 286.0, 519.0, dark, 1.0);
        r(d, 142.0, 499.0, 161.0, 518.0, mix(wall, head, 0.3), 1.0);
        d.poly(&[p(140.0, 500.0), p(161.0, 494.0), p(161.0, 500.0)], slate, 1.0);
        r(d, 214.0, 461.0, 222.0, 482.0, mix(wall, head, 0.2), 1.0);
        r(d, 213.0, 460.0, 223.0, 462.6, dark, 1.0);
        r(d, 160.0, 488.0, 232.0, 518.0, wall, 1.0);
        d.poly(&[p(153.0, 489.0), p(196.0, 467.0), p(239.0, 489.0)], slate, 1.0);
        // light down the side that faces it
        if light.k > 0.03 {
            let left = light.x < lay.dx(196.0);
            let (a, b) = if left { (p(153.0, 489.0), p(196.0, 467.0)) } else { (p(196.0, 467.0), p(239.0, 489.0)) };
            d.line(a, b, 1.6 * u, mix(slate, light.col, 0.65), (light.k * 0.9).min(0.85));
            let x = if left { 160.0 } else { 230.4 };
            r(d, x, 489.0, x + 1.6, 518.0, mix(wall, light.col, 0.65), (light.k * 0.8).min(0.8));
        }
        // the door, a small window, and the window with the lamp in it
        r(d, 208.0, 499.0, 218.0, 518.0, dark, 0.92);
        let (wx0, wy0, wx1, wy1) = (172.0, 494.0, 196.0, 510.0);
        if sc.machine.agents > 0 {
            let (lamp, glow, low) = lamp_colours(sc);
            r(d, wx0, wy0, wx1, wy1, lamp, 1.0);
            bloom.light(lay.dx(wx0 - 3.0), lay.dy(wy0 - 3.0), lay.dx(wx1 + 3.0), lay.dy(wy1 + 3.0), glow, (0.9 + 0.9 * sc.dark) * sc.p.bloom.max(0.6));
            let (cx, cy) = p((wx0 + wx1) / 2.0, (wy0 + wy1) / 2.0);
            out.breath.push(Breath { x: cx, y: cy, rx: lay.d(48.0), ry: lay.d(36.0), col: glow, strength: 0.45 + 0.55 * sc.dark, low, behind: Some(3) });
        } else {
            r(d, wx0, wy0, wx1, wy1, mix(head, sc.sky[5], 0.10 + 0.35 * sc.day), 1.0);
        }
        d.line(p((wx0 + wx1) / 2.0, wy0), p((wx0 + wx1) / 2.0, wy1), 1.4 * u, dark, 1.0);
        d.line(p(wx0, (wy0 + wy1) / 2.0), p(wx1, (wy0 + wy1) / 2.0), 1.1 * u, dark, 1.0);
        // the stovepipe on the lean-to smokes when the processor is busy; the chimney, when it is cold
        let pipe = p(148.0, 482.0);
        d.rect(pipe.0 - 1.6 * u, pipe.1, pipe.0 + 1.6 * u, lay.dy(499.0), dark, 1.0);
        d.rect(pipe.0 - 2.6 * u, pipe.1 - 1.6 * u, pipe.0 + 2.6 * u, pipe.1, dark, 1.0);
        let smoke = mix(head, sc.sky[5], 0.72);
        out.stove = Some(Smoke { x: pipe.0, y: pipe.1 - 1.6 * u, scale: 0.7, col: smoke, behind: 9 });
        out.music = Some((pipe.0 + lay.d(20.0), pipe.1));
        if sc.machine.load > 0.35 && !sc.live {
            props::smoke_still(d, sc, pipe.0, pipe.1 - 1.6 * u, u * 0.7, smoke, 1.3);
        }
        if sc.smoke > 0.08 {
            let s = Smoke { x: lay.dx(218.0), y: lay.dy(460.0), scale: 0.8, col: smoke, behind: 9 };
            if !sc.live {
                props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, 4.0);
            }
            out.smoke.push(s);
        }
        if !sc.live {
            props::pennant(d, sc, lay.dx(136.0), lay.dy(516.0), lay.dy(470.0), u * 0.9, dark, (sc.ts / 200.0) as f32);
            if sc.laundry > 0 {
                props::laundry(d, sc, lay.dx(244.0), lay.dx(296.0), lay.dy(519.0), u * 0.8, dark, 0, (sc.ts / 240.0) as f32);
            }
            if sc.sleepers > 0 {
                props::sleep_z(d, sc, lay.dx(206.0), lay.dy(458.0), u * 0.9, (sc.ts / 400.0) as f32);
            }
            if sc.machine.audio {
                let ink = mix(hex(sc.p.ink), AMBER, 0.4);
                props::note(d, pipe.0 + lay.d(22.0), pipe.1 - 12.0 * u, u, ink, 0.8, false);
                props::note(d, pipe.0 + lay.d(6.0), pipe.1 - 30.0 * u, u, ink, 0.55, true);
            }
        }
        // a buoy off the point, flashing green
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let (bx, by) = p(866.0, 742.0);
        d.poly(&[(bx - 2.4 * u, by), (bx, by - 9.0 * u), (bx + 2.4 * u, by)], shade(head, -0.1), 1.0);
        if dusk > 0.1 {
            out.blink.push(Blink { x: bx, y: by - 9.0 * u, r: 10.0 * u, col: SAGE, every: 4.0, phase: 0.2, behind: Some(3) });
        }
    }
}

fn show_tide(v: f64) -> String {
    let word = if v > 0.6 {
        "high water"
    } else if v > 0.15 {
        "flood"
    } else if v > -0.15 {
        "half tide"
    } else if v > -0.6 {
        "ebb"
    } else {
        "low water"
    };
    format!("{:+.1}  {}", v, word)
}

const KNOBS: &[Knob] = &[Knob { key: "tide", label: "tide", lo: -1.0, hi: 1.0, step: 0.1, big: 0.5, show: show_tide }];

impl View for Coast {
    fn id(&self) -> &'static str {
        "coast"
    }

    fn about(&self) -> &'static str {
        "a headland and its lighthouse, over the strait"
    }

    fn horizon(&self) -> f64 {
        HORIZON
    }

    fn knobs(&self) -> &'static [Knob] {
        KNOBS
    }

    fn real(&self, key: &str, sc: &Scene) -> f64 {
        match key {
            "tide" => sc.tide as f64,
            _ => 0.0,
        }
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Shore::new(w, h))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let week = [
            "a working day: boats out fishing before sunrise, the ferry across on the hour",
            "a working day: boats out fishing before sunrise, the ferry across on the hour",
            "a working day: boats out fishing before sunrise, the ferry across on the hour",
            "a working day: boats out fishing before sunrise, the ferry across on the hour",
            "boats out early; a fire on the dune after dark",
            "sails out by day if there is wind, washing by the cottage, a fire on the dune tonight",
            "nobody up early; sails out, washing by the cottage",
        ][sc.dow];
        let level = tide(sc);
        let state = if level > 0.75 {
            "high water".to_string()
        } else if level < -0.75 {
            "low water, the skerries and the sandbar showing".to_string()
        } else {
            format!("the tide {}{}", if rising(sc) { "coming in" } else { "going out" }, if level < -0.15 { ", the sandbar showing" } else { "" })
        };
        let rough = chop(sc);
        let mut sea = vec![state, if rough < 0.15 { "the water like glass" } else if rough < 0.45 { "a light chop" } else if rough < 0.75 { "white horses" } else { "a heavy sea" }.to_string()];
        let since = sc.ts.rem_euclid(3600.0);
        for (flag, text) in [
            (since < FERRY && sc.fog < 0.7, "the ferry crossing"),
            (sc.el_eff < -1.0, "the light turning"),
            (sc.dow < 5 && (4.4..8.6).contains(&sc.t), "fishing boats out"),
            (sc.dow >= 5 && sc.day > 0.4 && sc.wind > 5.0 && sc.rain < 0.3, "sails out"),
            (sc.festoon, "a fire on the dune"),
            (sc.kite, "a kite up"),
            (sc.laundry > 0, "washing by the cottage"),
            (sc.balloon, "a balloon drifting over"),
            (!sc.birds.is_empty(), "gulls crossing"),
            (sc.smoke > 0.08, "the chimney smoking"),
            (sc.mist > 0.25, "mist on the water"),
        ] {
            if flag {
                sea.push(text.to_string());
            }
        }
        if sc.cat_out {
            sea.push(if sc.cat_asleep { "the cat asleep on the wall" } else { "the cat about" }.to_string());
        }
        let house = Yours { lamp: "the lamp in the keeper's window", containers: ("blue lantern on the boats", "blue lanterns on the boats"), smoke: "the stovepipe smoking" };
        vec![("week", week.to_string()), ("sea", sea.join(", ")), ("yours", yours(sc, &house))]
    }
}
