//! Buildings placed by hand.
//!
//! A view that wants a skyline writes it as a table of `Block`s in design
//! units: where each one stands, how wide, the tiers it steps back in, what
//! sits on top, how its windows are set out and who uses it. `row` turns one
//! table into a layer: its silhouette as a mask, and every window with a
//! household or an office behind it that keeps hours of its own.

use crate::color::{mix, shade, smooth, Rgb, WARM};
use crate::gfx::{Bloom, Canvas, Draw, Shaper, Surface, P};
use crate::paint::Light;
use crate::props::Layout;
use crate::rng::hash2;
use crate::scene::Scene;
use crate::town::window_state;
use crate::views::Lights;

/// The colours walls and roofs are washed in by day. At night they all sink back into the dark.
pub const WALLS: [Rgb; 6] = [
    [230.0, 223.0, 207.0],
    [222.0, 184.0, 166.0],
    [176.0, 198.0, 178.0],
    [226.0, 200.0, 152.0],
    [172.0, 192.0, 212.0],
    [236.0, 214.0, 196.0],
];
pub const TILES: [Rgb; 3] = [
    [176.0, 106.0, 84.0],
    [122.0, 118.0, 138.0],
    [150.0, 96.0, 86.0],
];

/// Who is behind the windows, which decides when they are lit.
#[derive(Clone, Copy, PartialEq)]
pub enum Use {
    Office,
    Home,
    /// a hotel, or flats over offices
    Mixed,
}

/// What stands on the roof. Each one sits on the one before it.
#[derive(Clone, Copy)]
pub enum Cap {
    /// a plant room: its width as a share of what it stands on, and its height
    Plant(f32, f32),
    /// a pyramid over the full width
    Spire(f32),
    /// a thin mast
    Mast(f32),
    /// a roof in one slope, rising to the right, or to the left when negative
    Slant(f32),
    Dome(f32),
    /// three steps in
    Steps(f32),
    /// two shoulders with a gap between them
    Notch(f32),
    /// a pitched roof over the full width
    Gable(f32),
    /// a steep roof with a flat top, as old town houses have
    Mansard(f32),
    /// a ledge that overhangs the wall a little
    Cornice(f32),
    /// a stair head or lift room to one side: how far off centre, its width as a share, its height
    Hut(f32, f32, f32),
    /// a chimney: how far off centre, and its height
    Stack(f32, f32),
}

/// How the windows are set out.
#[derive(Clone, Copy)]
pub enum Skin {
    Bare,
    /// windows punched in a wall: spacing across and down
    Punch(f32, f32),
    /// floors of glass from edge to edge: the height of a floor
    Ribbon(f32),
    /// tall narrow lights between fins: spacing across, height of a floor
    Fins(f32, f32),
    /// the wide windows of a warehouse or a workshop
    Loft(f32, f32),
}

pub struct Block {
    /// the middle of it, and its width at the ground
    pub x: f32,
    pub w: f32,
    /// from the ground up: width as a share of `w`, height, and how far off centre as a share of `w`
    pub tiers: &'static [(f32, f32, f32)],
    pub caps: &'static [Cap],
    pub skin: Skin,
    pub who: Use,
}

/// A block laid out in screen pixels.
pub struct Shape {
    /// each tier: left, top, right, bottom
    pub tiers: Vec<[f32; 4]>,
    /// the highest point
    pub tip: P,
    /// the roof of the top tier: left, right, height
    pub roof: [f32; 3],
    pub glassy: bool,
    /// a little lighter or darker than its neighbours, so that they do not run together
    pub tone: f32,
    /// which of the wall colours it is washed in by day, and which of the roof colours
    pub hue: (usize, usize),
}

pub struct Win {
    pub x: f32,
    pub y: f32,
    pub w: f32,
    pub h: f32,
    pub who: [f64; 4],
    pub tone: u8,
    pub office: bool,
    /// part of a ribbon floor rather than a window of its own
    pub strip: bool,
}

/// One layer of a skyline.
pub struct Row {
    pub mask: (usize, Vec<u8>),
    pub blocks: &'static [Block],
    pub shapes: Vec<Shape>,
    pub wins: Vec<Win>,
}

/// A piece of outline: a closed run of points.
type Piece = Vec<P>;

fn rect(x0: f32, y0: f32, x1: f32, y1: f32) -> Piece {
    vec![(x0, y0), (x1, y0), (x1, y1), (x0, y1)]
}

/// Walk up the caps of a block, handing each piece of outline to `f`. Returns the highest point.
fn caps(b: &Block, top: [f32; 3], lay: &Layout, f: &mut dyn FnMut(Piece, bool)) -> P {
    let [mut x0, mut x1, mut y] = top;
    let mut tip = ((x0 + x1) / 2.0, y);
    for cap in b.caps {
        let mid = (x0 + x1) / 2.0;
        match *cap {
            Cap::Plant(share, h) => {
                let (half, h) = ((x1 - x0) * share / 2.0, lay.d(h));
                f(rect(mid - half, y - h, mid + half, y + 1.0), false);
                (x0, x1, y) = (mid - half, mid + half, y - h);
                tip = (mid, y);
            }
            Cap::Spire(h) => {
                let h = lay.d(h);
                f(vec![(x0, y + 1.0), (mid, y - h), (x1, y + 1.0)], true);
                (x0, x1, y) = (mid, mid, y - h);
                tip = (mid, y);
            }
            Cap::Mast(h) => {
                let (half, h) = ((lay.d(1.5) / 2.0).max(0.7), lay.d(h));
                f(rect(mid - half, y - h, mid + half, y + 2.0), false);
                (x0, x1, y) = (mid, mid, y - h);
                tip = (mid, y);
            }
            Cap::Slant(h) => {
                let rise = lay.d(h.abs());
                if h >= 0.0 {
                    f(vec![(x0, y + 1.0), (x1, y - rise), (x1, y + 1.0)], true);
                    tip = (x1, y - rise);
                } else {
                    f(vec![(x0, y + 1.0), (x0, y - rise), (x1, y + 1.0)], true);
                    tip = (x0, y - rise);
                }
            }
            Cap::Dome(h) => {
                let h = lay.d(h);
                let half = (x1 - x0) / 2.0;
                let arc: Vec<P> = (0..=18)
                    .map(|k| std::f32::consts::PI * k as f32 / 18.0)
                    .map(|a| (mid - half * a.cos(), y + 1.0 - (h + 1.0) * a.sin()))
                    .collect();
                f(arc, true);
                (x0, x1, y) = (mid, mid, y - h);
                tip = (mid, y);
            }
            Cap::Steps(h) => {
                let h = lay.d(h) / 3.0;
                for _ in 0..3 {
                    let inset = (x1 - x0) * 0.16;
                    f(rect(x0 + inset, y - h, x1 - inset, y + 1.0), false);
                    (x0, x1, y) = (x0 + inset, x1 - inset, y - h);
                }
                tip = ((x0 + x1) / 2.0, y);
            }
            Cap::Notch(h) => {
                let (h, wide) = (lay.d(h), (x1 - x0) * 0.30);
                f(rect(x0, y - h, x0 + wide, y + 1.0), false);
                f(rect(x1 - wide, y - h, x1, y + 1.0), false);
                tip = (x0 + wide / 2.0, y - h);
            }
            Cap::Gable(h) => {
                let (h, eave) = (lay.d(h), lay.d(1.6));
                f(
                    vec![(x0 - eave, y + 1.0), (mid, y - h), (x1 + eave, y + 1.0)],
                    true,
                );
                tip = (mid, y - h);
            }
            Cap::Mansard(h) => {
                let (h, inset, eave) = (lay.d(h), (x1 - x0) * 0.11, lay.d(1.4));
                f(
                    vec![
                        (x0 - eave, y + 1.0),
                        (x0 + inset, y - h),
                        (x1 - inset, y - h),
                        (x1 + eave, y + 1.0),
                    ],
                    true,
                );
                (x0, x1, y) = (x0 + inset, x1 - inset, y - h);
                tip = (mid, y);
            }
            Cap::Cornice(h) => {
                let (h, over) = (lay.d(h), lay.d(1.8));
                f(rect(x0 - over, y - h, x1 + over, y + 1.0), false);
                y -= h;
                tip = (mid, y);
            }
            Cap::Hut(off, share, h) => {
                let (c, half, h) = (mid + (x1 - x0) * off, (x1 - x0) * share / 2.0, lay.d(h));
                f(rect(c - half, y - h, c + half, y + 1.0), false);
                if y - h < tip.1 {
                    tip = (c, y - h);
                }
            }
            Cap::Stack(off, h) => {
                let (c, half, h) = (mid + (x1 - x0) * off, lay.d(2.4), lay.d(h));
                f(rect(c - half, y - h, c + half, y + 1.0), false);
                f(
                    rect(
                        c - half * 1.4,
                        y - h - lay.d(1.6),
                        c + half * 1.4,
                        y - h + 0.5,
                    ),
                    false,
                );
            }
        }
    }
    tip
}

fn shape(b: &Block, base: f32, lay: &Layout, k: u32) -> Shape {
    let mut y = lay.dy(base);
    let mut tiers = Vec::with_capacity(b.tiers.len());
    for &(share, h, off) in b.tiers {
        let (half, mid, h) = (lay.d(b.w * share) / 2.0, lay.dx(b.x + off * b.w), lay.d(h));
        tiers.push([mid - half, y - h, mid + half, y]);
        y -= h;
    }
    let last = tiers[tiers.len() - 1];
    let roof = [last[0], last[2], last[1]];
    let tip = caps(b, roof, lay, &mut |_, _| {});
    Shape {
        tiers,
        tip,
        roof,
        glassy: matches!(b.skin, Skin::Ribbon(..) | Skin::Fins(..)),
        tone: hash2(k, 977) * 2.0 - 1.0,
        hue: (
            (hash2(k, 983) * WALLS.len() as f32) as usize % WALLS.len(),
            (hash2(k, 991) * TILES.len() as f32) as usize % TILES.len(),
        ),
    }
}

fn windows(b: &Block, s: &Shape, lay: &Layout, salt: u32, out: &mut Vec<Win>) {
    let mut n = out.len() as u32;
    let who_of = |k: u32| {
        [
            hash2(k, salt + 11) as f64,
            hash2(k, salt + 23) as f64,
            hash2(k, salt + 37) as f64,
            hash2(k, salt + 41) as f64,
        ]
    };
    let office_of = |k: u32| match b.who {
        Use::Office => true,
        Use::Home => false,
        Use::Mixed => hash2(k, salt + 67) < 0.3,
    };
    for t in &s.tiers {
        let [x0, top, x1, bottom] = *t;
        let tw = x1 - x0;
        match b.skin {
            Skin::Bare => {}
            Skin::Punch(px, py) | Skin::Fins(px, py) | Skin::Loft(px, py) => {
                let (px, py) = (lay.d(px), lay.d(py));
                let (ww, wh) = match b.skin {
                    Skin::Fins(..) => (px * 0.52, py * 0.68),
                    Skin::Loft(..) => (px * 0.70, py * 0.56),
                    _ => (px * 0.46, py * 0.54),
                };
                let cols = (((tw - px * 0.4) / px).floor() as i32).max(1);
                let pad = (tw - (cols as f32 * px - (px - ww))) / 2.0;
                let mut y = top + py * 0.5;
                while y + wh < bottom - py * 0.25 {
                    for c in 0..cols {
                        n += 1;
                        out.push(Win {
                            x: x0 + pad + c as f32 * px,
                            y,
                            w: ww,
                            h: wh,
                            who: who_of(n),
                            tone: (hash2(n, salt + 53) * 4.0) as u8 & 3,
                            office: office_of(n),
                            strip: false,
                        });
                    }
                    y += py;
                }
            }
            Skin::Ribbon(floor) => {
                let fh = lay.d(floor);
                let (band, inset) = (fh * 0.44, lay.d(1.6));
                let segs = ((tw / lay.d(22.0)).round() as i32).max(1);
                let (step, gap) = ((tw - 2.0 * inset) / segs as f32, lay.d(1.2));
                let mut y = top + fh * 0.5;
                while y + band < bottom - fh * 0.2 {
                    // a floor is let to one firm: its lights go out more or less together
                    n += 1;
                    let floor_who = who_of(n);
                    for k in 0..segs {
                        n += 1;
                        let mut who = floor_who;
                        if hash2(n, salt + 71) < 0.14 {
                            who = who_of(n);
                        } else {
                            who[1] = (who[1] + 0.16 * (hash2(n, salt + 73) as f64 - 0.5))
                                .clamp(0.0, 1.0);
                        }
                        out.push(Win {
                            x: x0 + inset + k as f32 * step + gap / 2.0,
                            y,
                            w: step - gap,
                            h: band,
                            who,
                            tone: (hash2(n, salt + 53) * 4.0) as u8 & 3,
                            office: office_of(n),
                            strip: true,
                        });
                    }
                    y += fh;
                }
            }
        }
    }
}

/// Lay one row of blocks out for the screen. Nothing in it may rise above `top` (design units);
/// `extra` adds whatever else belongs to the silhouette.
pub fn row(
    sh: &mut Shaper,
    lay: &Layout,
    base: f32,
    top: f32,
    blocks: &'static [Block],
    salt: u32,
    extra: impl FnOnce(&mut Shaper, &Layout),
) -> Row {
    let (w, h) = (lay.w as usize, lay.h as usize);
    let shapes: Vec<Shape> = blocks
        .iter()
        .enumerate()
        .map(|(k, b)| shape(b, base, lay, salt + k as u32))
        .collect();
    let mut wins = Vec::new();
    let mask = if sh.begin(0.0, lay.dy(top).max(0.0), lay.w, lay.h, w, h) {
        for (b, s) in blocks.iter().zip(&shapes) {
            for t in &s.tiers {
                sh.poly(&[
                    (t[0], t[1]),
                    (t[2], t[1]),
                    (t[2], t[3] + 2.0),
                    (t[0], t[3] + 2.0),
                ]);
            }
            caps(b, s.roof, lay, &mut |piece, _| sh.poly(&piece));
            windows(b, s, lay, salt, &mut wins);
        }
        extra(sh, lay);
        sh.poly(&[
            (-2.0, lay.dy(base)),
            (lay.w + 2.0, lay.dy(base)),
            (lay.w + 2.0, lay.h + 2.0),
            (-2.0, lay.h + 2.0),
        ]);
        sh.mask(w)
    } else {
        (h, Vec::new())
    };
    Row {
        mask,
        blocks,
        shapes,
        wins,
    }
}

/// Is this point of the screen covered by a mask?
pub fn covered(mask: &(usize, Vec<u8>), w: usize, x: f32, y: f32) -> bool {
    let (y0, m) = mask;
    if x < 0.0 || y < *y0 as f32 {
        return false;
    }
    let (xi, yi) = (x as usize, y as usize - y0);
    xi < w && m.get(yi * w + xi).map_or(false, |v| *v > 128)
}

/// Add up the windows of a row that are lit just now.
pub fn lights(wins: &[Win], sc: &Scene, out: &mut Lights, lit: &mut usize) {
    for w in wins {
        // every office has a few lights nobody turned off; most of those go out with the cleaners at one
        let left_on = w.office
            && sc.el_eff < 0.5
            && (w.who[3] < 0.025 || (w.who[3] < 0.075 && (sc.t >= 15.0 || sc.t < 1.0)));
        let s = window_state(
            &w.who,
            w.office,
            w.tone as usize,
            sc.t,
            if w.office { sc.dow } else { sc.home_dow },
            sc.el_eff as f64,
        )
        .or(if left_on {
            Some(crate::color::COOL[w.tone as usize])
        } else {
            None
        });
        *lit += s.is_some() as usize;
        out.on.push(s);
    }
}

/// How one row is painted.
pub struct Paint {
    pub col: Rgb,
    /// how plainly the unlit windows show by day, 0 to 1
    pub glass: f32,
    /// how much its lit windows bloom
    pub bloom: f32,
    /// how strongly the light picks out its edges
    pub rim: f32,
    /// how far one block's colour may stray from the next, as a share of the way to black or white
    pub tone: f32,
    /// how much of its own colour each block shows by day
    pub hue: f32,
}

/// Paint a row: the silhouette, the light along the edges that face it, and the windows.
pub fn paint(
    d: &mut Draw<Canvas>,
    sc: &Scene,
    lay: &Layout,
    row: &Row,
    on: &[Option<Rgb>],
    bloom: &mut Bloom,
    p: &Paint,
) {
    let (my0, mask) = &row.mask;
    let w = lay.w as usize;
    if sc.snow > 0.3 {
        let snow = mix(p.col, crate::color::hex(sc.p.snow), 0.85);
        d.s.stamp(mask, *my0 as i32, -(lay.d(2.6)).round() as i32, |_| {
            (snow, 1.0)
        });
    } else if sc.frost > 0.05 {
        let rime = mix(p.col, crate::color::hex(sc.p.snow), 0.55);
        d.s.stamp(
            mask,
            *my0 as i32,
            -(lay.d(1.1)).round().max(1.0) as i32,
            |_| (rime, 0.75 * sc.frost),
        );
    }
    d.s.stamp(mask, *my0 as i32, 0, |_| (p.col, 1.0));
    bloom.hide(mask, w, *my0);
    if p.tone > 0.0 {
        let warm = p.hue * (0.22 + 0.78 * sc.day);
        for (b, s) in row.blocks.iter().zip(&row.shapes) {
            let col = mix(shade(p.col, p.tone * s.tone), WALLS[s.hue.0], warm);
            let roof = mix(
                shade(p.col, p.tone * s.tone - 0.085),
                TILES[s.hue.1],
                warm * 1.5,
            );
            for t in &s.tiers {
                d.rect(t[0], t[1], t[2], t[3], col, 1.0);
            }
            caps(b, s.roof, lay, &mut |piece, slates| {
                d.poly(&piece, if slates { roof } else { col }, 1.0)
            });
        }
    }

    let light = Light::of(sc, lay);
    let rim_col = mix(p.col, light.col, 0.62);
    let rim_a = (light.k * p.rim).min(0.9);
    if rim_a > 0.02 {
        let rw = lay.d(1.7).max(1.0);
        for s in &row.shapes {
            let mid = (s.tiers[0][0] + s.tiers[0][2]) / 2.0;
            let left = light.x < mid;
            // light falls off with how far round the corner the sun is
            let k = (0.45 + 0.55 * ((light.x - mid).abs() / (0.5 * lay.w)).min(1.0)) * rim_a;
            for t in &s.tiers {
                let x = if left { t[0] } else { t[2] - rw };
                d.rect(x, t[1], x + rw, t[3], rim_col, k);
                if s.glassy {
                    // glass takes the colour of the sky it faces, fading across the face
                    for (share, a) in [(0.46, 0.07), (0.30, 0.08), (0.15, 0.09)] {
                        let wide = (t[2] - t[0]) * share;
                        let gx = if left { t[0] } else { t[2] - wide };
                        d.rect(gx, t[1], gx + wide, t[3], rim_col, k * a);
                    }
                }
            }
        }
    }

    // by day a window shows the sky; as the light goes it is a dark hole in the wall
    let glass = mix(shade(p.col, -0.16), sc.sky[5], 0.60 * sc.day);
    let glass_a = 0.85 * (0.30 + 0.70 * smooth(-7.0, 4.0, sc.el_eff)) * p.glass;
    let power = (0.30 + 0.70 * sc.dark) * sc.p.bloom * p.bloom;
    for (win, state) in row.wins.iter().zip(on) {
        if win.x > lay.w || win.x + win.w < 0.0 || win.y > lay.h {
            continue;
        }
        if let Some(g) = *state {
            d.rect(
                win.x,
                win.y,
                win.x + win.w,
                win.y + win.h,
                g,
                if win.strip { 0.86 } else { 0.94 },
            );
            let pad = win.h.min(win.w) * 0.2;
            bloom.light(
                win.x - pad,
                win.y - pad,
                win.x + win.w + pad,
                win.y + win.h + pad,
                mix(g, WARM[1], 0.5),
                if win.strip { 0.5 } else { 0.74 } * power,
            );
        } else if glass_a > 0.03 {
            d.rect(
                win.x,
                win.y,
                win.x + win.w,
                win.y + win.h,
                glass,
                glass_a * if win.strip { 0.55 } else { 1.0 },
            );
        }
    }
}

// ── land drawn by hand ───────────────────────────────────────────────────────

/// A smooth line through hand-placed points.
pub fn curve(pts: &[P], steps: usize) -> Vec<P> {
    let n = pts.len();
    let mut out = Vec::with_capacity((n - 1) * steps + 1);
    for i in 0..n - 1 {
        let (p0, p1, p2, p3) = (
            pts[i.saturating_sub(1)],
            pts[i],
            pts[i + 1],
            pts[(i + 2).min(n - 1)],
        );
        for k in 0..steps {
            let t = k as f32 / steps as f32;
            let (t2, t3) = (t * t, t * t * t);
            let f = |a: f32, b: f32, c: f32, d: f32| {
                0.5 * (2.0 * b
                    + (c - a) * t
                    + (2.0 * a - 5.0 * b + 4.0 * c - d) * t2
                    + (3.0 * b - a - 3.0 * c + d) * t3)
            };
            out.push((f(p0.0, p1.0, p2.0, p3.0), f(p0.1, p1.1, p2.1, p3.1)));
        }
    }
    out.push(pts[n - 1]);
    out
}

/// The height of a line of points at `x`.
pub fn height_at(line: &[P], x: f32) -> f32 {
    for w in line.windows(2) {
        if x <= w[1].0 {
            let f = ((x - w[0].0) / (w[1].0 - w[0].0).max(1e-3)).clamp(0.0, 1.0);
            return w[0].1 + (w[1].1 - w[0].1) * f;
        }
    }
    line[line.len() - 1].1
}

pub fn to_px(lay: &Layout, pts: &[P]) -> Vec<P> {
    pts.iter().map(|p| (lay.dx(p.0), lay.dy(p.1))).collect()
}

/// Grass along the top of a bank, leaning with the wind: `n` blades between `x0` and `x1`.
pub fn grass<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, bank: &[P], (x0, x1, n): (f32, f32, u32), t: f32, col: Rgb) {
    let u = lay.u;
    let lean = sc.wind_x.clamp(-1.4, 1.4);
    let gust = (0.25 + lean.abs()).min(1.0);
    let salt = x0 as u32;
    for i in 0..n {
        let x = x0 + (x1 - x0) * hash2(i, salt + 101);
        let foot = height_at(bank, x) + 2.0;
        let tall = 9.0 + 19.0 * hash2(i, salt + 103).powf(1.6);
        let own = (hash2(i, salt + 107) - 0.5) * 0.5;
        let sway = (t * (1.3 + hash2(i, salt + 109)) + x * 0.045).sin() * 0.10 * gust;
        let bend = lean * 0.34 + own + sway;
        let (px, py, h) = (lay.dx(x), lay.dy(foot), lay.d(tall));
        d.path(&[(px, py), (px + bend * h * 0.35, py - h * 0.58), (px + bend * h, py - h)], (1.25 * u).max(0.9), col, 1.0);
    }
}
