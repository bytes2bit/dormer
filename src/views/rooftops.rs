//! Rooftops: the old town, seen from a window in the roof.
//!
//! Five rows of roofs with haze between them, a clock tower that keeps the
//! real time, a crane that works on working days, and one roof room that
//! stands for this machine. The streets come from `town.rs`; the landmarks
//! are placed by hand.

use std::f32::consts::PI;

use crate::color::{hex, mix, shade, smooth, Rgb, AMBER, BEACON, BLUE, SAGE, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, P};
use crate::paint::{band, Light};
use crate::props::{self, Layout};
use crate::rng::{hash2, PyRng};
use crate::scene::Scene;
use crate::town::{window_state, Kind, Layer, Roof, Town};
use crate::views::{lamp_colours, yours, Anchors, Blink, Breath, Lights, Smoke, Stage, View, Yours};

pub struct Rooftops;

impl View for Rooftops {
    fn id(&self) -> &'static str {
        "rooftops"
    }

    fn about(&self) -> &'static str {
        "the old town, from a window in the roof"
    }

    fn stage(&self, seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Roofs::new(seed, w, h, self.horizon()))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let week = [
            "the city starts early: more lamps before sunrise, more planes, the crane at work",
            "an ordinary working day",
            "an ordinary working day",
            "an ordinary working day",
            "offices empty early; string lights go up on a roof after dark",
            "no office lights; washing out by day, string lights and late windows at night",
            "the city sleeps in, hangs the washing out, and goes to bed early",
        ][sc.dow];
        let mut doing = vec![format!("{}% of windows lit", (sc.lit_share * 100.0).round())];
        for (flag, text) in [
            (sc.sleepers > 0, "people still asleep"),
            (sc.laundry > 0, "washing on the line"),
            (sc.kite, "a kite up"),
            (sc.balloon, "a balloon drifting over"),
            (sc.festoon, "string lights on"),
            (sc.crane_works, "the crane working"),
            (!sc.planes.is_empty(), "a plane crossing"),
            (!sc.birds.is_empty(), "birds crossing"),
            (sc.smoke > 0.05, "chimneys smoking"),
            (sc.traffic * sc.dark > 0.3, "rush-hour glow in the street"),
            (sc.mist > 0.25, "mist between the roofs"),
            (sc.frost > 0.3, "frost on the roofs"),
        ] {
            if flag {
                doing.push(text.to_string());
            }
        }
        if sc.cat_out {
            doing.push(if sc.cat_asleep { "the cat asleep at home" } else { "the cat out on a roof" }.to_string());
        }
        let house = Yours { lamp: "lamp", containers: ("container window", "container windows"), smoke: "the stovepipe smoking" };
        vec![("week", week.to_string()), ("city", doing.join(", ")), ("yours", yours(sc, &house))]
    }
}

const REF_FLOOR: f64 = 4000.0;
const CRANE_X: f32 = 0.795;
const CRANE_FOOT: f32 = 0.87;
const CRANE_TOP: f32 = 0.655;
const DOME_X: f32 = 0.475;
const DOME_TOP: f32 = 0.730;

/// The old town laid out for one screen.
struct Roofs {
    town: Town,
    w: usize,
    lay: Layout,
    /// each row of roofs as a mask: first row, then one byte a pixel
    masks: Vec<(usize, Vec<u8>)>,
    depth: Depth,
}

impl Roofs {
    fn new(seed: u64, w: usize, h: usize, horizon: f64) -> Roofs {
        let town = Town::build(seed);
        let lay = Layout::new(w, h, horizon);
        let mut sh = Shaper::new();
        let mut masks = Vec::new();
        for layer in &town.layers {
            let pts: Vec<P> = layer.poly.iter().map(|p| (lay.cx(p.0), lay.cy(p.1.min(REF_FLOOR)))).collect();
            let top = pts.iter().map(|p| p.1).fold(f32::MAX, f32::min).min(lay.y(DOME_TOP) - 40.0 * lay.u);
            if sh.begin(0.0, top - 2.0, w as f32, h as f32, w, h) {
                sh.poly(&pts);
                for c in &layer.chimneys {
                    let (x0, y0, x1, y1) = (lay.cx(c[0]), lay.cy(c[1]), lay.cx(c[2]), lay.cy(c[3]));
                    sh.poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
                }
                if masks.len() == 1 {
                    // the domed hall stands in the second row
                    let (x, top, u) = (lay.x(DOME_X), lay.y(DOME_TOP), lay.u);
                    sh.poly(&[(x - 24.0 * u, top), (x + 24.0 * u, top), (x + 24.0 * u, lay.y(0.86)), (x - 24.0 * u, lay.y(0.86))]);
                    sh.ellipse(x, top, 21.0 * u, 19.0 * u);
                    sh.poly(&[(x - 3.2 * u, top - 28.0 * u), (x + 3.2 * u, top - 28.0 * u), (x + 3.2 * u, top - 16.0 * u), (x - 3.2 * u, top - 16.0 * u)]);
                }
                masks.push(sh.mask(w));
            } else {
                masks.push((h, Vec::new()));
            }
        }
        let depth = Depth::stack(&masks, w, h);
        Roofs { town, w, lay, masks, depth }
    }
}

struct Ctx<'a> {
    sc: &'a Scene,
    city: &'a Town,
    st: &'a Roofs,
    lay: Layout,
}

impl Stage for Roofs {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        let (mut lit, mut total) = (0usize, 0usize);
        let mut on = Vec::new();
        for (i, l) in self.town.layers.iter().enumerate() {
            for win in &l.windows {
                if i == 3 && win.b == self.town.house.0 {
                    on.push(None);
                    continue;
                }
                total += 1;
                let s = window_state(&win.who, win.office, win.tone, sc.t, if win.office { sc.dow } else { sc.home_dow }, sc.el_eff as f64);
                lit += s.is_some() as usize;
                on.push(s);
            }
        }
        Lights { share: lit as f32 / total.max(1) as f32, on }
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        let (city, lay) = (&self.town, self.lay);
        let cx = Ctx { sc, city, st: self, lay };
        if sc.kite && !sc.live {
            props::kite(d, sc, lay.cx(city.kite.0), lay.cy(city.kite.1), lay.u, (sc.ts / 420.0) as f32, (sc.ts / 700.0) as f32, (sc.ts / 300.0) as f32);
        }
        ridge(d, &cx, bloom);
        let mut at = 0;
        for i in 0..city.layers.len() {
            let n = city.layers[i].windows.len();
            row_of_roofs(d, &cx, i, &lights.on[at..at + n], bloom, out);
            at += n;
        }
        trees(d, &cx, bloom, out);
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let (city, lay) = (&self.town, &self.lay);
        let u = lay.u;
        if sc.kite {
            d.s.behind = Some(3);
            props::kite(d, sc, lay.cx(city.kite.0), lay.cy(city.kite.1), u, t * 0.35, t * 0.23, t * 3.0);
        }
        d.s.behind = Some(2);
        props::crane_jib(d, lay.x(CRANE_X), lay.y(CRANE_TOP), u, sc.layers[1], props::crane_pose(sc, now));
        for (li, apex) in city.sleepers.iter().take(sc.sleepers) {
            d.s.behind = Some(li + 1);
            props::sleep_z(d, sc, lay.cx(apex.0) + 6.0 * u, lay.cy(apex.1) - 12.0 * u, u, t * 0.8);
        }
        d.s.behind = Some(4);
        for (i, flat) in city.laundry.iter().take(sc.laundry).enumerate() {
            props::laundry(d, sc, lay.cx(flat[0]), lay.cx(flat[1]), lay.cy(flat[2]), u, sc.layers[3], i, t * 1.5);
        }
        let (mx, foot, top) = mast(city, lay);
        props::pennant(d, sc, mx, foot, top, u, sc.layers[3], t * (2.0 + 3.0 * sc.wind_x.abs().min(1.5)));
        if sc.cat_out && !sc.cat_asleep {
            let (cl, x, y, cu, face) = cat_place(sc, city, lay);
            d.s.behind = Some(cl + 1);
            // the tail swings for a while, then rests
            let tail = if (t * 0.11).sin() > -0.3 { t * 1.4 } else { 0.0 };
            props::cat(d, x, y, cu, sc.layers[cl], false, face, tail);
        }
        d.s.behind = None;
    }

    fn lively(&self, sc: &Scene, _now: f64) -> bool {
        sc.kite
    }
}

/// The roof room that stands for this machine: centre, roof height, width and depth of the room.
fn house_box(city: &Town, lay: &Layout) -> (f32, f32, f32, f32) {
    let f = city.house.1;
    ((lay.cx(f[0]) + lay.cx(f[1])) / 2.0, lay.cy(f[2]), 46.0 * lay.u, 30.0 * lay.u)
}

/// The mast on the roof room: where it stands, its foot and its top.
fn mast(city: &Town, lay: &Layout) -> (f32, f32, f32) {
    let (cx, y, bw, bh) = house_box(city, lay);
    (cx + bw / 2.0 - 5.0 * lay.u, y - bh, y - bh - 46.0 * lay.u)
}

/// Where the cat is this hour and a half: (row of roofs, x, y, scale, facing).
fn cat_place(sc: &Scene, city: &Town, lay: &Layout) -> (usize, f32, f32, f32, f32) {
    let mut r = PyRng::new((sc.day_id * 7 + (sc.t / 1.5).floor() as i64) as u64);
    let (layer, flat) = r.pick(&city.cat_spots);
    let (at, face) = (r.uniform(0.25, 0.75), r.pick(&[-1.0f32, 1.0]));
    let x = lay.cx(flat[0] + (flat[1] - flat[0]) * at);
    (layer, x, lay.cy(flat[2]), lay.u * (0.85 + 0.12 * layer as f32), face)
}

fn row_of_roofs(d: &mut Draw<Canvas>, cx: &Ctx, i: usize, lit: &[Option<Rgb>], bloom: &mut Bloom, out: &mut Anchors) {
    let (sc, lay, city) = (cx.sc, &cx.lay, cx.city);
    let layer = &city.layers[i];
    let (u, s) = (lay.u, lay.s);
    let col = sc.layers[i];
    let p = sc.p;
    let top = lay.y((layer.base - layer.hi) as f32);
    let base = lay.y(layer.base as f32);

    // what hangs in the air between this row and the one behind it
    band(d.s, top, base + 0.10 * lay.hv, 52.0 * u, sc.haze, sc.haze_alpha[i] / 255.0);
    if sc.fog > 0.0 && i < 4 {
        band(d.s, top - 0.10 * lay.hv, lay.h * 1.3, 60.0 * u, mix(sc.sky[4], sc.haze, 0.5), 0.47 * sc.fog);
    }
    if sc.mist > 0.02 && (1..4).contains(&i) {
        band(d.s, top + 0.012 * lay.hv, lay.h * 1.3, 26.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.30 * sc.mist);
    }
    if i == 4 && sc.traffic * sc.dark > 0.05 {
        // the street between the last two rows of roofs
        band(d.s, lay.y(0.905), lay.y(1.02), 22.0 * u, WARM[3], (0.33 * sc.traffic * sc.dark).min(0.37));
    }

    let (my0, mask) = &cx.st.masks[i];
    if sc.snow > 0.3 {
        // snow lies on every upward edge
        let snow = mix(col, hex(p.snow), 0.85);
        d.s.stamp(mask, *my0 as i32, -((3.0 + i as f32) * u).round() as i32, |_| (snow, 1.0));
    } else if sc.frost > 0.05 {
        let rime = mix(col, hex(p.snow), 0.55);
        d.s.stamp(mask, *my0 as i32, -(1.3 * u).round().max(1.0) as i32, |_| (rime, 0.75 * sc.frost));
    }
    d.s.stamp(mask, *my0 as i32, 0, |_| (col, 1.0));
    bloom.hide(mask, cx.st.w, *my0); // light from the rows behind does not shine through this one

    let light = Light::of(sc, lay);
    let coats: Vec<Coat> = (0..layer.buildings.len()).map(|bi| coat(sc, i, bi, col)).collect();
    roofs(d, cx, layer, i, col, &light, &coats);
    if i == 1 {
        let (crane_x, crane_top) = (lay.x(CRANE_X), lay.y(CRANE_TOP));
        props::crane_mast(d, crane_x, lay.y(CRANE_FOOT), crane_top, u, col);
        if !sc.live {
            props::crane_jib(d, crane_x, crane_top, u, col, props::crane_pose(sc, sc.ts));
        }
        let apex = (crane_x, crane_top - 30.0 * u);
        if sc.dark > 0.3 {
            d.glow(apex.0, apex.1, 9.0 * u, 9.0 * u, BEACON, 0.6 * sc.dark, 2, Blend::Screen);
            d.circle(apex.0, apex.1, 2.2 * u, BEACON, 1.0);
            out.blink.push(Blink { x: apex.0, y: apex.1, r: 15.0 * u, col: BEACON, every: 2.86, phase: 0.0, behind: Some(2) });
        }
        dome(d, cx, mix(sc.layers[1], sc.layers[2], 0.30), &light, bloom);
        clock_tower(d, cx, mix(sc.layers[1], sc.layers[2], 0.45), &light, bloom, out);
    }
    furniture(d, cx, layer, i, col, bloom);

    // windows
    let (ww, wh) = (layer.ww as f32 * s, layer.wh as f32 * s);
    let glass = mix(col, sc.sky[5], 0.6);
    let glass_a = 0.92 * smooth(-5.0, 4.0, sc.el_eff);
    let power = (0.30 + 0.70 * sc.dark) * p.bloom;
    let frame = shade(col, -0.25);
    // by day every window shows, as a darker pane in the wall; the far rows keep only their lit ones
    let pane_a = [0.0, 0.0, 0.32, 0.46, 0.46][i] * smooth(-6.0, 5.0, sc.el_eff);
    for (win, state) in layer.windows.iter().zip(lit) {
        if i == 3 && win.b == city.house.0 {
            continue;
        }
        let (x, y) = (lay.cx(win.x), lay.cy(win.y));
        if x > lay.w || x + ww < 0.0 || y > lay.h {
            continue;
        }
        let coat = &coats[win.b];
        if i >= 3 {
            // a sill under it, and on some houses a shutter folded back to each side
            d.rect(x - ww * 0.12, y + wh, x + ww * 1.12, y + wh + (1.0 * u).max(0.8), shade(coat.wall, 0.10), 0.5);
            if let (Some(tint), false) = (coat.shutters, win.office) {
                d.rect(x - ww * 0.34, y, x - ww * 0.07, y + wh, tint, 0.8);
                d.rect(x + ww * 1.07, y, x + ww * 1.34, y + wh, tint, 0.8);
            }
        }
        if let Some(g) = *state {
            d.rect(x, y, x + ww, y + wh, g, 0.94);
            let dimmer = mix(g, col, 0.5);
            match win.look {
                1 => d.rect(x, y, x + ww * 0.36, y + wh, dimmer, 0.85),
                2 => d.rect(x + ww * 0.64, y, x + ww, y + wh, dimmer, 0.85),
                3 => d.rect(x, y, x + ww, y + wh * 0.42, dimmer, 0.8),
                4 => {
                    d.circle(x + ww * 0.5, y + wh * 0.50, ww * 0.20, frame, 0.9);
                    d.rect(x + ww * 0.22, y + wh * 0.66, x + ww * 0.78, y + wh, frame, 0.9);
                }
                5 => {
                    d.ellipse(x + ww * 0.72, y + wh * 0.74, ww * 0.24, wh * 0.16, mix(frame, SAGE, 0.35), 0.9);
                    d.rect(x + ww * 0.62, y + wh * 0.86, x + ww * 0.82, y + wh, frame, 0.9);
                }
                _ => {}
            }
            if i >= 3 {
                let t = (0.9 * u).max(0.8);
                d.rect(x + ww * 0.5 - t / 2.0, y, x + ww * 0.5 + t / 2.0, y + wh, frame, 0.55);
                d.rect(x, y + wh * 0.42 - t / 2.0, x + ww, y + wh * 0.42 + t / 2.0, frame, 0.55);
            }
            bloom.light(x - ww * 0.15, y - ww * 0.15, x + ww * 1.15, y + wh + ww * 0.15, mix(g, WARM[1], 0.5), 0.74 * power);
            if i >= 2 && !win.office && (win.who[2] * 13.7) % 1.0 < 0.07 && out.flicker.len() < 14 {
                out.flicker.push(([x, y, ww, wh], i + 1));
            }
        } else if win.glass && glass_a > 0.03 {
            d.rect(x, y, x + ww, y + wh, glass, glass_a);
            if i >= 3 {
                d.poly(&[(x, y), (x + ww, y), (x, y + wh * 0.75)], mix(glass, sc.sky[6], 0.5), glass_a * 0.45);
            }
        } else if pane_a > 0.03 {
            d.rect(x, y, x + ww, y + wh, shade(coat.wall, -0.30), pane_a);
        }
    }

    // the things that live on this row of roofs
    for (k, (li, c)) in city.smokers.iter().enumerate() {
        if *li == i && city.smoke_rank[k] < sc.smoke as f64 {
            let tone = mix(col, sc.sky[5], 0.72);
            let (x, y, scale) = (lay.cx((c[0] + c[2]) / 2.0), lay.cy(c[1]), 0.7 + 0.15 * i as f32);
            out.smoke.push(Smoke { x, y, scale, col: tone, behind: i + 1 });
            if !sc.live {
                props::smoke_still(d, sc, x, y, u * scale, tone, c[0] as f32);
            }
        }
    }
    for (li, apex) in city.sleepers.iter().take(sc.sleepers) {
        if *li == i && !sc.live {
            props::sleep_z(d, sc, lay.cx(apex.0) + 6.0 * u, lay.cy(apex.1) - 12.0 * u, u, (sc.ts / 400.0) as f32);
        }
    }
    if i == 3 {
        if !sc.live {
            for (k, flat) in city.laundry.iter().take(sc.laundry).enumerate() {
                props::laundry(d, sc, lay.cx(flat[0]), lay.cx(flat[1]), lay.cy(flat[2]), u, col, k, (sc.ts / 240.0) as f32);
            }
        }
        if sc.festoon {
            let f = &city.festoon;
            for b in props::festoon(d, lay.cx(f[0]), lay.cx(f[1]), lay.cy(f[2]), u, col) {
                bloom.light(b.0 - 3.0 * u, b.1 - 3.0 * u, b.0 + 3.0 * u, b.1 + 3.0 * u, b.2, 0.5);
                out.twinkle.push((b.0, b.1, b.2, 4));
            }
        }
        house(d, cx, col, bloom, out);
        if sc.cat_out && sc.cat_asleep {
            // it sleeps at home, beside the roof room
            let (hx, hy, hw, _) = house_box(city, lay);
            props::cat(d, hx - hw / 2.0 - 20.0 * u, hy, u, col, true, 1.0, 0.0);
        }
    }
    if sc.cat_out && !sc.cat_asleep && !sc.live {
        let (cl, x, y, cu, face) = cat_place(sc, city, lay);
        if cl == i {
            props::cat(d, x, y, cu, col, false, face, (sc.ts / 37.0) as f32);
        }
    }
}

/// The colours walls are washed in by day. At night they all sink back into the dark.
const WALLS: [Rgb; 6] = [[230.0, 223.0, 207.0], [222.0, 184.0, 166.0], [176.0, 198.0, 178.0], [226.0, 200.0, 152.0], [172.0, 192.0, 212.0], [236.0, 214.0, 196.0]];
const TILES: [Rgb; 3] = [[176.0, 106.0, 84.0], [122.0, 118.0, 138.0], [150.0, 96.0, 86.0]];
const SHUTTERS: [Rgb; 4] = [SAGE, BLUE, [206.0, 122.0, 108.0], [226.0, 214.0, 190.0]];

/// What one building is painted in.
struct Coat {
    wall: Rgb,
    slate: Rgb,
    /// its roof slopes towards us, rather than showing a gable end
    hip: bool,
    shutters: Option<Rgb>,
}

fn coat(sc: &Scene, i: usize, bi: usize, col: Rgb) -> Coat {
    let k = (i * 131 + bi) as u32;
    // colour comes with the daylight, and thins with distance
    let warm = 0.22 + 0.78 * sc.day;
    let pick = |n: usize, salt: u32| (hash2(k, salt) * n as f32) as usize % n;
    let tone = (hash2(k, 5) - 0.5) * 2.0 * (0.028 + 0.008 * i as f32);
    let wall = mix(shade(col, tone), WALLS[pick(WALLS.len(), 3)], (0.05 + 0.034 * i as f32) * warm);
    let slate = mix(shade(col, -0.085 + tone * 0.5), TILES[pick(TILES.len(), 7)], (0.07 + 0.045 * i as f32) * warm);
    let shutters = if i >= 3 && hash2(k, 11) < 0.42 { Some(mix(wall, SHUTTERS[pick(SHUTTERS.len(), 13)], 0.16 + 0.44 * sc.day)) } else { None };
    Coat { wall, slate, hip: hash2(k, 9) < 0.42, shutters }
}

/// Each building in its own colours: the wall, the roof a shade darker with its courses of tile,
/// a line of shadow under the eaves, and a rim of light on the edges that face it.
fn roofs(d: &mut Draw<Canvas>, cx: &Ctx, layer: &Layer, i: usize, col: Rgb, light: &Light, coats: &[Coat]) {
    let lay = &cx.lay;
    let u = lay.u;
    let rim_w = (1.5 + 0.28 * i as f32) * u;
    let rim_col = mix(col, light.col, 0.62);
    let rim_a = (light.k * (0.62 + 0.11 * i as f32)).min(0.9);
    let near = i >= 3;
    for (b, coat) in layer.buildings.iter().zip(coats) {
        let [l, t, r, floor] = b.rect;
        let (x0, x1, yt) = (lay.cx(l), lay.cx(r), lay.cy(t));
        if x1 < -20.0 || x0 > lay.w + 20.0 {
            continue;
        }
        let (ax, ay) = (lay.cx(b.apex.0), lay.cy(b.apex.1));
        let lit_left = light.x < (x0 + x1) / 2.0;
        let (wall, slate) = (coat.wall, coat.slate);
        let eave = shade(wall, -0.17);
        d.rect(x0, yt, x1, lay.cy(floor.min(REF_FLOOR)).min(lay.h), wall, 1.0);
        // a stroke just inside an edge, so the outline of the roof stays where it is
        let rim = |d: &mut Draw<Canvas>, a: P, bb: P, k: f32| {
            if rim_a * k > 0.02 {
                let (dx, dy) = (bb.0 - a.0, bb.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                let (mut nx, mut ny) = (-dy / len, dx / len);
                if ny < 0.0 {
                    nx = -nx;
                    ny = -ny;
                }
                let o = rim_w * 0.5;
                d.path(&[(a.0 + nx * o, a.1 + ny * o), (bb.0 + nx * o, bb.1 + ny * o)], rim_w, rim_col, rim_a * k);
            }
        };
        // courses of tile across a roof that runs from (a0..a1) at the eaves up to (b0..b1)
        let courses = |d: &mut Draw<Canvas>, a0: f32, a1: f32, ya: f32, b0: f32, b1: f32, yb: f32| {
            let n = (((ya - yb) / (5.5 * u)) as usize).clamp(1, 7);
            for k in 1..=n {
                let f = k as f32 / (n + 1) as f32;
                let y = ya + (yb - ya) * f;
                d.rect(a0 + (b0 - a0) * f, y, a1 + (b1 - a1) * f, y + (0.9 * u).max(0.8), shade(slate, -0.13), 0.30);
            }
        };
        match b.roof {
            Roof::Gable => {
                d.poly(&[(x0, yt + 0.5), (ax, ay), (x1, yt + 0.5)], if coat.hip { slate } else { wall }, 1.0);
                if coat.hip && near {
                    courses(d, x0, x1, yt, ax, ax, ay);
                }
                d.rect(x0, yt, x1, yt + (1.1 * u).max(0.8), eave, if coat.hip { 0.6 } else { 0.28 });
                if lit_left {
                    rim(d, (x0, yt), (ax, ay), 1.0);
                } else {
                    rim(d, (ax, ay), (x1, yt), 1.0);
                }
            }
            Roof::Mansard => {
                let f = b.flat.unwrap();
                let (fx0, fx1, fy) = (lay.cx(f[0]), lay.cx(f[1]), lay.cy(f[2]));
                d.poly(&[(x0, yt + 0.5), (fx0, fy), (fx1, fy), (x1, yt + 0.5)], slate, 1.0);
                if near {
                    courses(d, x0, x1, yt, fx0, fx1, fy);
                }
                d.rect(x0, yt, x1, yt + (1.1 * u).max(0.8), eave, 0.6);
                rim(d, (fx0, fy), (fx1, fy), 0.55);
                if lit_left {
                    rim(d, (x0, yt), (fx0, fy), 1.0);
                } else {
                    rim(d, (fx1, fy), (x1, yt), 1.0);
                }
            }
            Roof::Flat => {
                // a coping stone along the parapet
                d.rect(x0, yt, x1, yt + (1.6 * u).max(1.0), shade(wall, 0.07), 0.55);
                rim(d, (x0, yt), (x1, yt), 0.5)
            }
            Roof::Spire => {
                let bw = (r - l) as i64;
                let half = ((bw / 3).max(30) / 2) as f64;
                let mid = l + (bw / 2) as f64;
                let (sx0, sx1, sy) = (lay.cx(mid - half), lay.cx(mid + half), lay.cy(t - b.bh * 0.25));
                d.rect(sx0, sy, sx1, yt + 1.0, wall, 1.0);
                d.poly(&[(sx0, sy), (ax, ay), (sx1, sy)], slate, 1.0);
                if lit_left {
                    rim(d, (sx0, sy), (ax, ay), 1.0);
                } else {
                    rim(d, (ax, ay), (sx1, sy), 1.0);
                }
            }
        }
        // every chimney has its cap, and the near ones a pot or two
        if let Some(c) = b.chimney {
            let (c0, cy0, c1) = (lay.cx(c[0]), lay.cy(c[1]), lay.cx(c[2]));
            d.rect(c0, cy0, c1, yt.min(lay.cy(c[3])), shade(wall, -0.05), 1.0);
            d.rect(c0 - 0.9 * u, cy0 - 1.3 * u, c1 + 0.9 * u, cy0 + 0.3 * u, eave, 1.0);
            if near {
                let pot = mix(eave, TILES[0], 0.25 * sc_day(cx));
                d.rect(c0 + (c1 - c0) * 0.18, cy0 - 4.6 * u, c0 + (c1 - c0) * 0.42, cy0 - 1.2 * u, pot, 1.0);
                d.rect(c0 + (c1 - c0) * 0.58, cy0 - 3.8 * u, c0 + (c1 - c0) * 0.82, cy0 - 1.2 * u, pot, 1.0);
            }
        }
    }
}

fn sc_day(cx: &Ctx) -> f32 {
    cx.sc.day
}

/// Aerials, water tanks, attic windows and the rest of what stands on the roofs.
fn furniture(d: &mut Draw<Canvas>, cx: &Ctx, layer: &Layer, i: usize, col: Rgb, bloom: &mut Bloom) {
    let (sc, lay, city) = (cx.sc, &cx.lay, cx.city);
    let u = lay.u;
    let slate = shade(col, -0.085);
    let glass = mix(col, sc.sky[5], 0.6);
    let glass_a = 0.92 * smooth(-5.0, 4.0, sc.el_eff);
    let power = (0.30 + 0.70 * sc.dark) * sc.p.bloom;
    let leaf = mix(col, hex(sc.p.seasons[sc.season][1]), 0.25 + 0.45 * sc.day);
    for det in city.details.iter().filter(|d| d.layer == i) {
        let (x, y, sz) = (lay.cx(det.x), lay.cy(det.y), det.size as f32 * lay.s);
        if x < -60.0 || x > lay.w + 60.0 {
            continue;
        }
        let glowing = |office: bool| window_state(&det.who, office, det.tone, sc.t, sc.home_dow, sc.el_eff as f64);
        match det.kind {
            Kind::Aerial => {
                let lw = (0.9 * u * (0.7 + 0.1 * i as f32)).max(0.7);
                d.line((x, y + 1.0), (x, y - sz), lw, col, 1.0);
                for (at, half) in [(0.70, 0.30), (0.84, 0.23), (0.98, 0.15)] {
                    d.line((x - sz * half, y - sz * at), (x + sz * half, y - sz * at), lw, col, 1.0);
                }
            }
            Kind::Tank => {
                for e in [-0.34, 0.34] {
                    d.line((x + sz * e, y + 1.0), (x + sz * e, y - sz * 0.42), (1.2 * u).max(0.9), col, 1.0);
                }
                d.rect(x - sz * 0.48, y - sz * 1.15, x + sz * 0.48, y - sz * 0.36, col, 1.0);
                d.poly(&[(x - sz * 0.54, y - sz * 1.15), (x, y - sz * 1.42), (x + sz * 0.54, y - sz * 1.15)], slate, 1.0);
                d.rect(x - sz * 0.48, y - sz * 0.80, x + sz * 0.48, y - sz * 0.72, slate, 1.0);
            }
            Kind::Vent => {
                d.rect(x - sz * 0.45, y - sz * 0.85, x + sz * 0.45, y + 1.0, col, 1.0);
                d.rect(x - sz * 0.62, y - sz * 1.05, x + sz * 0.62, y - sz * 0.85, slate, 1.0);
            }
            Kind::Garden => {
                let n = ((sz / (7.5 * u)) as usize).max(2);
                d.rect(x, y - 3.0 * u, x + sz, y + 1.0, slate, 1.0);
                for k in 0..n {
                    let bx = x + sz * (k as f32 + 0.5) / n as f32;
                    let big = 1.0 + 0.5 * hash2(k as u32, det.b as u32);
                    d.ellipse(bx, y - (3.5 + 1.8 * big) * u, 4.2 * u, (2.6 + 1.9 * big) * u, leaf, 1.0);
                }
            }
            Kind::Attic => {
                if let Some(g) = glowing(false) {
                    d.circle(x, y, sz, g, 0.94);
                    d.rect(x - sz, y - 0.45 * u, x + sz, y + 0.45 * u, shade(col, -0.25), 0.5);
                    bloom.light(x - sz * 1.2, y - sz * 1.2, x + sz * 1.2, y + sz * 1.2, mix(g, WARM[1], 0.5), 0.74 * power);
                } else if glass_a > 0.03 {
                    d.circle(x, y, sz, glass, glass_a * 0.8);
                }
            }
            Kind::Dormer => {
                let b = &layer.buildings[det.b];
                let rise = lay.s * (b.rect[1] - b.apex.1) as f32;
                let (ww, wh) = (layer.ww as f32 * lay.s, layer.wh as f32 * lay.s);
                let (bh, bw) = ((rise * 0.66).min(wh * 1.75), (rise * 0.40).clamp(2.5 * u, ww * 0.95));
                d.rect(x - bw, y - bh, x + bw, y + 0.5, col, 1.0);
                d.poly(&[(x - bw * 1.25, y - bh), (x, y - bh - bw * 0.75), (x + bw * 1.25, y - bh)], shade(slate, -0.06), 1.0);
                let (wx, wy0, wy1) = (bw * 0.56, y - bh * 0.82, y - bh * 0.16);
                if let Some(g) = glowing(false) {
                    d.rect(x - wx, wy0, x + wx, wy1, g, 0.94);
                    d.rect(x - 0.4 * u, wy0, x + 0.4 * u, wy1, shade(col, -0.25), 0.5);
                    bloom.light(x - wx * 1.3, wy0 - wx * 0.3, x + wx * 1.3, wy1 + wx * 0.3, mix(g, WARM[1], 0.5), 0.74 * power);
                } else if glass_a > 0.03 {
                    d.rect(x - wx, wy0, x + wx, wy1, glass, glass_a);
                }
            }
        }
    }
}

/// Hills beyond the last roofs, and a hamlet on them that goes to bed early.
fn ridge(d: &mut Draw<Canvas>, cx: &Ctx, bloom: &mut Bloom) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let line = |f: f32| 0.782 - 0.020 * (f * 4.3 + 0.7).sin() - 0.011 * (f * 9.7 + 2.1).sin() - 0.016 * smooth(0.55, 0.95, f);
    let mut pts: Vec<P> = (0..=48).map(|k| k as f32 / 48.0).map(|f| (lay.x(f), lay.y(line(f)))).collect();
    pts.extend([(lay.w, lay.y(0.82)), (0.0, lay.y(0.82))]);
    let col = mix(mix(sc.layers[0], sc.sky[5], 0.50), sc.haze, 0.2);
    d.poly(&pts, col, 0.92);
    let awake = smooth(1.0, -4.0, sc.el_eff) * if sc.t >= 16.0 && sc.t < 22.6 { 1.0 } else { 0.18 };
    if awake > 0.05 {
        for k in 0..14u32 {
            if hash2(k, 301) > awake {
                continue;
            }
            let f = 0.80 + 0.13 * hash2(k, 303);
            let (x, y) = (lay.x(f), lay.y(line(f) + 0.004 + 0.012 * hash2(k, 305)));
            d.dot(x, y, 0.85 * u, 0.85 * u, WARM[k as usize % 4], 0.9, Blend::Over);
            bloom.light(x - 1.4 * u, y - 1.4 * u, x + 1.4 * u, y + 1.4 * u, WARM[1], 0.45 * sc.p.bloom);
        }
    }
}

/// The domed hall. Its tall windows are lit of an evening.
fn dome(d: &mut Draw<Canvas>, cx: &Ctx, col: Rgb, light: &Light, bloom: &mut Bloom) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let (x, top) = (lay.x(DOME_X), lay.y(DOME_TOP));
    let wall = mix(col, WALLS[0], 0.10 * (0.22 + 0.78 * sc.day));
    let copper = mix(shade(col, -0.06), [118.0, 168.0, 150.0], 0.10 + 0.26 * sc.day);
    let dark = shade(col, -0.16);
    d.rect(x - 24.0 * u, top, x + 24.0 * u, lay.y(0.86), wall, 1.0);
    let cap: Vec<P> = (0..=20).map(|k| PI * k as f32 / 20.0).map(|a| (x - 21.0 * u * a.cos(), top - 19.0 * u * a.sin())).collect();
    d.poly(&cap, copper, 1.0);
    // ribs on the copper, a lantern on top, and a cornice where the dome sits
    for f in [-0.5f32, 0.0, 0.5] {
        let rib: Vec<P> = (0..=8).map(|k| k as f32 / 8.0).map(|t| (x + f * 21.0 * u * (1.0 - t * t).sqrt().max(0.0) * 1.0 * (1.0 - 0.0 * t), top - 19.0 * u * t)).collect();
        d.path(&rib, 0.9 * u, shade(copper, -0.14), 0.55);
    }
    d.rect(x - 3.2 * u, top - 28.0 * u, x + 3.2 * u, top - 17.0 * u, wall, 1.0);
    d.poly(&[(x - 4.4 * u, top - 28.0 * u), (x, top - 35.0 * u), (x + 4.4 * u, top - 28.0 * u)], copper, 1.0);
    d.line((x, top - 34.0 * u), (x, top - 41.0 * u), 1.0 * u, dark, 1.0);
    d.rect(x - 26.0 * u, top - 1.6 * u, x + 26.0 * u, top + 1.6 * u, dark, 1.0);
    let side = if light.x < x { -1.0 } else { 1.0 };
    let a = (light.k * 0.8).min(0.9);
    if a > 0.02 {
        let rim: Vec<P> = (0..=10).map(|k| PI * (0.06 + 0.40 * k as f32 / 10.0)).map(|ang| (x + side * 20.0 * u * ang.cos(), top - 18.0 * u * ang.sin())).collect();
        d.path(&rim, 1.7 * u, mix(copper, light.col, 0.62), a);
        d.rect(x + side * 23.0 * u - 0.9 * u, top + 2.0 * u, x + side * 23.0 * u + 0.9 * u, lay.y(0.86), mix(wall, light.col, 0.62), a * 0.8);
    }
    let lit = smooth(1.5, -3.5, sc.el_eff) * if sc.t >= 16.0 && sc.t < 22.25 { 1.0 } else { 0.0 };
    let glass = mix(mix(col, sc.sky[5], 0.5), WARM[0], lit);
    for k in -2i32..=2 {
        let wx = x + k as f32 * 8.6 * u;
        d.rect(wx - 2.0 * u, top + 9.0 * u, wx + 2.0 * u, top + 30.0 * u, glass, 0.45 + 0.5 * lit.max(smooth(-5.0, 4.0, sc.el_eff) * 0.6));
        d.circle(wx, top + 9.0 * u, 2.0 * u, glass, 0.45 + 0.5 * lit.max(smooth(-5.0, 4.0, sc.el_eff) * 0.6));
        if lit > 0.05 {
            bloom.light(wx - 2.6 * u, top + 7.0 * u, wx + 2.6 * u, top + 30.0 * u, WARM[1], 0.6 * lit * sc.p.bloom);
        }
    }
}

/// The clock tower. Its hands show the real time.
fn clock_tower(d: &mut Draw<Canvas>, cx: &Ctx, col: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
    let (sc, lay) = (cx.sc, &cx.lay);
    let u = lay.u;
    let (x, foot, top) = (lay.x(0.262), lay.y(0.93), lay.y(0.742));
    let slate = shade(col, -0.10);
    d.rect(x - 14.0 * u, top, x + 14.0 * u, foot, col, 1.0);
    d.rect(x - 17.0 * u, top - 33.0 * u, x + 17.0 * u, top, col, 1.0);
    d.rect(x - 19.5 * u, top - 36.0 * u, x + 19.5 * u, top - 33.0 * u, slate, 1.0);
    d.rect(x - 18.5 * u, top - 1.5 * u, x + 18.5 * u, top + 1.5 * u, slate, 1.0);
    let tip = top - 36.0 * u - 44.0 * u;
    d.poly(&[(x - 18.5 * u, top - 36.0 * u), (x, tip), (x + 18.5 * u, top - 36.0 * u)], slate, 1.0);
    d.line((x, tip + 2.0 * u), (x, tip - 9.0 * u), 1.3 * u, col, 1.0);
    d.circle(x, tip - 10.0 * u, 2.0 * u, col, 1.0);
    for k in 0..3 {
        let y = top + (16.0 + 22.0 * k as f32) * u;
        d.rect(x - 2.0 * u, y, x + 2.0 * u, y + 9.0 * u, slate, 0.9);
    }
    // a rim of light down the side that faces it
    let side = if light.x < x { -1.0 } else { 1.0 };
    let rim = mix(col, light.col, 0.62);
    let a = (light.k * 0.75).min(0.9);
    if a > 0.02 {
        d.rect(x + side * 15.4 * u - 0.9 * u, top - 33.0 * u, x + side * 15.4 * u + 0.9 * u, top, rim, a);
        d.rect(x + side * 12.6 * u - 0.8 * u, top + 2.0 * u, x + side * 12.6 * u + 0.8 * u, foot, rim, a * 0.8);
        d.path(&[(x + side * 17.0 * u, top - 36.5 * u), (x + side * 0.8 * u, tip + 1.5 * u)], 1.7 * u, rim, a);
    }
    props::clock_face(d, sc, x, top - 16.5 * u, 11.5 * u, u, col, bloom);
    out.perch = Some((x, tip));
}

/// The house that stands for this machine.
fn house(d: &mut Draw<Canvas>, cx: &Ctx, col: Rgb, bloom: &mut Bloom, out: &mut Anchors) {
    let (sc, lay, city) = (cx.sc, &cx.lay, cx.city);
    let u = lay.u;
    let (hx, y, bw, bh) = house_box(city, lay);
    let slate = shade(col, -0.085);
    // the stovepipe: it smokes when the processor is working hard
    let (px, py) = (hx - bw / 2.0 + 9.0 * u, y - bh - 17.0 * u);
    d.rect(px - 2.2 * u, py, px + 2.2 * u, y - bh, col, 1.0);
    d.rect(px - 3.4 * u, py - 2.0 * u, px + 3.4 * u, py, slate, 1.0);
    out.stove = Some(Smoke { x: px, y: py - 2.0 * u, scale: 0.7, col: mix(col, sc.sky[5], 0.72), behind: 4 });
    out.music = Some((px, py - 2.0 * u));
    if sc.machine.load > 0.35 && !sc.live {
        props::smoke_still(d, sc, px, py - 2.0 * u, u * 0.8, mix(col, sc.sky[5], 0.72), 1.3);
    }
    d.rect(hx - bw / 2.0, y - bh, hx + bw / 2.0, y + 1.0, col, 1.0);
    d.poly(&[(hx - bw / 2.0 - 5.0 * u, y - bh), (hx, y - bh - 13.0 * u), (hx + bw / 2.0 + 5.0 * u, y - bh)], slate, 1.0);
    let (wx0, wy0, wx1, wy1) = (hx - 14.0 * u, y - bh + 7.0 * u, hx + 14.0 * u, y - 6.0 * u);
    let frame = shade(col, -0.2);
    if sc.machine.agents > 0 {
        // someone is working in the roof room
        let (lamp, glow, low) = lamp_colours(sc);
        d.rect(wx0, wy0, wx1, wy1, lamp, 1.0);
        bloom.light(wx0 - 4.0 * u, wy0 - 4.0 * u, wx1 + 4.0 * u, wy1 + 4.0 * u, glow, (0.9 + 0.9 * sc.dark) * sc.p.bloom.max(0.6));
        out.breath.push(Breath { x: hx, y: y - 16.5 * u, rx: 56.0 * u, ry: 41.0 * u, col: glow, strength: 0.45 + 0.55 * sc.dark, low, behind: Some(4) });
    } else {
        d.rect(wx0, wy0, wx1, wy1, mix(col, sc.sky[5], 0.10 + 0.35 * sc.day), 1.0);
    }
    d.line((hx, wy0), (hx, wy1), 1.6 * u, frame, 1.0);
    d.line((wx0, (wy0 + wy1) / 2.0), (wx1, (wy0 + wy1) / 2.0), 1.2 * u, frame, 1.0);
    if !sc.live {
        let (mx, foot, top) = mast(city, lay);
        props::pennant(d, sc, mx, foot, top, u, col, (sc.ts / 200.0) as f32);
        if sc.machine.audio {
            let ink = mix(hex(sc.p.ink), AMBER, 0.4);
            props::note(d, hx + bw * 0.1, y - bh - 26.0 * u, u, ink, 0.8, false);
            props::note(d, hx - bw * 0.35, y - bh - 44.0 * u, u, ink, 0.55, true);
        }
    }
    // one blue window for each running container
    let n = sc.machine.containers.min(8) as f32;
    for k in 0..n as usize {
        let bx = hx - (n - 1.0) * 7.0 * u + k as f32 * 14.0 * u;
        let (x0, y0, x1, y1) = (bx - 4.0 * u, y + 10.0 * u, bx + 4.0 * u, y + 22.0 * u);
        d.rect(x0, y0, x1, y1, mix(BLUE, WHITE, 0.18), 1.0);
        bloom.light(x0 - 2.0 * u, y0 - 2.0 * u, x1 + 2.0 * u, y1 + 2.0 * u, BLUE, (0.5 + 0.7 * sc.dark) * sc.p.bloom.max(0.6));
    }
}

fn trees(d: &mut Draw<Canvas>, cx: &Ctx, bloom: &mut Bloom, out: &mut Anchors) {
    out.leaves = true;
    for side in [0.02, 0.83] {
        out.glades.push([cx.lay.x(side), cx.lay.y(0.80), cx.lay.x(side + 0.17), cx.lay.y(0.975)]);
    }
    let (sc, lay, city) = (cx.sc, &cx.lay, cx.city);
    let p = sc.p;
    let u = lay.u;
    let tints = p.seasons[sc.season];
    let light = p.tree_light * smooth(-8.0, 16.0, sc.eph.sun.el as f32) * (1.0 - 0.4 * sc.gloom);
    let sun_x = lay.x(sc.eph.sun.x as f32);
    let mut order: Vec<usize> = (0..city.trees.len()).collect();
    order.sort_by(|a, b| city.trees[*a].y.partial_cmp(&city.trees[*b].y).unwrap());
    for k in order {
        let t = &city.trees[k];
        let (x, y, r) = (lay.cx(t.x), lay.cy(t.y), t.r as f32 * lay.s);
        if x + r * 1.4 < 0.0 || x - r * 1.4 > lay.w || y - r * 1.4 > lay.h {
            continue;
        }
        let base = mix(hex(if t.n % 5 == 0 { p.tree[1] } else { p.tree[0] }), hex(p.tree_night), 0.6 * (1.0 - sc.day));
        let col = mix(base, hex(tints[t.n % 3]), light);
        let side = if sun_x < x { -1.0 } else { 1.0 };
        d.ellipse(x - r * 0.50, y + r * 0.24, r * 0.66, r * 0.58, shade(col, -0.05), 1.0);
        d.ellipse(x + r * 0.52, y + r * 0.28, r * 0.62, r * 0.54, shade(col, -0.08), 1.0);
        d.ellipse(x, y, r, r * 0.9, col, 1.0);
        bloom.shade(x, y, r, r * 0.9, 0.12);
        d.ellipse(x + side * r * 0.30, y - r * 0.30, r * 0.52, r * 0.42, shade(col, 0.12 * light), 0.9 * sc.day);
        d.ellipse(x - side * r * 0.28, y + r * 0.34, r * 0.50, r * 0.36, shade(col, -0.07), 0.6);
        if sc.snow > 0.3 {
            d.ellipse(x, y - r * 0.46, r * 0.80, r * 0.40, mix(col, hex(p.snow), 0.75), 1.0);
        } else if sc.season == 1 && t.n % 4 == 0 && light > 0.1 {
            let mut bl = PyRng::new(t.n as u64);
            for _ in 0..9 {
                let (bx, by) = (x + bl.uniform(-0.7, 0.7) as f32 * r, y + bl.uniform(-0.7, 0.3) as f32 * r);
                d.dot(bx, by, 2.2 * u, 2.2 * u, mix(col, [232.0, 178.0, 192.0], 0.3 + light), 1.0, Blend::Over);
            }
        }
    }
}
