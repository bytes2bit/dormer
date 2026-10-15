//! The city: its roofs, its windows, and the hours each window keeps.
//!
//! Everything here is decided by one seed. The order of the random calls in
//! `skyline` and `Town::build` is the city's fingerprint: change it and every
//! roof moves. Anything added later (aerials, water tanks, attic windows)
//! draws from its own stream, so the roofs stay exactly as first drawn.

use crate::color::{Rgb, COOL, SCREEN, WARM};
use crate::rng::PyRng;

/// The canvas the city is laid out on; drawing scales from it.
pub const REF_W: f64 = 5120.0;
pub const REF_H: f64 = 3200.0;
/// One pixel of a 1200-line screen, in reference pixels.
pub const UNIT: f64 = REF_H / 1200.0;

/// Roof rows, far to near: base, min h, max h, min w, max w (fractions of H / W).
const SPEC: [(f64, f64, f64, f64, f64); 5] = [
    (0.800, 0.020, 0.060, 0.018, 0.045),
    (0.835, 0.030, 0.085, 0.024, 0.060),
    (0.880, 0.040, 0.110, 0.034, 0.080),
    (0.940, 0.045, 0.115, 0.045, 0.100),
    (1.020, 0.050, 0.120, 0.070, 0.140),
];
const LIT: [f64; 5] = [0.0, 0.08, 0.13, 0.15, 0.10];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Roof {
    Spire,
    Gable,
    Mansard,
    Flat,
}

/// A flat stretch of roof: left, right, height.
pub type Flat = [f64; 3];

pub struct Building {
    /// left, top of the wall, right, floor
    pub rect: [f64; 4],
    pub flat: Option<Flat>,
    /// 0 for the lowest a row allows, 1 for the tallest
    pub height: f64,
    pub chimney: Option<[f64; 4]>,
    pub roof: Roof,
    pub apex: (f64, f64),
    /// how tall the wall is
    pub bh: f64,
}

pub struct Window {
    pub x: f64,
    pub y: f64,
    pub b: usize,
    pub office: bool,
    /// some windows show as glass by day; the rest are only seen when lit
    pub glass: bool,
    /// four numbers that decide the hours this household keeps
    pub who: [f64; 4],
    pub tone: usize,
    /// what is seen in it when lit: 0 nothing, 1/2 a curtain, 3 a blind, 4 someone, 5 a plant
    pub look: u8,
}

pub struct Layer {
    pub base: f64,
    pub hi: f64,
    pub poly: Vec<(f64, f64)>,
    pub chimneys: Vec<[f64; 4]>,
    pub buildings: Vec<Building>,
    pub windows: Vec<Window>,
    pub ww: f64,
    pub wh: f64,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// a television aerial: x, y of its foot, height
    Aerial,
    /// a water tank on legs: x of its middle, y of the roof, size
    Tank,
    /// a vent box
    Vent,
    /// planters along a roof edge: x0, y, length
    Garden,
    /// a small round window under a gable
    Attic,
    /// a window standing out of a mansard roof
    Dormer,
}

pub struct Detail {
    pub kind: Kind,
    pub layer: usize,
    pub b: usize,
    pub x: f64,
    pub y: f64,
    pub size: f64,
    pub who: [f64; 4],
    pub tone: usize,
}

pub struct Tree {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub n: usize,
}

pub struct Town {
    pub seed: u64,
    pub layers: Vec<Layer>,
    pub trees: Vec<Tree>,
    pub details: Vec<Detail>,
    /// the roof that stands for this machine: building index in row 3, and its flat
    pub house: (usize, Flat),
    pub laundry: Vec<Flat>,
    pub festoon: Flat,
    pub cat_spots: Vec<(usize, Flat)>,
    pub sleepers: Vec<(usize, (f64, f64))>,
    pub kite: (f64, f64),
    pub smokers: Vec<(usize, [f64; 4])>,
    /// which of `smokers` light up first as it gets colder
    pub smoke_rank: Vec<f64>,
}

fn skyline(rng: &mut PyRng, w: i64, base: i64, lo: i64, hi: i64, wmin: i64, wmax: i64, depth: usize) -> Layer {
    let floor = (base + hi) as f64;
    let mut pts = vec![(0.0, floor)];
    let (mut chimneys, mut buildings) = (Vec::new(), Vec::new());
    let mut x = -rng.randint(0, wmax);
    while x < w {
        let bw = rng.randint(wmin, wmax);
        let bh = rng.randint(lo, hi);
        let top = (base - bh) as f64;
        let kind = rng.random();
        let (left, right) = (x as f64, (x + bw) as f64);
        let (bwf, bhf) = (bw as f64, bh as f64);
        pts.push((left, floor));
        let mut b = Building {
            rect: [left, top, right, floor],
            flat: None,
            height: (bh - lo) as f64 / (hi - lo).max(1) as f64,
            chimney: None,
            roof: Roof::Flat,
            apex: (0.0, 0.0),
            bh: bhf,
        };
        if kind < 0.06 && depth < 3 {
            // a spire on a slim tower
            let tw = (bw / 3).max(30);
            let cx = (x + bw / 2) as f64;
            let half = (tw / 2) as f64;
            let tip = top - bhf * rng.uniform(0.9, 1.4);
            pts.extend_from_slice(&[
                (left, top),
                (cx - half, top),
                (cx - half, top - bhf * 0.25),
                (cx, tip),
                (cx + half, top - bhf * 0.25),
                (cx + half, top),
                (right, top),
            ]);
            b.roof = Roof::Spire;
            b.apex = (cx, tip);
        } else if kind < 0.45 {
            let rise = bwf * rng.uniform(0.22, 0.40);
            pts.extend_from_slice(&[(left, top), (left + bwf / 2.0, top - rise), (right, top)]);
            b.roof = Roof::Gable;
            b.apex = (left + bwf / 2.0, top - rise);
        } else if kind < 0.70 {
            let inset = bwf * 0.14;
            let rise = bwf * rng.uniform(0.16, 0.26);
            pts.extend_from_slice(&[(left, top), (left + inset, top - rise), (right - inset, top - rise), (right, top)]);
            b.roof = Roof::Mansard;
            b.apex = (left + bwf / 2.0, top - rise);
            b.flat = Some([left + inset, right - inset, top - rise]);
        } else {
            pts.extend_from_slice(&[(left, top), (right, top)]);
            b.roof = Roof::Flat;
            b.apex = (left + bwf / 2.0, top);
            b.flat = Some([left, right, top]);
        }
        pts.push((right, floor));
        if rng.random() < 0.45 {
            let cw = (bw / 10).max(8) as f64;
            let cxp = left + rng.uniform(0.15, 0.75) * bwf;
            let ch = bhf * rng.uniform(0.10, 0.18) + bwf * 0.12;
            let c = [cxp, top - ch, cxp + cw, top + 2.0];
            b.chimney = Some(c);
            chimneys.push(c);
        }
        buildings.push(b);
        x = x + bw + rng.randint(0, (wmin / 5).max(2));
    }
    pts.extend_from_slice(&[(w as f64, floor), (w as f64, 1e6), (0.0, 1e6)]);
    Layer { base: 0.0, hi: 0.0, poly: pts, chimneys, buildings, windows: Vec::new(), ww: 0.0, wh: 0.0 }
}

impl Town {
    /// Roofs, windows and trees in reference pixels, plus who lives behind each window.
    pub fn build(seed: u64) -> Town {
        let mut rng = PyRng::new(seed);
        let mut people = PyRng::new(seed * 7919 + 17);
        let (w, h) = (REF_W, REF_H);
        // the first twenty numbers place the high cloud (see `paint::cloud_bands`)
        for _ in 0..20 {
            rng.random();
        }
        let mut layers = Vec::new();
        for (i, &(base, lo, hi, wmin, wmax)) in SPEC.iter().enumerate() {
            let mut layer = skyline(
                &mut rng,
                w as i64,
                (base * h) as i64,
                (lo * h) as i64,
                (hi * h) as i64,
                (wmin * w) as i64,
                (wmax * w) as i64,
                i,
            );
            layer.base = base;
            layer.hi = hi;
            let ww = (((0.0020 + 0.0007 * i as f64) * w) as i64).max(3) as f64;
            let wh = ((ww * 1.45) as i64) as f64;
            layer.ww = ww;
            layer.wh = wh;
            if LIT[i] > 0.0 {
                for (bi, b) in layer.buildings.iter().enumerate() {
                    let [l, t, r, bot] = b.rect;
                    let office = b.roof == Roof::Flat && (i == 1 || i == 2) && b.height > 0.5;
                    let cols = (((r - l - ww) / (ww * 2.6)) as i64).max(1);
                    let pad = ((r - l) - (cols as f64 * ww * 2.6 - ww * 1.6)) / 2.0;
                    let mut y = t + wh * 1.4;
                    while y + wh < bot.min(h) - wh {
                        for c in 0..cols {
                            let glass = rng.random() < LIT[i];
                            let mut tone = 0;
                            if glass {
                                tone = rng.below(4) as usize;
                            }
                            let who = [people.random(), people.random(), people.random(), people.random()];
                            if !glass {
                                tone = people.below(4) as usize;
                            }
                            layer.windows.push(Window { x: l + pad + c as f64 * ww * 2.6, y, b: bi, office, glass, who, tone, look: 0 });
                        }
                        y += wh * 2.4;
                    }
                }
            }
            layers.push(layer);
        }
        let mut trees = Vec::new();
        for side in 0..2 {
            for n in 0..70 {
                let cx = if side == 0 { rng.uniform(-0.03, 0.17) } else { rng.uniform(0.83, 1.03) } * w;
                let edge = (cx / w - side as f64).abs();
                let cy = (0.915 + edge * 0.60 + rng.uniform(-0.012, 0.030)) * h;
                trees.push(Tree { x: cx, y: cy, r: rng.uniform(0.010, 0.024) * w, n });
            }
        }
        let mut city = Town {
            seed,
            layers,
            trees,
            details: Vec::new(),
            house: (0, [0.0; 3]),
            laundry: Vec::new(),
            festoon: [0.0; 3],
            cat_spots: Vec::new(),
            sleepers: Vec::new(),
            kite: (0.0, 0.0),
            smokers: Vec::new(),
            smoke_rank: Vec::new(),
        };
        city.cast();
        city.furnish();
        city
    }

    /// Give roofs their parts: the house that is yours, where the washing hangs, where the cat goes.
    fn cast(&mut self) {
        let mid = |f: &Flat| (f[0] + f[1]) / 2.0 / REF_W;
        let wide = |f: &Flat| f[1] - f[0];
        let flats3: Vec<(usize, Flat)> =
            self.layers[3].buildings.iter().enumerate().filter_map(|(bi, b)| b.flat.map(|f| (bi, f))).collect();
        let mut homes: Vec<(usize, Flat)> =
            flats3.iter().copied().filter(|c| (0.52..=0.80).contains(&mid(&c.1)) && wide(&c.1) >= 86.0 * UNIT).collect();
        if homes.is_empty() {
            let mut all = flats3.clone();
            all.sort_by(|a, b| wide(&b.1).partial_cmp(&wide(&a.1)).unwrap());
            homes = all.into_iter().take(1).collect();
        }
        // the tallest of them
        let mut house = homes[0];
        for c in &homes {
            if c.1[2] < house.1[2] {
                house = *c;
            }
        }
        self.house = house;
        let mut rest: Vec<(usize, Flat)> =
            flats3.iter().copied().filter(|c| c.0 != house.0 && (0.16..=0.86).contains(&mid(&c.1))).collect();
        rest.sort_by(|a, b| wide(&b.1).partial_cmp(&wide(&a.1)).unwrap());
        self.laundry = rest.iter().take(2).map(|c| c.1).collect();
        self.festoon = rest.get(2).map(|c| c.1).or(self.laundry.first().copied()).unwrap_or(house.1);
        let mut spots: Vec<(usize, Flat)> = rest.iter().skip(3).map(|c| (3, c.1)).collect();
        for b in &self.layers[4].buildings {
            if let Some(f) = b.flat {
                if (0.22..=0.80).contains(&mid(&f)) {
                    spots.push((4, f));
                }
            }
        }
        for b in &self.layers[2].buildings {
            if let Some(f) = b.flat {
                if (0.25..=0.75).contains(&mid(&f)) && wide(&f) > 60.0 * UNIT {
                    spots.push((2, f));
                }
            }
        }
        if spots.is_empty() {
            spots.push((3, house.1));
        }
        self.cat_spots = spots;

        let nearest = |layer: usize, kinds: &[Roof], fx: f64| -> (f64, f64) {
            let all = &self.layers[layer].buildings;
            let pool: Vec<&Building> = all.iter().filter(|b| kinds.contains(&b.roof)).collect();
            let pool: Vec<&Building> = if pool.is_empty() { all.iter().collect() } else { pool };
            let mut best = pool[0];
            for b in &pool {
                if (b.apex.0 / REF_W - fx).abs() < (best.apex.0 / REF_W - fx).abs() {
                    best = b;
                }
            }
            best.apex
        };
        self.sleepers = vec![
            (3, nearest(3, &[Roof::Gable, Roof::Mansard], 0.30)),
            (2, nearest(2, &[Roof::Gable], 0.50)),
            (3, nearest(3, &[Roof::Gable, Roof::Mansard], 0.45)),
            (2, nearest(2, &[Roof::Gable], 0.72)),
        ];
        self.kite = nearest(2, &[Roof::Gable, Roof::Mansard, Roof::Flat], 0.34);
        for i in 2..5 {
            for c in &self.layers[i].chimneys {
                if (0.05..=0.95).contains(&(c[0] / REF_W)) {
                    self.smokers.push((i, *c));
                }
            }
        }
        let mut pick = PyRng::new(self.seed + 3);
        self.smoke_rank = self.smokers.iter().map(|_| pick.random()).collect();
    }

    /// The small things on the roofs, and what is seen in the windows. Its own random stream.
    fn furnish(&mut self) {
        let mut r = PyRng::new(self.seed * 104729 + 71);
        let taken: Vec<Flat> =
            [self.house.1, self.festoon].into_iter().chain(self.laundry.iter().copied()).collect();
        let mut details = Vec::new();
        for (i, layer) in self.layers.iter_mut().enumerate() {
            let scale = 0.62 + 0.13 * i as f64;
            for (bi, b) in layer.buildings.iter().enumerate() {
                let [l, t, rgt, _] = b.rect;
                let bw = rgt - l;
                let who = [r.random(), r.random(), r.random(), r.random()];
                let tone = r.below(4) as usize;
                let (roll, at, size) = (r.random(), r.random(), r.random());
                if i == 0 {
                    continue;
                }
                let mut put = |kind, x, y, size| details.push(Detail { kind, layer: i, b: bi, x, y, size, who, tone });
                match b.roof {
                    Roof::Gable => {
                        if roll < 0.20 {
                            put(Kind::Aerial, b.apex.0, b.apex.1, (15.0 + 13.0 * size) * UNIT * scale);
                        } else if roll < 0.52 && i >= 2 && bw > 34.0 * UNIT {
                            let rise = t - b.apex.1;
                            put(Kind::Attic, b.apex.0, b.apex.1 + rise * 0.58, (3.0 + 0.9 * i as f64) * UNIT);
                        }
                    }
                    Roof::Mansard => {
                        let f = b.flat.unwrap();
                        if roll < 0.55 && i >= 2 {
                            let n = if bw > 62.0 * UNIT * scale { 2 } else { 1 };
                            for k in 0..n {
                                let fx = if n == 1 { 0.5 } else { 0.30 + 0.40 * k as f64 };
                                put(Kind::Dormer, l + bw * fx, t, (4.6 + 1.1 * i as f64) * UNIT);
                            }
                        } else if roll > 0.80 {
                            put(Kind::Aerial, f[0] + (f[1] - f[0]) * (0.2 + 0.6 * at), f[2], (15.0 + 13.0 * size) * UNIT * scale);
                        }
                    }
                    Roof::Flat => {
                        let f = b.flat.unwrap();
                        if taken.iter().any(|t| (t[0] - f[0]).abs() < 1.0 && (t[2] - f[2]).abs() < 1.0) {
                            continue;
                        }
                        let x = f[0] + bw * (0.16 + 0.68 * at);
                        if roll < 0.22 && i >= 2 && bw > 40.0 * UNIT {
                            put(Kind::Tank, x, f[2], (9.0 + 4.0 * size) * UNIT * scale);
                        } else if roll < 0.40 {
                            put(Kind::Aerial, x, f[2], (15.0 + 13.0 * size) * UNIT * scale);
                        } else if roll < 0.58 {
                            put(Kind::Vent, x, f[2], (5.0 + 3.0 * size) * UNIT * scale);
                        } else if roll < 0.70 && i >= 3 && bw > 50.0 * UNIT {
                            put(Kind::Garden, f[0] + bw * 0.08, f[2], bw * (0.30 + 0.25 * size));
                        }
                    }
                    Roof::Spire => {}
                }
            }
            if i >= 3 {
                for w in layer.windows.iter_mut() {
                    let roll = r.random();
                    w.look = if w.office {
                        0
                    } else if roll < 0.16 {
                        1
                    } else if roll < 0.32 {
                        2
                    } else if roll < 0.42 {
                        3
                    } else if roll < 0.47 && i == 4 {
                        4
                    } else if roll < 0.53 && i == 4 {
                        5
                    } else {
                        0
                    };
                }
            }
        }
        self.details = details;
    }

    /// A short fingerprint of the layout, for checking that nothing has moved.
    pub fn fingerprint(&self) -> String {
        let mut out = String::new();
        for (i, l) in self.layers.iter().enumerate() {
            let sx: f64 = l.poly.iter().map(|p| p.0).sum();
            let sy: f64 = l.poly.iter().filter(|p| p.1 < 1e5).map(|p| p.1).sum();
            let who: f64 = l.windows.iter().map(|w| w.who.iter().sum::<f64>()).sum();
            let glass = l.windows.iter().filter(|w| w.glass).count();
            let tone: usize = l.windows.iter().map(|w| w.tone).sum();
            out += &format!(
                "L{} pts={} sx={:.3} sy={:.3} ch={} b={} win={} glass={} tone={} who={:.6}\n",
                i,
                l.poly.len(),
                sx,
                sy,
                l.chimneys.len(),
                l.buildings.len(),
                l.windows.len(),
                glass,
                tone,
                who
            );
        }
        let t: f64 = self.trees.iter().map(|t| t.x + t.y + t.r).sum();
        out += &format!("trees={} sum={:.3}\n", self.trees.len(), t);
        out += &format!("house={} {:.3} {:.3} {:.3}\n", self.house.0, self.house.1[0], self.house.1[1], self.house.1[2]);
        out += &format!("laundry={} cat={} smokers={}\n", self.laundry.len(), self.cat_spots.len(), self.smokers.len());
        out += &format!("kite={:.3},{:.3} sleepers={:.3}\n", self.kite.0, self.kite.1, self.sleepers.iter().map(|s| (s.1).0).sum::<f64>());
        out
    }
}

// ── who is awake ─────────────────────────────────────────────────────────────

pub fn home_on(who: &[f64; 4], t: f64, dow: usize) -> bool {
    let [a, b, c, d] = *who;
    // an evening belongs to the day it began on
    let evening = |day: usize, tt: f64| -> bool {
        let late = day == 4 || day == 5; // Friday and Saturday nights
        let share = if late { 0.30 } else if day == 6 { 0.23 } else { 0.26 };
        if a >= share {
            return false;
        }
        let back = if day >= 5 { 14.0 + 5.5 * b } else { 16.4 + 3.4 * b };
        let mut bed = if late { 22.6 + 4.6 * d.powf(1.4) } else { 21.4 + 3.0 * d.powf(1.6) };
        if a < 0.012 {
            bed = 28.5; // the night owls
        }
        back <= tt && tt < bed
    };
    if evening(dow, t) || (t < 6.0 && evening((dow + 6) % 7, t + 24.0)) {
        return true;
    }
    let early = (a * 7.31 + c * 3.17) % 1.0;
    if early >= [0.34, 0.26, 0.26, 0.26, 0.24, 0.09, 0.045][dow] {
        return false;
    }
    let wake = [5.3, 5.7, 5.7, 5.7, 5.8, 7.6, 8.6][dow] + [1.7, 2.0, 2.0, 2.0, 2.0, 2.6, 2.6][dow] * c;
    wake <= t && t < wake + 0.7 + 1.3 * d
}

pub fn office_on(who: &[f64; 4], t: f64, dow: usize) -> bool {
    let [a, b, c, _] = *who;
    if dow < 5 {
        if c < 0.62 {
            let close = 17.3 + 3.2 * b.powf(1.5) + if dow == 0 { 0.8 } else if dow == 4 { -0.9 } else { 0.0 };
            if 7.0 + 1.6 * a <= t && t < close {
                return true;
            }
        }
        return c > 0.985 && (t >= 21.0 || t < 5.0); // the cleaners
    }
    c < 0.03 && (9.5..16.5).contains(&t) // someone catching up
}

/// None, or the colour this window glows right now. `el_eff` is how bright it feels in the street.
pub fn window_state(who: &[f64; 4], office: bool, tone: usize, t: f64, dow: usize, el_eff: f64) -> Option<Rgb> {
    let [a, b, _, d] = *who;
    if office {
        if office_on(who, t, dow) && el_eff < 3.0 - 2.0 * a {
            return Some(COOL[tone]);
        }
        return None;
    }
    if home_on(who, t, dow) && el_eff < 1.5 - 7.5 * ((b * 5.77 + d * 2.3) % 1.0) {
        return Some(if a < 0.012 && (t >= 23.5 || t < 6.0) { SCREEN } else { WARM[tone] });
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The city of seed 12 is the one people have on their walls: it must never move.
    #[test]
    fn seed_twelve_has_not_moved() {
        let print = Town::build(12).fingerprint();
        assert!(print.contains("L1 pts=120 sx=297115.500 sy=315229.417 ch=10 b=24 win=1260 glass=100 tone=1853 who=2517.586113"), "{}", print);
        assert!(print.contains("L2 pts=95 sx=256805.000 sy=258292.041 ch=6 b=17 win=941 glass=116 tone=1417 who=1883.948899"), "{}", print);
    }

    #[test]
    fn households_keep_sensible_hours() {
        let city = Town::build(12);
        let lit = |t: f64, dow: usize| -> f64 {
            let all: Vec<&Window> = city.layers.iter().flat_map(|l| l.windows.iter()).filter(|w| !w.office).collect();
            all.iter().filter(|w| home_on(&w.who, t, dow)).count() as f64 / all.len() as f64
        };
        assert!(lit(21.0, 1) > 0.15, "evenings are lit");
        assert!(lit(3.5, 1) < 0.03, "the small hours are dark");
        assert!(lit(23.5, 5) > lit(23.5, 1), "Saturday stays up later than Tuesday");
        assert!(lit(7.0, 0) > lit(7.0, 6), "Monday wakes before Sunday");
    }
}

