//! Street: a block of walk-ups across a narrow street, seen from your own windowsill.
//!
//! This is the view for being indoors. You are four floors up, and the glass is between you and
//! the weather: rain falls beyond the sill and beads on the pane, and when it rains the cat comes
//! in to watch it with you. Across the way the block keeps its day. The diner opens at six and
//! the bodega never shuts, cabs and the bus go by, people walk past with dogs and umbrellas, and
//! the lamps come on.
//!
//! The street runs across the picture, so the sun comes up behind the low diner on the left,
//! and a cross street runs away from you exactly where it sets: every evening it goes down at
//! the far end of it.
//!
//!   the neighbour            yours: sits down at the desk and lights the lamp while an agent is
//!                            working, picks up the guitar while sound is playing, and keeps
//!                            hours of their own otherwise
//!   lanterns on the escape   a blue one lit for each running container
//!   a manhole in the road    steams with the processor
//!   the shops                keep their hours: diner six till one, the bar from five, the
//!                            bodega always
//!   the cat                  along the roofs when it is dry, on your sill when it rains, asleep
//!                            there at night

use std::f32::consts::{PI, TAU};

use crate::color::{gauss, hex, mix, shade, smooth, Rgb, AMBER, BEACON, BLUE, CREAM, ROSE, SAGE, SCREEN, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, Surface, P};
use crate::paint::{band, Light};
use crate::props::{self, Layout, DW};
use crate::rng::hash2;
use crate::scene::Scene;
use crate::story::draw::Tone;
use crate::story::{Damage, Snapshot};
use crate::views::episodes::street as episode;
use crate::views::kit::{self, to_px, Block, Cap, Paint, Row, Skin, Use, Win};
use crate::views::{lamp_colours, yours, Anchors, Breath, Knob, Lights, Smoke, Stage, View, Yours};

pub struct Street;

/// The sun meets the far roofs half way up the picture: the lower half is the street.
const HORIZON: f64 = 0.50;
/// Where the walls across the way meet the pavement, the edge of the pavement, and our own sill.
const BASE: f32 = 850.0;
const KERB: f32 = 876.0;
const SILL: f32 = 966.0;
/// The shop at the foot of every house, and each floor over it.
const SHOP: f32 = 100.0;
const FLOOR: f32 = 82.0;
const WIN: (f32, f32) = (34.0, 46.0);
/// The cross street: where it meets ours, and the point it runs away to. The sun sets there.
const GAP: (f32, f32) = (1290.0, 1462.0);
const VP: P = (1376.0, 548.0);
/// The lanes: where the parked cars stand, then the two that move.
const PARK: f32 = 902.0;
const LANES: [f32; 2] = [929.0, 958.0];

/// The masks, far to near: the far town and the block behind come first.
const KERBSIDE: usize = 3;
const OURS: usize = 4;

struct House {
    x0: f32,
    x1: f32,
    /// floors over the shop
    floors: usize,
    cols: usize,
    /// the colour of its brick, and of its shopfront
    brick: Rgb,
    front: Rgb,
}

impl House {
    fn top(&self) -> f32 {
        BASE - SHOP - FLOOR * self.floors as f32
    }

    /// The top of floor `f`, counting up from the one over the shop.
    fn storey(&self, f: usize) -> f32 {
        BASE - SHOP - FLOOR * (f + 1) as f32
    }

    fn col(&self, k: usize) -> f32 {
        self.x0 + (self.x1 - self.x0) * (k as f32 + 0.5) / self.cols as f32
    }
}

const fn house(x0: f32, x1: f32, floors: usize, cols: usize, brick: Rgb, front: Rgb) -> House {
    House { x0, x1, floors, cols, brick, front }
}

/// Left to right: a walk-up with a stoop, the diner, the tall one with the bodega and the water
/// tank, the neighbour's, the bookshop, the pizza place on the corner; then across the cross
/// street, a low corner house with a bar in it.
const HOUSES: [House; 7] = [
    house(-30.0, 160.0, 3, 2, [128.0, 88.0, 72.0], [128.0, 88.0, 72.0]),
    house(160.0, 400.0, 1, 3, [210.0, 194.0, 168.0], [176.0, 188.0, 194.0]),
    house(400.0, 640.0, 5, 4, [160.0, 84.0, 68.0], [66.0, 104.0, 86.0]),
    house(640.0, 890.0, 4, 4, [196.0, 162.0, 124.0], [138.0, 168.0, 190.0]),
    house(890.0, 1130.0, 3, 4, [116.0, 138.0, 158.0], [54.0, 90.0, 78.0]),
    house(1130.0, 1290.0, 4, 2, [176.0, 112.0, 86.0], [150.0, 64.0, 54.0]),
    house(1462.0, 1640.0, 2, 2, [190.0, 142.0, 88.0], [74.0, 58.0, 70.0]),
];
const STOOP: usize = 0;
const DINER: usize = 1;
const BODEGA: usize = 2;
const HOME: usize = 3;
const BOOKS: usize = 4;
const PIZZA: usize = 5;
const BAR: usize = 6;

/// The neighbour's window, where two of the ordinary ones would be.
const ROOM: [f32; 4] = [772.0, 512.0, 878.0, 576.0];
/// The fire escapes: left, right, and which house.
const ESCAPES: [(f32, f32, usize); 2] = [(463.0, 577.0, BODEGA), (648.0, 760.0, HOME)];
/// Water tanks: x, the roof they stand on, how big.
const TANKS: [(f32, f32, f32); 2] = [(500.0, 331.0, 1.0), (1572.0, 577.0, 0.7)];
/// Chimneys: x, the roof, how tall.
const CHIMNEYS: [(f32, f32, f32); 6] = [(95.0, 495.0, 26.0), (378.0, 659.0, 22.0), (424.0, 331.0, 30.0), (668.0, 413.0, 34.0), (1152.0, 413.0, 28.0), (1490.0, 577.0, 22.0)];
/// Stair heads, an air conditioner on the diner, and the pigeon loft: left, top, right, foot.
const HUTS: [[f32; 4]; 4] = [[590.0, 306.0, 632.0, 332.0], [824.0, 390.0, 872.0, 414.0], [330.0, 646.0, 360.0, 660.0], [1072.0, 470.0, 1112.0, 496.0]];
const LOFT: usize = 3;
/// Street lamps, and the traffic light at the corner.
const LAMPS: [f32; 4] = [146.0, 660.0, 1176.0, 1530.0];
const SIGNAL: f32 = 1294.0;
/// Trees: x, the middle of the crown, its size.
const TREES: [(f32, f32, f32); 2] = [(22.0, 712.0, 72.0), (1094.0, 704.0, 74.0)];
const HYDRANT: f32 = 600.0;
/// the manhole that steams while the processor works hard
const STACK: (f32, f32) = (700.0, 948.0);
/// Where the bodega's door is: some people only go that far.
const DOOR: f32 = 588.0;

/// The far side of town, pale with distance. Low where the sun comes up over the diner and at
/// the end of the cross street, where it goes down.
const FAR: (f32, f32) = (566.0, 200.0);
const MID: (f32, f32) = (612.0, 330.0);

const fn b(x: f32, w: f32, tiers: &'static [(f32, f32, f32)], caps: &'static [Cap], skin: Skin, who: Use) -> Block {
    Block { x, w, tiers, caps, skin, who }
}

use Cap::{Cornice, Dome, Gable, Hut, Mansard, Mast, Plant, Slant, Spire, Stack, Steps};
use Skin::{Bare, Fins, Loft, Punch, Ribbon};
use Use::{Home, Mixed, Office};

const FAR_BLOCKS: &[Block] = &[
    b(52.0, 44.0, &[(1.0, 128.0, 0.0)], &[Plant(0.5, 8.0)], Bare, Office),
    b(112.0, 30.0, &[(1.0, 96.0, 0.0)], &[], Bare, Office),
    b(196.0, 60.0, &[(1.0, 40.0, 0.0)], &[], Bare, Office),
    b(270.0, 40.0, &[(1.0, 54.0, 0.0)], &[Steps(6.0)], Bare, Office),
    b(342.0, 52.0, &[(1.0, 34.0, 0.0)], &[Dome(16.0)], Bare, Office),
    b(470.0, 38.0, &[(1.0, 246.0, 0.0)], &[Plant(0.5, 8.0), Mast(18.0)], Bare, Office),
    b(560.0, 30.0, &[(1.0, 286.0, 0.0)], &[Slant(16.0)], Bare, Office),
    b(684.0, 40.0, &[(1.0, 176.0, 0.0)], &[Plant(0.5, 7.0)], Bare, Office),
    // the one with the needle
    b(760.0, 40.0, &[(1.0, 172.0, 0.0), (0.7, 50.0, 0.0), (0.44, 34.0, 0.0)], &[Spire(40.0), Mast(24.0)], Bare, Office),
    b(940.0, 36.0, &[(1.0, 118.0, 0.0)], &[], Bare, Office),
    // the tall stepped one, uptown
    b(1040.0, 48.0, &[(1.0, 150.0, 0.0), (0.72, 70.0, 0.0), (0.44, 40.0, 0.0)], &[Steps(18.0), Spire(34.0), Mast(30.0)], Bare, Office),
    b(1196.0, 30.0, &[(1.0, 150.0, 0.0)], &[Plant(0.5, 6.0)], Bare, Office),
    b(1300.0, 50.0, &[(1.0, 70.0, 0.0)], &[], Bare, Office),
    b(1376.0, 70.0, &[(1.0, 46.0, 0.0)], &[], Bare, Office),
    b(1452.0, 44.0, &[(1.0, 92.0, 0.0)], &[Steps(8.0)], Bare, Office),
    b(1530.0, 34.0, &[(1.0, 74.0, 0.0)], &[], Bare, Office),
    b(1596.0, 44.0, &[(1.0, 112.0, 0.0)], &[Plant(0.4, 8.0)], Bare, Office),
];

/// The block behind the one across the way, and the towers past it.
const MID_BLOCKS: &[Block] = &[
    b(70.0, 80.0, &[(1.0, 146.0, 0.0)], &[Cornice(3.0), Hut(0.2, 0.3, 9.0)], Punch(9.0, 12.0), Home),
    b(186.0, 64.0, &[(1.0, 80.0, 0.0)], &[Cornice(3.0), Stack(0.3, 9.0)], Punch(9.0, 12.0), Home),
    b(256.0, 62.0, &[(1.0, 60.0, 0.0)], &[Gable(13.0)], Loft(11.0, 13.0), Home),
    b(324.0, 66.0, &[(1.0, 86.0, 0.0)], &[Mansard(12.0)], Punch(9.0, 12.0), Home),
    b(388.0, 56.0, &[(1.0, 70.0, 0.0)], &[Cornice(3.0)], Punch(9.0, 12.0), Home),
    b(704.0, 66.0, &[(1.0, 170.0, 0.0), (0.8, 36.0, 0.0)], &[Plant(0.5, 9.0)], Ribbon(9.6), Office),
    b(834.0, 56.0, &[(1.0, 186.0, 0.0)], &[Plant(0.5, 8.0), Mast(20.0)], Fins(7.0, 9.6), Office),
    b(946.0, 72.0, &[(1.0, 150.0, 0.0)], &[Cornice(3.0), Hut(-0.2, 0.3, 9.0)], Punch(8.0, 10.6), Home),
    b(1028.0, 58.0, &[(1.0, 124.0, 0.0)], &[Mansard(12.0)], Punch(9.0, 12.0), Home),
    b(1100.0, 60.0, &[(1.0, 168.0, 0.0)], &[Cornice(3.0)], Punch(8.0, 10.6), Mixed),
    b(1196.0, 66.0, &[(1.0, 196.0, 0.0)], &[Cornice(3.0), Stack(-0.2, 9.0)], Punch(8.0, 10.6), Mixed),
    b(1500.0, 72.0, &[(1.0, 122.0, 0.0)], &[Cornice(3.0), Stack(0.25, 9.0)], Punch(9.0, 12.0), Home),
    b(1576.0, 66.0, &[(1.0, 140.0, 0.0)], &[Gable(14.0)], Loft(11.0, 13.0), Home),
];

/// The tanks on the roofs of the block behind: x, and the roof.
const MID_TANKS: [(f32, f32); 3] = [(946.0, 459.0), (1500.0, 487.0), (70.0, 463.0)];

/// How far down the cross street each building along it ends (0 at our corner, 1 at the
/// vanishing point), and how high its roof would stand if it were at the corner.
const DOWN: [f32; 5] = [0.0, 0.36, 0.60, 0.76, 0.88];
const ALONG: [[f32; 4]; 2] = [[422.0, 470.0, 400.0, 486.0], [586.0, 470.0, 520.0, 400.0]];

// ── small tools: drawing in design units ─────────────────────────────────────

fn bar<S: Surface>(d: &mut Draw<S>, lay: &Layout, b: [f32; 4], col: Rgb, a: f32) {
    d.rect(lay.dx(b[0]), lay.dy(b[1]), lay.dx(b[2]), lay.dy(b[3]), col, a);
}

fn oval<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, y: f32, rx: f32, ry: f32, col: Rgb, a: f32) {
    d.ellipse(lay.dx(x), lay.dy(y), lay.d(rx), lay.d(ry), col, a);
}

fn ink<S: Surface>(d: &mut Draw<S>, lay: &Layout, pts: &[P], w: f32, col: Rgb, a: f32) {
    d.path(&to_px(lay, pts), lay.d(w), col, a);
}

fn box_shape(sh: &mut Shaper, lay: &Layout, x0: f32, y0: f32, x1: f32, y1: f32) {
    sh.poly(&[(lay.dx(x0), lay.dy(y0)), (lay.dx(x1), lay.dy(y0)), (lay.dx(x1), lay.dy(y1) + 1.0), (lay.dx(x0), lay.dy(y1) + 1.0)]);
}

/// Light lying on the ground: strongest along the top edge, gone at the bottom.
fn spill<S: Surface>(d: &mut Draw<S>, lay: &Layout, b: [f32; 4], col: Rgb, a: f32, mode: Blend) {
    if a <= 0.004 {
        return;
    }
    let (x0, y0, x1, y1) = (lay.dx(b[0]), lay.dy(b[1]), lay.dx(b[2]), lay.dy(b[3]));
    let (w, h) = (d.s.w(), d.s.h());
    if d.sh.begin(x0, y0, x1, y1, w, h) {
        d.sh.poly(&[(x0, y0), (x1, y0), (x1, y1), (x0, y1)]);
        let span = (y1 - y0).max(1.0);
        d.sh.fill_rows(d.s, mode, |y| (col, a * (1.0 - ((y - y0) / span).clamp(0.0, 1.0)).powf(1.7)));
    }
}

/// Letters made of a few strokes each, on a unit square, for the signs.
fn strokes(c: char) -> &'static [&'static [P]] {
    match c {
        'A' => &[&[(0.0, 1.0), (0.5, 0.0), (1.0, 1.0)], &[(0.2, 0.62), (0.8, 0.62)]],
        'B' => &[&[(0.0, 0.5), (0.7, 0.5), (1.0, 0.64), (1.0, 0.86), (0.7, 1.0), (0.0, 1.0), (0.0, 0.0), (0.62, 0.0), (0.9, 0.12), (0.9, 0.38), (0.62, 0.5)]],
        'D' => &[&[(0.0, 0.0), (0.0, 1.0), (0.55, 1.0), (1.0, 0.72), (1.0, 0.28), (0.55, 0.0), (0.0, 0.0)]],
        'E' => &[&[(1.0, 0.0), (0.0, 0.0), (0.0, 1.0), (1.0, 1.0)], &[(0.0, 0.5), (0.75, 0.5)]],
        'I' => &[&[(0.25, 0.0), (0.25, 1.0)]],
        'K' => &[&[(0.0, 0.0), (0.0, 1.0)], &[(1.0, 0.0), (0.05, 0.56), (1.0, 1.0)]],
        'L' => &[&[(0.0, 0.0), (0.0, 1.0), (1.0, 1.0)]],
        'N' => &[&[(0.0, 1.0), (0.0, 0.0), (1.0, 1.0), (1.0, 0.0)]],
        'O' => &[&[(0.3, 0.0), (0.7, 0.0), (1.0, 0.25), (1.0, 0.75), (0.7, 1.0), (0.3, 1.0), (0.0, 0.75), (0.0, 0.25), (0.3, 0.0)]],
        'P' => &[&[(0.0, 1.0), (0.0, 0.0), (0.7, 0.0), (1.0, 0.15), (1.0, 0.4), (0.7, 0.55), (0.0, 0.55)]],
        'R' => &[&[(0.0, 1.0), (0.0, 0.0), (0.7, 0.0), (1.0, 0.15), (1.0, 0.4), (0.7, 0.55), (0.0, 0.55)], &[(0.45, 0.55), (1.0, 1.0)]],
        'S' => &[&[(1.0, 0.12), (0.75, 0.0), (0.25, 0.0), (0.0, 0.14), (0.0, 0.36), (0.25, 0.5), (0.75, 0.5), (1.0, 0.64), (1.0, 0.86), (0.75, 1.0), (0.25, 1.0), (0.0, 0.88)]],
        'U' => &[&[(0.0, 0.0), (0.0, 0.78), (0.25, 1.0), (0.75, 1.0), (1.0, 0.78), (1.0, 0.0)]],
        'Y' => &[&[(0.0, 0.0), (0.5, 0.5), (1.0, 0.0)], &[(0.5, 0.5), (0.5, 1.0)]],
        'Z' => &[&[(0.0, 0.0), (1.0, 0.0), (0.0, 1.0), (1.0, 1.0)]],
        _ => &[],
    }
}

/// How wide `text` comes out at this height.
fn measure(text: &str, tall: f32) -> f32 {
    text.chars().map(|c| tall * if c == 'I' { 0.31 } else { 0.62 } + tall * 0.28).sum::<f32>() - tall * 0.28
}

/// Write `text` from its top left corner, `tall` high, in strokes `lw` wide (all in design units).
fn write<S: Surface>(d: &mut Draw<S>, lay: &Layout, text: &str, (x, y): P, tall: f32, lw: f32, col: Rgb, a: f32, mode: Blend) {
    let mut at = x;
    for c in text.chars() {
        let wide = tall * if c == 'I' { 0.31 } else { 0.62 };
        for s in strokes(c) {
            let pts: Vec<P> = s.iter().map(|p| (lay.dx(at + p.0 * tall * 0.62), lay.dy(y + p.1 * tall))).collect();
            d.path_mode(&pts, lay.d(lw), col, a, mode);
        }
        at += wide + tall * 0.28;
    }
}

/// A sign in neon tube: the glass dark by day, glowing by night.
fn neon(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, text: &str, at: P, tall: f32, col: Rgb, on: f32, bloom: &mut Bloom) {
    let tube = mix(shade(sc.layers[3], -0.1), col, 0.35 + 0.25 * sc.day);
    write(d, lay, text, at, tall, 2.0, tube, 0.9, Blend::Over);
    if on > 0.02 {
        let lit = 0.45 + 0.55 * smooth(3.0, -3.0, sc.el_eff);
        write(d, lay, text, at, tall, 5.0, col, 0.20 * on * lit, Blend::Screen);
        write(d, lay, text, at, tall, 1.8, mix(col, WHITE, 0.5), on * (0.55 + 0.45 * lit), Blend::Over);
        let w = measure(text, tall);
        bloom.light(lay.dx(at.0 - 3.0), lay.dy(at.1 - 3.0), lay.dx(at.0 + w + 3.0), lay.dy(at.1 + tall + 3.0), col, 0.55 * on * lit * sc.p.bloom.max(0.5));
    }
}

/// How much of the street is up and about, 0 to 1.
fn bustle(sc: &Scene) -> f32 {
    if let Some(v) = sc.knob("bustle") {
        return v.clamp(0.0, 1.0);
    }
    let t = sc.t as f32;
    let late = if t < 4.0 { t + 24.0 } else { t };
    let up = if t < 4.0 { 1.0 } else { smooth(5.0, 8.5, t) } * smooth(25.5, 21.0, late);
    let weather = (1.0 - 0.35 * sc.rain) * (1.0 - 0.45 * sc.snow) * if sc.storm { 0.5 } else { 1.0 };
    ((0.04 + 0.50 * up * (0.6 + 0.4 * sc.busy) + 0.42 * sc.traffic.min(1.0)) * weather).min(1.0)
}

/// How wet the street is: it shines and holds the lights upside down.
fn wet(sc: &Scene) -> f32 {
    if let Some(v) = sc.knob("wet") {
        return v.clamp(0.0, 1.0);
    }
    smooth(0.02, 0.3, sc.rain).max(if sc.storm { 1.0 } else { 0.0 }).max(0.35 * smooth(0.2, 0.7, sc.snow))
}

/// Puddles last a while on a damp day.
fn damp(sc: &Scene) -> f32 {
    wet(sc).max(0.45 * smooth(84.0, 97.0, sc.wx.humidity as f32))
}

/// Is a shop open just now?
fn open(shop: usize, sc: &Scene) -> bool {
    let t = sc.t;
    match shop {
        DINER => !(1.0..6.0).contains(&t),
        BODEGA => true,
        HOME => (7.0..23.0).contains(&t),
        BOOKS => (10.0..21.0).contains(&t) && !(sc.dow == 6 && t < 12.0),
        PIZZA => t >= 11.0 || t < 3.0,
        BAR => t >= 17.0 || t < 2.0,
        _ => false,
    }
}

/// What the lit windows of the shops look like.
fn shop_light(shop: usize) -> Rgb {
    match shop {
        DINER => [255.0, 214.0, 150.0],
        BODEGA => [226.0, 238.0, 222.0],
        HOME => [214.0, 232.0, 244.0],
        BOOKS => [255.0, 200.0, 128.0],
        PIZZA => [255.0, 178.0, 104.0],
        _ => [238.0, 120.0, 128.0],
    }
}

/// Where a shop's window is: left, top, right, bottom.
fn shop_window(shop: usize) -> [f32; 4] {
    match shop {
        DINER => [174.0, 774.0, 358.0, 836.0],
        BODEGA => [416.0, 786.0, 566.0, 838.0],
        HOME => [652.0, 770.0, 786.0, 838.0],
        BOOKS => [902.0, 776.0, 1040.0, 838.0],
        PIZZA => [1142.0, 776.0, 1240.0, 838.0],
        _ => [1508.0, 780.0, 1600.0, 836.0],
    }
}

/// How strongly the low evening sun shines down the cross street: nothing by day or after dark,
/// most as it sits at the far end between the walls.
fn evening(sc: &Scene) -> f32 {
    let s = &sc.eph.sun;
    sc.sun_vis * smooth(-2.5, 0.5, s.el as f32) * smooth(16.0, 3.0, s.el as f32) * if s.ha > 0.0 { 1.0 } else { 0.0 }
}

/// Where the light out of the cross street lies across our road: the street's own edges carried on
/// towards us, from its mouth down to `y`.
fn sunlit(y: f32, inset: f32) -> [P; 4] {
    let f = 1.0 - (y - VP.1) / (BASE - VP.1);
    let (a, b) = (down(0, f, y), down(1, f, y));
    [(GAP.0 + inset, BASE), (GAP.1 - inset, BASE), (b.0 - inset, y), (a.0 + inset, y)]
}

/// The colour of that light: the sky's own glow, pushed towards gold.
fn gold(sc: &Scene) -> Rgb {
    mix(sc.glow, [255.0, 172.0, 86.0], 0.45)
}

fn street_col(sc: &Scene) -> Rgb {
    mix(sc.layers[2], sc.layers[3], 0.40)
}

/// A wall in its own colour: plain by day, sunk into the dark at night.
fn brick(sc: &Scene, paint: Rgb) -> Rgb {
    mix(street_col(sc), paint, 0.14 + 0.30 * sc.day)
}

// ── the neighbour ────────────────────────────────────────────────────────────

/// What the neighbour across the way is doing.
#[derive(Clone, Copy, PartialEq, Debug)]
enum Doing {
    Out,
    Asleep,
    Coffee,
    Code,
    Guitar,
    Film,
    Read,
}

const DOINGS: [Doing; 7] = [Doing::Out, Doing::Asleep, Doing::Coffee, Doing::Code, Doing::Guitar, Doing::Film, Doing::Read];

fn doing(sc: &Scene) -> Doing {
    if let Some(k) = sc.knob("doing") {
        return DOINGS[(k.round().max(0.0) as usize).min(DOINGS.len() - 1)];
    }
    // they keep your hours: while an agent works, so do they, and while music plays they play along
    if sc.machine.agents > 0 {
        return Doing::Code;
    }
    if sc.machine.audio {
        return Doing::Guitar;
    }
    let t = sc.t;
    let weekend = sc.dow >= 5;
    let wake = if weekend { 9.0 } else { 6.8 };
    // some evenings the film starts earlier than others
    let film = 20.2 + (hash2((sc.day_id % 100000) as u32, 71) as f64 - 0.5) * 1.2;
    if (0.75..wake).contains(&t) {
        Doing::Asleep
    } else if t < wake + 1.4 && t >= wake {
        Doing::Coffee
    } else if !weekend && t < 17.8 && t >= wake {
        Doing::Out
    } else if weekend && (11.0..14.5).contains(&t) {
        Doing::Out
    } else if t >= film && t < 23.6 {
        Doing::Film
    } else {
        Doing::Read
    }
}

fn doing_words(what: Doing) -> &'static str {
    match what {
        Doing::Out => "out",
        Doing::Asleep => "asleep",
        Doing::Coffee => "at the window with a coffee",
        Doing::Code => "at the desk, working by the lamp",
        Doing::Guitar => "playing the guitar",
        Doing::Film => "watching a film",
        Doing::Read => "reading on the sofa",
    }
}

/// Is the room lit, and how: the lamp they work by, the ceiling light, the television.
fn room_light(sc: &Scene, what: Doing) -> (Rgb, f32) {
    let dusk = smooth(3.0, -3.0, sc.el_eff);
    let (lamp, _, _) = lamp_colours(sc);
    match what {
        Doing::Out | Doing::Asleep => ([0.0; 3], 0.0),
        Doing::Code => (lamp, 0.30 + 0.22 * dusk),
        Doing::Film => ([96.0, 120.0, 168.0], 0.15 + 0.40 * dusk),
        _ => (WARM[0], 0.25 + 0.42 * dusk),
    }
}

/// The neighbour, and the things in the room that move: drawn by the live layer, or once into the
/// still picture. `t` runs in seconds.
fn neighbour<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, t: f32) {
    let what = doing(sc);
    let [x0, y0, x1, y1] = ROOM;
    let col = street_col(sc);
    let (glow, k) = room_light(sc, what);
    // against a lit room they are a shape; in a dim one by day, the paler thing in it
    let back = k > 0.45 || sc.day < 0.4;
    let body = if back { shade(col, -0.50) } else { mix(col, CREAM, 0.28) };
    let jumper = mix(body, AMBER, if back { 0.30 } else { 0.55 });
    let skin = if back { mix(body, [214.0, 170.0, 140.0], 0.18) } else { mix(col, [226.0, 190.0, 160.0], 0.55) };
    let hair = shade(body, -0.25);
    let head = |d: &mut Draw<S>, x: f32, y: f32| {
        oval(d, lay, x, y, 4.3, 4.6, skin, 1.0);
        oval(d, lay, x - 0.6, y - 1.6, 4.6, 3.4, hair, 1.0);
    };
    match what {
        Doing::Code => {
            // code going by on the screen, a line or two at a time
            for i in 0..6u32 {
                let lead = (t * 1.3) as u32;
                let len = 3.0 + 8.0 * hash2(i + lead, 31);
                let y = 537.0 + 2.6 * i as f32;
                let indent = 2.0 * (hash2(i + lead, 37) * 3.0).floor();
                bar(d, lay, [857.0 + indent, y, (857.0 + indent + len).min(870.0), y + 1.0], mix(SCREEN, [30.0, 40.0, 60.0], 0.6), 0.55);
            }
            let typing = if (t * 0.21).sin() > -0.4 { ((t * 11.0).sin() * 0.7).round() } else { 0.0 };
            ink(d, lay, &[(827.0, 546.0), (827.0, 566.0)], 2.2, body, 1.0);
            ink(d, lay, &[(833.0, 547.0), (832.0, 561.0)], 9.0, jumper, 1.0);
            ink(d, lay, &[(832.0, 562.0), (841.0, 563.0), (841.0, 575.0)], 4.0, body, 1.0);
            ink(d, lay, &[(834.0, 550.0), (841.0, 556.0), (848.0 + typing, 556.0)], 2.6, jumper, 1.0);
            head(d, 835.0, 538.5);
            if sc.machine.audio {
                ink(d, lay, &[(830.0, 538.0), (834.5, 532.0), (839.5, 537.0)], 1.2, shade(body, -0.3), 1.0);
                oval(d, lay, 839.0, 539.0, 1.6, 2.4, shade(body, -0.3), 1.0);
            }
            d.glow(lay.dx(840.0), lay.dy(540.0), lay.d(8.0), lay.d(8.0), SCREEN, 0.30 * (1.0 - sc.day * 0.5), 2, Blend::Screen);
        }
        Doing::Guitar => {
            let nod = (t * 1.8).sin() * 0.5;
            let strum = (t * 6.5).sin() * 2.4;
            ink(d, lay, &[(806.0, 548.0), (806.0, 562.0)], 9.5, jumper, 1.0);
            ink(d, lay, &[(806.0, 563.0), (815.0, 564.0), (815.0, 576.0)], 4.2, body, 1.0);
            let wood = mix(body, [196.0, 140.0, 88.0], if back { 0.35 } else { 0.6 });
            oval(d, lay, 811.0, 558.0, 6.4, 5.0, wood, 1.0);
            oval(d, lay, 811.0, 558.0, 1.6, 1.6, shade(wood, -0.5), 1.0);
            ink(d, lay, &[(815.0, 555.0), (830.0, 545.0)], 1.8, shade(wood, -0.3), 1.0);
            ink(d, lay, &[(806.0, 551.0), (821.0, 549.0), (826.0, 547.0)], 2.4, jumper, 1.0);
            ink(d, lay, &[(806.0, 550.0), (810.0, 553.0), (812.0, 556.0 + strum * 0.4)], 2.6, jumper, 1.0);
            head(d, 807.0, 539.5 + nod);
        }
        Doing::Film => {
            // the television's light, on them and the room
            let flick = 0.10 + 0.20 * hash2((t * 5.0) as u32, 41) + 0.10 * hash2((t * 1.3) as u32, 43);
            d.rect_mode(lay.dx(x0), lay.dy(y0), lay.dx(x1), lay.dy(y1), [150.0, 196.0, 236.0], flick * (0.4 + 0.6 * sc.dark), Blend::Screen);
            ink(d, lay, &[(787.0, 557.0), (797.0, 562.0)], 9.5, jumper, 1.0);
            ink(d, lay, &[(797.0, 564.0), (814.0, 564.0), (818.0, 561.0)], 4.6, body, 1.0);
            oval(d, lay, 801.0, 558.0, 3.6, 2.2, mix(body, CREAM, 0.5), 1.0);
            head(d, 784.0, 551.0);
        }
        Doing::Coffee => {
            let sip = ((t * 0.09).fract() < 0.09) as i32 as f32;
            ink(d, lay, &[(818.0, 542.0), (818.0, 576.0)], 12.0, jumper, 1.0);
            let mug = (823.0, 547.0 - 5.0 * sip);
            ink(d, lay, &[(822.0, 545.0), (826.0, 552.0), (mug.0 + 1.0, mug.1 + 2.0)], 2.8, jumper, 1.0);
            bar(d, lay, [mug.0 - 1.5, mug.1 - 2.5, mug.0 + 3.5, mug.1 + 2.5], mix(CREAM, body, 0.3), 1.0);
            if sip == 0.0 {
                for k in 0..2 {
                    let kf = k as f32;
                    let wisp: Vec<P> = (0..5).map(|i| (mug.0 + 1.0 + (t * 1.6 + i as f32 * 0.9 + kf * 2.0).sin() * 1.2, mug.1 - 3.5 - i as f32 * 2.4)).collect();
                    ink(d, lay, &wisp, 0.9, WHITE, 0.22);
                }
            }
            head(d, 818.5, 533.5 - sip * 0.6);
        }
        Doing::Read => {
            ink(d, lay, &[(796.0, 547.0), (795.0, 562.0)], 9.0, jumper, 1.0);
            ink(d, lay, &[(795.0, 563.0), (808.0, 564.0), (810.0, 575.0)], 4.2, body, 1.0);
            d.poly(&to_px(lay, &[(801.0, 549.0), (807.0, 546.5), (808.5, 553.0), (802.0, 555.0)]), mix(CREAM, glow, 0.3), 1.0);
            ink(d, lay, &[(797.0, 551.0), (803.0, 554.0)], 2.4, jumper, 1.0);
            head(d, 797.5, 539.0 + (t * 0.05).sin() * 0.4);
        }
        Doing::Out => {}
        Doing::Asleep => {
            if sc.sleepers > 0 {
                props::sleep_z(d, sc, lay.dx(866.0), lay.dy(506.0), lay.u * 0.8, t * 0.8);
            }
        }
    }
    // the window's own bars are nearer than whoever is in the room
    let frame = shade(col, -0.38);
    for x in [806.5, 842.5] {
        bar(d, lay, [x, y0, x + 1.8, y1], frame, 1.0);
    }
    bar(d, lay, [x0, y0 + 15.0, x1, y0 + 16.6], frame, 0.9);
}

// ── who goes by ──────────────────────────────────────────────────────────────

#[derive(Clone, Copy, PartialEq)]
enum Gait {
    Stroll,
    Bag,
    Dog,
    Jog,
}

struct Walker {
    x: f32,
    y: f32,
    way: f32,
    gait: Gait,
    k: u32,
    a: f32,
}

/// Each slot sends someone along now and then: seconds from one chance to the next.
const PACES: [f64; 8] = [67.0, 79.0, 97.0, 113.0, 131.0, 157.0, 181.0, 211.0];

/// Who is on the pavement across the way at this instant.
fn walkers(sc: &Scene, now: f64) -> Vec<Walker> {
    let p = (0.03 + 0.85 * bustle(sc)) * (1.0 - 0.3 * sc.rain) * if sc.storm { 0.4 } else { 1.0 };
    let t = sc.t as f32;
    let dogs = gauss(t, 7.4, 1.0).max(gauss(t, 19.5, 1.4));
    let mut out = Vec::new();
    for (i, every) in PACES.iter().enumerate() {
        let c = (now / every).floor();
        let k = ((c as i64).rem_euclid(1 << 20) as u32) * 8 + i as u32;
        if hash2(k, 701) >= p {
            continue;
        }
        let h = hash2(k, 703);
        let gait = if h < 0.10 + 0.25 * dogs {
            Gait::Dog
        } else if h > 0.97 - 0.3 * gauss(t, 6.9, 0.9) && sc.rain < 0.3 && sc.snow < 0.2 {
            Gait::Jog
        } else if h > 0.60 {
            Gait::Bag
        } else {
            Gait::Stroll
        };
        let speed = if gait == Gait::Jog { 72.0 } else { 27.0 + 15.0 * hash2(k, 707) };
        let way = if hash2(k, 709) < 0.5 { -1.0 } else { 1.0 };
        let go = (now - c * every - 2.0) as f32;
        let run = go * speed;
        if go < 0.0 || run > DW + 140.0 {
            continue;
        }
        let mut x = if way > 0.0 { -60.0 + run } else { DW + 60.0 - run };
        let mut a = 1.0;
        if gait == Gait::Bag {
            let errand = hash2(k, 711);
            if errand < 0.35 {
                // only going as far as the bodega
                let left = (DOOR - x) * way;
                if left < 0.0 {
                    continue;
                }
                a = (left / 14.0).min(1.0);
            } else if errand < 0.6 {
                // and someone coming out of it
                x = DOOR + way * run;
                a = (run / 14.0).min(1.0);
                if x < -80.0 || x > DW + 80.0 {
                    continue;
                }
            }
        }
        out.push(Walker { x, y: KERB - 10.0 + 6.0 * hash2(k, 713), way, gait, k, a });
    }
    out
}

const COATS: [Rgb; 7] = [[186.0, 92.0, 84.0], [212.0, 168.0, 92.0], [120.0, 150.0, 132.0], [104.0, 132.0, 170.0], [214.0, 204.0, 184.0], [130.0, 96.0, 128.0], [70.0, 76.0, 86.0]];
const BROLLIES: [Rgb; 5] = [[60.0, 66.0, 76.0], [200.0, 88.0, 80.0], [214.0, 176.0, 84.0], [96.0, 130.0, 176.0], [226.0, 218.0, 200.0]];

/// Someone walking past, feet at (x, y) in design units.
fn walker<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, w: &Walker) {
    let (x, y, way, k, a) = (w.x, w.y, w.way, w.k, w.a);
    if x < -40.0 || x > DW + 40.0 || a <= 0.02 {
        return;
    }
    let near = sc.layers[4];
    let tint = 0.18 + 0.50 * sc.day;
    let legs = shade(near, -0.15);
    let coat = mix(near, COATS[k as usize % COATS.len()], tint);
    let skin = mix(near, [222.0, 186.0, 156.0], tint * 0.8);
    let jog = w.gait == Gait::Jog;
    let stride = x / if jog { 11.0 } else { 7.5 };
    let (swing, reach) = (stride.sin(), if jog { 9.5 } else { 6.0 });
    let lift = if jog { 3.0 * stride.cos().abs() } else { 0.0 };
    let (hx, hy) = (x + way * if jog { 2.0 } else { 0.0 }, y - 19.0 - lift);
    ink(d, lay, &[(hx, hy), (x + swing * reach, y - lift * 0.3)], 3.4, legs, a);
    ink(d, lay, &[(hx, hy), (x - swing * reach, y - lift * 0.3)], 3.4, legs, a);
    let lean = if jog { way * 3.0 } else { 0.0 };
    ink(d, lay, &[(hx + lean, y - 35.0 - lift), (hx, y - 20.0 - lift)], 10.5, coat, a);
    let arm = shade(coat, -0.12);
    let brolly = sc.rain > 0.12 && !jog;
    if brolly {
        // one hand up on the umbrella
        let (ux, uy) = (hx + way * 3.0, y - 51.0);
        ink(d, lay, &[(hx + way * 2.0, y - 32.0), (hx + way * 4.0, y - 37.0), (ux, uy + 6.0)], 2.6, arm, a);
        ink(d, lay, &[(ux, uy + 9.0), (ux, uy)], 1.0, legs, a);
        let canopy = mix(near, BROLLIES[(k / 3) as usize % BROLLIES.len()], 0.25 + 0.5 * sc.day);
        let dome: Vec<P> = (0..=12).map(|i| PI * i as f32 / 12.0).map(|t| (ux - 16.0 * t.cos(), uy - 9.0 * t.sin())).collect();
        d.poly(&to_px(lay, &dome), canopy, a);
        for i in 0..4 {
            let f = -1.0 + 2.0 * i as f32 / 3.0;
            oval(d, lay, ux + f * 16.0 * 0.86, uy + 0.6, 2.3, 1.4, canopy, a);
        }
    } else {
        ink(d, lay, &[(hx, y - 33.0 - lift), (hx - swing * reach * 0.8, y - 21.0 - lift)], 2.6, arm, a);
    }
    ink(d, lay, &[(hx, y - 33.0 - lift), (hx + swing * reach * 0.7, y - 21.0 - lift)], 2.6, arm, a);
    let (headx, heady) = (hx + lean * 1.4 + way * 0.8, y - 41.5 - lift);
    oval(d, lay, headx, heady, 4.3, 4.6, skin, a);
    let hat = if sc.temp < 6.0 || hash2(k, 715) < 0.3 { mix(near, COATS[(k as usize + 3) % COATS.len()], tint) } else { shade(near, -0.2) };
    oval(d, lay, headx - way * 0.5, heady - 2.0, 4.7, 3.0, hat, a);
    match w.gait {
        Gait::Bag => {
            let side = hx - swing * reach * 0.8;
            bar(d, lay, [side - 4.0, y - 25.0 - lift, side + 4.0, y - 16.0], mix(near, [214.0, 196.0, 160.0], tint), a);
        }
        Gait::Dog => {
            let (dx, dy) = (x + way * 26.0, y);
            let fur = mix(near, [176.0, 140.0, 104.0], tint);
            let trot = (x / 4.0).sin() * 3.0;
            for (o, s) in [(-5.5, 1.0), (5.5, -1.0)] {
                ink(d, lay, &[(dx + o, dy - 7.0), (dx + o + s * trot, dy)], 1.8, fur, a);
            }
            ink(d, lay, &[(dx - 6.0, dy - 9.0), (dx + 6.0, dy - 9.0)], 7.0, fur, a);
            oval(d, lay, dx + way * 9.5, dy - 12.5, 3.6, 3.2, fur, a);
            ink(d, lay, &[(dx - way * 7.5, dy - 10.0), (dx - way * 11.0, dy - 15.0 + trot * 0.4)], 1.4, fur, a);
            ink(d, lay, &[(hx + swing * reach * 0.7, y - 21.0), (dx + way * 6.0, dy - 11.0)], 0.7, legs, 0.8 * a);
        }
        _ => {}
    }
}

#[derive(Clone, Copy, PartialEq)]
enum Ride {
    Cab,
    Saloon,
    Hatch,
    Van,
    Bike,
    Bus,
}

impl Ride {
    /// how long and how tall, in design units
    fn size(self) -> (f32, f32) {
        match self {
            Ride::Cab => (116.0, 36.0),
            Ride::Saloon => (114.0, 35.0),
            Ride::Hatch => (96.0, 36.0),
            Ride::Van => (132.0, 50.0),
            Ride::Bike => (46.0, 42.0),
            Ride::Bus => (292.0, 76.0),
        }
    }

    /// Its side, from the tail (0) to the nose (1) and from the road (0) to the roof (1).
    fn outline(self) -> &'static [P] {
        match self {
            Ride::Cab | Ride::Saloon => &[(0.0, 0.20), (0.0, 0.52), (0.03, 0.60), (0.17, 0.64), (0.30, 0.96), (0.35, 1.0), (0.60, 1.0), (0.66, 0.95), (0.79, 0.64), (0.95, 0.58), (1.0, 0.48), (1.0, 0.20)],
            Ride::Hatch => &[(0.0, 0.20), (0.0, 0.60), (0.05, 0.92), (0.10, 1.0), (0.56, 1.0), (0.62, 0.96), (0.76, 0.64), (0.95, 0.57), (1.0, 0.46), (1.0, 0.20)],
            Ride::Van => &[(0.0, 0.16), (0.0, 0.95), (0.03, 1.0), (0.72, 1.0), (0.76, 0.97), (0.87, 0.60), (0.98, 0.54), (1.0, 0.42), (1.0, 0.16)],
            Ride::Bus => &[(0.0, 0.12), (0.0, 0.96), (0.012, 1.0), (0.985, 1.0), (1.0, 0.94), (1.0, 0.12)],
            Ride::Bike => &[],
        }
    }

    fn glass(self) -> &'static [&'static [P]] {
        match self {
            Ride::Cab | Ride::Saloon => &[&[(0.21, 0.66), (0.32, 0.92), (0.45, 0.92), (0.45, 0.66)], &[(0.47, 0.66), (0.47, 0.92), (0.59, 0.92), (0.74, 0.66)]],
            Ride::Hatch => &[&[(0.09, 0.66), (0.12, 0.92), (0.33, 0.92), (0.33, 0.66)], &[(0.35, 0.66), (0.35, 0.92), (0.55, 0.92), (0.70, 0.66)]],
            Ride::Van => &[&[(0.63, 0.62), (0.64, 0.92), (0.73, 0.92), (0.85, 0.62)]],
            _ => &[],
        }
    }

    /// where its wheels are along it, and how big as a share of its height
    fn wheels(self) -> ([f32; 2], f32) {
        match self {
            Ride::Van => ([0.16, 0.82], 0.15),
            Ride::Bus => ([0.15, 0.80], 0.11),
            Ride::Hatch => ([0.19, 0.80], 0.19),
            _ => ([0.17, 0.80], 0.20),
        }
    }
}

struct Car {
    nose: f32,
    base: f32,
    way: f32,
    ride: Ride,
    k: u32,
    big: f32,
    moving: bool,
}

impl Car {
    fn at(&self, lay: &Layout, a: f32, b: f32) -> P {
        let (len, tall) = self.ride.size();
        (lay.dx(self.nose - self.way * (1.0 - a) * len * self.big), lay.dy(self.base - b * tall * self.big))
    }
}

/// The cars parked along the far kerb: the nose, what it is, and its colour.
const PARKED: [(f32, Ride, u32); 3] = [(250.0, Ride::Saloon, 2), (700.0, Ride::Hatch, 4), (1530.0, Ride::Van, 1)];

fn parked() -> impl Iterator<Item = Car> {
    PARKED.iter().map(|&(nose, ride, k)| Car { nose, base: PARK, way: -1.0, ride, k, big: 0.94, moving: false })
}

/// The far lane runs to the left, the near one to the right.
const SPEED: [f32; 2] = [182.0, 204.0];
/// Seconds between the gaps a car may come through, so that no two in a lane ever touch.
const CELL: f64 = 5.5;

/// When the bus comes through, as seconds between buses, or none in the small hours.
fn bus_every(sc: &Scene) -> Option<f64> {
    let t = sc.t;
    if (1.5..5.0).contains(&t) {
        None
    } else if (6.0..21.0).contains(&t) {
        Some(if sc.dow < 5 { 600.0 } else { 900.0 })
    } else {
        Some(1200.0)
    }
}

/// How far the bus has come along the near lane, wherever it is, while it is anywhere near.
fn bus_run(sc: &Scene, now: f64) -> Option<f32> {
    let every = bus_every(sc)?;
    let c = (now / every).floor();
    let run = ((now - c * every - 41.0) * SPEED[1] as f64) as f32;
    (-700.0..DW + 1100.0).contains(&run).then_some(run)
}

/// What is driving past at this instant.
fn traffic(sc: &Scene, now: f64) -> Vec<Car> {
    let p = ((0.05 + 0.62 * bustle(sc)) * if sc.snow > 0.3 { 0.6 } else { 1.0 }).min(0.8);
    let mut out = Vec::new();
    let bus = bus_run(sc, now);
    for lane in 0..2 {
        let v = SPEED[lane] as f64;
        let cell = (now / CELL).floor() as i64;
        for c in (cell - 3)..=cell {
            let k = (c.rem_euclid(1 << 20) as u32) * 2 + lane as u32;
            if hash2(k, 911) >= p {
                continue;
            }
            let ride = match hash2(k, 917) {
                h if h < 0.36 => Ride::Cab,
                h if h < 0.68 => Ride::Saloon,
                h if h < 0.82 => Ride::Hatch,
                h if h < 0.92 => Ride::Van,
                _ => Ride::Bike,
            };
            let big = if lane == 0 { 0.95 } else { 1.04 };
            let run = ((now - c as f64 * CELL - 2.0 * hash2(k, 913) as f64) * v) as f32;
            if run < 0.0 || run > DW + ride.size().0 * big + 140.0 {
                continue;
            }
            // the bus has the near lane to itself
            if let (1, Some(b)) = (lane, bus) {
                if (run - b).abs() < 440.0 {
                    continue;
                }
            }
            let (nose, way) = if lane == 0 { (DW + 60.0 - run, -1.0) } else { (-60.0 + run, 1.0) };
            out.push(Car { nose, base: LANES[lane], way, ride, k, big, moving: true });
        }
    }
    if let Some(run) = bus {
        if run > 0.0 && run < DW + 420.0 {
            out.push(Car { nose: -60.0 + run, base: LANES[1], way: 1.0, ride: Ride::Bus, k: 0, big: 1.0, moving: true });
        }
    }
    out
}

const PAINTS: [Rgb; 6] = [[178.0, 182.0, 188.0], [146.0, 62.0, 58.0], [62.0, 82.0, 116.0], [206.0, 204.0, 196.0], [72.0, 104.0, 86.0], [52.0, 56.0, 64.0]];

/// A car, a van, the bus, or a courier on a bike.
fn vehicle<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, c: &Car) {
    let near = sc.layers[4];
    let dusk = smooth(2.0, -4.0, sc.el_eff);
    let beams = c.moving && (dusk > 0.25 || sc.rain > 0.15 || sc.fog > 0.3);
    let wet = wet(sc);
    let (len, tall) = c.ride.size();
    let s = c.big;
    if c.ride == Ride::Bike {
        return cyclist(d, sc, lay, c, beams);
    }
    let paint = match c.ride {
        Ride::Cab => [230.0, 184.0, 58.0],
        Ride::Bus => [208.0, 216.0, 224.0],
        _ => PAINTS[c.k as usize % PAINTS.len()],
    };
    let body = mix(near, paint, 0.26 + 0.50 * sc.day + 0.12 * dusk);
    let glass = mix(shade(near, -0.25), sc.sky[5], 0.45 * sc.day);
    let at = |a: f32, b: f32| c.at(lay, a, b);
    // the dark under it
    let (p0, p1) = (at(0.03, 0.0), at(0.97, 0.0));
    d.rect(p0.0.min(p1.0), p0.1 - lay.d(2.0 * s), p0.0.max(p1.0), p0.1 + lay.d(1.5 * s), shade(near, -0.5), 0.5);
    let shape: Vec<P> = c.ride.outline().iter().map(|p| at(p.0, p.1)).collect();
    d.poly(&shape, body, 1.0);
    for g in c.ride.glass() {
        let pane: Vec<P> = g.iter().map(|p| at(p.0, p.1)).collect();
        d.poly(&pane, glass, 1.0);
    }
    let trim = shade(body, -0.30);
    let (a0, a1) = (at(0.0, 0.34), at(1.0, 0.34));
    d.line(a0, a1, lay.d(1.0 * s), trim, 0.6);
    if snowy(sc) && !c.moving {
        let roof: Vec<P> = [(0.30, 0.99), (0.62, 0.99), (0.60, 1.07), (0.33, 1.07)].iter().map(|p| at(p.0, p.1)).collect();
        d.poly(&roof, hex(sc.p.snow), 0.85);
    }
    match c.ride {
        Ride::Cab => {
            for i in 0..8 {
                let a = 0.22 + 0.06 * i as f32;
                if i % 2 == 0 {
                    let (q0, q1) = (at(a, 0.44), at(a + 0.06, 0.50));
                    d.rect(q0.0.min(q1.0), q1.1, q0.0.max(q1.0), q0.1, trim, 0.7);
                }
            }
            let free = hash2(c.k, 919) < 0.5;
            let (r0, r1) = (at(0.43, 1.0), at(0.55, 1.10));
            let lamp = if free && dusk > 0.2 { mix(AMBER, WHITE, 0.4) } else { shade(body, 0.1) };
            d.rect(r0.0.min(r1.0), r1.1, r0.0.max(r1.0), r0.1, lamp, 1.0);
            if free && dusk > 0.2 {
                d.glow((r0.0 + r1.0) / 2.0, (r0.1 + r1.1) / 2.0, lay.d(9.0), lay.d(6.0), AMBER, 0.4 * dusk, 2, Blend::Screen);
            }
        }
        Ride::Saloon | Ride::Hatch => {
            let (q0, q1) = (at(0.46, 0.36), at(0.46, 0.92));
            d.line(q0, q1, lay.d(0.8), trim, 0.6);
        }
        Ride::Van => {
            let (q0, q1) = (at(0.05, 0.52), at(0.58, 0.62));
            d.rect(q0.0.min(q1.0), q1.1, q0.0.max(q1.0), q0.1, mix(body, ROSE, 0.35), 0.8);
        }
        Ride::Bus => {
            let (q0, q1) = (at(0.0, 0.30), at(1.0, 0.38));
            d.rect(q0.0.min(q1.0), q1.1, q0.0.max(q1.0), q0.1, mix(body, BLUE, 0.6), 1.0);
            let inside = mix(glass, [240.0, 236.0, 212.0], 0.9 * dusk);
            for i in 0..8 {
                let a = 0.04 + 0.108 * i as f32;
                let (w0, w1) = (at(a, 0.50), at(a + 0.094, 0.86));
                d.rect(w0.0.min(w1.0), w1.1, w0.0.max(w1.0), w0.1, inside, 1.0);
                if hash2(c.k + i, (sc.ts / 600.0) as u32) < 0.45 {
                    let hd = at(a + 0.047, 0.62);
                    d.circle(hd.0, hd.1, lay.d(4.0), shade(near, -0.2), 0.9);
                }
            }
            let (w0, w1) = (at(0.93, 0.42), at(0.995, 0.88));
            d.rect(w0.0.min(w1.0), w1.1, w0.0.max(w1.0), w0.1, inside, 1.0);
            let (r0, r1) = (at(0.90, 0.90), at(0.99, 0.97));
            d.rect(r0.0.min(r1.0), r1.1, r0.0.max(r1.0), r0.1, mix(AMBER, WHITE, 0.2), 0.4 + 0.6 * dusk);
            if dusk > 0.1 {
                let (m0, m1) = (at(0.04, 0.68), at(0.90, 0.68));
                d.glow((m0.0 + m1.0) / 2.0, m0.1, (m1.0 - m0.0).abs() * 0.55, lay.d(26.0), [240.0, 236.0, 212.0], 0.10 * dusk, 1, Blend::Screen);
            }
        }
        Ride::Bike => {}
    }
    let (wa, wr) = c.ride.wheels();
    let r = lay.d(wr * tall * s);
    let tyre = shade(near, -0.45);
    for a in wa {
        let p = at(a, 0.0);
        d.circle(p.0, p.1 - r, r, tyre, 1.0);
        d.circle(p.0, p.1 - r, r * 0.42, mix(tyre, CREAM, 0.25), 1.0);
    }
    if beams {
        let (hx, hy) = at(0.99, if c.ride == Ride::Bus { 0.22 } else { 0.48 });
        let warm = [255.0, 240.0, 206.0];
        let road = lay.dy(c.base);
        d.poly_mode(&[(hx, hy - lay.d(2.0)), (hx + c.way * lay.d(150.0), road + lay.d(1.0)), (hx + c.way * lay.d(40.0), road + lay.d(4.0))], warm, 0.12, Blend::Screen);
        d.glow(hx, hy, lay.d(12.0), lay.d(8.0), warm, 0.7, 2, Blend::Screen);
        d.dot(hx, hy, lay.d(2.2), lay.d(1.8), WHITE, 1.0, Blend::Over);
        let (tx, ty) = at(0.008, if c.ride == Ride::Bus { 0.24 } else { 0.52 });
        d.glow(tx, ty, lay.d(9.0), lay.d(6.0), BEACON, 0.6, 2, Blend::Screen);
        d.dot(tx, ty, lay.d(1.8), lay.d(1.8), mix(BEACON, WHITE, 0.2), 1.0, Blend::Over);
        if wet > 0.05 {
            // the lights again, upside down in the wet road
            let (hxd, txd) = (c.nose - c.way * 0.01 * len * s, c.nose - c.way * 0.99 * len * s);
            spill(d, lay, [hxd - 3.0, c.base + 1.0, hxd + 3.0, c.base + 40.0], warm, 0.40 * wet, Blend::Screen);
            spill(d, lay, [txd - 2.5, c.base + 1.0, txd + 2.5, c.base + 30.0], BEACON, 0.35 * wet, Blend::Screen);
        }
    }
    if c.moving && wet > 0.3 && sc.rain > 0.1 {
        // spray off the back wheel
        let p = at(wa[0] - 0.08, 0.05);
        d.puff(p.0, p.1 - lay.d(4.0), lay.d(18.0), lay.d(7.0), mix(sc.sky[5], sc.haze, 0.5), 0.14 * wet, 0.2);
    }
}

/// A courier on a bike, with the food in a box on their back.
fn cyclist<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, c: &Car, beams: bool) {
    let near = sc.layers[4];
    let tint = 0.2 + 0.5 * sc.day;
    let (x, y, w) = (c.nose, c.base, c.way);
    let r = 9.5;
    let frame = shade(near, -0.3);
    let front = (x - w * r, y - r);
    let back = (x - w * (r + 30.0), y - r);
    for (cx, cy) in [front, back] {
        let ring: Vec<P> = (0..=16).map(|i| TAU * i as f32 / 16.0).map(|a| (cx + r * a.cos(), cy + r * a.sin())).collect();
        ink(d, lay, &ring, 1.5, frame, 1.0);
    }
    let crank = (x - w * (r + 15.0), y - r);
    let seat = (x - w * (r + 21.0), y - r - 17.0);
    let head_tube = (x - w * (r + 3.0), y - r - 15.0);
    ink(d, lay, &[back, crank, seat, back], 1.4, frame, 1.0);
    ink(d, lay, &[crank, head_tube, front], 1.4, frame, 1.0);
    ink(d, lay, &[seat, head_tube], 1.4, frame, 1.0);
    let turn = x / r;
    let pedal = (crank.0 + 6.0 * turn.cos(), crank.1 + 6.0 * turn.sin());
    let jacket = mix(near, COATS[c.k as usize % COATS.len()], tint);
    let shoulder = (x - w * (r + 11.0), y - r - 31.0);
    ink(d, lay, &[seat, (seat.0 + w * 8.0, seat.1 - 6.0 + 4.0 * turn.sin()), pedal], 3.0, shade(near, -0.15), 1.0);
    ink(d, lay, &[seat, shoulder], 9.0, jacket, 1.0);
    ink(d, lay, &[shoulder, (head_tube.0, head_tube.1 - 2.0)], 2.4, jacket, 1.0);
    let bag = mix(near, if c.k % 2 == 0 { [214.0, 170.0, 86.0] } else { [120.0, 170.0, 140.0] }, tint + 0.1);
    bar(d, lay, [shoulder.0.min(shoulder.0 - w * 16.0), y - r - 40.0, shoulder.0.max(shoulder.0 - w * 16.0), y - r - 24.0], bag, 1.0);
    oval(d, lay, shoulder.0 + w * 3.5, y - r - 38.0, 4.2, 4.4, mix(near, CREAM, tint * 0.6), 1.0);
    if beams {
        let on = ((sc.ts as f32 + x * 0.01) * 2.0).fract() < 0.5 || !sc.live;
        let (hx, hy) = (lay.dx(head_tube.0 + w * 2.0), lay.dy(head_tube.1 + 2.0));
        if on {
            d.glow(hx, hy, lay.d(8.0), lay.d(8.0), WHITE, 0.7, 2, Blend::Screen);
        }
        d.dot(lay.dx(back.0 - w * 4.0), lay.dy(back.1 - 9.0), lay.d(1.6), lay.d(1.6), BEACON, 1.0, Blend::Over);
    }
}

fn snowy(sc: &Scene) -> bool {
    sc.snow > 0.3
}

/// Where the traffic light is in its cycle: 0 red, 1 amber, 2 green; and whether people may cross.
fn signal_at(now: f64) -> (usize, bool, bool) {
    let p = now.rem_euclid(64.0);
    let lens = if p < 28.0 {
        2
    } else if p < 32.0 {
        1
    } else {
        0
    };
    // the little man comes on while the cars are held, and the hand blinks before he goes
    let walk = (34.0..54.0).contains(&p);
    let blink = (54.0..62.0).contains(&p);
    (lens, walk, blink)
}

/// The lit lenses of the traffic light, and their light in a wet road.
fn signal<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, now: f64) {
    let (lens, walk, blink) = signal_at(now);
    let col = [BEACON, AMBER, [110.0, 222.0, 176.0]][lens];
    let (x, y) = (SIGNAL, 748.0 + 10.5 * lens as f32);
    let dark = 0.45 + 0.55 * sc.dark;
    d.glow(lay.dx(x), lay.dy(y), lay.d(12.0), lay.d(12.0), col, 0.55 * dark, 2, Blend::Screen);
    oval(d, lay, x, y, 3.0, 3.0, mix(col, WHITE, 0.35), 1.0);
    let hand = if walk { None } else if blink && (now * 2.0).fract() < 0.5 { None } else { Some([255.0, 150.0, 70.0]) };
    match hand {
        Some(h) => bar(d, lay, [x - 3.0, 788.0, x + 3.0, 794.0], h, 0.95),
        None if walk => {
            bar(d, lay, [x - 1.0, 787.0, x + 1.2, 795.0], [236.0, 240.0, 236.0], 0.95);
            oval(d, lay, x + 0.2, 786.0, 1.4, 1.4, [236.0, 240.0, 236.0], 0.95);
        }
        None => {}
    }
    let wet = wet(sc);
    if wet > 0.05 {
        spill(d, lay, [x - 4.0, KERB + 4.0, x + 4.0, KERB + 70.0], col, 0.45 * wet * dark, Blend::Screen);
    }
}

/// Where the cat is.
enum Puss {
    Sill { asleep: bool },
    Sat(f32, f32, f32),
    Walking(f32, f32, f32, f32),
}

/// Ledges the cat walks along: from, to, and the height of the roof.
const LEDGES: [(f32, f32, f32); 5] = [(322.0, 394.0, 659.0), (900.0, 1062.0, 495.0), (690.0, 816.0, 413.0), (1470.0, 1540.0, 577.0), (650.0, 752.0, 570.0)];

fn puss(sc: &Scene, now: f64) -> Puss {
    if sc.cat_asleep || !sc.cat_out {
        return Puss::Sill { asleep: sc.cat_asleep };
    }
    let slot = (sc.t / 1.5) as u32;
    let k = hash2((sc.day_id % 100000) as u32, slot + 9);
    let (x0, x1, y) = LEDGES[(k * LEDGES.len() as f32) as usize % LEDGES.len()];
    // the time since this hour and a half began, carried on smoothly between redraws
    let since = (sc.t % 1.5) * 3600.0 + (now - sc.ts);
    let every = 150.0 + 200.0 * hash2(slot, 31) as f64;
    let n = (since / every).floor().max(0.0);
    let tau = (since - n * every) as f32;
    let span = (x1 - x0).abs() / 20.0;
    let (from, to) = if n as i64 % 2 == 0 { (x0, x1) } else { (x1, x0) };
    let way = if to > from { 1.0 } else { -1.0 };
    if tau < span {
        Puss::Walking(from + (to - from) * tau / span, y, way, tau)
    } else {
        Puss::Sat(to, y, -way)
    }
}

/// The cat on its way somewhere.
fn cat_walking<S: Surface>(d: &mut Draw<S>, x: f32, y: f32, u: f32, col: Rgb, way: f32, t: f32) {
    let step = t * 7.0;
    for (o, ph) in [(-5.5, 0.0), (-3.5, PI), (4.5, PI), (6.5, 0.0)] {
        let swing = (step + ph).sin() * 2.6 * u;
        d.line((x + way * o * u, y - 7.0 * u), (x + way * o * u + way * swing, y), 1.7 * u, col, 1.0);
    }
    d.line((x - way * 7.0 * u, y - 9.5 * u), (x + way * 6.0 * u, y - 10.0 * u), 7.4 * u, col, 1.0);
    let (hx, hy) = (x + way * 11.0 * u, y - 13.5 * u + (step * 2.0).sin() * 0.3 * u);
    d.ellipse(hx, hy, 4.6 * u, 4.1 * u, col, 1.0);
    for e in [-1.0, 1.0] {
        let ex = hx + way * e * 1.8 * u;
        d.poly(&[(ex - 1.8 * u, hy - 2.5 * u), (ex + way * 0.6 * u, hy - 7.6 * u), (ex + 2.0 * u, hy - 2.0 * u)], col, 1.0);
    }
    let sway = (t * 2.1).sin() * 1.5 * u;
    d.path(&[(x - way * 9.0 * u, y - 11.0 * u), (x - way * 13.5 * u, y - 16.0 * u), (x - way * (14.5 * u) + sway, y - 23.0 * u)], 2.2 * u, col, 1.0);
}

// ── the stage ────────────────────────────────────────────────────────────────

/// The street laid out for one screen.
struct Walkups {
    lay: Layout,
    far: Row,
    mid: Row,
    /// the windows across the way, and the house each is in
    wins: Vec<Win>,
    house_of: Vec<usize>,
    /// across the way, what stands along the kerb, and our side of the glass
    masks: Vec<(usize, Vec<u8>)>,
    depth: Depth,
    /// the stretch of street the living scene has shut, while it is: ambient walkers turn back
    /// at it and cars wait for it (the scene's state reaches the ambient drawing through here)
    zone: std::cell::Cell<Option<(f32, f32)>>,
    last_zone: std::cell::Cell<(f32, f32)>,
    /// the clock time the zone last opened again, so that cars pull away rather than jump
    released: std::cell::Cell<f64>,
}

/// A point on one wall of the cross street: `side` 0 the left wall, 1 the right; `f` how far
/// down it; `y` the height it would stand at if it were at our corner.
fn down(side: usize, f: f32, y: f32) -> P {
    let (x0, x1) = if side == 0 { (GAP.0, VP.0 - 9.0) } else { (GAP.1, VP.0 + 9.0) };
    (x0 + (x1 - x0) * f, VP.1 + (y - VP.1) * (1.0 - f))
}

fn wall_of(side: usize, k: usize) -> [P; 4] {
    let (f0, f1, top) = (DOWN[k], DOWN[k + 1], ALONG[side][k]);
    [down(side, f0, top), down(side, f1, top), down(side, f1, BASE), down(side, f0, BASE)]
}

fn tank_shape(sh: &mut Shaper, lay: &Layout, (x, foot, k): (f32, f32, f32)) {
    for e in [-19.0, 19.0] {
        box_shape(sh, lay, x + (e - 1.6) * k, foot - 37.0 * k, x + (e + 1.6) * k, foot);
    }
    box_shape(sh, lay, x - 27.0 * k, foot - 39.0 * k, x + 27.0 * k, foot - 35.0 * k);
    sh.poly(&to_px(lay, &[(x - 24.0 * k, foot - 38.0 * k), (x - 22.0 * k, foot - 88.0 * k), (x + 22.0 * k, foot - 88.0 * k), (x + 24.0 * k, foot - 38.0 * k)]));
    sh.poly(&to_px(lay, &[(x - 26.0 * k, foot - 87.0 * k), (x, foot - 108.0 * k), (x + 26.0 * k, foot - 87.0 * k)]));
}

/// The outline of everything across the way, down to the road and the cross street.
fn across_shape(sh: &mut Shaper, lay: &Layout) {
    for h in &HOUSES {
        box_shape(sh, lay, h.x0, h.top(), h.x1, BASE + 4.0);
        box_shape(sh, lay, h.x0 - 5.0, h.top() - 9.0, h.x1 + 5.0, h.top());
    }
    for (x, roof, tall) in CHIMNEYS {
        box_shape(sh, lay, x - 7.0, roof - tall, x + 7.0, roof);
        box_shape(sh, lay, x - 9.0, roof - tall - 4.0, x + 9.0, roof - tall + 1.0);
    }
    for b in HUTS {
        box_shape(sh, lay, b[0], b[1], b[2], b[3]);
    }
    let l = HUTS[LOFT];
    sh.poly(&to_px(lay, &[(l[0] - 4.0, l[1] + 1.0), ((l[0] + l[2]) / 2.0, l[1] - 13.0), (l[2] + 4.0, l[1] + 1.0)]));
    for t in TANKS {
        tank_shape(sh, lay, t);
    }
    // the diner's sign stands on its roof
    box_shape(sh, lay, 184.0, 660.0, 318.0, 664.0);
    for side in 0..2 {
        for k in 0..DOWN.len() - 1 {
            sh.poly(&to_px(lay, &wall_of(side, k)));
        }
    }
    sh.poly(&[(-2.0, lay.dy(BASE)), (lay.w + 2.0, lay.dy(BASE)), (lay.w + 2.0, lay.h + 2.0), (-2.0, lay.h + 2.0)]);
    sh.poly(&to_px(lay, &[(GAP.0, BASE + 1.0), down(0, 0.88, BASE), down(1, 0.88, BASE), (GAP.1, BASE + 1.0)]));
}

fn lamp_shape(sh: &mut Shaper, lay: &Layout, x: f32) {
    box_shape(sh, lay, x - 1.8, 712.0, x + 1.8, KERB + 2.0);
    box_shape(sh, lay, x - 4.0, 856.0, x + 4.0, KERB + 2.0);
    let crook = to_px(lay, &[(x, 713.0), (x + 2.0, 701.0), (x + 10.0, 694.0), (x + 19.0, 697.0), (x + 22.0, 705.0)]);
    for w in crook.windows(2) {
        sh.stroke(w[0], w[1], lay.d(2.6));
    }
    sh.ellipse(lay.dx(x + 22.0), lay.dy(713.0), lay.d(5.5), lay.d(7.5));
}

/// The outline of what stands along the far kerb: lamps, trees, the hydrant, parked cars.
fn kerb_shape(sh: &mut Shaper, lay: &Layout) {
    for x in LAMPS {
        lamp_shape(sh, lay, x);
    }
    box_shape(sh, lay, SIGNAL - 1.5, 740.0, SIGNAL + 1.5, KERB + 2.0);
    box_shape(sh, lay, SIGNAL - 6.0, 738.0, SIGNAL + 6.0, 772.0);
    box_shape(sh, lay, SIGNAL - 5.5, 783.0, SIGNAL + 5.5, 799.0);
    for (x, y, r) in TREES {
        sh.poly(&to_px(lay, &[(x - 3.0, y + 0.55 * r), (x + 3.0, y + 0.55 * r), (x + 4.5, KERB + 2.0), (x - 4.5, KERB + 2.0)]));
    }
    box_shape(sh, lay, HYDRANT - 3.5, 857.0, HYDRANT + 3.5, KERB + 2.0);
    sh.ellipse(lay.dx(HYDRANT), lay.dy(857.0), lay.d(4.2), lay.d(3.5));
    box_shape(sh, lay, HYDRANT - 6.5, 862.0, HYDRANT + 6.5, 866.0);
    box_shape(sh, lay, 882.0, 851.0, 899.0, KERB);
    sh.ellipse(lay.dx(890.5), lay.dy(851.0), lay.d(8.5), lay.d(6.0));
    sh.poly(&to_px(lay, &[(669.5, 856.0), (682.5, 856.0), (681.0, KERB + 1.0), (671.0, KERB + 1.0)]));
    box_shape(sh, lay, 559.2, 792.0, 560.8, KERB + 2.0);
    sh.ellipse(lay.dx(560.0), lay.dy(786.0), lay.d(7.5), lay.d(7.5));
    for c in parked() {
        let pts: Vec<P> = c.ride.outline().iter().map(|p| c.at(lay, p.0, p.1)).collect();
        sh.poly(&pts);
        let (wa, wr) = c.ride.wheels();
        let r = lay.d(wr * c.ride.size().1 * c.big);
        for a in wa {
            let p = c.at(lay, a, 0.0);
            sh.ellipse(p.0, p.1 - r, r, r);
        }
    }
}

/// A leaf of the plant on the sill: the middle of it, how long, and which way it points.
const LEAVES: [(f32, f32, f32, f32); 10] = [
    (50.0, 904.0, 17.0, -2.6),
    (62.0, 884.0, 19.0, -2.0),
    (80.0, 872.0, 20.0, -1.55),
    (99.0, 882.0, 19.0, -1.05),
    (114.0, 902.0, 17.0, -0.55),
    (68.0, 914.0, 14.0, -2.9),
    (98.0, 912.0, 14.0, -0.25),
    (86.0, 896.0, 15.0, -1.3),
    (128.0, 930.0, 13.0, 0.5),
    (138.0, 950.0, 11.0, 1.1),
];

fn leaf(x: f32, y: f32, len: f32, turn: f32) -> Vec<P> {
    let (c, s) = (turn.cos(), turn.sin());
    let w = len * 0.5;
    [(-1.0, 0.0), (-0.35, 0.95), (0.4, 0.8), (1.0, 0.0), (0.4, -0.8), (-0.35, -0.95)]
        .iter()
        .map(|&(a, b)| (x + (a * len) * c - (b * w) * s, y + (a * len) * s + (b * w) * c))
        .collect()
}

const POT: [P; 4] = [(56.0, 938.0), (100.0, 938.0), (94.0, SILL + 1.0), (62.0, SILL + 1.0)];
const MUG: (f32, f32) = (240.0, 944.0);
const SILL_CAT: f32 = 1410.0;

/// The outline of our side of the glass: the sill and what stands on it.
fn sill_shape(sh: &mut Shaper, lay: &Layout) {
    sh.poly(&[(-2.0, lay.dy(SILL)), (lay.w + 2.0, lay.dy(SILL)), (lay.w + 2.0, lay.h + 2.0), (-2.0, lay.h + 2.0)]);
    sh.poly(&to_px(lay, &POT));
    box_shape(sh, lay, 53.0, 934.0, 103.0, 941.0);
    for (x, y, len, turn) in LEAVES {
        sh.poly(&to_px(lay, &leaf(x, y, len, turn)));
    }
    box_shape(sh, lay, 156.0, 940.0, 222.0, SILL);
    box_shape(sh, lay, MUG.0 - 9.0, MUG.1, MUG.0 + 9.0, SILL);
    sh.poly(&to_px(lay, &[(1522.0, 948.0), (1550.0, 948.0), (1547.0, SILL + 1.0), (1525.0, SILL + 1.0)]));
    sh.ellipse(lay.dx(1536.0), lay.dy(930.0), lay.d(8.5), lay.d(20.0));
    sh.ellipse(lay.dx(1547.0), lay.dy(928.0), lay.d(4.5), lay.d(8.0));
}

impl Walkups {
    /// Walkers who reach a shut stretch of pavement turn round at its edge and walk back.
    fn turned_back(&self, mut walkers: Vec<Walker>) -> Vec<Walker> {
        let Some((z0, z1)) = self.zone.get() else { return walkers };
        for w in walkers.iter_mut() {
            if w.way > 0.0 && w.x > z0 {
                w.x = 2.0 * z0 - w.x;
                w.way = -1.0;
            } else if w.way < 0.0 && w.x < z1 {
                w.x = 2.0 * z1 - w.x;
                w.way = 1.0;
            }
        }
        walkers
    }

    /// Cars that have not yet reached a shut stretch of road wait for it in a queue, and pull
    /// away together a moment after it opens.
    fn held_up(&self, mut cars: Vec<Car>, now: f64) -> Vec<Car> {
        let (z0, z1) = self.zone.get().unwrap_or_else(|| self.last_zone.get());
        let shut = self.zone.get().is_some();
        let ease = if shut { 0.0 } else { (1.0 - (now - self.released.get()) / 2.5).clamp(0.0, 1.0) as f32 };
        if !shut && ease <= 0.0 {
            return cars;
        }
        let gap = 24.0;
        for way in [1.0f32, -1.0] {
            // those moving this way that have not yet got to the zone, nearest first
            let mut idx: Vec<usize> = (0..cars.len())
                .filter(|&i| {
                    let c = &cars[i];
                    let len = c.ride.size().0 * c.big;
                    c.way == way && c.moving && if way > 0.0 { c.nose - len < z0 } else { c.nose + len > z1 }
                })
                .collect();
            idx.sort_by(|a, b| if way > 0.0 { cars[*b].nose.partial_cmp(&cars[*a].nose) } else { cars[*a].nose.partial_cmp(&cars[*b].nose) }.unwrap());
            let mut limit = if way > 0.0 { z0 - gap } else { z1 + gap };
            for i in idx {
                let len = cars[i].ride.size().0 * cars[i].big;
                let true_nose = cars[i].nose;
                let held = if way > 0.0 { true_nose.min(limit) } else { true_nose.max(limit) };
                cars[i].nose = true_nose + (held - true_nose) * if shut { 1.0 } else { ease };
                cars[i].moving = (cars[i].nose - true_nose).abs() < 1.0;
                limit = if way > 0.0 { held - len - 14.0 } else { held + len + 14.0 };
            }
        }
        cars
    }

    fn new(w: usize, h: usize) -> Walkups {
        let lay = Layout::new(w, h, HORIZON);
        let mut sh = Shaper::new();
        let far = kit::row(&mut sh, &lay, FAR.0, FAR.1, FAR_BLOCKS, 600, |sh, lay| {
            // low hills behind everything: the sun comes up out of them and goes down into them
            let pts: Vec<P> = (0..=40)
                .map(|i| {
                    let x = i as f32 / 40.0 * DW;
                    let f = x / DW;
                    (lay.dx(x), lay.dy(522.0 - 8.0 * (f * 6.1 + 0.4).sin() - 5.0 * (f * 13.3).sin()))
                })
                .chain([(lay.w, lay.dy(FAR.0)), (0.0, lay.dy(FAR.0))])
                .collect();
            sh.poly(&pts);
        });
        let mut mid = kit::row(&mut sh, &lay, MID.0, MID.1, MID_BLOCKS, 700, |sh, lay| {
            for (x, roof) in MID_TANKS {
                tank_shape(sh, lay, (x, roof, 0.45));
            }
        });
        let mut mask = |top: f32, f: &dyn Fn(&mut Shaper, &Layout)| {
            if sh.begin(0.0, lay.dy(top).max(0.0), lay.w, lay.h, w, h) {
                f(&mut sh, &lay);
                sh.mask(w)
            } else {
                (h, Vec::new())
            }
        };
        let across = mask(200.0, &across_shape);
        let kerb = mask(680.0, &kerb_shape);
        let ours = mask(860.0, &sill_shape);

        let mut wins = Vec::new();
        let mut house_of = Vec::new();
        for (i, hs) in HOUSES.iter().enumerate() {
            for f in 0..hs.floors {
                for c in 0..hs.cols {
                    let (x, y) = (hs.col(c) - WIN.0 / 2.0, hs.storey(f) + 18.0);
                    if i == HOME && f == 2 && x + WIN.0 > ROOM[0] && x < ROOM[2] {
                        continue;
                    }
                    let k = wins.len() as u32 + 1;
                    // more of these households are home of an evening than the town's
                    let who = [hash2(k, 811) as f64 * 0.62, hash2(k, 823) as f64, hash2(k, 837) as f64, hash2(k, 841) as f64];
                    wins.push(Win { x: lay.dx(x), y: lay.dy(y), w: lay.d(WIN.0), h: lay.d(WIN.1), who, tone: (hash2(k, 853) * 4.0) as u8 & 3, office: false, strip: false });
                    house_of.push(i);
                }
            }
        }
        let hidden = |x: f32, y: f32| kit::covered(&across, w, x, y);
        mid.wins.retain(|v| !hidden(v.x + v.w / 2.0, v.y + v.h / 2.0));
        let masks = vec![far.mask.clone(), mid.mask.clone(), across, kerb, ours];
        let depth = Depth::stack(&masks, w, h);
        Walkups { lay, far, mid, wins, house_of, masks: masks[2..].to_vec(), depth, zone: Default::default(), last_zone: std::cell::Cell::new(episode::ZONE), released: std::cell::Cell::new(f64::NEG_INFINITY) }
    }
}

impl Stage for Walkups {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        let mut out = Lights { share: 0.0, on: Vec::new() };
        let mut lit = 0;
        kit::lights(&self.mid.wins, sc, &mut out, &mut lit);
        kit::lights(&self.wins, sc, &mut out, &mut lit);
        out.share = lit as f32 / out.on.len().max(1) as f32;
        out
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        self.ground_state(d, sc, lights, bloom, out, &Snapshot::intact())
    }

    fn story(&self) -> Option<crate::story::Definition> {
        Some(episode::define(&LAMPS))
    }

    fn story_tone(&self, sc: &Scene, layer: Option<u8>) -> Tone {
        let row = match layer {
            Some(l) if l <= 2 => 2,
            _ => 4,
        };
        Tone::new(sc.layers[row], 0.20 + 0.66 * sc.day, sc.sky[5])
    }

    fn story_actor(&self, d: &mut Draw<Frame>, sc: &Scene, a: &crate::story::actors::Actor, t: f32) -> bool {
        use crate::story::actors::Kind;
        let (ride, big, k) = match (a.kind, a.role) {
            (Kind::Truck, _) => (Ride::Van, 1.12, 5),
            (Kind::Van, _) => (Ride::Van, 1.04, 2),
            (Kind::Car, crate::story::actors::Role::Police) => (Ride::Saloon, 1.0, 3),
            (Kind::Car, _) => (Ride::Saloon, 1.0, 1),
            _ => return false,
        };
        let (len, _) = ride.size();
        let way = a.facing;
        let car = Car { nose: a.pos.0 + way * len * big / 2.0, base: a.pos.1, way, ride, k, big, moving: matches!(a.state, crate::story::actors::ActorState::Moving) };
        vehicle(d, sc, &self.lay, &car);
        // a beacon on the roof, turning
        let on = ((t * 4.0).sin() > 0.0) as u8 as f32;
        let (tx, ty) = (a.pos.0, a.pos.1 - ride.size().1 * big - 3.0);
        let police = a.role == crate::story::actors::Role::Police;
        let (r, g, b) = if police { (if on > 0.5 { 120.0 } else { 40.0 }, 70.0, if on > 0.5 { 40.0 } else { 220.0 }) } else { (255.0, 168.0, 30.0 * (1.0 + on)) };
        d.glow(self.lay.dx(tx), self.lay.dy(ty), self.lay.d(11.0), self.lay.d(8.0), [r, g, b], 0.85 * sc.dark.max(0.3), 2, Blend::Screen);
        true
    }

    fn story_alive(&self, d: &mut Draw<Frame>, sc: &Scene, state: &Snapshot, t: f32, indoors: bool) {
        if indoors {
            return;
        }
        let lay = &self.lay;
        let tone = self.story_tone(sc, None);
        // the shut stretch: the ambient walkers and cars on the next frame go by it
        let shut = state.get(episode::HYDRANT).open || state.in_base(episode::CONE_A) && state.get(episode::CONE_A).visible || state.get(episode::BARRIER).visible;
        let was = self.zone.get().is_some();
        if shut {
            self.zone.set(Some(episode::ZONE));
            self.last_zone.set(episode::ZONE);
        } else {
            self.zone.set(None);
            if was {
                self.released.set(t as f64);
            }
        }
        d.s.behind = Some(KERBSIDE);
        for id in [episode::CONE_A, episode::CONE_B] {
            let s = state.get(id);
            if s.visible && !state.in_base(id) {
                let def_at = if id == episode::CONE_A { (600.0 - 52.0, 874.0) } else { (600.0 + 56.0, 874.0) };
                crate::story::draw::cone(d, lay, (def_at.0 + s.pose.dx, def_at.1 + s.pose.dy), 1.0, &tone, 1.0);
            }
        }
        for (id, home) in [(episode::PARCEL_1, (500.0, 858.0)), (episode::PARCEL_2, (510.0, 856.0)), (episode::PARCEL_3, (506.0, 862.0))] {
            let s = state.get(id);
            if s.visible && !state.in_base(id) {
                crate::story::draw::parcel(d, lay, (home.0 + s.pose.dx, home.1 + s.pose.dy), 1.0, s.pose.turn, id.0 as u32, &tone, 1.0);
            }
        }
        // the diner's sign, when it is swinging or falling: drawn where the physics has it
        let s = state.get(episode::SIGN);
        if s.visible && s.damage != Damage::Missing && !state.in_base(episode::SIGN) {
            d.s.behind = Some(2);
            let col = mix(shade(street_col(sc), -0.1), ROSE, 0.5);
            episode::sign_tilted(d, lay, [190.0, 630.0, 292.0, 656.0], s.damage, col, s.pose);
        }
        d.s.behind = None;
    }

    fn ground_state(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let cols = sc.layers;
        let hz = |a: f32| a / 255.0;
        let fog = mix(sc.sky[4], sc.haze, 0.5);

        // ── the far side of town ──
        band(d.s, lay.dy(440.0), lay.dy(FAR.0 + 40.0), 40.0 * u, sc.haze, hz(sc.haze_alpha[0]));
        let far_col = mix(cols[0], sc.sky[5], 0.20);
        kit::paint(d, sc, lay, &self.far, &[], bloom, &Paint { col: far_col, glass: 0.0, bloom: 0.0, rim: 0.5, tone: 0.03, hue: 0.0 });
        self.sunset_street(d, sc);

        // ── the block behind ──
        band(d.s, lay.dy(MID.0 - 150.0), lay.dy(MID.0 + 40.0), 44.0 * u, sc.haze, hz(sc.haze_alpha[1]));
        if sc.fog > 0.0 {
            band(d.s, lay.dy(MID.1 + 40.0), lay.h * 1.3, 80.0 * u, fog, 0.42 * sc.fog);
        }
        let n = self.mid.wins.len();
        let mid_col = mix(cols[1], cols[2], 0.30);
        kit::paint(d, sc, lay, &self.mid, &lights.on[..n], bloom, &Paint { col: mid_col, glass: 0.5, bloom: 0.8, rim: 0.85, tone: 0.05, hue: 0.07 });
        for (x, roof) in MID_TANKS {
            tank(d, sc, lay, (x, roof, 0.45), mid_col, 0.5);
        }

        // ── across the way ──
        band(d.s, lay.dy(380.0), lay.dy(MID.0 + 30.0), 50.0 * u, sc.haze, hz(sc.haze_alpha[2]));
        if sc.mist > 0.02 {
            band(d.s, lay.dy(MID.0 - 80.0), lay.h * 1.3, 30.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.30 * sc.mist);
        }
        self.across(d, sc, &lights.on[n..], bloom, out, state);
        for (id, rect) in [(episode::WINDOW_A, episode::WINDOWS[0]), (episode::WINDOW_B, episode::WINDOWS[1])] {
            // somebody has come to the window: the pane is lit, a curtain drawn back each side
            if state.get(id).open {
                bar(d, lay, rect, [250.0, 214.0, 150.0], 0.96);
                let w = (rect[2] - rect[0]) * 0.24;
                bar(d, lay, [rect[0], rect[1], rect[0] + w, rect[3]], [196.0, 92.0, 82.0], 0.9);
                bar(d, lay, [rect[2] - w, rect[1], rect[2], rect[3]], [196.0, 92.0, 82.0], 0.9);
                bloom.light(lay.dx(rect[0]), lay.dy(rect[1]), lay.dx(rect[2]), lay.dy(rect[3]), WARM[1], 0.5 * sc.p.bloom);
            }
        }
        if sc.fog > 0.0 {
            band(d.s, lay.dy(300.0), lay.h * 1.3, 70.0 * u, fog, 0.28 * sc.fog);
        }
        if sc.rain > 0.0 {
            // the far side of the street softens behind the rain
            band(d.s, lay.dy(280.0), lay.dy(BASE), 60.0 * u, mix(sc.sky[5], sc.haze, 0.5), 0.10 * sc.rain);
        }

        // ── the street ──
        self.roadway(d, sc, bloom, out);
        if !sc.live {
            for w in walkers(sc, sc.ts) {
                walker(d, sc, lay, &w);
            }
        }
        self.kerbside(d, sc, bloom, out, state);
        if !sc.live {
            for c in traffic(sc, sc.ts) {
                vehicle(d, sc, lay, &c);
            }
        }
        if sc.mist > 0.02 {
            band(d.s, lay.dy(800.0), lay.dy(SILL), 26.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.18 * sc.mist);
        }
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let lay = &self.lay;
        let u = lay.u;
        d.s.behind = Some(KERBSIDE);
        neighbour(d, sc, lay, t);
        flicker(d, sc, lay, now);
        if sc.laundry > 0 {
            props::laundry(d, sc, lay.dx(700.0), lay.dx(800.0), lay.dy(413.0), u, street_col(sc), 0, t * 1.5);
        }
        roof_cat(d, sc, lay, now, t);
        for w in self.turned_back(walkers(sc, now)) {
            walker(d, sc, lay, &w);
        }
        d.s.behind = Some(OURS);
        signal(d, sc, lay, now);
        for c in self.held_up(traffic(sc, now), t as f64) {
            vehicle(d, sc, lay, &c);
        }
        d.s.behind = None;
    }

    fn lively(&self, sc: &Scene, now: f64) -> bool {
        !traffic(sc, now).is_empty() || matches!(puss(sc, now), Puss::Walking(..))
    }

    fn indoors(&self, d: &mut Draw<Canvas>, sc: &Scene, bloom: &mut Bloom, _out: &mut Anchors) {
        let t = (sc.ts % 3600.0) as f32;
        let lay = &self.lay;
        // on a cold or a wet day the warm room mists the bottom of the pane
        let mist = (smooth(0.0, 0.6, sc.rain) * 0.7).max(smooth(10.0, 0.0, sc.temp));
        if mist > 0.02 {
            let pane = mix(mix(sc.sky[5], CREAM, 0.55), WARM[1], 0.25 * sc.dark);
            band(d.s, lay.dy(SILL - 34.0), lay.dy(SILL + 6.0), lay.d(22.0), pane, 0.13 * mist);
        }
        beads(d, sc, lay);
        if !sc.live {
            runners(d, sc, lay, t); // the living wallpaper runs them itself
        }
        sill(d, sc, lay);
        let (y0, mask) = &self.masks[2];
        bloom.hide(mask, lay.w as usize, *y0);
        sill_cat(d, sc, lay, t);
        if !sc.live {
            steam(d, sc, lay, t);
        }
    }

    fn indoors_alive(&self, d: &mut Draw<Frame>, sc: &Scene, t: f32, _now: f64, falling: bool) {
        let lay = &self.lay;
        if falling {
            // the weather fell over the whole picture: put our side of the glass back in front of it
            // (the drops that sit still are already in the still frame, under the rain's faint streaks)
            runners(d, sc, lay, t);
            sill(d, sc, lay);
            sill_cat(d, sc, lay, t);
        }
        steam(d, sc, lay, t);
    }
}

/// The cat, when it is out on the roofs across the way.
fn roof_cat<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, now: f64, t: f32) {
    let col = shade(street_col(sc), -0.45);
    let u = lay.u * 1.1;
    match puss(sc, now) {
        Puss::Walking(x, y, way, step) => cat_walking(d, lay.dx(x), lay.dy(y), u, col, way, step),
        Puss::Sat(x, y, face) => {
            let tail = if (t * 0.11).sin() > -0.3 { t * 1.4 } else { 0.0 };
            props::cat(d, lay.dx(x), lay.dy(y), u, col, false, face, tail);
        }
        Puss::Sill { .. } => {}
    }
}

/// The R in the diner's sign has a loose connection.
fn flicker<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, now: f64) {
    if !open(DINER, sc) {
        return;
    }
    let burst = (now / 9.0).floor() as u32;
    let into = now - burst as f64 * 9.0;
    if hash2(burst, 51) < 0.55 && into < 1.1 && hash2((now * 16.0) as u32, 53) < 0.6 {
        let at = (DINER_SIGN.0 + measure("DINE", 26.0) + 26.0 * 0.28, DINER_SIGN.1);
        let tube = mix(shade(sc.layers[3], -0.1), ROSE, 0.35 + 0.25 * sc.day);
        write(d, lay, "R", at, 26.0, 2.6, tube, 1.0, Blend::Over);
    }
}

const DINER_SIGN: P = (190.0, 630.0);

/// Rain on the pane. It is a hand's breadth away and the eye is on the street, so it is soft and
/// out of focus: each drop a little lens, dark at its rim, holding a brighter crumb of the street
/// below it. More of them gather low on the glass.
fn beads<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout) {
    let k = rain_on_glass(sc);
    if k < 0.02 {
        return;
    }
    let (lit, rim) = bead_cols(sc);
    for i in 0..(110.0 * k) as u32 {
        let x = DW * hash2(i, 1201);
        let y = SILL * (0.10 + 0.90 * hash2(i, 1203).sqrt());
        let r = 1.5 + 4.6 * hash2(i, 1205).powi(2);
        bead(d, lay, x, y, r, lit, rim, 1.0);
    }
}

/// The drops that have grown heavy enough to run: they stop and start on the way down, and leave
/// a wet track behind them.
fn runners<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, t: f32) {
    let k = rain_on_glass(sc);
    if k < 0.02 {
        return;
    }
    let (lit, rim) = bead_cols(sc);
    for i in 0..(26.0 * k) as u32 {
        let every = 8.0 + 14.0 * hash2(i, 1211);
        let f = ((t + every * hash2(i, 1213)) / every).fract();
        let ease = f * 0.25 + 0.75 * (f * 3.0).floor() / 3.0 + 0.75 * smooth(0.0, 1.0, (f * 3.0).fract()) / 3.0;
        let x0 = DW * hash2(i, 1215);
        let y0 = (SILL - 140.0) * (0.25 + 0.75 * hash2(i, 1217));
        let y = y0 + ease * (50.0 + 110.0 * hash2(i, 1219));
        if y > SILL - 4.0 {
            continue;
        }
        let wob = |y: f32| x0 + (y * 0.06 + i as f32).sin() * 1.4;
        let r = 2.6 + 1.4 * hash2(i, 1221);
        // the track: glass wiped clean by the drop, a shade clearer and darker than the misted pane
        let from = (y - 70.0).max(y0);
        let track: Vec<P> = (0..6).map(|j| from + (y - r - from) * j as f32 / 5.0).map(|yy| (wob(yy), yy)).collect();
        ink(d, lay, &track, r * 0.9, rim, 0.07);
        // and a few crumbs of water left along it
        for j in 0..3 {
            let yy = from + (y - from) * (0.2 + 0.25 * j as f32);
            bead(d, lay, wob(yy) + 0.6, yy, 0.8 + 0.4 * hash2(i * 3 + j, 1223), lit, rim, 0.8);
        }
        bead(d, lay, wob(y), y, r, lit, rim, 1.2);
    }
}

fn rain_on_glass(sc: &Scene) -> f32 {
    smooth(0.04, 0.5, sc.rain).max(if sc.storm { 1.0 } else { 0.0 })
}

/// What a drop holds: the light of the street below it (lamps and shops after dark), and the dark of its rim.
fn bead_cols(sc: &Scene) -> (Rgb, Rgb) {
    let lit = mix(mix(sc.sky[5], CREAM, 0.5), WARM[1], 0.45 * sc.dark);
    (lit, shade(sc.layers[4], -0.35))
}

fn bead<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, y: f32, r: f32, lit: Rgb, rim: Rgb, k: f32) {
    oval(d, lay, x, y, r, r * 1.08, rim, 0.17 * k);
    oval(d, lay, x, y + r * 0.30, r * 0.64, r * 0.56, lit, 0.30 * k);
    if r > 2.2 {
        oval(d, lay, x - r * 0.32, y - r * 0.38, r * 0.20, r * 0.20, WHITE, 0.34 * k);
    }
}

/// The sill on our side of the glass, and what is on it.
fn sill<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout) {
    let near = sc.layers[4];
    let dusk = smooth(2.0, -4.0, sc.el_eff);
    // indoors the lamp is on after dark, and it lights what is on the sill
    let room = |c: Rgb, k: f32| mix(mix(near, c, k * (0.30 + 0.25 * sc.day)), mix(c, AMBER, 0.25), 0.22 * dusk);
    let wood = room([128.0, 94.0, 66.0], 1.0);
    bar(d, lay, [-2.0, SILL, DW + 2.0, 1001.0], wood, 1.0);
    bar(d, lay, [-2.0, SILL, DW + 2.0, SILL + 2.2], shade(wood, 0.16), 1.0);
    bar(d, lay, [-2.0, SILL + 9.0, DW + 2.0, SILL + 10.2], shade(wood, -0.22), 0.8);
    for k in 0..9u32 {
        let x = DW * hash2(k, 1301);
        bar(d, lay, [x, SILL + 3.0 + 4.0 * hash2(k, 1303), x + 40.0 + 80.0 * hash2(k, 1305), SILL + 3.8 + 4.0 * hash2(k, 1303)], shade(wood, -0.10), 0.35);
    }
    // the low sun of a morning or an evening comes in at the window and lies along the sill, and
    // what stands on it throws a long shadow into the room, away from the sun
    let (sun_in, away) = sun_in(sc);
    if sun_in > 0.02 {
        bar(d, lay, [-2.0, SILL, DW + 2.0, SILL + 9.0], gold(sc), 0.42 * sun_in);
        bar(d, lay, [-2.0, SILL, DW + 2.0, SILL + 1.6], mix(gold(sc), WHITE, 0.3), 0.6 * sun_in);
        for (x0, x1) in [(60.0, 96.0), (156.0, 222.0), (MUG.0 - 9.0, MUG.0 + 14.0), (1524.0, 1548.0)] {
            let reach = 40.0 + 0.5 * (x1 - x0);
            let shadow = [(x0, SILL + 0.3), (x1, SILL + 0.3), (x1 + away * reach, SILL + 9.0), (x0 + away * reach, SILL + 9.0)];
            d.poly(&to_px(lay, &shadow), shade(wood, -0.45), 0.62 * sun_in);
        }
    }
    // a pothos that has grown too big for its pot
    let leafy = room([86.0, 134.0, 98.0], 1.0);
    for (x, y, len, turn) in LEAVES {
        let base = (78.0, 936.0);
        let stem = (x - turn.cos() * len * 0.9, y - turn.sin() * len * 0.9);
        ink(d, lay, &[base, ((base.0 + stem.0) / 2.0, stem.1 + 4.0), stem], 1.3, shade(leafy, -0.2), 1.0);
    }
    for (i, (x, y, len, turn)) in LEAVES.iter().enumerate() {
        let tone = shade(leafy, (hash2(i as u32, 1311) - 0.5) * 0.18);
        d.poly(&to_px(lay, &leaf(*x, *y, *len, *turn)), tone, 1.0);
        ink(d, lay, &[(x - turn.cos() * len * 0.8, y - turn.sin() * len * 0.8), (x + turn.cos() * len * 0.7, y + turn.sin() * len * 0.7)], 0.8, shade(tone, 0.12), 0.6);
    }
    let clay = room([178.0, 106.0, 80.0], 1.0);
    d.poly(&to_px(lay, &POT), clay, 1.0);
    bar(d, lay, [53.0, 934.0, 103.0, 941.0], shade(clay, 0.06), 1.0);
    bar(d, lay, [62.0, 941.0, 92.0, 943.0], shade(clay, -0.25), 0.5);
    // books, and the mug on the sill beside them
    for (k, (b, tint)) in [([156.0, 957.0, 222.0, SILL], ROSE), ([160.0, 948.5, 218.0, 957.0], BLUE), ([158.0, 940.0, 214.0, 948.5], CREAM)].iter().enumerate() {
        let c = room(*tint, 0.7);
        bar(d, lay, *b, c, 1.0);
        bar(d, lay, [b[0] + 2.0, b[1] + 2.0, b[2] - 2.0, b[1] + 3.0], shade(c, 0.2), 0.6);
        bar(d, lay, [b[2] - 3.0 - k as f32, b[1], b[2], b[3]], shade(c, -0.2), 0.8);
    }
    let mug = room(CREAM, 0.85);
    bar(d, lay, [MUG.0 - 9.0, MUG.1, MUG.0 + 9.0, SILL], mug, 1.0);
    let ring: Vec<P> = (0..=10).map(|i| -PI / 2.0 + PI * i as f32 / 10.0).map(|a| (MUG.0 + 9.0 + 5.0 * a.cos(), MUG.1 + 10.0 + 6.0 * a.sin())).collect();
    ink(d, lay, &ring, 2.4, mug, 1.0);
    bar(d, lay, [MUG.0 - 9.0, MUG.1, MUG.0 + 9.0, MUG.1 + 2.0], shade(mug, -0.25), 0.8);
    bar(d, lay, [MUG.0 - 4.0, MUG.1 + 8.0, MUG.0 + 4.0, MUG.1 + 13.0], mix(mug, ROSE, 0.5), 0.8);
    // a cactus at the other end
    let spiny = room([110.0, 150.0, 104.0], 0.9);
    d.poly(&to_px(lay, &[(1522.0, 948.0), (1550.0, 948.0), (1547.0, SILL + 1.0), (1525.0, SILL + 1.0)]), clay, 1.0);
    oval(d, lay, 1536.0, 930.0, 8.5, 20.0, spiny, 1.0);
    oval(d, lay, 1547.0, 928.0, 4.5, 8.0, spiny, 1.0);
    ink(d, lay, &[(1536.0, 912.0), (1536.0, 946.0)], 0.9, shade(spiny, -0.2), 0.6);
    // the sun catches the edges of things on the side it comes from
    if sun_in > 0.02 {
        let rim = mix(gold(sc), WHITE, 0.25);
        let lit = |x0: f32, x1: f32| if away > 0.0 { x0 } else { x1 - 1.6 };
        for b in [[156.0, 957.0, 222.0, SILL], [160.0, 948.5, 218.0, 957.0], [158.0, 940.0, 214.0, 948.5], [MUG.0 - 9.0, MUG.1, MUG.0 + 9.0, SILL]] {
            bar(d, lay, [lit(b[0], b[2]), b[1], lit(b[0], b[2]) + 1.6, b[3]], rim, 0.6 * sun_in);
        }
        // the pots taper, so their lit edges lean
        let (pot, cactus) = if away > 0.0 { ([POT[0], POT[3]], [(1522.0, 948.0), (1525.0, SILL)]) } else { ([POT[1], POT[2]], [(1550.0, 948.0), (1547.0, SILL)]) };
        let inward = away * 0.9;
        ink(d, lay, &[(pot[0].0 + inward, pot[0].1), (pot[1].0 + inward, pot[1].1)], 1.6, rim, 0.6 * sun_in);
        ink(d, lay, &[(cactus[0].0 + inward, cactus[0].1), (cactus[1].0 + inward, cactus[1].1)], 1.6, rim, 0.6 * sun_in);
        oval(d, lay, 1536.0 - away * 6.0, 928.0, 1.8, 16.0, rim, 0.28 * sun_in);
        // light through the leaves on the side the sun comes from
        d.glow(lay.dx(84.0 - away * 26.0), lay.dy(900.0), lay.d(34.0), lay.d(34.0), gold(sc), 0.20 * sun_in, 2, Blend::Screen);
    }
    // and light from the room on the top of the sill
    if dusk > 0.05 {
        d.glow(lay.dx(220.0), lay.dy(SILL + 6.0), lay.d(420.0), lay.d(30.0), AMBER, 0.16 * dusk, 1, Blend::Screen);
        d.glow(lay.dx(1400.0), lay.dy(SILL + 6.0), lay.d(360.0), lay.d(24.0), AMBER, 0.10 * dusk, 1, Blend::Screen);
    }
}

/// The cat on our sill: watching the rain, or asleep.
/// How much low sun comes in at our window, and which way the shadows on the sill fall (+1 to the right).
fn sun_in(sc: &Scene) -> (f32, f32) {
    let s = &sc.eph.sun;
    let k = sc.sun_vis * smooth(-1.0, 2.0, s.el as f32) * smooth(14.0, 4.0, s.el as f32);
    (k, if s.ha > 0.0 { -1.0 } else { 1.0 })
}

fn sill_cat<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, t: f32) {
    if let Puss::Sill { asleep } = puss(sc, sc.ts) {
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let fur = mix(mix(sc.layers[4], [96.0, 90.0, 92.0], 0.45 + 0.2 * sc.day), AMBER, 0.20 * dusk);
        let tail = if (t * 0.13).sin() > -0.2 { t * 1.1 } else { 0.0 };
        props::cat(d, lay.dx(SILL_CAT), lay.dy(SILL + 0.5), lay.u * 2.5, fur, asleep, -1.0, if asleep { t * 0.6 } else { tail });
    }
}

/// Is there something hot in the mug?
fn brewing(sc: &Scene) -> bool {
    let t = sc.t;
    (6.5..10.5).contains(&t) || (14.5..17.0).contains(&t) || (sc.machine.agents > 0 && (t >= 21.0 || t < 5.0))
}

fn steam<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, t: f32) {
    if !brewing(sc) {
        return;
    }
    for k in 0..2 {
        let kf = k as f32;
        for i in 0..7 {
            let f = i as f32 / 7.0;
            let at = |j: f32| (MUG.0 - 3.0 + 6.0 * kf + (t * 1.1 + j * 0.8 + kf * 2.3).sin() * (1.0 + j * 0.9), MUG.1 - 3.0 - j * 5.0);
            ink(d, lay, &[at(i as f32), at(i as f32 + 1.0)], 1.4, WHITE, 0.16 * (1.0 - f));
        }
    }
}

/// A water tank on its legs: x, the roof it stands on, how big.
fn tank(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, (x, foot, k): (f32, f32, f32), col: Rgb, detail: f32) {
    let wood = mix(col, [150.0, 112.0, 84.0], (0.10 + 0.22 * sc.day) * detail);
    let iron = shade(col, -0.30);
    for e in [-19.0, 19.0] {
        bar(d, lay, [x + (e - 1.6) * k, foot - 37.0 * k, x + (e + 1.6) * k, foot], iron, 1.0);
    }
    ink(d, lay, &[(x - 19.0 * k, foot - 2.0), (x + 19.0 * k, foot - 34.0 * k)], 0.9 * k, iron, 0.8);
    ink(d, lay, &[(x + 19.0 * k, foot - 2.0), (x - 19.0 * k, foot - 34.0 * k)], 0.9 * k, iron, 0.8);
    bar(d, lay, [x - 27.0 * k, foot - 39.0 * k, x + 27.0 * k, foot - 35.0 * k], iron, 1.0);
    d.poly(&to_px(lay, &[(x - 24.0 * k, foot - 38.0 * k), (x - 22.0 * k, foot - 88.0 * k), (x + 22.0 * k, foot - 88.0 * k), (x + 24.0 * k, foot - 38.0 * k)]), wood, 1.0);
    if detail > 0.6 {
        for i in 0..7 {
            let f = -0.9 + 0.3 * i as f32;
            bar(d, lay, [x + f * 23.0 * k, foot - 87.0 * k, x + f * 23.0 * k + 0.7, foot - 38.0 * k], shade(wood, -0.15), 0.35);
        }
        bar(d, lay, [x + 15.0 * k, foot - 86.0 * k, x + 16.0 * k, foot - 39.0 * k], iron, 0.7);
        bar(d, lay, [x + 19.0 * k, foot - 86.0 * k, x + 20.0 * k, foot - 39.0 * k], iron, 0.7);
    }
    for y in [48.0, 60.0, 72.0, 83.0] {
        bar(d, lay, [x - 23.5 * k, foot - y * k, x + 23.5 * k, foot - y * k + 1.4 * k], iron, 0.8);
    }
    d.poly(&to_px(lay, &[(x - 26.0 * k, foot - 87.0 * k), (x, foot - 108.0 * k), (x + 26.0 * k, foot - 87.0 * k)]), shade(wood, -0.16), 1.0);
    ink(d, lay, &[(x, foot - 107.0 * k), (x, foot - 114.0 * k)], 1.2 * k, iron, 1.0);
    if snowy(sc) {
        d.poly(&to_px(lay, &[(x - 24.0 * k, foot - 89.0 * k), (x, foot - 106.0 * k), (x + 24.0 * k, foot - 89.0 * k)]), hex(sc.p.snow), 0.7);
    }
}

impl Walkups {
    /// The cross street: walls in perspective down both sides, the road between, and at its end
    /// the sky the sun goes down into. Under the far town; everything nearer is painted over it.
    fn sunset_street(&self, d: &mut Draw<Canvas>, sc: &Scene) {
        let lay = &self.lay;
        // the setting sun fills the far end of the street with light, and the air in it glows
        let low = evening(sc);
        if low > 0.02 {
            d.glow(lay.dx(VP.0), lay.dy(VP.1 - 40.0), lay.d(110.0), lay.d(150.0), gold(sc), 0.55 * low, 2, Blend::Screen);
            d.glow(lay.dx(VP.0), lay.dy(VP.1 - 10.0), lay.d(40.0), lay.d(60.0), mix(sc.glow, WHITE, 0.4), 0.45 * low, 2, Blend::Screen);
        }
    }

    fn across(&self, d: &mut Draw<Canvas>, sc: &Scene, on: &[Option<Rgb>], bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let col = street_col(sc);
        let (my0, mask) = &self.masks[0];
        let w = lay.w as usize;
        if snowy(sc) {
            let snow = mix(col, hex(sc.p.snow), 0.85);
            d.s.stamp(mask, *my0 as i32, -(lay.d(3.0)).round() as i32, |_| (snow, 1.0));
        } else if sc.frost > 0.05 {
            let rime = mix(col, hex(sc.p.snow), 0.55);
            d.s.stamp(mask, *my0 as i32, -(lay.d(1.2)).round().max(1.0) as i32, |_| (rime, 0.75 * sc.frost));
        }
        d.s.stamp(mask, *my0 as i32, 0, |_| (col, 1.0));
        bloom.hide(mask, w, *my0);
        let light = Light::of(sc, lay);

        self.cross_street(d, sc, col, &light, bloom);
        for i in 0..HOUSES.len() {
            facade(d, sc, lay, i, &light);
        }
        self.roofs(d, sc, col, &light, bloom, out, state);
        let power = (0.30 + 0.70 * sc.dark) * sc.p.bloom;
        for (k, ((v, state), i)) in self.wins.iter().zip(on).zip(&self.house_of).enumerate() {
            pane(d, sc, lay, v, k as u32, *state, brick(sc, HOUSES[*i].brick), power, bloom, out);
        }
        for (x0, x1, i) in ESCAPES {
            escape(d, sc, lay, x0, x1, HOUSES[i].floors, col);
        }
        self.room(d, sc, col, bloom, out);
        lanterns(d, sc, lay, col, bloom);
        for i in 0..HOUSES.len() {
            shopfront(d, sc, lay, i, col, bloom, out);
        }
        let _ = u;
    }

    fn cross_street(&self, d: &mut Draw<Canvas>, sc: &Scene, col: Rgb, light: &Light, bloom: &mut Bloom) {
        let lay = &self.lay;
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let road = mix(sc.layers[3], sc.layers[2], 0.25);
        let far = |f: f32| mix(mix(col, sc.layers[1], f * 0.8), sc.haze, f * 0.25);
        // the road, getting paler with distance, and the pavements along it
        let wedge = [(GAP.0 + 4.0, BASE), (GAP.1 - 4.0, BASE), down(1, 0.88, BASE), down(0, 0.88, BASE)];
        d.poly(&to_px(lay, &wedge), road, 1.0);
        for k in 0..6 {
            let (f0, f1) = (0.88 * k as f32 / 6.0, 0.88 * (k + 1) as f32 / 6.0);
            let side = [down(0, f0, BASE), down(1, f0, BASE), down(1, f1, BASE), down(0, f1, BASE)];
            d.poly(&to_px(lay, &side), mix(road, sc.haze, f1 * 0.5), 0.10 + 0.08 * k as f32);
        }
        for side in 0..2 {
            let kerb = |f: f32| {
                let p = down(side, f, BASE);
                (p.0 + if side == 0 { 12.0 } else { -12.0 } * (1.0 - f), p.1)
            };
            let walk = [down(side, 0.0, BASE), kerb(0.0), kerb(0.88), down(side, 0.88, BASE)];
            d.poly(&to_px(lay, &walk), mix(road, sc.sky[5], 0.12), 1.0);
        }
        // a dashed line down the middle, each dash the same length on the ground, so the road lies flat
        let mid = |f: f32| ((down(0, f, BASE).0 + down(1, f, BASE).0) / 2.0, down(0, f, BASE).1);
        let paint = mix(road, CREAM, 0.45 + 0.2 * dusk);
        let mut z = 1.08f32;
        while 1.0 - 1.0 / z < 0.85 {
            let (f0, f1) = (1.0 - 1.0 / z, 1.0 - 1.0 / (z + 0.16));
            ink(d, lay, &[mid(f0), mid(f1)], (2.2 * (1.0 - f0)).max(0.5), paint, 0.55 * (1.0 - f0 * 0.8));
            z += 0.42;
        }
        if dusk > 0.05 {
            // pools of light under the lamps, which stand on alternate sides
            let wet = wet(sc);
            for (k, f) in [0.10f32, 0.30, 0.46, 0.60, 0.71, 0.80].iter().enumerate() {
                let (a, b) = (down(0, *f, BASE), down(1, *f, BASE));
                let span = b.0 - a.0;
                let cx = if k % 2 == 0 { a.0 + span * 0.32 } else { b.0 - span * 0.32 };
                d.glow(lay.dx(cx), lay.dy(a.1), lay.d(span * 0.36), lay.d(9.0 * (1.0 - f) + 1.0), [255.0, 190.0, 110.0], (0.26 + 0.25 * wet) * dusk, 2, Blend::Screen);
            }
        }
        // the low sun shines straight down it: the road glares towards it, brightest at the far end
        let s = &sc.eph.sun;
        let low = evening(sc);
        if low > 0.02 {
            let shine = low * (1.0 + 0.6 * wet(sc));
            for k in 0..10 {
                let (f0, f1) = (0.88 * k as f32 / 10.0, 0.88 * (k + 1) as f32 / 10.0);
                let slab = [down(0, f0, BASE), down(1, f0, BASE), down(1, f1, BASE), down(0, f1, BASE)];
                d.poly_mode(&to_px(lay, &slab), gold(sc), (0.16 + 0.66 * f1 * f1) * shine, Blend::Screen);
            }
        }
        // the buildings along it, each further and paler than the last
        for side in 0..2 {
            for k in 0..DOWN.len() - 1 {
                let f = (DOWN[k] + DOWN[k + 1]) / 2.0;
                let wall = mix(far(f), HOUSES[if side == 0 { PIZZA } else { BAR }].brick, (0.06 + 0.16 * sc.day) * (1.0 - f));
                // further down, the walls stand in the glow of the sun at the end
                let wall = mix(wall, gold(sc), 0.45 * low * f * f);
                d.poly(&to_px(lay, &wall_of(side, k)), wall, 1.0);
                // windows: two rows of them a floor, smaller as they go
                let top = ALONG[side][k];
                let floors = ((BASE - SHOP - top) / FLOOR).floor() as usize;
                for fl in 0..floors {
                    for j in 0..3 {
                        let g = DOWN[k] + (DOWN[k + 1] - DOWN[k]) * (0.2 + 0.3 * j as f32);
                        let y = BASE - SHOP - FLOOR * (fl + 1) as f32 + 20.0;
                        let (a, b) = (down(side, g, y), down(side, g + 0.03, y + 40.0));
                        let who = hash2((side * 100 + k * 10 + fl) as u32, j as u32 + 1500);
                        let lit = dusk > 0.1 && who < 0.42 && !(sc.t > 1.0 && sc.t < 6.0 && who > 0.08);
                        let glass = if lit { mix(WARM[(j + fl) % 4], far(f), f * 0.5) } else { shade(wall, -0.2) };
                        d.rect(lay.dx(a.0.min(b.0)), lay.dy(a.1), lay.dx(a.0.max(b.0)), lay.dy(b.1), glass, if lit { 0.9 } else { 0.5 });
                        if lit {
                            bloom.light(lay.dx(a.0.min(b.0)) - 2.0, lay.dy(a.1) - 2.0, lay.dx(a.0.max(b.0)) + 2.0, lay.dy(b.1) + 2.0, WARM[1], 0.4 * dusk * sc.p.bloom);
                        }
                    }
                }
                // the corner of each catches the sun going down the street
                let edge = wall_of(side, k);
                if light.k > 0.02 && s.ha > 0.0 {
                    d.line((lay.dx(edge[0].0), lay.dy(edge[0].1)), (lay.dx(edge[3].0), lay.dy(edge[3].1)), lay.d(1.4), mix(wall, light.col, 0.6), (light.k * 0.8).min(0.8));
                }
            }
        }
        // shop lights and lamps down the far end
        if dusk > 0.05 {
            for (k, f) in [0.18f32, 0.34, 0.50, 0.64, 0.76].iter().enumerate() {
                for side in 0..2 {
                    let p = down(side, *f, BASE - 30.0);
                    let tint = [WARM[0], [226.0, 238.0, 222.0], WARM[2], ROSE][(k + side) % 4];
                    let r = lay.d(3.0 * (1.0 - f) + 0.8);
                    d.dot(lay.dx(p.0), lay.dy(p.1), r, r * 0.8, tint, 0.8 * dusk, Blend::Over);
                    bloom.light(lay.dx(p.0) - r * 2.0, lay.dy(p.1) - r * 2.0, lay.dx(p.0) + r * 2.0, lay.dy(p.1) + r * 2.0, tint, 0.5 * dusk);
                }
            }
            // a traffic light far down it, and a car's lights coming
            let p = down(0, 0.70, 780.0);
            d.dot(lay.dx(p.0 + 6.0), lay.dy(p.1), lay.d(1.4), lay.d(1.4), BEACON, dusk, Blend::Over);
            bloom.light(lay.dx(p.0 + 4.0), lay.dy(p.1) - 2.0, lay.dx(p.0 + 8.0), lay.dy(p.1) + 2.0, BEACON, 0.5 * dusk);
        }
        // parked cars along both kerbs
        for (side, f) in [(0, 0.16f32), (0, 0.40), (0, 0.62), (1, 0.24), (1, 0.52), (1, 0.70)] {
            let p = down(side, f, BASE);
            let s = 1.0 - f;
            let x = p.0 + if side == 0 { 14.0 } else { -14.0 } * s;
            let car = mix(far(f), PAINTS[(side * 3 + (f * 10.0) as usize) % PAINTS.len()], 0.2 * sc.day);
            d.poly(&to_px(lay, &[(x - 9.0 * s, p.1), (x - 8.0 * s, p.1 - 13.0 * s), (x + 8.0 * s, p.1 - 13.0 * s), (x + 9.0 * s, p.1)]), car, 1.0);
        }
    }

    fn roofs(&self, d: &mut Draw<Canvas>, sc: &Scene, col: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let slate = shade(col, -0.12);
        let rim = mix(col, light.col, 0.62);
        let rim_a = (light.k * 0.9).min(0.9);
        let lit_left = |x: f32| light.x < lay.dx(x);
        for (k, (x, roof, tall)) in CHIMNEYS.iter().enumerate() {
            let c = brick(sc, HOUSES[[0, 1, 2, 3, 5, 6][k]].brick);
            bar(d, lay, [x - 7.0, roof - tall, x + 7.0, *roof], shade(c, -0.05), 1.0);
            bar(d, lay, [x - 9.0, roof - tall - 4.0, x + 9.0, roof - tall + 1.0], shade(c, -0.18), 1.0);
            bar(d, lay, [x - 5.0, roof - tall - 9.0, x - 1.0, roof - tall - 3.0], mix(slate, [176.0, 106.0, 84.0], 0.3 * sc.day), 1.0);
            if rim_a > 0.02 {
                let ex = if lit_left(*x) { x - 7.0 } else { x + 5.6 };
                bar(d, lay, [ex, roof - tall, ex + 1.4, *roof], rim, rim_a);
            }
            // chimneys smoke when it is cold enough to heat, the diner's flue when it cooks
            let cooks = k == 1 && open(DINER, sc) && gauss(sc.t as f32, 7.6, 1.0).max(gauss(sc.t as f32, 12.8, 1.0)).max(gauss(sc.t as f32, 19.0, 1.4)) > 0.3;
            if sc.smoke > 0.1 + 0.08 * k as f32 || cooks {
                let s = Smoke { x: lay.dx(x - 3.0), y: lay.dy(roof - tall - 9.0), scale: 0.75, col: mix(col, sc.sky[5], 0.72), behind: KERBSIDE };
                if !sc.live {
                    props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, *x);
                }
                out.smoke.push(s);
            }
        }
        for (k, b) in HUTS.iter().enumerate() {
            let c = if k == 2 { mix(col, CREAM, 0.12 + 0.2 * sc.day) } else { shade(col, -0.04) };
            bar(d, lay, *b, c, 1.0);
            bar(d, lay, [b[0] - 1.5, b[1] - 1.5, b[2] + 1.5, b[1] + 1.0], shade(c, -0.18), 1.0);
            if k < 2 {
                bar(d, lay, [b[0] + 6.0, b[1] + 6.0, b[0] + 15.0, b[3]], shade(c, -0.22), 0.8);
            }
            if k == 2 {
                for i in 0..5 {
                    bar(d, lay, [b[0] + 3.0, b[1] + 3.0 + 2.2 * i as f32, b[2] - 3.0, b[1] + 3.8 + 2.2 * i as f32], shade(c, -0.25), 0.6);
                }
            }
        }
        // the pigeon loft, and its pigeons
        let l = HUTS[LOFT];
        d.poly(&to_px(lay, &[(l[0] - 4.0, l[1] + 1.0), ((l[0] + l[2]) / 2.0, l[1] - 13.0), (l[2] + 4.0, l[1] + 1.0)]), slate, 1.0);
        for i in 0..3 {
            bar(d, lay, [l[0] + 6.0 + 11.0 * i as f32, l[1] + 9.0, l[0] + 12.0 + 11.0 * i as f32, l[1] + 15.0], shade(col, -0.3), 0.9);
        }
        out.perch = Some((lay.dx((l[0] + l[2]) / 2.0), lay.dy(l[1] - 13.0)));
        if sc.el_eff > -3.0 && !sc.storm && sc.rain < 0.4 {
            for (x, y) in [(1080.0, 469.0), (1094.0, 462.0), (1102.0, 469.0), (1036.0, 495.0), (1046.0, 495.0)] {
                pigeon(d, lay, x, y, shade(col, -0.30));
            }
        }
        for t in TANKS {
            tank(d, sc, lay, t, col, 1.0);
        }
        // an aerial and a dish on the neighbour's roof, a mast on the corner
        let iron = shade(col, -0.3);
        ink(d, lay, &[(748.0, 413.0), (748.0, 360.0)], 1.2, iron, 1.0);
        for (at, half) in [(372.0, 12.0), (382.0, 9.0), (392.0, 6.0)] {
            ink(d, lay, &[(748.0 - half, at), (748.0 + half, at)], 1.0, iron, 1.0);
        }
        oval(d, lay, 1236.0, 404.0, 6.0, 7.5, shade(col, 0.08), 1.0);
        ink(d, lay, &[(1236.0, 404.0), (1243.0, 400.0)], 1.0, iron, 1.0);
        ink(d, lay, &[(1236.0, 410.0), (1236.0, 413.0)], 2.0, iron, 1.0);
        // the roof garden on the bookshop, with lights on Friday and Saturday nights
        let leaf = mix(shade(col, -0.2), mix(hex(sc.p.seasons[sc.season.max(1).min(2)][1]), SAGE, 0.4), 0.2 + 0.35 * sc.day);
        for (x0, x1) in [(906.0, 950.0), (1006.0, 1050.0)] {
            bar(d, lay, [x0, 489.0, x1, 496.0], shade(col, 0.04), 1.0);
            for i in 0..4 {
                let x = x0 + (x1 - x0) * (i as f32 + 0.5) / 4.0;
                oval(d, lay, x, 486.0, 5.0, 3.6 + 2.0 * hash2(i, x0 as u32), shade(leaf, if i % 2 == 0 { 0.0 } else { -0.1 }), 1.0);
            }
        }
        if sc.festoon {
            for b in props::festoon(d, lay.dx(904.0), lay.dx(1064.0), lay.dy(495.0), u, col) {
                bloom.light(b.0 - 3.0 * u, b.1 - 3.0 * u, b.0 + 3.0 * u, b.1 + 3.0 * u, b.2, 0.5);
                out.twinkle.push((b.0, b.1, b.2, KERBSIDE));
            }
        }
        if sc.laundry > 0 && !sc.live {
            props::laundry(d, sc, lay.dx(700.0), lay.dx(800.0), lay.dy(413.0), u, col, 0, (sc.ts / 240.0) as f32);
        }
        if !sc.live {
            let t = (sc.ts % 3600.0) as f32;
            roof_cat(d, sc, lay, sc.ts, t);
        }
        // the diner's sign, standing on its roof: the sun comes up behind it
        let frame = shade(col, -0.32);
        ink(d, lay, &[(184.0, 662.0), (318.0, 662.0)], 2.0, frame, 1.0);
        for x in [196.0, 250.0, 304.0] {
            ink(d, lay, &[(x, 659.0), (x, 668.0)], 1.6, frame, 1.0);
            ink(d, lay, &[(x - 6.0, 668.0), (x + 6.0, 659.0)], 0.8, frame, 0.8);
        }
        // the sign: on its roof, hanging by one fixing, or not there at all (the live layer draws
        // it while it moves, and where it lands)
        let tone = Tone::new(col, 0.20 + 0.66 * sc.day, sc.sky[5]);
        let sign = state.get(episode::SIGN);
        if sign.visible && sign.damage == Damage::Intact && state.in_base(episode::SIGN) {
            neon(d, sc, lay, "DINER", DINER_SIGN, 26.0, ROSE, if open(DINER, sc) { 1.0 } else { 0.0 }, bloom);
        } else if sign.visible && sign.damage != Damage::Missing && state.in_base(episode::SIGN) {
            // dead tube, one corner still on the frame
            let tube = mix(shade(sc.layers[3], -0.1), ROSE, 0.35 + 0.25 * sc.day);
            episode::sign_tilted(d, lay, [190.0, 630.0, 292.0, 656.0], sign.damage, tube, sign.pose);
        }
        if state.in_base(episode::LADDER) {
            crate::story::draw::ladder(d, lay, (232.0, 868.0), (226.0, 664.0), 1.0, &tone, 1.0);
        }
    }

    /// The neighbour's room: the walls and things in it. The neighbour themselves is drawn by
    /// `neighbour`, by the live layer when there is one.
    fn room(&self, d: &mut Draw<Canvas>, sc: &Scene, col: Rgb, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let [x0, y0, x1, y1] = ROOM;
        let what = doing(sc);
        let wall = brick(sc, HOUSES[HOME].brick);
        let stone = shade(wall, 0.12);
        bar(d, lay, [x0 - 4.0, y0 - 8.0, x1 + 4.0, y0 - 2.5], stone, 0.6);
        bar(d, lay, [x0 - 5.0, y1 + 1.5, x1 + 5.0, y1 + 5.5], stone, 0.8);
        bar(d, lay, [x0 - 2.0, y0 - 2.0, x1 + 2.0, y1 + 2.0], shade(wall, -0.4), 1.0);
        let dim = mix(shade(col, -0.30), sc.sky[5], 0.45 * sc.day);
        let (glow, k) = room_light(sc, what);
        let back = mix(dim, mix(glow, wall, 0.25), k);
        bar(d, lay, ROOM, back, 1.0);
        let thing = mix(back, shade(col, -0.5), 0.55);
        // a shelf of plants, a poster, the sofa, the desk with its screen, a floor lamp
        bar(d, lay, [x0 + 2.0, 527.0, x0 + 28.0, 528.6], thing, 1.0);
        for (i, x) in [x0 + 6.0, x0 + 14.0, x0 + 22.0].iter().enumerate() {
            oval(d, lay, *x, 523.5 - i as f32 * 0.6, 3.6, 4.0, mix(thing, SAGE, 0.35), 1.0);
            ink(d, lay, &[(x + 2.0, 528.0), (x + 4.0, 534.0 + 2.0 * i as f32)], 1.0, mix(thing, SAGE, 0.35), 0.9);
        }
        bar(d, lay, [794.0, 520.0, 806.0, 536.0], mix(back, ROSE, 0.25), 0.8);
        bar(d, lay, [x0 + 4.0, 558.0, 816.0, 576.0], thing, 1.0);
        bar(d, lay, [x0 + 2.0, 552.0, x0 + 8.0, 576.0], shade(thing, -0.1), 1.0);
        bar(d, lay, [826.0, 556.0, x1, 558.5], thing, 1.0);
        bar(d, lay, [828.0, 558.0, 830.0, 576.0], thing, 1.0);
        bar(d, lay, [x1 - 3.0, 558.0, x1 - 1.0, 576.0], thing, 1.0);
        let screen = if what == Doing::Code { mix(SCREEN, WHITE, 0.25) } else { shade(thing, -0.1) };
        d.poly(&to_px(lay, &[(855.0, 534.0), (872.0, 532.0), (872.0, 553.0), (855.0, 552.0)]), screen, 1.0);
        bar(d, lay, [862.0, 553.0, 865.0, 556.0], thing, 1.0);
        let (lamp, lamp_glow, low) = lamp_colours(sc);
        // the desk lamp: lit while an agent works
        ink(d, lay, &[(846.0, 556.0), (848.0, 544.0), (853.0, 538.0)], 1.3, thing, 1.0);
        d.poly(&to_px(lay, &[(849.0, 536.0), (856.0, 535.0), (858.0, 541.0), (851.0, 543.0)]), thing, 1.0);
        if what == Doing::Code {
            oval(d, lay, 854.0, 542.0, 2.4, 1.6, lamp, 1.0);
            d.glow(lay.dx(853.0), lay.dy(552.0), lay.d(30.0), lay.d(18.0), lamp, 0.45, 2, Blend::Screen);
            bloom.light(lay.dx(846.0), lay.dy(536.0), lay.dx(862.0), lay.dy(558.0), lamp_glow, (0.6 + 0.6 * sc.dark) * sc.p.bloom.max(0.6));
            bloom.light(lay.dx(x0), lay.dy(y0), lay.dx(x1), lay.dy(y1), lamp_glow, 0.25 * sc.dark * sc.p.bloom);
            out.breath.push(Breath { x: lay.dx(853.0), y: lay.dy(546.0), rx: lay.d(70.0), ry: lay.d(48.0), col: lamp_glow, strength: 0.45 + 0.55 * sc.dark, low, behind: Some(KERBSIDE) });
        } else if k > 0.05 {
            bloom.light(lay.dx(x0), lay.dy(y0), lay.dx(x1), lay.dy(y1), mix(glow, WARM[1], 0.5), 0.40 * k * (0.3 + 0.7 * sc.dark) * sc.p.bloom);
        }
        // the guitar waits on its stand when nobody is playing it
        if what != Doing::Guitar {
            let wood = mix(thing, [196.0, 140.0, 88.0], 0.25);
            oval(d, lay, 820.0, 568.0, 5.0, 6.5, wood, 1.0);
            ink(d, lay, &[(820.0, 562.0), (821.0, 546.0)], 1.6, wood, 1.0);
        }
        out.music = Some((lay.dx(x0 + 6.0), lay.dy(y0 - 2.0)));
        if sc.machine.audio && !sc.live {
            let ink_col = mix(hex(sc.p.ink), AMBER, 0.4);
            props::note(d, lay.dx(x0 + 18.0), lay.dy(y0 - 12.0), lay.u, ink_col, 0.8, false);
            props::note(d, lay.dx(x0 + 2.0), lay.dy(y0 - 34.0), lay.u, ink_col, 0.55, true);
        }
        if !sc.live {
            neighbour(d, sc, lay, (sc.ts % 3600.0) as f32);
        } else {
            let frame = shade(col, -0.38);
            for x in [806.5, 842.5] {
                bar(d, lay, [x, y0, x + 1.8, y1], frame, 1.0);
            }
            bar(d, lay, [x0, y0 + 15.0, x1, y0 + 16.6], frame, 0.9);
        }
    }

    /// The pavement, the road, and the light lying on them.
    fn roadway(&self, d: &mut Draw<Canvas>, sc: &Scene, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let cols = sc.layers;
        let wet = wet(sc);
        let damp = damp(sc);
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let pave = mix(mix(cols[3], cols[2], 0.55), sc.sky[5], 0.10 + 0.06 * sc.day - 0.04 * wet);
        let road = mix(mix(cols[4], cols[3], 0.30), sc.sky[5], 0.06 + 0.16 * wet);
        let road_near = shade(road, -0.12);
        bar(d, lay, [-2.0, BASE, DW + 2.0, KERB], pave, 1.0);
        // the cross street's corners are cut away for the crossing
        let (w, h) = (d.s.w(), d.s.h());
        if d.sh.begin(0.0, lay.dy(KERB), lay.w, lay.dy(SILL + 2.0), w, h) {
            d.sh.poly(&[(-2.0, lay.dy(KERB)), (lay.w + 2.0, lay.dy(KERB)), (lay.w + 2.0, lay.dy(SILL + 2.0)), (-2.0, lay.dy(SILL + 2.0))]);
            let (a, b) = (lay.dy(KERB), lay.dy(SILL));
            d.sh.fill_rows(d.s, Blend::Over, |y| (mix(road, road_near, ((y - a) / (b - a)).clamp(0.0, 1.0)), 1.0));
        }
        let mut x = -10.0;
        while x < DW {
            bar(d, lay, [x, BASE + 1.0, x + 0.9, KERB - 1.0], shade(pave, -0.12), 0.35);
            x += 36.0;
        }
        bar(d, lay, [-2.0, KERB - 3.0, DW + 2.0, KERB + 0.5], shade(pave, 0.10), 1.0);
        bar(d, lay, [-2.0, KERB + 0.5, DW + 2.0, KERB + 3.5], shade(road, -0.30), 0.8);
        if wet > 0.05 {
            bar(d, lay, [-2.0, KERB + 3.5, DW + 2.0, KERB + 6.0], mix(road, sc.sky[5], 0.5), 0.40 * wet);
        }
        // the lines down the middle
        let mut x = -20.0;
        while x < DW {
            for dy in [0.0, 2.6] {
                bar(d, lay, [x, 942.0 + dy, x + 58.0, 943.4 + dy], mix(road, AMBER, 0.55), 0.55);
            }
            x += 96.0;
        }
        // a crossing over the mouth of the cross street
        for k in 0..9 {
            let x = GAP.0 + 14.0 + 18.0 * k as f32;
            let lean = (x - VP.0) * 0.05;
            d.poly(&to_px(lay, &[(x + lean, BASE + 2.0), (x + 9.0 + lean, BASE + 2.0), (x + 9.0, KERB - 3.0), (x, KERB - 3.0)]), mix(pave, CREAM, 0.35), 0.5);
        }
        // the evening sun comes out of the cross street and lies across ours, soft at its edges
        let low = evening(sc);
        if low > 0.02 {
            let shine = low * (1.0 + 0.8 * wet);
            // in slices, fading as it comes nearer; three widths of it make the edges soft
            for (inset, a) in [(-14.0, 0.05), (0.0, 0.07), (14.0, 0.09), (28.0, 0.10)] {
                let [l0, r0, r1, l1] = sunlit(SILL, inset);
                for k in 0..8 {
                    let (f0, f1) = (k as f32 / 8.0, (k + 1) as f32 / 8.0);
                    let at = |p: P, q: P, f: f32| (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
                    let slab = [at(l0, l1, f0), at(r0, r1, f0), at(r0, r1, f1), at(l0, l1, f1)];
                    d.poly_mode(&to_px(lay, &slab), gold(sc), a * shine * (1.0 - 0.75 * f0), Blend::Screen);
                }
            }
        }
        // manholes, steaming on cold days
        for (k, (x, y)) in [STACK, (1376.0, 912.0), (1120.0, 952.0)].into_iter().enumerate() {
            oval(d, lay, x, y, 13.0, 3.6, shade(road, -0.30), 0.9);
            oval(d, lay, x, y - 0.6, 11.0, 2.6, shade(road, 0.04), 0.6);
            if sc.smoke > 0.30 + 0.1 * k as f32 {
                let s = Smoke { x: lay.dx(x), y: lay.dy(y - 2.0), scale: 0.65, col: mix(road, sc.sky[5], 0.75), behind: OURS };
                if !sc.live {
                    props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, x);
                }
                out.smoke.push(s);
            }
        }
        // and the one by the laundromat steams harder while the processor works hard
        let s = Smoke { x: lay.dx(STACK.0), y: lay.dy(STACK.1 - 2.0), scale: 0.95, col: mix(road, sc.sky[5], 0.75), behind: OURS };
        out.stove = Some(s);
        if sc.machine.load > 0.35 && !sc.live {
            props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, 1.3);
        }
        // puddles, holding a piece of the sky
        if damp > 0.03 {
            for (x, y, rx, ry) in [(520.0, 884.0, 46.0, 3.8), (1010.0, 951.0, 74.0, 5.5), (1330.0, 893.0, 40.0, 3.4), (210.0, 940.0, 54.0, 4.6), (760.0, 915.0, 30.0, 2.8)] {
                oval(d, lay, x, y, rx, ry, mix(road, sc.sky[4], 0.62), 0.6 * damp);
                oval(d, lay, x - rx * 0.2, y - ry * 0.3, rx * 0.5, ry * 0.25, mix(sc.sky[5], WHITE, 0.2), 0.25 * damp);
            }
        }
        if snowy(sc) {
            let snow = hex(sc.p.snow);
            bar(d, lay, [-2.0, BASE, DW + 2.0, KERB - 3.0], snow, 0.55 * sc.snow);
            bar(d, lay, [-2.0, KERB + 3.0, DW + 2.0, SILL], snow, 0.30 * sc.snow);
            for y in LANES {
                bar(d, lay, [-2.0, y - 8.0, DW + 2.0, y - 3.0], road, 0.4);
            }
        }
        if sc.season == 3 && !snowy(sc) {
            // fallen leaves along the kerb
            for k in 0..60u32 {
                let x = DW * hash2(k, 1401);
                let y = KERB - 6.0 + 14.0 * hash2(k, 1403);
                let tint = [[160.0, 86.0, 44.0], [176.0, 138.0, 51.0], [201.0, 122.0, 60.0]][k as usize % 3];
                oval(d, lay, x, y, 2.4, 1.2, mix(pave, tint, 0.25 + 0.45 * sc.day), 0.9);
            }
        }
        // light from the shops, the lamps and the signs lying on the pavement, and on a wet night
        // stretched all the way down the road towards us
        let mut lights: Vec<(f32, f32, Rgb, f32)> = Vec::new();
        for i in 1..HOUSES.len() {
            if open(i, sc) {
                let b = shop_window(i);
                lights.push((b[0], b[2], shop_light(i), 1.0));
            }
        }
        for x in LAMPS {
            lights.push((x + 14.0, x + 30.0, [255.0, 196.0, 120.0], 1.4));
        }
        if open(DINER, sc) {
            lights.push((196.0, 316.0, ROSE, 0.6));
        }
        if open(BAR, sc) {
            lights.push((1520.0, 1580.0, [240.0, 110.0, 130.0], 0.9));
        }
        for (x0, x1, col, k) in lights {
            spill(d, lay, [x0 - 10.0, BASE, x1 + 10.0, KERB + 8.0], col, 0.16 * k * dusk, Blend::Screen);
            if wet > 0.05 {
                spill(d, lay, [x0 + 4.0, KERB + 3.0, x1 - 4.0, SILL], col, 0.30 * k * dusk * wet, Blend::Screen);
            } else {
                spill(d, lay, [x0 + 4.0, KERB + 3.0, x1 - 4.0, KERB + 34.0], col, 0.06 * k * dusk, Blend::Screen);
            }
        }
        if wet > 0.05 {
            // the reflections break up across ripples
            for k in 0..14u32 {
                let y = KERB + 8.0 + (SILL - KERB - 12.0) * hash2(k, 1411);
                bar(d, lay, [-2.0, y, DW + 2.0, y + 1.0 + hash2(k, 1413)], road, 0.35 * wet);
            }
        }
        let _ = bloom;
    }

    /// Along the far kerb: lamps, trees, the hydrant, the parked cars.
    fn kerbside(&self, d: &mut Draw<Canvas>, sc: &Scene, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let near = sc.layers[4];
        let iron = shade(near, 0.05);
        let tint = 0.15 + 0.45 * sc.day;
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let (my0, mask) = &self.masks[1];
        let w = lay.w as usize;
        if snowy(sc) {
            let snow = mix(near, hex(sc.p.snow), 0.85);
            d.s.stamp(mask, *my0 as i32, -(lay.d(2.5)).round() as i32, |_| (snow, 1.0));
        }
        d.s.stamp(mask, *my0 as i32, 0, |_| (iron, 1.0));
        bloom.hide(mask, w, *my0);
        let light = Light::of(sc, lay);
        for (n, (x, y, r)) in TREES.into_iter().enumerate() {
            tree(d, sc, lay, (x, y, r), n, &light, bloom);
        }
        // the tree pits' little fences
        for (x, _, _) in TREES {
            ink(d, lay, &[(x - 13.0, 864.0), (x + 13.0, 864.0)], 1.1, iron, 1.0);
            for e in [-13.0, 0.0, 13.0] {
                ink(d, lay, &[(x + e, 864.0), (x + e, KERB - 1.0)], 1.0, iron, 1.0);
            }
        }
        // the hydrant, the post box and the bin
        let red = mix(near, [196.0, 72.0, 60.0], tint + 0.1);
        episode::hydrant_body(d, lay, state.damage(episode::HYDRANT), red, KERB);
        {
            let tone = Tone::new(near, tint, sc.sky[5]);
            for (id, at) in [(episode::CONE_A, (HYDRANT - 52.0, 874.0)), (episode::CONE_B, (HYDRANT + 56.0, 874.0))] {
                if state.in_base(id) {
                    let s = state.get(id);
                    crate::story::draw::cone(d, lay, (at.0 + s.pose.dx, at.1 + s.pose.dy), 1.0, &tone, 1.0);
                }
            }
            if state.in_base(episode::BARRIER) {
                crate::story::draw::barrier(d, lay, (HYDRANT, 904.0), 72.0, 1.0, &tone, 1.0);
            }
            // a stack of parcels at the kerb: whole, or (when it has gone over) gone from here
            if state.in_base(episode::STACK) {
                for (k, (dx, dy)) in [(-6.0f32, 0.0f32), (4.0, -2.0), (0.0, -9.0)].iter().enumerate() {
                    crate::story::draw::parcel(d, lay, (506.0 + dx, 866.0 + dy - 4.0), 1.0, 0.0, k as u32, &tone, 1.0);
                }
            }
            for (id, home) in [(episode::PARCEL_1, (500.0, 858.0)), (episode::PARCEL_2, (510.0, 856.0)), (episode::PARCEL_3, (506.0, 862.0))] {
                if state.in_base(id) {
                    let s = state.get(id);
                    crate::story::draw::parcel(d, lay, (home.0 + s.pose.dx, home.1 + s.pose.dy), 1.0, s.pose.turn, id.0 as u32, &tone, 1.0);
                }
            }
        }
        let post = mix(near, BLUE, tint);
        bar(d, lay, [882.0, 851.0, 899.0, KERB - 4.0], post, 1.0);
        oval(d, lay, 890.5, 851.0, 8.5, 6.0, post, 1.0);
        bar(d, lay, [884.0, 856.0, 897.0, 857.5], shade(post, -0.3), 0.8);
        bar(d, lay, [669.5, 856.0, 682.5, 858.0], shade(iron, 0.1), 1.0);
        oval(d, lay, 560.0, 786.0, 7.5, 7.5, mix(near, [96.0, 140.0, 190.0], tint), 1.0);
        bar(d, lay, [557.0, 784.0, 563.0, 788.0], mix(near, CREAM, tint), 1.0);
        // the traffic light's housing; its lenses are lit by `signal`
        let housing = mix(near, [120.0, 116.0, 70.0], tint);
        bar(d, lay, [SIGNAL - 6.0, 738.0, SIGNAL + 6.0, 772.0], housing, 1.0);
        for k in 0..3 {
            oval(d, lay, SIGNAL, 748.0 + 10.5 * k as f32, 3.4, 3.4, shade(housing, -0.5), 1.0);
        }
        bar(d, lay, [SIGNAL - 5.5, 783.0, SIGNAL + 5.5, 799.0], housing, 1.0);
        bar(d, lay, [SIGNAL - 4.0, 785.0, SIGNAL + 4.0, 797.0], shade(housing, -0.5), 1.0);
        if !sc.live {
            signal(d, sc, lay, sc.ts);
        }
        // the lamps: a sodium glow, a pool under each, and a cone where the air is wet
        let haze = sc.rain.max(sc.fog).max(sc.mist).max(0.15);
        for x in LAMPS {
            let (lx, ly) = (x + 22.0, 713.0);
            let glass = mix(iron, [255.0, 214.0, 150.0], 0.25 + 0.75 * dusk);
            oval(d, lay, lx, ly, 3.6, 5.4, glass, 1.0);
            bar(d, lay, [lx - 5.5, ly - 7.5, lx + 5.5, ly - 5.5], iron, 1.0);
            if dusk > 0.05 {
                let sodium = [255.0, 190.0, 110.0];
                d.poly_mode(&to_px(lay, &[(lx - 4.0, ly + 4.0), (lx + 4.0, ly + 4.0), (lx + 62.0, KERB + 2.0), (lx - 62.0, KERB + 2.0)]), sodium, 0.10 * haze * dusk, Blend::Screen);
                d.glow(lay.dx(lx), lay.dy(KERB - 4.0), lay.d(80.0), lay.d(16.0), sodium, 0.22 * dusk, 1, Blend::Screen);
                d.glow(lay.dx(lx), lay.dy(ly), lay.d(26.0), lay.d(26.0), sodium, 0.55 * dusk, 2, Blend::Screen);
                bloom.light(lay.dx(lx - 6.0), lay.dy(ly - 7.0), lay.dx(lx + 6.0), lay.dy(ly + 7.0), sodium, 1.1 * dusk * sc.p.bloom.max(0.6));
            }
        }
        for c in parked() {
            vehicle(d, sc, lay, &c);
        }
        out.leaves = true;
        out.glades.push([lay.dx(900.0), lay.dy(470.0), lay.dx(1060.0), lay.dy(500.0)]);
        out.glades.push([lay.dx(1030.0), lay.dy(650.0), lay.dx(1160.0), lay.dy(760.0)]);
    }
}

fn pigeon<S: Surface>(d: &mut Draw<S>, lay: &Layout, x: f32, y: f32, col: Rgb) {
    oval(d, lay, x, y - 3.0, 4.0, 2.8, col, 1.0);
    oval(d, lay, x + 3.4, y - 5.6, 1.8, 1.8, col, 1.0);
    ink(d, lay, &[(x - 3.5, y - 2.5), (x - 6.5, y - 1.5)], 1.4, col, 1.0);
}

/// A street tree, in the colour of the season: bare branches in winter.
fn tree(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, (x, y, r): (f32, f32, f32), n: usize, light: &Light, bloom: &mut Bloom) {
    let p = sc.p;
    let bark = mix(sc.layers[4], hex(p.trunk), 0.4);
    let fork = (x, y + 0.55 * r);
    let limbs: [(f32, f32); 5] = [(-0.62, -0.05), (0.58, 0.02), (-0.22, -0.66), (0.28, -0.70), (0.02, -0.30)];
    for (k, (lx, ly)) in limbs.iter().enumerate() {
        let end = (x + lx * r, y + ly * r);
        ink(d, lay, &[fork, ((fork.0 + end.0) / 2.0 + lx * 6.0, (fork.1 + end.1) / 2.0), end], 3.0 - 0.3 * k as f32, bark, 1.0);
        if sc.season == 0 {
            for j in 0..3 {
                let a = (ly).atan2(*lx) + (j as f32 - 1.0) * 0.6;
                let len = r * (0.22 + 0.12 * hash2(k as u32, j));
                ink(d, lay, &[end, (end.0 + a.cos() * len, end.1 + a.sin() * len)], 1.0, bark, 0.9);
            }
        }
    }
    if sc.season == 0 {
        if snowy(sc) {
            for (lx, ly) in limbs {
                oval(d, lay, x + lx * r * 0.9, y + ly * r - 2.0, 7.0, 1.8, hex(p.snow), 0.8);
            }
        }
        return;
    }
    let tints = p.seasons[sc.season];
    let shine = p.tree_light * smooth(-8.0, 16.0, sc.eph.sun.el as f32) * (1.0 - 0.4 * sc.gloom);
    let base = mix(hex(if n % 2 == 0 { p.tree[1] } else { p.tree[0] }), hex(p.tree_night), 0.6 * (1.0 - sc.day));
    let dusk = smooth(2.0, -4.0, sc.el_eff);
    let col = mix(mix(base, hex(tints[n % 3]), shine), [255.0, 190.0, 110.0], 0.10 * dusk);
    for (k, (ox, oy, rx, ry)) in [(-0.45, 0.18, 0.62, 0.50), (0.48, 0.20, 0.60, 0.50), (0.0, 0.0, 0.92, 0.78), (-0.30, -0.40, 0.55, 0.45), (0.30, -0.38, 0.55, 0.45)].iter().enumerate() {
        let c = shade(col, [-0.05, -0.08, 0.0, 0.03, 0.02][k]);
        oval(d, lay, x + ox * r, y + oy * r, rx * r, ry * r, c, 1.0);
    }
    bloom.shade(lay.dx(x), lay.dy(y), lay.d(r), lay.d(r * 0.85), 0.2);
    let side = if light.x < lay.dx(x) { -1.0 } else { 1.0 };
    oval(d, lay, x + side * r * 0.3, y - r * 0.32, r * 0.5, r * 0.38, shade(col, 0.12 * shine), 0.9 * sc.day);
    oval(d, lay, x - side * r * 0.25, y + r * 0.34, r * 0.5, r * 0.32, shade(col, -0.08), 0.6);
    if snowy(sc) {
        oval(d, lay, x, y - r * 0.48, r * 0.78, r * 0.36, mix(col, hex(p.snow), 0.75), 1.0);
    } else if sc.season == 1 {
        for k in 0..16u32 {
            let (bx, by) = (x + (hash2(k, n as u32 + 1501) - 0.5) * 1.5 * r, y + (hash2(k, n as u32 + 1503) - 0.6) * 1.2 * r);
            oval(d, lay, bx, by, 2.0, 2.0, mix(col, [236.0, 184.0, 198.0], 0.3 + shine), 1.0);
        }
    }
}

/// One house's face: brick, the stone bands under its windows, and the cornice along its top.
fn facade(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, i: usize, light: &Light) {
    let h = &HOUSES[i];
    let wall = brick(sc, h.brick);
    let top = h.top();
    bar(d, lay, [h.x0, top, h.x1, BASE - SHOP], wall, 1.0);
    // a brick here and there a shade off, so that the wall is not a flat colour
    for k in 0..(h.floors as u32 * 22) {
        let x = h.x0 + 4.0 + (h.x1 - h.x0 - 20.0) * hash2(k, 1601 + i as u32);
        let y = top + 6.0 + (BASE - SHOP - top - 10.0) * hash2(k, 1603 + i as u32);
        let wide = 8.0 + 10.0 * hash2(k, 1605);
        bar(d, lay, [x, y, x + wide, y + 2.6], shade(wall, if k % 2 == 0 { 0.07 } else { -0.08 }), 0.45);
    }
    for f in 0..h.floors {
        let y = h.storey(f) + 66.0;
        bar(d, lay, [h.x0, y, h.x1, y + 2.0], shade(wall, 0.09), 0.35);
    }
    // the cornice, and its little teeth
    let ledge = shade(wall, -0.14);
    bar(d, lay, [h.x0 - 5.0, top - 9.0, h.x1 + 5.0, top], ledge, 1.0);
    bar(d, lay, [h.x0 - 5.0, top - 9.0, h.x1 + 5.0, top - 7.2], shade(wall, 0.05), 0.9);
    let mut x = h.x0;
    while x < h.x1 - 3.0 {
        bar(d, lay, [x + 1.0, top - 4.5, x + 4.0, top], shade(wall, -0.30), 0.7);
        x += 8.0;
    }
    // where it stands taller than the house beside it, the edge facing the light catches it
    let rim_a = (light.k * 0.9).min(0.9);
    if rim_a > 0.02 {
        let rim = mix(wall, light.col, 0.62);
        let left = light.x < lay.dx((h.x0 + h.x1) / 2.0);
        let beside = if left { i.checked_sub(1) } else { Some(i + 1).filter(|j| *j < HOUSES.len()) };
        let foot = beside.map_or(BASE - SHOP, |j| if (HOUSES[j].x0 - h.x1).abs() < 1.0 || (HOUSES[j].x1 - h.x0).abs() < 1.0 { HOUSES[j].top() - 9.0 } else { BASE - SHOP });
        if foot > top {
            let ex = if left { h.x0 - 5.0 } else { h.x1 + 3.4 };
            bar(d, lay, [ex, top - 9.0, ex + 1.6, foot], rim, rim_a);
        }
        bar(d, lay, [h.x0 - 5.0, top - 9.0, h.x1 + 5.0, top - 8.0], rim, rim_a * 0.6);
    }
    bar(d, lay, [h.x1 - 1.2, top, h.x1, BASE], shade(wall, -0.25), 0.5);
}

/// One window across the way: its stone, its glass, and what shows of the people behind it.
#[allow(clippy::too_many_arguments)]
fn pane(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, v: &Win, k: u32, lit: Option<Rgb>, wall: Rgb, power: f32, bloom: &mut Bloom, out: &mut Anchors) {
    let (x, y, w, h) = (v.x, v.y, v.w, v.h);
    if x > lay.w || x + w < 0.0 {
        return;
    }
    let s = w / WIN.0;
    let stone = shade(wall, 0.12);
    let dark = shade(wall, -0.40);
    d.rect(x - 3.0 * s, y - 7.0 * s, x + w + 3.0 * s, y - 2.0 * s, stone, 0.55);
    d.rect(x - 4.0 * s, y + h + 1.5 * s, x + w + 4.0 * s, y + h + 5.0 * s, stone, 0.75);
    d.rect(x - 1.5 * s, y - 1.5 * s, x + w + 1.5 * s, y + h + 1.5 * s, dark, 1.0);
    let look = (hash2(k, 861) * 10.0) as u32;
    let sash = |d: &mut Draw<Canvas>, col: Rgb, a: f32| d.rect(x, y + h * 0.47, x + w, y + h * 0.47 + 1.4 * s, col, a);
    match lit {
        Some(g) => {
            d.rect(x, y, x + w, y + h, g, 0.95);
            let dim = mix(g, wall, 0.5);
            let shape = mix(shade(wall, -0.55), g, 0.12);
            match look {
                1 => d.rect(x, y, x + w * 0.34, y + h, dim, 0.85),
                2 => d.rect(x + w * 0.66, y, x + w, y + h, dim, 0.85),
                3 => {
                    d.rect(x, y, x + w, y + h * 0.40, dim, 0.8);
                    for i in 1..5 {
                        d.rect(x, y + h * 0.08 * i as f32, x + w, y + h * 0.08 * i as f32 + s * 0.6, shade(dim, -0.2), 0.5);
                    }
                }
                4 => {
                    // someone at the window
                    d.circle(x + w * 0.55, y + h * 0.50, w * 0.11, shape, 0.95);
                    d.poly(&[(x + w * 0.30, y + h), (x + w * 0.36, y + h * 0.70), (x + w * 0.55, y + h * 0.63), (x + w * 0.74, y + h * 0.70), (x + w * 0.80, y + h)], shape, 0.95);
                }
                5 => {
                    d.ellipse(x + w * 0.72, y + h * 0.76, w * 0.20, h * 0.13, mix(shape, SAGE, 0.35), 0.9);
                    d.rect(x + w * 0.62, y + h * 0.86, x + w * 0.82, y + h, shape, 0.9);
                }
                6 => {
                    // a lamp on a table
                    d.poly(&[(x + w * 0.20, y + h * 0.56), (x + w * 0.40, y + h * 0.56), (x + w * 0.44, y + h * 0.70), (x + w * 0.16, y + h * 0.70)], mix(g, WHITE, 0.4), 1.0);
                    d.rect(x + w * 0.29, y + h * 0.70, x + w * 0.31, y + h * 0.88, shape, 0.9);
                    d.rect(x + w * 0.05, y + h * 0.88, x + w * 0.55, y + h, shape, 0.9);
                }
                7 => {
                    // shelves of books
                    for i in 0..3 {
                        let yy = y + h * (0.25 + 0.27 * i as f32);
                        d.rect(x + w * 0.5, yy, x + w, yy + s, shape, 0.8);
                        for j in 0..5 {
                            let bx = x + w * (0.54 + 0.09 * j as f32);
                            d.rect(bx, yy - h * 0.12, bx + w * 0.06, yy, mix(shape, [ROSE, BLUE, AMBER, SAGE, CREAM][(j + i) % 5], 0.3), 0.85);
                        }
                    }
                }
                8 => {
                    d.rect(x, y, x + w * 0.26, y + h, dim, 0.85);
                    d.rect(x + w * 0.74, y, x + w, y + h, dim, 0.85);
                }
                9 if k % 3 == 0 => {
                    // a cat on the inside sill
                    props::cat(d, x + w * 0.35, y + h, s * 0.75, shape, false, 1.0, 0.0);
                }
                _ => {}
            }
            sash(d, shape, 0.55);
            let pad = w * 0.18;
            bloom.light(x - pad, y - pad, x + w + pad, y + h + pad, mix(g, WARM[1], 0.5), 0.74 * power);
            if look == 0 && (v.who[2] * 13.7) % 1.0 < 0.5 && out.flicker.len() < 8 {
                out.flicker.push(([x, y, w, h], KERBSIDE));
            }
        }
        None => {
            let glass = mix(shade(wall, -0.32), sc.sky[5], 0.55 * sc.day);
            let a = 0.9 * (0.30 + 0.70 * smooth(-7.0, 4.0, sc.el_eff));
            d.rect(x, y, x + w, y + h, glass, a);
            d.poly(&[(x, y), (x + w, y), (x, y + h * 0.7)], mix(glass, sc.sky[6], 0.5), a * 0.35 * sc.day);
            if look == 3 {
                d.rect(x, y, x + w, y + h * 0.40, mix(glass, CREAM, 0.25 * sc.day), a);
            }
            sash(d, shade(wall, -0.5), 0.5);
        }
    }
    // what hangs outside: an air conditioner, or a box of flowers
    let hang = hash2(k, 867);
    if hang < 0.16 {
        let unit = mix(wall, CREAM, 0.18 + 0.2 * sc.day);
        d.rect(x + 5.0 * s, y + h - 12.0 * s, x + w - 5.0 * s, y + h + 2.0 * s, unit, 1.0);
        for i in 0..4 {
            let gx = x + (8.0 + 5.5 * i as f32) * s;
            d.rect(gx, y + h - 9.0 * s, gx + 2.6 * s, y + h - 1.0 * s, shade(unit, -0.2), 0.7);
        }
    } else if hang < 0.28 && !snowy(sc) {
        let leaf = mix(shade(wall, -0.3), mix(hex(sc.p.seasons[sc.season.max(1).min(2)][1]), SAGE, 0.4), 0.2 + 0.35 * sc.day);
        d.rect(x - 1.0 * s, y + h + 1.0 * s, x + w + 1.0 * s, y + h + 7.0 * s, shade(wall, -0.2), 1.0);
        for i in 0..5 {
            let bx = x + w * (0.1 + 0.2 * i as f32);
            d.ellipse(bx, y + h - 0.5 * s, 3.6 * s, 2.8 * s, shade(leaf, if i % 2 == 0 { 0.0 } else { -0.1 }), 1.0);
            if sc.season == 1 || sc.season == 2 {
                d.circle(bx + s, y + h - 2.0 * s, 1.3 * s, mix(leaf, [ROSE, CREAM, AMBER][i % 3], 0.4 + 0.4 * sc.day), 1.0);
            }
        }
    }
}

/// A fire escape: a landing at each floor, stairs between, a ladder up to the roof and a drop
/// ladder at the bottom.
fn escape(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, x0: f32, x1: f32, floors: usize, col: Rgb) {
    let iron = shade(col, -0.45);
    let leaf = mix(shade(col, -0.2), mix(hex(sc.p.seasons[sc.season.max(1).min(2)][1]), SAGE, 0.4), 0.2 + 0.35 * sc.day);
    for f in 0..floors {
        let y = 734.0 - FLOOR * f as f32;
        bar(d, lay, [x0, y, x1, y + 2.6], iron, 1.0);
        bar(d, lay, [x0, y - 21.0, x1, y - 19.6], iron, 1.0);
        let mut x = x0;
        while x <= x1 + 0.1 {
            ink(d, lay, &[(x, y - 20.0), (x, y)], 0.8, iron, 0.75);
            x += 6.5;
        }
        if snowy(sc) {
            bar(d, lay, [x0, y - 2.0, x1, y], hex(sc.p.snow), 0.75);
        }
        if f + 1 < floors {
            let (a, b) = if f % 2 == 0 { (x0 + 8.0, x1 - 30.0) } else { (x1 - 8.0, x0 + 30.0) };
            ink(d, lay, &[(a, y), (b, y - FLOOR)], 1.8, iron, 1.0);
            ink(d, lay, &[(a, y - 16.0), (b + (a - b) * 0.15, y - FLOOR * 0.85 - 16.0)], 0.9, iron, 0.9);
            for k in 1..9 {
                let t = k as f32 / 9.0;
                let (sx, sy) = (a + (b - a) * t, y - FLOOR * t);
                ink(d, lay, &[(sx - 3.0, sy), (sx + 3.0, sy)], 0.9, iron, 0.9);
            }
        }
        // a few pots on the landings
        if (f + x0 as usize) % 2 == 0 {
            for k in 0..2 {
                let px = x0 + 10.0 + 14.0 * k as f32 + 30.0 * (f % 3) as f32;
                bar(d, lay, [px - 3.0, y - 6.0, px + 3.0, y], shade(col, -0.15), 1.0);
                oval(d, lay, px, y - 8.0, 4.5, 3.5, leaf, 1.0);
            }
        }
    }
    let top = 734.0 - FLOOR * (floors - 1) as f32;
    for x in [x1 - 12.0, x1 - 5.0] {
        ink(d, lay, &[(x, top), (x, top - FLOOR + 12.0)], 1.0, iron, 1.0);
    }
    let mut y = top - 6.0;
    while y > top - FLOOR + 14.0 {
        ink(d, lay, &[(x1 - 12.0, y), (x1 - 5.0, y)], 0.8, iron, 0.9);
        y -= 6.0;
    }
    for x in [x0 + 12.0, x0 + 19.0] {
        ink(d, lay, &[(x, 736.0), (x, 770.0)], 1.0, iron, 1.0);
    }
    let mut y = 742.0;
    while y < 770.0 {
        ink(d, lay, &[(x0 + 12.0, y), (x0 + 19.0, y)], 0.8, iron, 0.9);
        y += 6.0;
    }
}

/// The jars hung along the rail of the neighbour's landing: one lit blue for each running container.
fn lanterns(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, col: Rgb, bloom: &mut Bloom) {
    let (x0, x1, _) = ESCAPES[1];
    let rail = 734.0 - FLOOR * 2.0 - 20.0;
    let n = sc.machine.containers.min(8) as usize;
    let iron = shade(col, -0.45);
    ink(d, lay, &[(x0 + 4.0, rail + 1.0), ((x0 + x1) / 2.0, rail + 5.0), (x1 - 4.0, rail + 1.0)], 0.6, iron, 0.8);
    for k in 0..8 {
        let x = x0 + 8.0 + (x1 - x0 - 16.0) * k as f32 / 7.0;
        let sag = 4.0 * (1.0 - (2.0 * k as f32 / 7.0 - 1.0).powi(2));
        let y = rail + 3.0 + sag;
        if k < n {
            let blue = mix(BLUE, WHITE, 0.25);
            bar(d, lay, [x - 2.2, y, x + 2.2, y + 6.0], blue, 1.0);
            d.glow(lay.dx(x), lay.dy(y + 3.0), lay.d(10.0), lay.d(10.0), BLUE, 0.35 + 0.35 * sc.dark, 2, Blend::Screen);
            bloom.light(lay.dx(x - 3.0), lay.dy(y - 1.0), lay.dx(x + 3.0), lay.dy(y + 7.0), BLUE, (0.5 + 0.7 * sc.dark) * sc.p.bloom.max(0.6));
        } else {
            bar(d, lay, [x - 2.0, y, x + 2.0, y + 5.5], mix(iron, sc.sky[5], 0.2), 0.8);
        }
    }
}

/// The ground floor of one house: its shop, or its front door.
fn shopfront(d: &mut Draw<Canvas>, sc: &Scene, lay: &Layout, i: usize, col: Rgb, bloom: &mut Bloom, out: &mut Anchors) {
    let h = &HOUSES[i];
    let u = lay.u;
    let wall = brick(sc, h.brick);
    let front = mix(col, h.front, 0.16 + 0.34 * sc.day);
    let dusk = smooth(2.0, -4.0, sc.el_eff);
    let open_now = open(i, sc);
    bar(d, lay, [h.x0, BASE - SHOP, h.x1, BASE], if i == STOOP { wall } else { front }, 1.0);
    bar(d, lay, [h.x0, BASE - SHOP - 2.0, h.x1, BASE - SHOP + 2.0], shade(wall, 0.1), 0.8);
    let dark = shade(front, -0.45);
    let thing = shade(front, -0.55);
    let glow = shop_light(i);
    // a window of the shop: lit while it is open, and showing the street by day when it is not
    let window = |d: &mut Draw<Canvas>, b: [f32; 4], lit: f32| {
        let day_glass = mix(dark, sc.sky[5], 0.45 * sc.day);
        let c = mix(day_glass, glow, lit * (0.40 + 0.60 * dusk));
        bar(d, lay, [b[0] - 2.0, b[1] - 2.0, b[2] + 2.0, b[3] + 2.0], dark, 1.0);
        bar(d, lay, b, c, 1.0);
    };
    let door = |d: &mut Draw<Canvas>, x0: f32, x1: f32, top: f32, lit: f32| {
        bar(d, lay, [x0 - 2.0, top - 2.0, x1 + 2.0, BASE], dark, 1.0);
        bar(d, lay, [x0, top, x1, BASE], mix(shade(front, -0.3), glow, lit * 0.6 * (0.4 + 0.6 * dusk)), 1.0);
        bar(d, lay, [x0 + 2.0, top + 3.0, x1 - 2.0, top + (BASE - top) * 0.55], mix(dark, glow, lit * 0.8 * (0.4 + 0.6 * dusk)), 0.9);
        ink(d, lay, &[(x1 - 4.0, top + (BASE - top) * 0.6), (x1 - 4.0, top + (BASE - top) * 0.66)], 1.2, mix(front, CREAM, 0.4), 0.9);
    };
    let lit = if open_now { 1.0 } else { 0.0 };
    if open_now && i != STOOP {
        let b = shop_window(i);
        bloom.light(lay.dx(b[0]), lay.dy(b[1]), lay.dx(b[2]), lay.dy(b[3]), glow, (0.25 + 0.75 * sc.dark) * 0.9 * sc.p.bloom);
    }
    match i {
        STOOP => {
            // steps up to the front door, with railings, and a lamp over it
            let stone = mix(wall, CREAM, 0.12 + 0.12 * sc.day);
            for k in 0..5 {
                let kf = k as f32;
                bar(d, lay, [44.0 + 3.0 * kf, BASE - 6.0 * (kf + 1.0), 120.0 - 3.0 * kf, BASE - 6.0 * kf], shade(stone, -0.04 * kf), 1.0);
            }
            door(d, 66.0, 100.0, 760.0, 0.0);
            bar(d, lay, [64.0, 750.0, 102.0, 758.0], mix(dark, WARM[1], 0.6 * dusk), 1.0);
            let iron = shade(col, -0.45);
            for x in [44.0, 120.0] {
                ink(d, lay, &[(x, BASE), (x + if x < 80.0 { 12.0 } else { -12.0 }, BASE - 34.0), (x + if x < 80.0 { 12.0 } else { -12.0 }, BASE - 48.0)], 1.2, iron, 1.0);
            }
            if dusk > 0.05 {
                oval(d, lay, 83.0, 754.0, 2.0, 2.0, WARM[0], dusk);
                bloom.light(lay.dx(78.0), lay.dy(749.0), lay.dx(88.0), lay.dy(759.0), WARM[1], 0.7 * dusk);
            }
            bar(d, lay, [130.0, 816.0, 156.0, 840.0], mix(dark, WARM[2], 0.5 * dusk * (sc.t > 18.0 || sc.t < 1.0) as i32 as f32), 1.0);
            for k in 0..4 {
                ink(d, lay, &[(132.0 + 6.5 * k as f32, 816.0), (132.0 + 6.5 * k as f32, 840.0)], 0.9, iron, 1.0);
            }
            // someone sitting out on the steps on a warm evening, their phone lighting their face
            let warm_night = sc.temp > 14.0 && sc.rain < 0.05 && (18.5..23.5).contains(&sc.t);
            if warm_night {
                let body = shade(col, -0.5);
                ink(d, lay, &[(112.0, 826.0), (113.0, 813.0)], 9.0, mix(body, ROSE, 0.2), 1.0);
                ink(d, lay, &[(113.0, 827.0), (104.0, 829.0), (104.0, 844.0)], 3.4, body, 1.0);
                oval(d, lay, 112.5, 805.0, 4.2, 4.4, mix(body, [214.0, 180.0, 150.0], 0.2), 1.0);
                if dusk > 0.2 {
                    bar(d, lay, [106.0, 812.0, 108.5, 816.0], SCREEN, 1.0);
                    d.glow(lay.dx(108.0), lay.dy(809.0), lay.d(9.0), lay.d(9.0), SCREEN, 0.4 * dusk, 2, Blend::Screen);
                }
            }
        }
        DINER => {
            let b = shop_window(i);
            window(d, b, lit);
            if open_now {
                // the counter, its stools, the lamps over it, and whoever is eating
                bar(d, lay, [b[0], 818.0, b[2], 836.0], thing, 0.85);
                for k in 0..6 {
                    let x = b[0] + 16.0 + 30.0 * k as f32;
                    oval(d, lay, x, 812.0, 5.0, 2.0, thing, 1.0);
                    oval(d, lay, x, 786.0, 2.6, 2.0, mix(WARM[0], WHITE, 0.5), 0.95);
                    ink(d, lay, &[(x, b[1]), (x, 784.0)], 0.6, thing, 0.8);
                }
                let t = sc.t as f32;
                let meal = gauss(t, 7.8, 1.0).max(gauss(t, 12.9, 1.0)).max(gauss(t, 19.2, 1.3)).max(0.4 * gauss(t, 23.6, 0.8));
                for k in 0..6u32 {
                    if hash2(k, (sc.ts / 1800.0) as u32 + 1701) < 0.15 + 0.7 * meal {
                        let x = b[0] + 16.0 + 30.0 * k as f32;
                        ink(d, lay, &[(x, 809.0), (x, 798.0)], 8.0, mix(thing, COATS[k as usize % 7], 0.25), 1.0);
                        oval(d, lay, x, 791.0, 4.0, 4.3, thing, 1.0);
                    }
                }
                // the cook, behind the pass
                oval(d, lay, b[2] - 20.0, 796.0, 4.0, 4.3, thing, 1.0);
                bar(d, lay, [b[2] - 24.0, 789.0, b[2] - 16.0, 793.0], mix(thing, WHITE, 0.6), 1.0);
                ink(d, lay, &[(b[2] - 20.0, 802.0), (b[2] - 20.0, 816.0)], 9.0, mix(thing, WHITE, 0.4), 1.0);
            }
            for k in 1..4 {
                let x = b[0] + (b[2] - b[0]) * k as f32 / 4.0;
                bar(d, lay, [x - 0.8, b[1], x + 0.8, b[3]], mix(front, CREAM, 0.2), 1.0);
            }
            bar(d, lay, [b[0] - 4.0, 758.0, b[2] + 4.0, 768.0], mix(front, CREAM, 0.25), 1.0);
            door(d, 366.0, 394.0, 774.0, lit);
        }
        BODEGA => {
            let b = shop_window(i);
            window(d, b, lit);
            // shelves stacked to the ceiling
            for k in 0..4 {
                let y = b[1] + 10.0 + 11.0 * k as f32;
                bar(d, lay, [b[0], y, b[2], y + 1.0], thing, 0.7);
                for j in 0..14u32 {
                    let x = b[0] + 3.0 + (b[2] - b[0] - 6.0) * j as f32 / 14.0;
                    let tint = [ROSE, AMBER, BLUE, SAGE, CREAM, WARM[0]][(hash2(j, k + 1801) * 6.0) as usize % 6];
                    bar(d, lay, [x, y - 7.0, x + 6.0, y], mix(thing, tint, 0.35 + 0.25 * lit), 0.85);
                }
            }
            oval(d, lay, b[0] + 22.0, 820.0, 4.0, 4.3, thing, 1.0);
            ink(d, lay, &[(b[0] + 22.0, 826.0), (b[0] + 22.0, 836.0)], 9.0, thing, 1.0);
            bar(d, lay, [b[0], 828.0, b[0] + 46.0, 838.0], shade(front, -0.2), 1.0);
            door(d, 572.0, 604.0, 782.0, lit);
            door(d, 610.0, 634.0, 778.0, 0.0);
            // the awning, striped, over the window and the stands
            let stripe = mix(col, [214.0, 172.0, 76.0], 0.22 + 0.48 * sc.day);
            let pale = mix(col, CREAM, 0.20 + 0.45 * sc.day);
            for k in 0..12 {
                let (a, bb) = (406.0 + 16.5 * k as f32, 406.0 + 16.5 * (k + 1) as f32);
                d.poly(&to_px(lay, &[(a + 2.0, 752.0), (bb + 2.0, 752.0), (bb, 778.0), (a, 778.0)]), if k % 2 == 0 { stripe } else { pale }, 1.0);
            }
            bar(d, lay, [404.0, 778.0, 606.0, 784.0], shade(stripe, -0.1), 1.0);
            if snowy(sc) {
                bar(d, lay, [408.0, 749.0, 608.0, 753.0], hex(sc.p.snow), 0.85);
            }
            write(d, lay, "DELI", (470.0, 758.0), 14.0, 2.2, mix(col, [120.0, 40.0, 36.0], 0.4 + 0.3 * sc.day), 0.95, Blend::Over);
            neon(d, sc, lay, "OPEN", (b[2] - 44.0, b[1] + 4.0), 9.0, [110.0, 214.0, 200.0], 1.0, bloom);
            // fruit and flowers out on the pavement in the daytime
            if (7.0..21.0).contains(&sc.t) {
                for (x0, x1) in [(416.0, 470.0), (478.0, 532.0)] {
                    d.poly(&to_px(lay, &[(x0, 834.0), (x1, 834.0), (x1 + 3.0, BASE), (x0 - 3.0, BASE)]), mix(thing, [150.0, 112.0, 80.0], 0.3), 1.0);
                    for row in 0..2 {
                        for j in 0..9u32 {
                            let x = x0 + 4.0 + (x1 - x0 - 8.0) * j as f32 / 8.0;
                            let tint = [[230.0, 150.0, 60.0], [200.0, 70.0, 60.0], [150.0, 190.0, 90.0], [240.0, 210.0, 90.0]][((x0 as u32 / 60) as usize + row) % 4];
                            oval(d, lay, x, 832.0 + 4.0 * row as f32, 3.0, 2.6, mix(thing, tint, 0.3 + 0.45 * sc.day + 0.2 * dusk), 1.0);
                        }
                    }
                }
                for k in 0..4u32 {
                    let x = 542.0 + 9.0 * k as f32;
                    bar(d, lay, [x - 3.0, 838.0, x + 3.0, BASE], mix(thing, BLUE, 0.2), 1.0);
                    for j in 0..3u32 {
                        let tint = [ROSE, CREAM, [190.0, 120.0, 190.0], AMBER][((k + j) % 4) as usize];
                        oval(d, lay, x - 2.0 + 2.0 * j as f32, 832.0 - 2.0 * (j % 2) as f32, 2.4, 2.4, mix(thing, tint, 0.35 + 0.45 * sc.day), 1.0);
                    }
                }
            }
            let _ = u;
        }
        HOME => {
            let b = shop_window(i);
            window(d, b, lit);
            // the machines, round doors in a row, and a few going round
            for k in 0..5 {
                let x = b[0] + 14.0 + 26.0 * k as f32;
                bar(d, lay, [x - 11.0, 812.0, x + 11.0, 838.0], mix(thing, CREAM, 0.35 + 0.2 * lit), 1.0);
                oval(d, lay, x, 825.0, 7.5, 7.5, thing, 1.0);
                if open_now && hash2(k, (sc.ts / 1500.0) as u32 + 1901) < 0.5 {
                    oval(d, lay, x, 825.0, 5.5, 5.5, mix(thing, [150.0, 190.0, 230.0], 0.5), 1.0);
                }
                bar(d, lay, [x - 11.0, 786.0, x + 11.0, 806.0], mix(thing, CREAM, 0.25 + 0.15 * lit), 1.0);
                oval(d, lay, x, 796.0, 6.0, 6.0, thing, 1.0);
            }
            // somebody waiting on their wash, on their phone
            if open_now && hash2((sc.ts / 2400.0) as u32, 1903) < 0.6 {
                oval(d, lay, 748.0, 794.0, 4.0, 4.3, thing, 1.0);
                ink(d, lay, &[(748.0, 800.0), (748.0, 812.0)], 8.5, thing, 1.0);
                bar(d, lay, [750.0, 803.0, 752.5, 807.0], SCREEN, 0.9);
            }
            write(d, lay, "LAUNDRY", (664.0, 755.0), 11.0, 1.8, mix(front, CREAM, 0.6), 0.9, Blend::Over);
            door(d, 794.0, 822.0, 774.0, lit);
            door(d, 846.0, 878.0, 772.0, 0.0);
            bar(d, lay, [846.0, 762.0, 878.0, 769.0], mix(dark, WARM[1], 0.5 * dusk), 1.0);
            for k in 0..3 {
                bar(d, lay, [840.0 - 2.0 * k as f32, BASE - 3.0 * k as f32 - 3.0, 884.0 + 2.0 * k as f32, BASE - 3.0 * k as f32], shade(wall, 0.05 - 0.03 * k as f32), 1.0);
            }
            if dusk > 0.05 {
                oval(d, lay, 886.0, 770.0, 1.8, 1.8, WARM[0], dusk);
                bloom.light(lay.dx(881.0), lay.dy(765.0), lay.dx(891.0), lay.dy(775.0), WARM[1], 0.6 * dusk);
            }
        }
        BOOKS => {
            let b = shop_window(i);
            // after hours one small lamp is left on in the window
            let after = if open_now { 1.0 } else { 0.3 * dusk };
            window(d, b, after);
            for k in 1..6 {
                let x = b[0] + (b[2] - b[0]) * k as f32 / 6.0;
                bar(d, lay, [x - 0.7, b[1], x + 0.7, b[3]], mix(front, CREAM, 0.15), 1.0);
            }
            bar(d, lay, [b[0], b[1] + 20.0, b[2], b[1] + 21.2], mix(front, CREAM, 0.15), 1.0);
            for j in 0..26u32 {
                let x = b[0] + 3.0 + (b[2] - b[0] - 6.0) * j as f32 / 26.0;
                let tall = 8.0 + 6.0 * hash2(j, 2001);
                let tint = [ROSE, BLUE, AMBER, SAGE, CREAM][(hash2(j, 2003) * 5.0) as usize % 5];
                bar(d, lay, [x, b[3] - tall, x + 4.4, b[3]], mix(thing, tint, 0.3 + 0.2 * after), 1.0);
            }
            oval(d, lay, b[0] + 22.0, 806.0, 4.0, 2.6, mix(WARM[0], WHITE, 0.3), 0.6 + 0.4 * after);
            bar(d, lay, [b[0] + 21.0, 808.0, b[0] + 23.0, 822.0], thing, 1.0);
            door(d, 1048.0, 1080.0, 780.0, lit);
            bar(d, lay, [1088.0, 790.0, 1122.0, 838.0], mix(dark, glow, after * 0.4 * (0.4 + 0.6 * dusk)), 1.0);
            let sign = mix(front, CREAM, 0.7);
            write(d, lay, "BOOKS", (952.0, 755.0), 13.0, 2.0, sign, 0.95, Blend::Over);
            if dusk > 0.05 {
                for x in [944.0, 1030.0] {
                    ink(d, lay, &[(x, 752.0), (x + 4.0, 748.0), (x + 10.0, 750.0)], 1.0, thing, 1.0);
                    d.glow(lay.dx(x + 10.0), lay.dy(760.0), lay.d(26.0), lay.d(12.0), WARM[0], 0.25 * dusk, 2, Blend::Screen);
                }
            }
            // a cart of cheap books outside by day, and the bench
            if (10.0..20.0).contains(&sc.t) && sc.rain < 0.05 {
                bar(d, lay, [914.0, 834.0, 968.0, 846.0], mix(thing, [150.0, 112.0, 80.0], 0.3), 1.0);
                for j in 0..12u32 {
                    let x = 916.0 + 4.3 * j as f32;
                    bar(d, lay, [x, 828.0, x + 3.6, 834.0], mix(thing, [ROSE, BLUE, AMBER, CREAM][j as usize % 4], 0.35 + 0.3 * sc.day), 1.0);
                }
                ink(d, lay, &[(918.0, 846.0), (918.0, BASE)], 1.2, thing, 1.0);
                ink(d, lay, &[(964.0, 846.0), (964.0, BASE)], 1.2, thing, 1.0);
            }
            bar(d, lay, [1086.0, 836.0, 1124.0, 839.0], thing, 1.0);
            for x in [1089.0, 1121.0] {
                ink(d, lay, &[(x, 839.0), (x, BASE)], 1.2, thing, 1.0);
            }
        }
        PIZZA => {
            let b = shop_window(i);
            window(d, b, lit);
            bar(d, lay, [b[0], 816.0, b[2], 838.0], thing, 0.85);
            for k in 0..3 {
                let x = b[0] + 18.0 + 22.0 * k as f32;
                oval(d, lay, x, 815.0, 9.0, 2.6, mix(thing, [230.0, 170.0, 90.0], 0.3 + 0.4 * lit), 1.0);
            }
            if open_now {
                d.glow(lay.dx(b[2] - 16.0), lay.dy(808.0), lay.d(16.0), lay.d(12.0), [255.0, 140.0, 60.0], 0.4 * (0.4 + 0.6 * dusk), 2, Blend::Screen);
                oval(d, lay, b[2] - 40.0, 797.0, 4.0, 4.3, thing, 1.0);
                ink(d, lay, &[(b[2] - 40.0, 803.0), (b[2] - 40.0, 815.0)], 9.0, mix(thing, WHITE, 0.3), 1.0);
            }
            neon(d, sc, lay, "PIZZA", (1150.0, 755.0), 13.0, BEACON, lit, bloom);
            door(d, 1248.0, 1280.0, 780.0, lit);
        }
        _ => {
            // the bar on the far corner: dim and red inside, a sign in the window
            door(d, 1474.0, 1500.0, 782.0, lit * 0.5);
            let b = shop_window(i);
            window(d, b, lit * 0.8);
            if open_now {
                for k in 0..3u32 {
                    if hash2(k, (sc.ts / 1200.0) as u32 + 2101) < 0.3 + 0.5 * smooth(18.0, 22.0, sc.t as f32).max(if sc.t < 2.0 { 1.0 } else { 0.0 }) {
                        let x = b[0] + 16.0 + 28.0 * k as f32;
                        oval(d, lay, x, 806.0, 4.0, 4.3, thing, 1.0);
                        ink(d, lay, &[(x, 812.0), (x, 826.0)], 9.0, thing, 1.0);
                    }
                }
                bar(d, lay, [b[0], 826.0, b[2], 836.0], thing, 0.9);
            }
            neon(d, sc, lay, "BAR", (b[0] + 26.0, 760.0), 14.0, [240.0, 110.0, 130.0], lit, bloom);
            if open_now {
                for k in 0..4 {
                    out.twinkle.push((lay.dx(b[0] + 30.0 + 10.0 * k as f32), lay.dy(767.0), [240.0, 110.0, 130.0], KERBSIDE));
                }
            }
        }
    }
}

fn show_doing(v: f64) -> String {
    doing_words(DOINGS[(v.round().max(0.0) as usize).min(DOINGS.len() - 1)]).to_string()
}

fn show_share(v: f64) -> String {
    format!("{}%", (v * 100.0).round())
}

const KNOBS: &[Knob] = &[
    Knob { key: "doing", label: "neighbour", lo: 0.0, hi: 6.0, step: 1.0, big: 1.0, show: show_doing },
    Knob { key: "bustle", label: "how busy", lo: 0.0, hi: 1.0, step: 0.05, big: 0.25, show: show_share },
    Knob { key: "wet", label: "wet street", lo: 0.0, hi: 1.0, step: 0.05, big: 0.25, show: show_share },
];

impl View for Street {
    fn id(&self) -> &'static str {
        "street"
    }

    fn about(&self) -> &'static str {
        "a block of walk-ups across a narrow street, from your own windowsill"
    }

    fn horizon(&self) -> f64 {
        HORIZON
    }

    fn knobs(&self) -> &'static [Knob] {
        KNOBS
    }

    fn real(&self, key: &str, sc: &Scene) -> f64 {
        match key {
            "doing" => DOINGS.iter().position(|d| *d == doing(sc)).unwrap_or(0) as f64,
            "bustle" => bustle(sc) as f64,
            _ => wet(sc) as f64,
        }
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Walkups::new(w, h))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let week = [
            "Monday: the bus every ten minutes, the street busy early",
            "a working day: busy at both ends of it",
            "a working day: busy at both ends of it",
            "a working day: busy at both ends of it",
            "Friday: lights on the roof garden after dark, the bar full late",
            "Saturday: a slow morning, washing on a roof, a late night",
            "Sunday: quiet, the bus every quarter of an hour, the bookshop opens at noon",
        ][sc.dow];
        let mut street = vec![format!("{}% of windows lit", (sc.lit_share * 100.0).round())];
        let shops: Vec<&str> = [(DINER, "the diner"), (HOME, "the laundromat"), (BOOKS, "the bookshop"), (PIZZA, "the pizza place"), (BAR, "the bar")]
            .iter()
            .filter(|(i, _)| open(*i, sc))
            .map(|(_, name)| *name)
            .collect();
        street.push(if shops.is_empty() { "only the bodega open".to_string() } else { format!("the bodega, {} open", shops.join(", ")) });
        let b = bustle(sc);
        street.push(
            if b > 0.7 {
                "busy"
            } else if b > 0.35 {
                "people about"
            } else if b > 0.12 {
                "quiet"
            } else {
                "nearly empty"
            }
            .to_string(),
        );
        street.push(match bus_every(sc) {
            Some(gap) => format!("a bus every {} minutes", (gap / 60.0).round()),
            None => "no buses until five".to_string(),
        });
        let w = wet(sc);
        if w > 0.5 {
            street.push("the road shining wet".to_string());
        } else if damp(sc) > 0.2 {
            street.push("puddles about".to_string());
        }
        for (flag, text) in [
            (sc.festoon, "lights on the roof garden"),
            (sc.laundry > 0, "washing on a roof"),
            (sc.smoke > 0.3, "the manholes steaming"),
            (sc.mist > 0.25, "mist in the street"),
            (sc.frost > 0.3, "frost on the roofs"),
            (!sc.birds.is_empty(), "birds crossing"),
            (!sc.planes.is_empty(), "a plane crossing"),
        ] {
            if flag {
                street.push(text.to_string());
            }
        }
        let s = &sc.eph.sun;
        if s.ha > 0.0 && (-2.0..12.0).contains(&s.el) && sc.sun_vis > 0.3 {
            street.push("the sun going down at the end of the cross street".to_string());
        } else if s.ha < 0.0 && (-2.0..10.0).contains(&s.el) && sc.sun_vis > 0.3 {
            street.push("the sun coming up behind the diner".to_string());
        }
        let cat = match puss(sc, sc.ts) {
            Puss::Sill { asleep: true } => "the cat asleep on your sill",
            Puss::Sill { asleep: false } => "the cat on your sill, watching the rain",
            Puss::Walking(..) => "the cat out along the roofs",
            Puss::Sat(..) => "the cat sitting out across the way",
        };
        let across = format!("your neighbour {}; {}", doing_words(doing(sc)), cat);
        let house = Yours { lamp: "the desk lamp across the way", containers: ("blue lantern on the fire escape", "blue lanterns on the fire escape"), smoke: "the stack in the street steaming" };
        vec![("week", week.to_string()), ("street", street.join(", ")), ("across", across), ("yours", yours(sc, &house))]
    }
}
