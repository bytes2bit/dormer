//! The painter: one still frame of the scene.
//!
//! The sky and what is in it are painted here, the same for every view. Then
//! the view paints its ground over that, far to near, and last comes the
//! weather and the light that spills from every lit window.

use std::f32::consts::PI;

use crate::color::{hex, mix, shade, smooth, Rgb, AMBER, BLUE, ROSE, SAGE, STOPS, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Draw, Patch, Shaper, Veil, P};
use crate::props::{self, Layout};
use crate::rng::{hash2, PyRng};
use crate::scene::Scene;
use crate::sky::{galactic, sky_at, star_at, Ephemeris, Star};
use crate::views::{Anchors, Sky, Spark, Stage, View};

/// A view laid out for one size of screen, and the stars that go over it.
pub struct Cache {
    pub w: usize,
    pub h: usize,
    pub view: &'static dyn View,
    pub seed: u64,
    pub lay: Layout,
    pub stage: Box<dyn Stage>,
    pub stars: Vec<Star>,
}

impl Cache {
    pub fn new(view: &'static dyn View, seed: u64, w: usize, h: usize) -> Cache {
        let stage = view.stage(seed, w, h);
        Cache { w, h, view, seed, lay: *stage.lay(), stage, stars: crate::sky::stars(seed) }
    }
}

struct Ctx<'a> {
    sc: &'a Scene,
    cache: &'a Cache,
    lay: Layout,
}

/// Draw one frame. Also fills in `sc.lit_share`.
pub fn render(sc: &mut Scene, cache: &Cache) -> (Canvas, Anchors) {
    let lay = cache.lay;
    let (w, h) = (cache.w, cache.h);
    let mut out = Anchors::default();

    // Which windows are lit is needed twice: for the glow the place throws at the sky, and to draw them.
    let lights = cache.stage.lights(sc);
    sc.lit_share = lights.share;
    let sc = &*sc;
    let cx = Ctx { sc, cache, lay };

    let timing = std::env::var("DORMER_TIME").is_ok();
    let mut clock = std::time::Instant::now();
    let mut lap = |what: &str| {
        if timing {
            eprintln!("{:>10} {:6.2} ms", what, clock.elapsed().as_secs_f64() * 1e3);
            clock = std::time::Instant::now();
        }
    };
    let mut cv = Canvas::new(w, h);
    let mut sh = Shaper::new();
    let mut bloom = Bloom::new(w, h, 4);
    let home = cache.view.sky() == Sky::Home;
    if home {
        let rows = sky_rows(sc, &lay);
        cv.rows(&rows);
        lap("sky");
        milky_way(&mut cv, sc, &lay, &sc.eph, sc.night * sc.night * (1.0 - 0.85 * sc.moonlight));
        lap("milky way");
        wide_glows(&mut cv, &cx);
        lap("wide glows");
    }
    {
        let mut d = Draw::new(&mut cv, &mut sh);
        if home {
            stars(&mut d, &cx, &mut out);
            lap("stars");
            moon(&mut d, &cx);
            sun(&mut d, &cx);
            lap("sun, moon");
        }
        cache.stage.heavens(&mut d, sc, &cache.stars, &mut out);
        lap("heavens");
        if home {
            clouds(&mut d, &cx);
            lap("clouds");
        }
        if !sc.live && home {
            for p in &sc.planes {
                props::plane(&mut d, sc, &lay, lay.x(p[0]), lay.sy(p[1]), p[2], p[3], if sc.slot % 2 == 0 { 1.0 } else { 0.0 });
            }
            if sc.storm {
                lightning(&mut d, &cx);
            }
            flock(&mut d, &cx);
        }
        if sc.fireworks && !sc.live && home {
            fireworks(&mut d, &cx);
        }
        if sc.balloon && home {
            props::balloon(&mut d, sc, &lay, (sc.ts / 500.0) as f32);
        }
        cache.stage.ground(&mut d, sc, &lights, &mut bloom, &mut out);
        lap("ground");
        if !sc.live && home {
            weather(&mut d, &cx);
        }
    }
    bloom.apply(&mut cv, ((2.0 * lay.u).round() as usize).max(1));
    lap("bloom");
    (cv, out)
}

/// The colour of the sky on each row of the screen.
fn sky_rows(sc: &Scene, lay: &Layout) -> Vec<Rgb> {
    let h = lay.h as usize;
    let yh = lay.sy(0.79).max(1.0);
    let mut rows: Vec<Rgb> = (0..h)
        .map(|y| {
            let yy = y as f32 + 0.5;
            let p = if yy <= yh { 0.79 * yy / yh } else { 0.79 + 0.21 * (yy - yh) / (lay.h - yh).max(1.0) };
            let mut k = 0;
            while k < STOPS.len() - 2 && p > STOPS[k + 1] {
                k += 1;
            }
            let f = ((p - STOPS[k]) / (STOPS[k + 1] - STOPS[k])).clamp(0.0, 1.0);
            mix(sc.sky[k], sc.sky[k + 1], f)
        })
        .collect();
    // soften the joins between the stops, so the gradient has no visible steps in it
    let r = (h as f32 * 0.035) as usize;
    for _ in 0..2 {
        let src = rows.clone();
        let mut sum = [0.0f32; 3];
        let at = |i: isize| src[i.clamp(0, h as isize - 1) as usize];
        for i in -(r as isize)..=(r as isize) {
            let c = at(i);
            sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
        }
        let n = (2 * r + 1) as f32;
        for y in 0..h {
            rows[y] = [sum[0] / n, sum[1] / n, sum[2] / n];
            let (add, sub) = (at(y as isize + r as isize + 1), at(y as isize - r as isize));
            sum = [sum[0] + add[0] - sub[0], sum[1] + add[1] - sub[1], sum[2] + add[2] - sub[2]];
        }
    }
    rows
}

/// The band of the Milky Way, where it lies in the sky `eph` describes, as strongly as `vis`.
pub fn milky_way(cv: &mut Canvas, sc: &Scene, lay: &Layout, eph: &Ephemeris, vis: f32) {
    if vis < 0.05 {
        return;
    }
    let div = ((16.0 * lay.u).round() as usize).max(4);
    let mut field = Bloom::new(cv.w, cv.h, div);
    let cell = div as f32;
    let bottom = lay.horizon() + cell;
    let star = hex(sc.p.star);
    let mut y = 0.0;
    while y < bottom {
        let mut x = 0.0;
        while x < lay.w {
            let (fx, fy) = ((x + cell / 2.0) / lay.w, (y + cell / 2.0 - lay.oy) / lay.hv);
            let (ra, dec) = sky_at(eph, fx as f64, fy as f64);
            let (l, b) = galactic(ra, dec);
            let (l, b) = (l as f32, (b as f32).to_degrees());
            let band = (-(b / 8.5).powi(2)).exp();
            if band > 0.02 {
                let core = 0.38 + 0.62 * (0.5 + 0.5 * l.cos()).powf(1.6);
                let knots = 0.74 + 0.26 * (l * 11.0 + (b * 0.5).sin() * 2.0).sin() * (l * 4.3 + 1.7).cos();
                let lane = 1.0 - 0.42 * (-((b - 1.2 * (l * 3.0).sin()) / 1.9).powi(2)).exp() * (0.5 + 0.5 * (l * 2.2).sin());
                let low = smooth(lay.horizon(), lay.horizon() - 0.16 * lay.hv, y);
                let tint = mix(star, [176.0, 196.0, 240.0], 0.45 - 0.35 * (0.5 + 0.5 * l.cos()));
                field.light(x, y, x + cell, y + cell, tint, band * core * knots * lane * low * 0.105 * vis);
            }
            x += cell;
        }
        y += cell;
    }
    field.apply(cv, 1);
}

/// The widest, softest light in the picture: the glow a city throws up into its own sky (stronger
/// the more of it is awake), and the wash the sun leaves along the horizon.
fn wide_glows(cv: &mut Canvas, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let div = ((16.0 * lay.u).round() as usize).max(4);
    let k = sc.dark * (0.25 + 2.2 * sc.lit_share).min(1.0) * (1.0 - 0.5 * sc.fog);
    if k > 0.02 {
        let mut v = Veil::new(cx.cache.w, cx.cache.h, div);
        v.glow(lay.x(0.5), lay.sy(0.83), lay.w * 0.85, lay.hv * 0.25, 0.20 * k, 1);
        v.apply(cv, mix([150.0, 124.0, 104.0], sc.haze, 0.35), Blend::Screen);
    }
    let s = &sc.eph.sun;
    let ga = sc.glow_alpha / 255.0;
    let low = 1.0 - 0.75 * smooth(4.0, 26.0, s.el as f32); // the wash fades as the sun climbs
    if ga * low > 0.01 {
        let gx = lay.x(s.x as f32).clamp(0.02 * lay.w, 0.98 * lay.w);
        let mut v = Veil::new(cx.cache.w, cx.cache.h, div);
        for (rr, a) in [(0.56, 0.42), (0.30, 0.72), (0.14, 0.9)] {
            let rad = rr * lay.w;
            v.glow(gx, lay.sy(0.80), rad, rad * 0.56, (ga * a * low).min(1.0), 1);
        }
        v.apply(cv, sc.glow, Blend::Over);
    }
}

/// How the stars are to be shown.
pub struct Starlight<'a> {
    /// whose sky they are in
    pub eph: &'a Ephemeris,
    /// the faintest magnitude that shows
    pub lim: f32,
    /// how much of them the weather lets through
    pub clear: f32,
    /// seen through air: they twinkle, the planets are out, and now and then one falls
    pub air: bool,
}

fn stars(d: &mut Draw<Canvas>, cx: &Ctx, out: &mut Anchors) {
    let sc = cx.sc;
    let el = sc.eph.sun.el as f32;
    let clear = (1.0 - smooth(0.45, 1.0, sc.cover)) * (1.0 - sc.fog);
    // The faintest magnitude that shows: only planets at sunset, everything by full night.
    let lim = -4.7 + 11.1 * smooth(0.5, -16.0, el) - 1.5 * sc.moonlight - 1.2 * (sc.lit_share * 2.0).min(1.0) * 0.5;
    starfield(d, sc, &cx.lay, &cx.cache.stars, &Starlight { eph: &sc.eph, lim, clear, air: true }, out);
}

pub fn starfield(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, all: &[Star], how: &Starlight, out: &mut Anchors) {
    let u = lay.u;
    let (lim, clear) = (how.lim, how.clear);
    if clear < 0.03 || lim < -4.6 {
        return;
    }
    let star = hex(sc.p.star);
    let slot = if sc.live || !how.air { 0 } else { sc.slot as u32 };
    let tinted = |tint: f32| {
        if tint > 0.0 {
            mix(star, [255.0, 196.0, 150.0], 0.62 * tint)
        } else {
            mix(star, [178.0, 206.0, 255.0], 0.62 * -tint)
        }
    };
    let mut point = |d: &mut Draw<Canvas>, x: f32, y: f32, mag: f32, col: Rgb, height: f32, steady: bool, k: u32| {
        let vis = ((lim - mag) / 0.9).clamp(0.0, 1.0) * smooth(0.5, 9.0, height) * clear;
        let a = (1.04 - 0.125 * mag).clamp(0.16, 1.0) * vis;
        if a < 0.03 {
            return;
        }
        let r = u * (1.45 - 0.19 * mag).clamp(0.52, 2.3);
        let tw = if steady { 1.0 } else { 0.78 + 0.22 * hash2(k, slot) };
        if mag < 1.7 {
            d.glow(x, y, r * 5.0, r * 5.0, col, 0.20 * a, 2, Blend::Screen);
        }
        if mag < 0.7 {
            // the brightest few get the cross a lens gives them
            let reach = r * (6.5 - 1.6 * mag.max(-2.0));
            for (dx, dy) in [(1.0, 0.0), (0.0, 1.0)] {
                d.path_mode(&[(x - dx * reach, y - dy * reach), (x + dx * reach, y + dy * reach)], 0.9 * u, col, 0.20 * a, Blend::Screen);
            }
        }
        d.dot(x, y, r, r, col, a * tw, Blend::Screen);
        if !steady && how.air && mag < 3.4 && out.stars.len() < 110 {
            out.stars.push(Spark { x, y, r, a, col, low: 1.0 - smooth(4.0, 40.0, height) });
        }
    };
    for (k, s) in all.iter().enumerate() {
        if s.mag > lim {
            continue;
        }
        let (fx, fy, height) = star_at(how.eph, s.ra, s.dec);
        if height < 0.6 || !(-0.01..=1.01).contains(&fx) {
            continue;
        }
        let (x, y) = (lay.x(fx as f32), lay.y(fy as f32));
        if y > -3.0 {
            point(d, x, y, s.mag, tinted(s.tint), height as f32, false, k as u32);
        }
    }
    if !how.air {
        return;
    }
    for p in &how.eph.planets {
        if p.body.el > 1.0 && (p.mag as f32) < lim && p.elong > 9.0 {
            let col = match p.name {
                "Mars" => [255.0, 170.0, 130.0],
                "Saturn" => [246.0, 226.0, 180.0],
                "Jupiter" => [255.0, 240.0, 214.0],
                _ => [255.0, 252.0, 240.0],
            };
            point(d, lay.x(p.body.x as f32), lay.y(p.body.y as f32), p.mag as f32, col, p.body.el as f32, true, 0);
        }
    }
    if sc.night > 0.6 && sc.slot % 23 == 7 && !sc.live {
        // now and then, a shooting star
        let (x, y) = (lay.x(0.2 + 0.6 * hash2(slot, 1)), lay.sy(0.10 + 0.24 * hash2(slot, 2)));
        for k in 0..12 {
            let f = k as f32 / 12.0;
            d.path_mode(
                &[(x + f * 90.0 * u, y + f * 34.0 * u), (x + (f + 0.09) * 90.0 * u, y + (f + 0.09) * 34.0 * u)],
                1.3 * u,
                star,
                0.78 * f,
                Blend::Screen,
            );
        }
    }
}

fn moon(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let m = &sc.eph.moon;
    if m.body.el < -3.0 {
        return;
    }
    let (x, y, r) = (lay.x(m.body.x as f32), lay.y(m.body.y as f32), 15.0 * lay.u);
    let vis = (0.32 + 0.68 * (1.0 - sc.day)) * (1.0 - 0.75 * smooth(0.5, 0.95, sc.cover)) * (1.0 - 0.8 * sc.fog);
    if vis < 0.05 {
        return;
    }
    let col = hex(sc.p.moon);
    let lit = m.lit as f32;
    let dusk = 1.0 - sc.day;
    d.glow(x, y, r * 7.5, r * 7.5, col, 0.17 * lit * vis * dusk, 2, Blend::Screen);
    d.glow(x, y, r * 2.8, r * 2.8, col, 0.30 * lit * vis * (0.3 + 0.7 * dusk), 2, Blend::Screen);
    if (0.2..0.65).contains(&sc.cover) && lit > 0.5 && sc.night > 0.3 {
        // thin high cloud puts a ring round a bright moon
        let ring: Vec<P> = (0..=72).map(|i| i as f32 / 72.0 * 2.0 * PI).map(|a| (x + a.cos() * r * 6.6, y + a.sin() * r * 6.6)).collect();
        d.path_mode(&ring, 3.0 * lay.u, col, 0.07 * lit * vis, Blend::Screen);
    }
    // The bright limb faces the sun, wherever the sun is, so the crescent tilts as the real one does.
    let Some(face) = lit_disc(d, (x, y), r, col, lit, (lay.x(sc.eph.sun.x as f32), lay.y(sc.eph.sun.y as f32)), vis) else { return };
    // the seas
    if d.sh.begin(x - r, y - r, x + r, y + r, d.s.w, d.s.h) {
        for (mx, my, rx, ry) in [(-0.34, -0.30, 0.30, 0.24), (0.14, -0.42, 0.24, 0.17), (0.34, 0.06, 0.24, 0.26), (-0.20, 0.30, 0.26, 0.17), (0.0, -0.04, 0.15, 0.13)] {
            d.sh.ellipse(x + mx * r, y + my * r, rx * r, ry * r);
        }
        let mut seas = d.sh.take();
        seas.clip(&face);
        let sea = mix(col, sc.sky[2], 0.55);
        seas.paint(d.s, Blend::Over, |_| (sea, 0.26 * vis));
    }
}

/// A moon: a disc of which the fraction `lit` is bright, on the side `sun` lies. The dark part
/// shows faintly. Returns the bright part, for marking.
pub fn lit_disc(d: &mut Draw<Canvas>, (x, y): P, r: f32, col: Rgb, lit: f32, sun: P, vis: f32) -> Option<Patch> {
    d.circle(x, y, r, col, 0.10 * vis); // earthshine
    if lit <= 0.02 {
        return None;
    }
    let turn = (sun.1 - y).atan2(sun.0 - x);
    let (ct, st) = (turn.cos(), turn.sin());
    let place = |lx: f32, ly: f32| (x + (lx * ct - ly * st) * r, y + (lx * st + ly * ct) * r);
    let mut pts: Vec<P> = Vec::with_capacity(82);
    for i in 0..=40 {
        let a = -PI / 2.0 + PI * i as f32 / 40.0;
        pts.push(place(a.cos(), a.sin()));
    }
    for i in 0..=40 {
        let a = PI / 2.0 - PI * i as f32 / 40.0;
        pts.push(place(a.cos() * (1.0 - 2.0 * lit), a.sin()));
    }
    if !d.sh.begin(x - r - 1.0, y - r - 1.0, x + r + 1.0, y + r + 1.0, d.s.w, d.s.h) {
        return None;
    }
    d.sh.poly(&pts);
    let face = d.sh.take();
    face.paint(d.s, Blend::Over, |_| (col, vis));
    Some(face)
}

fn sun(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let s = &sc.eph.sun;
    let el = s.el as f32;
    let (x, y) = (lay.x(s.x as f32), lay.y(s.y as f32));
    let ga = sc.glow_alpha / 255.0;
    if el > -1.5 && sc.sun_vis > 0.02 {
        let (up, r) = (smooth(-1.5, 5.0, el), 21.0 * lay.u);
        for (m, a) in [(11.0, 0.26), (4.6, 0.50)] {
            d.glow(x, y, r * m, r * m, sc.glow, (ga * a * up * 1.5 * sc.sun_vis).min(1.0), 2, Blend::Over);
        }
        let squash = 1.0 - 0.10 * smooth(7.0, 0.0, el);
        d.glow(x, y, r * 1.9, r * 1.9 * squash, mix(sc.sun_col, WHITE, 0.3), 0.55 * sc.sun_vis * up, 3, Blend::Over);
        d.ellipse(x, y, r, r * squash, sc.sun_col, sc.sun_vis);
        d.glow(x, y, r * 0.95, r * 0.95 * squash, mix(sc.sun_col, WHITE, 0.6), 0.40 * sc.sun_vis, 1, Blend::Over);
    }
}

fn clouds(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let mut r = PyRng::new(cx.cache.seed + 5);
    let mut bands: Vec<[f64; 4]> = cloud_bands(cx.cache.seed);
    for _ in 0..12 {
        bands.push([r.uniform(-0.3, 0.9), r.uniform(0.12, 0.52), r.uniform(0.3, 0.6), r.uniform(0.035, 0.08)]);
    }
    let sun_x = sc.eph.sun.x as f32;
    let near_sun = |fx: f32| (1.0 - (fx - sun_x).abs() / 0.6).max(0.0);
    let drift = sc.drift;

    // long soft bands of high cloud
    let n = ((0.4 + sc.cover * 14.0).round() as usize).min(bands.len());
    let band_col = mix(sc.cloud, sc.sky[3], 0.35);
    let band_a = 0.13 + 0.40 * sc.cover;
    let thick = smooth(0.25, 0.9, sc.cover);
    for (k, b) in bands.iter().take(n).enumerate() {
        let (bx, by, bw, bh) = (b[0] as f32, b[1] as f32, b[2] as f32, b[3] as f32);
        let x = ((bx + drift * (0.75 + 0.06 * (k % 5) as f32) + 0.6).rem_euclid(1.7) - 0.6) * lay.w;
        let (pw, ph) = (bw * lay.w, bh * lay.hv);
        let (mx, my) = (x + pw / 2.0, lay.sy(by) + ph / 2.0);
        d.puff(mx, my, pw * 0.66, ph * (0.22 + 0.30 * thick) + (12.0 + 30.0 * thick) * u, band_col, band_a, 0.2);
        let lit = 0.6 * sc.twilight * near_sun(mx / lay.w) * sc.sun_vis;
        if lit > 0.03 {
            // the low sun catches their undersides
            d.puff(mx, my + ph * 0.3, pw * 0.52, ph * (0.20 + 0.24 * thick) + (9.0 + 18.0 * thick) * u, sc.glow, band_a * lit, 0.2);
        }
    }

    // rain on the way: a bank builds on the side the wind comes from
    if sc.front > 0.05 {
        let side = if sc.wind_x >= 0.0 { 0.0 } else { 1.0 };
        let col = mix(sc.cloud, hex(sc.p.storm), 0.55);
        for k in 0..6 {
            let kf = k as f32;
            let x = lay.x(side + (if side == 0.0 { 1.0 } else { -1.0 }) * (0.02 + 0.075 * kf));
            let y = lay.sy(0.70 - 0.035 * (kf * 1.7).sin().abs() - 0.02 * (5.0 - kf) * sc.front);
            d.puff(x, y, (190.0 - 16.0 * kf) * u, (95.0 - 9.0 * kf) * u * (0.6 + 0.4 * sc.front), col, 0.62 * sc.front, 0.35);
        }
    }

    // heaps of cumulus, drawn as shapes: lit from the side the sun or moon is on
    // under a full deck there are no separate heaps to see
    let heaps = if sc.cover > 0.10 { (sc.cover * 11.0 * (1.0 - smooth(0.55, 0.92, sc.cover))).round() as usize } else { 0 };
    let alpha = sc.cloud_alpha / 255.0 * (0.80 + 0.20 * sc.cover);
    let moon_x = sc.eph.moon.body.x as f32;
    let light_x = if sc.day > 0.15 { sun_x } else { moon_x };
    let under = 0.55 * sc.twilight * sc.sun_vis;
    for _ in 0..heaps {
        let (px0, fy, size, lobes) = (r.uniform(0.0, 1.5) as f32, r.uniform(0.30, 0.61) as f32, r.uniform(80.0, 185.0) as f32, 5 + r.below(3) as usize);
        let wobble: Vec<f32> = (0..lobes).map(|_| r.uniform(0.84, 1.16) as f32).collect();
        let near = 1.28 - 0.70 * (fy - 0.30) / 0.31; // clouds low in the sky are far away
        let pw = size * near * u * (1.0 + 0.35 * smooth(0.6, 1.0, sc.cover));
        let x = ((px0 + drift * 1.2 * near + 0.25).rem_euclid(1.5) - 0.25) * lay.w;
        let base = lay.sy(fy);
        if x + pw < 0.0 || x - pw > lay.w {
            continue;
        }
        let side = if light_x * lay.w < x { -1.0 } else { 1.0 };
        let shape = |sh: &mut Shaper, shift: P, grow: f32| {
            for (j, wob) in wobble.iter().enumerate() {
                let f = j as f32 / (lobes - 1) as f32;
                let lr = pw * (0.19 + 0.17 * (f * PI).sin()) * wob;
                sh.ellipse(x + (f - 0.5) * pw * 1.45 + shift.0 * lr, base - lr * 0.50 + shift.1 * lr, lr * grow, lr * grow);
            }
            for e in [-1.0f32, 1.0] {
                let lr = pw * 0.13;
                sh.ellipse(x + e * pw * 0.80 + shift.0 * lr, base - lr * 0.86 + shift.1 * lr, lr * grow, lr * grow);
            }
        };
        let top = base - pw * 0.72;
        if !d.sh.begin(x - pw * 1.05, top, x + pw * 1.05, base - 1.0, d.s.w, d.s.h) {
            continue;
        }
        shape(d.sh, (0.0, 0.0), 1.0);
        let body = d.sh.take();
        // the flat base dissolves rather than ending on a ruled line
        let soft = |y: f32| smooth(base - 1.0, base - (base - top) * 0.16, y);
        let dim = shade(sc.cloud, -0.07);
        body.paint(d.s, Blend::Over, |y| {
            let t = smooth(top + (base - top) * 0.35, base, y);
            (mix(mix(sc.cloud, dim, t), sc.glow, under * t * near_sun(x / lay.w)), alpha * soft(y))
        });
        if d.sh.begin(x - pw * 1.05, top, x + pw * 1.05, base - 1.0, d.s.w, d.s.h) {
            shape(d.sh, (side * 0.22, -0.40), 0.80);
            let mut bright = d.sh.take();
            bright.clip(&body);
            // with no sun or moon on them there is nothing to pick the tops out
            let shine = 0.22 + 0.78 * sc.day.max(sc.moonlight).max(sc.twilight);
            bright.paint(d.s, Blend::Over, |y| (sc.cloud_lit, alpha * 0.9 * shine * soft(y)));
        }
    }
}

/// The evening or morning flock, caught in one frame.
fn flock(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let col = mix(hex(sc.p.bird), sc.sky[3], 0.25);
    for b in &sc.birds {
        for k in -4i32..4 {
            let bx = lay.x(b[0]) - b[2] * k.abs() as f32 * 15.0 * u + (k.rem_euclid(2)) as f32 * 3.0 * u;
            let by = lay.sy(b[1]) + k as f32 * 8.0 * u;
            let lift = -0.3 + 0.55 * ((sc.slot + k as i64).rem_euclid(3)) as f32;
            props::bird(d, bx, by, 17.0 * u, lift, col, 0.86);
        }
    }
}

fn fireworks(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let mut r = PyRng::new(sc.slot as u64);
    let n = 3 + (sc.slot % 2) as usize;
    for col in [AMBER, ROSE, BLUE, SAGE].into_iter().take(n) {
        let (fx, fy, rad) = (r.uniform(0.2, 0.8) as f32, r.uniform(0.22, 0.5) as f32, r.uniform(50.0, 95.0) as f32 * u);
        let (x, y) = (lay.x(fx), lay.sy(fy));
        d.glow(x, y, rad * 1.8, rad * 1.8, col, 0.30, 2, Blend::Screen);
        for k in 0..22 {
            let a = k as f32 / 22.0 * 2.0 * PI + r.uniform(-0.08, 0.08) as f32;
            let (x1, y1) = (x + a.cos() * rad, y + a.sin() * rad + rad * 0.12);
            d.path_mode(&[(x + a.cos() * rad * 0.35, y + a.sin() * rad * 0.35), (x1, y1)], 1.2 * u, col, 0.67, Blend::Screen);
            d.dot(x1, y1, 1.8 * u, 1.8 * u, mix(col, WHITE, 0.5), 0.94, Blend::Screen);
        }
    }
}

fn lightning(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let mut r = PyRng::new((sc.slot * 3 + 1) as u64);
    if r.random() < 0.35 {
        return;
    }
    let (mut x, mut y) = (lay.x(r.uniform(0.15, 0.85) as f32), lay.sy(0.20));
    d.glow(x, lay.sy(0.30), 0.60 * lay.w, 0.50 * lay.hv, [214.0, 224.0, 240.0], 0.16, 2, Blend::Screen);
    let mut pts = vec![(x, y)];
    while y < lay.sy(0.80) {
        x += r.uniform(-34.0, 34.0) as f32 * u;
        y += r.uniform(22.0, 60.0) as f32 * u;
        pts.push((x, y));
    }
    d.path_mode(&pts, 5.0 * u, [190.0, 205.0, 240.0], 0.22, Blend::Screen);
    d.path(&pts, 2.0 * u, [236.0, 240.0, 250.0], 0.92);
    let fork: Vec<P> = pts[pts.len() / 2..(pts.len() / 2 + 4).min(pts.len())].iter().enumerate().map(|(i, p)| (p.0 + i as f32 * 16.0 * u, p.1)).collect();
    d.path(&fork, 1.2 * u, [236.0, 240.0, 250.0], 0.6);
}

/// The first five bands of high cloud keep the places the first city gave them.
fn cloud_bands(seed: u64) -> Vec<[f64; 4]> {
    let mut r = PyRng::new(seed);
    (0..5).map(|_| [r.uniform(-0.1, 0.55), r.uniform(0.10, 0.42), r.uniform(0.25, 0.5), r.uniform(0.03, 0.06)]).collect()
}

/// A band of something lying across the whole width, soft at its top and bottom edges.
pub fn band(cv: &mut Canvas, top: f32, bottom: f32, soft: f32, col: Rgb, a: f32) {
    if a > 0.004 {
        cv.wash((top - soft * 1.7) as i32, (bottom + soft * 1.7) as i32, col, |y| {
            a * smooth(top - soft * 1.7, top + soft * 1.7, y) * smooth(bottom + soft * 1.7, bottom - soft * 1.7, y)
        });
    }
}

/// Where the light falls from: its place across the picture, its colour, and how strong it is.
pub struct Light {
    pub x: f32,
    pub col: Rgb,
    pub k: f32,
}

impl Light {
    /// The low sun while it is up; the moon after that.
    pub fn of(sc: &Scene, lay: &Layout) -> Light {
        let low_sun = sc.sun_vis * smooth(-5.0, 1.5, sc.eph.sun.el as f32) * smooth(36.0, 6.0, sc.eph.sun.el as f32);
        if low_sun > 0.03 || sc.day > 0.5 {
            Light { x: lay.x(sc.eph.sun.x as f32), col: mix(sc.glow, WHITE, 0.15), k: 0.10 * sc.day * sc.sun_vis + 0.62 * low_sun }
        } else {
            Light { x: lay.x(sc.eph.moon.body.x as f32), col: hex(sc.p.moon), k: 0.26 * sc.moonlight * (1.0 - sc.day) }
        }
    }
}

/// Rain and snow caught in one frame.
fn weather(d: &mut Draw<Canvas>, cx: &Ctx) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let mut r = PyRng::new((sc.slot * 31 + 7) as u64);
    let area = (lay.w * lay.h) / (1920.0 * 1200.0);
    if sc.rain > 0.0 {
        let lean = sc.wind_x * 0.45;
        let vis = 0.55 + 0.45 * sc.day;
        let col = hex(sc.p.rain);
        for _ in 0..(sc.rain * 1700.0 * area) as usize {
            let (x, y) = (lay.w * r.uniform(-0.2, 1.2) as f32, lay.h * r.uniform(-0.05, 1.0) as f32);
            let len = r.uniform(16.0, 44.0) as f32 * u * (0.6 + 0.5 * sc.rain);
            let a = r.uniform(28.0, 82.0) as f32 / 255.0 * vis;
            d.path(&[(x, y), (x + lean * len, y + len)], 1.15 * u, col, a);
        }
    }
    if sc.snow > 0.0 {
        let col = hex(sc.p.snow);
        for _ in 0..(sc.snow * 900.0 * area) as usize {
            let (x, y) = (lay.w * r.random() as f32, lay.h * r.random() as f32);
            let rr = r.pick(&[1.0f32, 1.3, 1.8, 2.6]) * u;
            d.dot(x, y, rr, rr, col, r.uniform(110.0, 230.0) as f32 / 255.0, Blend::Over);
        }
    }
}
