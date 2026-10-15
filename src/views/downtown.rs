//! City: the middle of town, seen across the railway.
//!
//! Every tower here is placed by hand. The skyline is highest a little right
//! of the middle and dips at both ends, where the sun comes up behind the
//! gasworks and goes down behind the bridge. Offices keep office hours, so
//! the towers go dark at the weekend while the flats stay lit.
//!
//!   the tallest tower        yours: the lantern at its top is the lamp, the floors under it
//!                            the containers, the steam off its roof the processor
//!   the old exchange         a weather beacon on its mast: blue for fair, red for rain on the
//!                            way, flashing when the sky is closing in
//!   the station clock        the real time
//!   the viaduct              trains to a timetable: every three minutes at the rush on a
//!                            working day, none in the small hours

use crate::color::{hex, mix, shade, smooth, Rgb, AMBER, BEACON, BLUE, CREAM, ROSE, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, Surface};
use crate::paint::{band, Light};
use crate::props::{self, Layout, DW};
use crate::rng::hash2;
use crate::scene::Scene;
use crate::views::kit::{self, Block, Cap, Paint, Row, Skin, Use};
use crate::story::draw::Tone;
use crate::story::{Damage, Snapshot};
use crate::views::episodes::city::{self as episode, CityBindings};
use crate::views::{lamp_colours, yours, Anchors, Blink, Breath, Lights, Smoke, Stage, View, Yours};
use std::cell::Cell;

pub struct Downtown;

const fn b(x: f32, w: f32, tiers: &'static [(f32, f32, f32)], caps: &'static [Cap], skin: Skin, who: Use) -> Block {
    Block { x, w, tiers, caps, skin, who }
}

use Cap::{Cornice, Dome, Gable, Hut, Mansard, Mast, Notch, Plant, Slant, Spire, Stack, Steps};
use Skin::{Bare, Fins, Loft, Punch, Ribbon};
use Use::{Home, Mixed, Office};

/// Where each layer meets the ground, and the highest anything in it reaches.
const FAR: (f32, f32) = (792.0, 660.0);
const TOWERS: (f32, f32) = (830.0, 280.0);
const MID: (f32, f32) = (884.0, 720.0);
const RAIL: (f32, f32) = (962.0, 690.0);
const NEAR: (f32, f32) = (1012.0, 820.0);

/// The far side of town, pale with distance: the power station and the gasholder on the left,
/// where the sun comes up; the backs of more towers; and low sheds by the bridge on the right.
const FAR_BLOCKS: &[Block] = &[
    b(52.0, 46.0, &[(1.0, 58.0, 0.0)], &[], Bare, Office),
    b(150.0, 8.0, &[(1.0, 98.0, 0.0)], &[], Bare, Office),
    b(170.0, 8.0, &[(1.0, 86.0, 0.0)], &[], Bare, Office),
    b(160.0, 52.0, &[(1.0, 34.0, 0.0)], &[], Bare, Office),
    b(240.0, 56.0, &[(1.0, 24.0, 0.0)], &[Dome(20.0)], Bare, Office),
    b(566.0, 30.0, &[(1.0, 120.0, 0.0), (0.7, 30.0, 0.0)], &[Mast(14.0)], Bare, Office),
    b(612.0, 28.0, &[(1.0, 178.0, 0.0)], &[Plant(0.5, 7.0)], Bare, Office),
    b(706.0, 34.0, &[(1.0, 96.0, 0.0), (0.6, 22.0, 0.0)], &[], Bare, Office),
    b(797.0, 30.0, &[(1.0, 150.0, 0.0)], &[Steps(9.0)], Bare, Office),
    b(904.0, 34.0, &[(1.0, 214.0, 0.0)], &[Slant(14.0)], Bare, Office),
    b(1008.0, 28.0, &[(1.0, 188.0, 0.0)], &[Plant(0.5, 6.0), Mast(16.0)], Bare, Office),
    b(1188.0, 34.0, &[(1.0, 170.0, 0.0), (0.7, 30.0, 0.0)], &[], Bare, Office),
    b(1276.0, 30.0, &[(1.0, 132.0, 0.0)], &[Steps(8.0)], Bare, Office),
    b(1500.0, 54.0, &[(1.0, 40.0, 0.0)], &[], Bare, Office),
];

/// The towers. Left to right: a first cluster round the old exchange, a dip where the winter sun
/// shows at noon, then the tall ones, falling away towards the bridge.
const TOWER_BLOCKS: &[Block] = &[
    b(116.0, 54.0, &[(1.0, 126.0, 0.0)], &[Plant(0.45, 9.0)], Punch(6.6, 9.0), Home),
    b(306.0, 64.0, &[(1.0, 218.0, 0.0)], &[Plant(0.5, 12.0)], Ribbon(8.4), Office),
    // the old exchange
    b(392.0, 72.0, &[(1.0, 150.0, 0.0), (0.72, 74.0, 0.0), (0.44, 44.0, 0.0)], &[Steps(18.0), Spire(40.0), Mast(26.0)], Punch(7.0, 9.6), Office),
    b(470.0, 46.0, &[(1.0, 330.0, 0.0)], &[Slant(26.0)], Fins(6.2, 9.0), Office),
    b(540.0, 38.0, &[(1.0, 236.0, 0.0)], &[Plant(0.5, 8.0)], Punch(6.4, 9.0), Home),
    b(584.0, 38.0, &[(1.0, 208.0, 0.0)], &[Plant(0.5, 8.0)], Punch(6.4, 9.0), Home),
    b(668.0, 62.0, &[(1.0, 166.0, 0.0)], &[Dome(13.0), Mast(14.0)], Fins(7.0, 9.0), Mixed),
    // the hotel, with its sign
    b(750.0, 56.0, &[(1.0, 194.0, 0.0), (0.62, 16.0, 0.0)], &[], Punch(7.0, 9.4), Mixed),
    b(846.0, 70.0, &[(1.0, 250.0, 0.0), (0.74, 52.0, 0.13), (0.46, 34.0, 0.27)], &[Plant(0.5, 9.0)], Ribbon(8.4), Office),
    // yours
    b(YOURS_X, 76.0, &[(1.0, 300.0, 0.0), (0.82, 96.0, 0.0), (0.6, 58.0, 0.0)], &[Plant(0.74, 24.0), Steps(12.0), Mast(46.0)], Fins(6.4, 9.0), Office),
    b(1048.0, 54.0, &[(1.0, 368.0, 0.0)], &[Slant(-44.0)], Ribbon(8.4), Office),
    b(1134.0, 84.0, &[(1.0, 262.0, 0.0), (0.84, 20.0, 0.0)], &[Notch(15.0)], Punch(7.0, 9.6), Home),
    // going up: the crane stands on this one
    b(WORKS_X, 66.0, &[(1.0, 190.0, 0.0)], &[], Ribbon(8.4), Office),
    b(1312.0, 50.0, &[(1.0, 150.0, 0.0)], &[Plant(0.5, 8.0)], Punch(6.6, 9.0), Home),
    b(1470.0, 58.0, &[(1.0, 176.0, 0.0)], &[Plant(0.4, 10.0), Mast(28.0)], Ribbon(8.4), Office),
    b(1568.0, 52.0, &[(1.0, 118.0, 0.0)], &[], Punch(6.6, 9.0), Home),
];
const EXCHANGE: usize = 2;
const SLIP: usize = 3;
const HOTEL: usize = 7;
const YOURS: usize = 9;
const SHARD: usize = 10;
const WORKS: usize = 12;
const LAST: usize = 14;
const YOURS_X: f32 = 962.0;
const WORKS_X: f32 = 1230.0;

/// The streets in front of them. Low at 190 to 260 and at 1340 to 1430, so that the sun can be
/// seen to touch the horizon when it rises and sets.
const MID_BLOCKS: &[Block] = &[
    b(30.0, 84.0, &[(1.0, 104.0, 0.0)], &[Cornice(3.0), Stack(0.3, 9.0)], Punch(9.0, 12.0), Home),
    b(114.0, 70.0, &[(1.0, 132.0, 0.0)], &[Hut(-0.25, 0.3, 9.0)], Ribbon(9.6), Office),
    b(186.0, 62.0, &[(1.0, 66.0, 0.0)], &[Gable(15.0)], Loft(11.0, 13.0), Home),
    b(250.0, 58.0, &[(1.0, 50.0, 0.0)], &[Mansard(12.0), Stack(-0.2, 7.0)], Punch(9.0, 12.0), Home),
    b(316.0, 68.0, &[(1.0, 96.0, 0.0)], &[Cornice(3.0), Hut(0.2, 0.25, 8.0)], Punch(8.0, 10.6), Home),
    b(386.0, 66.0, &[(1.0, 142.0, 0.0)], &[Steps(10.0)], Punch(7.4, 10.0), Mixed),
    b(452.0, 58.0, &[(1.0, 106.0, 0.0)], &[Mansard(13.0)], Punch(9.0, 12.0), Home),
    b(522.0, 74.0, &[(1.0, 80.0, 0.0)], &[Gable(17.0), Stack(0.28, 13.0)], Loft(11.0, 13.0), Office),
    b(598.0, 70.0, &[(1.0, 138.0, 0.0)], &[Cornice(3.0)], Ribbon(9.6), Office),
    b(666.0, 58.0, &[(1.0, 96.0, 0.0)], &[Mansard(12.0), Stack(0.22, 7.0)], Punch(9.0, 12.0), Home),
    b(732.0, 74.0, &[(1.0, 74.0, 0.0)], &[Cornice(3.0)], Loft(11.0, 13.0), Home),
    b(806.0, 68.0, &[(1.0, 120.0, 0.0)], &[Hut(0.22, 0.3, 9.0)], Punch(8.0, 10.6), Home),
    b(876.0, 64.0, &[(1.0, 88.0, 0.0)], &[Gable(15.0)], Punch(9.0, 12.0), Home),
    b(944.0, 70.0, &[(1.0, 130.0, 0.0)], &[Cornice(3.0), Plant(0.24, 9.0), Dome(6.0)], Punch(8.0, 10.6), Mixed),
    b(1016.0, 66.0, &[(1.0, 98.0, 0.0)], &[Mansard(13.0), Stack(-0.25, 7.0)], Punch(9.0, 12.0), Home),
    b(1084.0, 66.0, &[(1.0, 148.0, 0.0)], &[Cornice(3.0)], Ribbon(9.6), Office),
    b(1154.0, 62.0, &[(1.0, 90.0, 0.0)], &[Gable(15.0)], Loft(11.0, 13.0), Home),
    b(1222.0, 70.0, &[(1.0, 118.0, 0.0)], &[Steps(9.0)], Punch(7.4, 10.0), Home),
    b(1292.0, 62.0, &[(1.0, 78.0, 0.0)], &[Mansard(12.0), Stack(0.2, 7.0)], Punch(9.0, 12.0), Home),
    b(1356.0, 60.0, &[(1.0, 44.0, 0.0)], &[Gable(12.0)], Loft(11.0, 13.0), Home),
    b(1418.0, 60.0, &[(1.0, 58.0, 0.0)], &[Cornice(3.0)], Punch(9.0, 12.0), Home),
    b(1486.0, 72.0, &[(1.0, 104.0, 0.0)], &[Mansard(13.0), Stack(0.25, 7.0)], Punch(9.0, 12.0), Home),
    b(1564.0, 80.0, &[(1.0, 140.0, 0.0)], &[Hut(-0.2, 0.3, 9.0)], Ribbon(9.6), Office),
];

/// The roofs under the window, dark and close. Left to right: the one with the water tank, a long
/// low one with a skylight, the terrace, one with a pigeon loft, and the one with the sign.
const NEAR_BLOCKS: &[Block] = &[
    b(112.0, 262.0, &[(1.0, 84.0, 0.0)], &[], Bare, Home),
    b(404.0, 290.0, &[(1.0, 48.0, 0.0)], &[], Bare, Home),
    b(736.0, 336.0, &[(1.0, 40.0, 0.0)], &[], Bare, Home),
    b(1072.0, 300.0, &[(1.0, 54.0, 0.0)], &[], Bare, Home),
    b(1428.0, 380.0, &[(1.0, 88.0, 0.0)], &[], Bare, Home),
];

/// The top of the viaduct, where the rails are.
const DECK: f32 = 902.0;
const ARCH: f32 = 66.0;
const CLOCK_X: f32 = 448.0;
const CARS: usize = 5;
const CAR: f32 = 50.0;
/// seconds a train takes to cross the picture
const CROSSING: f64 = 18.0;

/// Seconds between trains at this hour, or none in the small hours.
fn headway(sc: &Scene) -> Option<f64> {
    let (t, working) = (sc.t, sc.dow < 5);
    if (0.5..5.0).contains(&t) {
        None
    } else if working && ((7.0..9.5).contains(&t) || (16.5..19.0).contains(&t)) {
        Some(180.0)
    } else if (6.0..22.0).contains(&t) {
        Some(if working { 360.0 } else { 540.0 })
    } else {
        Some(780.0)
    }
}

/// Where a train is at this instant: the x of its nose in design units, and which way it runs.
fn train_at(sc: &Scene, now: f64) -> Option<(f32, f32)> {
    let gap = headway(sc)?;
    let k = (now / gap).floor();
    let f = (now - k * gap - 12.0) / CROSSING;
    if !(0.0..=1.0).contains(&f) {
        return None;
    }
    let way = if k as i64 % 2 == 0 { 1.0 } else { -1.0 };
    let length = CARS as f32 * (CAR + 3.0);
    let run = DW + length + 80.0;
    Some((if way > 0.0 { -40.0 + run * f as f32 } else { DW + 40.0 - run * f as f32 }, way))
}

fn train<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, nose: f32, way: f32) {
    let (rail, tall) = (lay.dy(DECK - 2.2), lay.d(13.5));
    let body = mix(sc.layers[3], CREAM, 0.16 + 0.30 * sc.day);
    let stripe = mix(body, ROSE, 0.55);
    let lit = smooth(2.0, -4.0, sc.el_eff);
    let glass = mix(mix(sc.layers[3], sc.sky[5], 0.55), WARM[0], lit);
    for k in 0..CARS {
        let front = nose - way * k as f32 * (CAR + 3.0);
        let (x0, x1) = (lay.dx(front.min(front - way * CAR)), lay.dx(front.max(front - way * CAR)));
        if x1 < -4.0 || x0 > lay.w + 4.0 {
            continue;
        }
        let slope = if k == 0 { lay.d(7.0) } else { 0.0 };
        // the leading car has a raked nose
        let (nl, nr) = if way > 0.0 { (0.0, slope) } else { (slope, 0.0) };
        d.poly(&[(x0 + nl, rail - tall), (x1 - nr, rail - tall), (x1, rail - tall * 0.45), (x1, rail), (x0, rail), (x0, rail - tall * 0.45)], body, 1.0);
        d.rect(x0, rail - tall * 0.30, x1, rail - tall * 0.16, stripe, 0.9);
        let (wx0, wx1) = (x0 + lay.d(4.0) + nl, x1 - lay.d(4.0) - nr);
        let n = 6;
        for i in 0..n {
            let a = wx0 + (wx1 - wx0) * i as f32 / n as f32;
            d.rect(a, rail - tall * 0.82, a + (wx1 - wx0) / n as f32 - lay.d(1.6), rail - tall * 0.42, glass, 0.95);
        }
    }
    if lit > 0.1 {
        let x = lay.dx(nose);
        d.glow(x + way * lay.d(3.0), rail - tall * 0.3, lay.d(20.0), lay.d(9.0), [255.0, 240.0, 206.0], 0.75 * lit, 2, Blend::Screen);
        // and light from the windows spills on the parapet
        let tail = lay.dx(nose - way * CARS as f32 * (CAR + 3.0));
        d.rect_mode(x.min(tail), rail - tall, x.max(tail), rail + lay.d(3.0), WARM[1], 0.10 * lit, Blend::Screen);
    }
}

/// The arches of the viaduct and the station they run into, as outline.
fn railway(sh: &mut Shaper, lay: &Layout) {
    let (deck, spring, foot) = (lay.dy(DECK), lay.dy(DECK + 26.0), lay.dy(RAIL.0 + 2.0));
    // the parapet and the deck
    sh.poly(&[(-2.0, deck - lay.d(2.2)), (lay.w + 2.0, deck - lay.d(2.2)), (lay.w + 2.0, deck + lay.d(7.0)), (-2.0, deck + lay.d(7.0))]);
    let mut x = -30.0;
    while x < DW + ARCH {
        let (l, r) = (lay.dx(x), lay.dx(x + ARCH));
        let pier = lay.d(5.0);
        sh.poly(&[(l - pier, deck), (l + pier, deck), (l + pier, foot), (l - pier, foot)]);
        // the stone between the deck and the curve of each arch
        let (mid, half, rise) = ((l + r) / 2.0, (r - l) / 2.0 - pier, lay.d(19.0));
        let mut pts = vec![(l + pier, deck)];
        for i in 0..=14 {
            let a = std::f32::consts::PI * (1.0 - i as f32 / 14.0);
            pts.push((mid + half * a.cos(), spring - rise * a.sin()));
        }
        pts.push((r - pier, deck));
        sh.poly(&pts);
        x += ARCH;
    }
    // the train shed, and the tower with the clock
    sh.ellipse(lay.dx(356.0), lay.dy(934.0), lay.d(78.0), lay.d(52.0));
    sh.poly(&[(lay.dx(278.0), lay.dy(934.0)), (lay.dx(434.0), lay.dy(934.0)), (lay.dx(434.0), foot), (lay.dx(278.0), foot)]);
    let (cx, top) = (lay.dx(CLOCK_X), lay.dy(CLOCK_TOP));
    let half = lay.d(15.0);
    sh.poly(&[(cx - half, top), (cx + half, top), (cx + half, foot), (cx - half, foot)]);
    sh.poly(&[(cx - half * 1.25, top - lay.d(34.0)), (cx + half * 1.25, top - lay.d(34.0)), (cx + half * 1.25, top + 1.0), (cx - half * 1.25, top + 1.0)]);
    sh.poly(&[(cx - half * 1.38, top - lay.d(34.0) + 1.0), (cx, top - lay.d(74.0)), (cx + half * 1.38, top - lay.d(34.0) + 1.0)]);
}
const CLOCK_TOP: f32 = 786.0;

/// What stands on the near roofs, as outline: the water tank, bulkheads, the pigeon loft, the sign.
fn roofscape(sh: &mut Shaper, lay: &Layout) {
    let r = |sh: &mut Shaper, x0: f32, y0: f32, x1: f32, y1: f32| {
        sh.poly(&[(lay.dx(x0), lay.dy(y0)), (lay.dx(x1), lay.dy(y0)), (lay.dx(x1), lay.dy(y1) + 1.0), (lay.dx(x0), lay.dy(y1) + 1.0)]);
    };
    // the tank on its legs
    for x in [100.0, 142.0] {
        r(sh, x - 1.6, 884.0, x + 1.6, 928.0);
    }
    r(sh, 92.0, 846.0, 150.0, 886.0);
    sh.poly(&[(lay.dx(88.0), lay.dy(846.0) + 1.0), (lay.dx(121.0), lay.dy(828.0)), (lay.dx(154.0), lay.dy(846.0) + 1.0)]);
    // a stair head and a parapet beside it
    r(sh, 186.0, 902.0, 226.0, 928.0);
    r(sh, -4.0, 922.0, 243.0, 928.0);
    // the long low roof: a skylight's kerb and two vents
    r(sh, 300.0, 958.0, 356.0, 964.0);
    r(sh, 486.0, 950.0, 498.0, 964.0);
    r(sh, 510.0, 954.0, 520.0, 964.0);
    // the terrace: a low wall
    r(sh, 568.0, 966.0, 904.0, 972.0);
    // the pigeon loft, with its pitched roof
    r(sh, 1010.0, 936.0, 1056.0, 958.0);
    sh.poly(&[(lay.dx(1004.0), lay.dy(936.0) + 1.0), (lay.dx(1033.0), lay.dy(922.0)), (lay.dx(1062.0), lay.dy(936.0) + 1.0)]);
    r(sh, 1150.0, 946.0, 1190.0, 958.0);
    // the legs of the sign
    for x in [1452.0, 1506.0, 1560.0] {
        r(sh, x - 1.5, 846.0, x + 1.5, 924.0);
    }
    r(sh, 1444.0, 846.0, 1568.0, 849.0);
    r(sh, 1444.0, 884.0, 1568.0, 887.0);
    r(sh, 1238.0, 918.0, 1620.0, 924.0);
}

/// The city laid out for one screen.
struct City {
    lay: Layout,
    rows: Vec<Row>,
    depth: Depth,
    /// the lantern at the top of your tower: left, top, right, bottom
    lantern: [f32; 4],
    /// the tier under it, where the container floors show
    floors: [f32; 4],
    /// what the living scene works with, in design units
    bind: CityBindings,
    /// the instant the crane stopped, while its load is caught
    held: Cell<Option<f64>>,
    /// the load is caught, as of the last frame
    caught: Cell<bool>,
    /// the signal has failed: no passing train goes through the one that is stopped
    signal_down: Cell<bool>,
}

impl City {
    fn new(w: usize, h: usize, horizon: f64) -> City {
        let lay = Layout::new(w, h, horizon);
        let mut sh = Shaper::new();
        let far = kit::row(&mut sh, &lay, FAR.0, FAR.1, FAR_BLOCKS, 100, |sh, lay| {
            // low hills behind everything, and the bridge the sun sets behind
            let pts: Vec<(f32, f32)> = (0..=40)
                .map(|i| {
                    let x = i as f32 / 40.0 * DW;
                    let f = x / DW;
                    (lay.dx(x), lay.dy(772.0 - 13.0 * (f * 5.2 + 0.6).sin() - 7.0 * (f * 12.7).sin()))
                })
                .chain([(lay.w, lay.dy(FAR.0)), (0.0, lay.dy(FAR.0))])
                .collect();
            sh.poly(&pts);
            for x in BRIDGE {
                sh.poly(&[(lay.dx(x - 3.0), lay.dy(BRIDGE_TOP)), (lay.dx(x + 3.0), lay.dy(BRIDGE_TOP)), (lay.dx(x + 4.5), lay.dy(FAR.0)), (lay.dx(x - 4.5), lay.dy(FAR.0))]);
            }
            sh.poly(&[(lay.dx(1322.0), lay.dy(774.0)), (lay.w + 2.0, lay.dy(774.0)), (lay.w + 2.0, lay.dy(779.0)), (lay.dx(1322.0), lay.dy(779.0))]);
        });
        let mut towers = kit::row(&mut sh, &lay, TOWERS.0, TOWERS.1, TOWER_BLOCKS, 200, |_, _| {});
        let mut mid = kit::row(&mut sh, &lay, MID.0, MID.1, MID_BLOCKS, 300, |_, _| {});
        let rail = kit::row(&mut sh, &lay, RAIL.0, RAIL.1, &[], 400, railway);
        let near = kit::row(&mut sh, &lay, NEAR.0, NEAR.1, NEAR_BLOCKS, 500, roofscape);

        let yours = &towers.shapes[YOURS];
        let top = yours.tiers[2];
        let half = (top[2] - top[0]) * 0.74 / 2.0;
        let mid_x = (top[0] + top[2]) / 2.0;
        let lantern = [mid_x - half, top[1] - lay.d(24.0), mid_x + half, top[1]];
        let floors = top;
        // windows nobody can see are not worth lighting: those behind nearer roofs, and the top
        // of your tower, which shows something else
        let hidden = |x: f32, y: f32, nearer: &[&Row]| nearer.iter().any(|r| kit::covered(&r.mask, w, x, y));
        towers.wins.retain(|v| {
            let (x, y) = (v.x + v.w / 2.0, v.y + v.h / 2.0);
            !(hidden(x, y, &[&mid, &rail, &near]) || (x > floors[0] && x < floors[2] && y > floors[1] && y < floors[3]))
        });
        mid.wins.retain(|v| !hidden(v.x + v.w / 2.0, v.y + v.h / 2.0, &[&rail, &near]));

        let rows = vec![far, towers, mid, rail, near];
        let masks: Vec<(usize, Vec<u8>)> = rows.iter().map(|r| (r.mask.0, r.mask.1.clone())).collect();
        let depth = Depth::stack(&masks, w, h);
        let bind = City::bindings(&lay, &rows, w);
        City { lay, rows, depth, lantern, floors, bind, held: Cell::new(None), caught: Cell::new(false), signal_down: Cell::new(false) }
    }

    /// Where the living scene's objects are in this city, in design units.
    fn bindings(lay: &Layout, rows: &[Row], w: usize) -> CityBindings {
        let works = &rows[1].shapes[WORKS];
        let back = |x: f32, y: f32| lay.back(x, y);
        let (wx0, wy) = back(works.roof[0], works.roof[2]);
        let (wx1, _) = back(works.roof[1], works.roof[2]);
        // the first window of the streets row nearest to a place, as a design rectangle
        let window_near = |x: f32, y: f32| -> [f32; 4] {
            let (px, py) = (lay.dx(x), lay.dy(y));
            let best = rows[2].wins.iter().filter(|v| !v.strip).min_by(|a, b| {
                let d = |v: &kit::Win| (v.x + v.w / 2.0 - px).powi(2) + (v.y + v.h / 2.0 - py).powi(2);
                d(a).partial_cmp(&d(b)).unwrap_or(std::cmp::Ordering::Equal)
            });
            match best {
                Some(v) => {
                    let (x0, y0) = back(v.x, v.y);
                    let (x1, y1) = back(v.x + v.w, v.y + v.h);
                    [x0, y0, x1, y1]
                }
                None => [x - 4.0, y - 5.0, x + 4.0, y + 5.0],
            }
        };
        let _ = w;
        CityBindings {
            signal_x: 620.0,
            works: (wx0, wx1, wy),
            pallet: (wx0 + 20.0, wy - 22.0),
            box_at: (538.0, 956.0),
            cat: (CAT_SPOTS[0].0, CAT_SPOTS[0].1),
            perch: (CLOCK_X, CLOCK_TOP - 74.0),
            group: [560.0, 740.0, 700.0, 890.0],
            windows: [window_near(500.0, 800.0), window_near(800.0, 820.0)],
        }
    }

    /// The top of the crane's mast, in pixels.
    fn crane(&self) -> (f32, f32, f32) {
        let s = &self.rows[1].shapes[WORKS];
        (s.roof[0] + (s.roof[1] - s.roof[0]) * 0.70, s.roof[2] - self.lay.d(40.0), s.roof[2] - self.lay.d(40.0) - 96.0 * self.lay.u)
    }
}
const BRIDGE: [f32; 2] = [1392.0, 1556.0];
const BRIDGE_TOP: f32 = 694.0;

/// The towers stand a little darker than their distance alone would make them, so that the
/// skyline keeps its edge against a dusk sky.
fn tower_col(sc: &Scene) -> Rgb {
    mix(sc.layers[1], sc.layers[2], 0.34)
}

/// Where the cat sits, hour by hour: design x, y and the layer it is on.
const CAT_SPOTS: [(f32, f32); 5] = [(336.0, 958.0), (468.0, 964.0), (640.0, 966.0), (1100.0, 958.0), (1033.0, 922.0)];

fn cat_place(sc: &Scene) -> (f32, f32, f32) {
    let k = (hash2((sc.day_id % 100000) as u32, (sc.t / 1.5) as u32 + 9) * CAT_SPOTS.len() as f32) as usize % CAT_SPOTS.len();
    let face = if hash2(k as u32, (sc.t / 1.5) as u32) < 0.5 { -1.0 } else { 1.0 };
    (CAT_SPOTS[k].0, CAT_SPOTS[k].1, face)
}

/// What the beacon on the exchange is saying: its colour, and whether it flashes.
fn beacon(sc: &Scene) -> (Rgb, bool, &'static str) {
    let wet = sc.rain > 0.05 || sc.snow > 0.05 || sc.storm;
    if wet || sc.front > 0.0 {
        (BEACON, sc.snow > 0.05 || sc.storm, if wet { "red (rain or snow now)" } else { "red (rain on the way)" })
    } else if sc.cover > 0.55 || sc.fog > 0.3 {
        (BLUE, true, "flashing blue (cloud about, no rain in sight)")
    } else {
        (BLUE, false, "steady blue (fair ahead)")
    }
}

impl Stage for City {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        let mut out = Lights { share: 0.0, on: Vec::new() };
        let mut lit = 0;
        for r in &self.rows {
            kit::lights(&r.wins, sc, &mut out, &mut lit);
        }
        out.share = lit as f32 / out.on.len().max(1) as f32;
        out
    }

    fn lights_state(&self, sc: &Scene, state: &Snapshot) -> Lights {
        let mut out = self.lights(sc);
        if state.damage(episode::BLOCK) != Damage::Intact {
            // a block on one supply goes dark together
            let [g0, g1, g2, g3] = self.bind.group;
            let (x0, y0, x1, y1) = (self.lay.dx(g0), self.lay.dy(g1), self.lay.dx(g2), self.lay.dy(g3));
            let at: usize = self.rows[..2].iter().map(|r| r.wins.len()).sum();
            for (k, v) in self.rows[2].wins.iter().enumerate() {
                let (cx, cy) = (v.x + v.w / 2.0, v.y + v.h / 2.0);
                if cx >= x0 && cx <= x1 && cy >= y0 && cy <= y1 {
                    out.on[at + k] = None;
                }
            }
            out.share = out.on.iter().filter(|w| w.is_some()).count() as f32 / out.on.len().max(1) as f32;
        }
        out
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        self.ground_state(d, sc, lights, bloom, out, &Snapshot::intact())
    }

    fn story(&self) -> Option<crate::story::Definition> {
        Some(episode::define(&self.bind))
    }

    fn story_tone(&self, sc: &Scene, layer: Option<u8>) -> Tone {
        let row = match layer {
            Some(l) if l <= 1 => 1,
            Some(2) => 2,
            Some(3) => 3,
            _ => 4,
        };
        Tone::new(if row == 1 { tower_col(sc) } else { sc.layers[row] }, 0.22 + 0.64 * sc.day, sc.sky[5])
    }

    fn story_actor(&self, d: &mut Draw<Frame>, sc: &Scene, a: &crate::story::actors::Actor, _t: f32) -> bool {
        if a.kind != crate::story::actors::Kind::Train {
            return false;
        }
        train(d, sc, &self.lay, a.pos.0, 1.0);
        true
    }

    fn story_alive(&self, d: &mut Draw<Frame>, sc: &Scene, state: &Snapshot, t: f32, indoors: bool) {
        if indoors {
            return;
        }
        let lay = &self.lay;
        let b = &self.bind;
        self.signal_down.set(state.damage(episode::SIGNAL) != Damage::Intact);
        let caught = state.damage(episode::LOAD) != Damage::Intact;
        self.caught.set(caught);
        if !caught {
            self.held.set(None);
        }
        let tone = |l: u8| self.story_tone(sc, Some(l));
        d.s.behind = Some(3);
        if state.damage(episode::SIGNAL) != Damage::Intact {
            episode::signal_blink(d, lay, b.signal_x, t);
        }
        let s = state.get(episode::SIGNAL);
        if s.visible && !state.in_base(episode::SIGNAL) && s.damage != Damage::Missing {
            episode::signal_mast(d, lay, b.signal_x + s.pose.dx, s.damage, &tone(3));
        }
        for (id, at, kind) in [(episode::CONE, (b.signal_x - 20.0, episode::DECK_Y), 0), (episode::BARRIER, (b.signal_x - 70.0, episode::DECK_Y), 1), (episode::PART, (b.pallet.0 + 6.0, b.pallet.1 + 4.0), 2)] {
            let s = state.get(id);
            if !s.visible || state.in_base(id) || s.damage == Damage::Missing {
                continue;
            }
            self.paint_thing(d, sc, kind, (at.0 + s.pose.dx, at.1 + s.pose.dy), s.pose.turn);
        }
        d.s.behind = Some(1);
        let s = state.get(episode::LOAD);
        if s.visible && !state.in_base(episode::LOAD) {
            episode::hoist(d, lay, b.works, b.pallet, s.damage, &tone(1), s.pose);
        }
        d.s.behind = None;
        let s = state.get(episode::BOX);
        if s.visible && !state.in_base(episode::BOX) && s.damage != Damage::Missing {
            episode::junction_box(d, lay, (b.box_at.0 + s.pose.dx, b.box_at.1 + s.pose.dy), s.damage, &tone(4));
        }
    }

    fn ground_state(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let cols = sc.layers;
        let light = Light::of(sc, lay);
        let hz = |a: f32| a / 255.0;
        let mut at = 0;
        let mut on = |r: &Row| {
            let s = &lights.on[at..at + r.wins.len()];
            at += r.wins.len();
            s
        };
        let night = smooth(-2.0, -8.0, sc.el_eff);

        // ── the far side of town ──
        band(d.s, lay.dy(700.0), lay.dy(FAR.0 + 90.0), 52.0 * u, sc.haze, hz(sc.haze_alpha[0]));
        let far = &self.rows[0];
        let far_col = mix(cols[0], tower_col(sc), 0.30);
        kit::paint(d, sc, lay, far, on(far), bloom, &Paint { col: far_col, glass: 0.0, bloom: 0.0, rim: 0.5, tone: 0.03, hue: 0.0 });
        // the bridge: two cables from tower to tower, and the hangers down to the deck
        let (ty, dy) = (lay.dy(BRIDGE_TOP + 3.0), lay.dy(774.0));
        let sag = |f: f32| ty + (dy - lay.d(8.0) - ty) * 4.0 * f * (1.0 - f);
        let (a, b) = (lay.dx(BRIDGE[0]), lay.dx(BRIDGE[1]));
        let main: Vec<(f32, f32)> = (0..=24).map(|i| i as f32 / 24.0).map(|f| (a + (b - a) * f, sag(f))).collect();
        d.path(&main, 1.1 * u, far_col, 1.0);
        d.line((lay.dx(1322.0), dy), (a, ty), 1.1 * u, far_col, 1.0);
        d.line((b, ty), (b + (a - lay.dx(1322.0)), dy), 1.1 * u, far_col, 1.0);
        for i in 1..12 {
            let f = i as f32 / 12.0;
            d.line((a + (b - a) * f, sag(f)), (a + (b - a) * f, dy), 0.7 * u, far_col, 0.8);
        }
        if night > 0.1 {
            for x in BRIDGE {
                d.glow(lay.dx(x), lay.dy(BRIDGE_TOP - 2.0), 6.0 * u, 6.0 * u, BEACON, 0.55 * night, 2, Blend::Screen);
                out.blink.push(Blink { x: lay.dx(x), y: lay.dy(BRIDGE_TOP - 2.0), r: 10.0 * u, col: BEACON, every: 3.4, phase: 0.5, behind: Some(1) });
            }
        }
        // the power station smokes when it is cold enough for the city to want heating
        for (k, x) in [150.0f32, 170.0].into_iter().enumerate() {
            if sc.smoke > 0.12 * k as f32 {
                let s = Smoke { x: lay.dx(x), y: lay.dy(FAR.0 - 98.0 + 12.0 * k as f32), scale: 0.62, col: mix(cols[0], sc.sky[5], 0.72), behind: 1 };
                if !sc.live {
                    props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, x);
                }
                out.smoke.push(s);
            }
        }

        // ── the towers ──
        band(d.s, lay.dy(TOWERS.0 - 190.0), lay.dy(TOWERS.0 + 90.0), 60.0 * u, sc.haze, hz(sc.haze_alpha[1]));
        if sc.fog > 0.0 {
            band(d.s, lay.dy(TOWERS.1 + 60.0), lay.h * 1.3, 90.0 * u, mix(sc.sky[4], sc.haze, 0.5), 0.40 * sc.fog);
        }
        let towers = &self.rows[1];
        kit::paint(d, sc, lay, towers, on(towers), bloom, &Paint { col: tower_col(sc), glass: 0.55, bloom: 0.8, rim: 0.85, tone: 0.045, hue: 0.035 });
        self.landmarks(d, sc, &light, bloom, out, night, state);

        // ── the streets in front of them ──
        band(d.s, lay.dy(MID.0 - 120.0), lay.dy(MID.0 + 90.0), 52.0 * u, sc.haze, hz(sc.haze_alpha[2]));
        if sc.fog > 0.0 {
            band(d.s, lay.dy(MID.1), lay.h * 1.3, 60.0 * u, mix(sc.sky[4], sc.haze, 0.5), 0.47 * sc.fog);
        }
        if sc.mist > 0.02 {
            band(d.s, lay.dy(MID.0 - 96.0), lay.h * 1.3, 30.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.30 * sc.mist);
        }
        let mid = &self.rows[2];
        kit::paint(d, sc, lay, mid, on(mid), bloom, &Paint { col: cols[2], glass: 0.55, bloom: 1.0, rim: 0.95, tone: 0.06, hue: 0.12 });
        for (k, v) in mid.wins.iter().enumerate() {
            if !v.office && (v.who[2] * 13.7) % 1.0 < 0.05 && out.flicker.len() < 12 && lights.on[towers.wins.len() + k].is_some() {
                out.flicker.push(([v.x, v.y, v.w, v.h], 3));
            }
        }

        // ── the railway ──
        if sc.traffic * sc.dark > 0.05 {
            // the avenue behind the arches
            band(d.s, lay.dy(DECK + 12.0), lay.dy(1010.0), 22.0 * u, WARM[3], (0.36 * sc.traffic * sc.dark).min(0.40));
        }
        if sc.mist > 0.02 {
            band(d.s, lay.dy(DECK - 30.0), lay.h * 1.3, 26.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.24 * sc.mist);
        }
        if !sc.live && state.damage(episode::SIGNAL) == Damage::Intact {
            if let Some((nose, way)) = train_at(sc, sc.ts) {
                train(d, sc, lay, nose, way);
            }
        }
        let rail = &self.rows[3];
        let stone = mix(cols[3], cols[2], 0.28);
        kit::paint(d, sc, lay, rail, on(rail), bloom, &Paint { col: stone, glass: 0.0, bloom: 0.0, rim: 0.0, tone: 0.0, hue: 0.0 });
        self.station(d, sc, stone, &light, bloom, out, state);

        // ── the roofs under the window ──
        let near = &self.rows[4];
        kit::paint(d, sc, lay, near, on(near), bloom, &Paint { col: cols[4], glass: 0.0, bloom: 0.0, rim: 0.0, tone: 0.0, hue: 0.0 });
        self.roofs(d, sc, &light, bloom, out, state);
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let lay = &self.lay;
        let u = lay.u;
        let (cx, foot, top) = self.crane();
        let _ = foot;
        d.s.behind = Some(2);
        // while its load is caught the crane stands where it stopped
        let at = if self.held.get().is_none() && self.hold_wanted() { self.held.set(Some(now)); now } else { self.held.get().unwrap_or(now) };
        props::crane_jib(d, cx, top, u, tower_col(sc), props::crane_pose(sc, at));
        if let Some((nose, way)) = train_at(sc, now).filter(|_| !self.signal_down.get()) {
            d.s.behind = Some(3);
            train(d, sc, lay, nose, way);
        }
        d.s.behind = None;
        if sc.laundry > 0 {
            props::laundry(d, sc, lay.dx(376.0), lay.dx(470.0), lay.dy(964.0), u, sc.layers[4], 0, t * 1.5);
        }
        if sc.cat_out && !sc.cat_asleep {
            let (x, y, face) = cat_place(sc);
            let tail = if (t * 0.11).sin() > -0.3 { t * 1.4 } else { 0.0 };
            props::cat(d, lay.dx(x), lay.dy(y), u * 1.25, sc.layers[4], false, face, tail);
        }
    }

    fn lively(&self, sc: &Scene, now: f64) -> bool {
        train_at(sc, now).is_some() && !self.signal_down.get()
    }
}

impl City {
    /// What marks out particular towers: your lantern, the beacon, the sign, the crane.
    fn landmarks(&self, d: &mut Draw<Canvas>, sc: &Scene, light: &Light, bloom: &mut Bloom, out: &mut Anchors, night: f32, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let col = tower_col(sc);
        let shapes = &self.rows[1].shapes;
        let _ = light;

        // yours: the lantern is the lamp, and the floors under it are the containers
        let [lx0, ly0, lx1, ly1] = self.lantern;
        let pad = lay.d(2.2);
        let frame = shade(col, -0.22);
        if sc.machine.agents > 0 {
            let (lamp, glow, low) = lamp_colours(sc);
            d.rect(lx0 + pad, ly0 + pad, lx1 - pad, ly1 - pad * 0.6, lamp, 1.0);
            bloom.light(lx0 - pad, ly0 - pad, lx1 + pad, ly1 + pad, glow, (0.9 + 0.9 * sc.dark) * sc.p.bloom.max(0.6));
            out.breath.push(Breath { x: (lx0 + lx1) / 2.0, y: (ly0 + ly1) / 2.0, rx: (lx1 - lx0) * 1.5, ry: (ly1 - ly0) * 2.0, col: glow, strength: 0.45 + 0.55 * sc.dark, low, behind: Some(2) });
        } else {
            d.rect(lx0 + pad, ly0 + pad, lx1 - pad, ly1 - pad * 0.6, mix(col, sc.sky[5], 0.12 + 0.36 * sc.day), 1.0);
        }
        for k in 1..4 {
            let x = lx0 + pad + (lx1 - lx0 - 2.0 * pad) * k as f32 / 4.0;
            d.rect(x - 0.6 * u, ly0 + pad, x + 0.6 * u, ly1 - pad * 0.6, frame, 1.0);
        }
        let [fx0, fy0, fx1, fy1] = self.floors;
        let step = (fy1 - fy0) / 8.4;
        for k in 0..sc.machine.containers.min(8) as usize {
            let y = fy0 + step * (0.45 + k as f32);
            d.rect(fx0 + pad, y, fx1 - pad, y + step * 0.52, mix(BLUE, WHITE, 0.18), 1.0);
            bloom.light(fx0, y - pad, fx1, y + step * 0.52 + pad, BLUE, (0.5 + 0.7 * sc.dark) * sc.p.bloom.max(0.6));
        }
        let roof = (shapes[YOURS].roof[1] - lay.d(7.0), self.lantern[1]);
        out.stove = Some(Smoke { x: roof.0, y: roof.1, scale: 0.8, col: mix(col, sc.sky[5], 0.72), behind: 2 });
        out.music = Some((lx0 - lay.d(6.0), ly0));
        if sc.machine.load > 0.35 && !sc.live {
            props::smoke_still(d, sc, roof.0, roof.1, u * 0.8, mix(col, sc.sky[5], 0.72), 1.3);
        }
        if sc.machine.audio && !sc.live {
            let ink = mix(hex(sc.p.ink), AMBER, 0.4);
            props::note(d, lx0 - lay.d(10.0), ly0 - 14.0 * u, u, ink, 0.8, false);
            props::note(d, lx0 - lay.d(26.0), ly0 - 34.0 * u, u, ink, 0.55, true);
        }

        // the weather beacon on the old exchange
        let (tint, flashing, _) = beacon(sc);
        let tip = shapes[EXCHANGE].tip;
        let shown = 0.35 + 0.65 * sc.dark;
        d.glow(tip.0, tip.1, 13.0 * u, 13.0 * u, tint, 0.62 * shown, 2, Blend::Screen);
        d.circle(tip.0, tip.1, 2.6 * u, mix(tint, WHITE, 0.35), 1.0);
        bloom.light(tip.0 - 3.0 * u, tip.1 - 3.0 * u, tip.0 + 3.0 * u, tip.1 + 3.0 * u, tint, 0.8 * shown);
        if flashing {
            out.blink.push(Blink { x: tip.0, y: tip.1, r: 26.0 * u, col: tint, every: 1.7, phase: 0.0, behind: Some(2) });
        }

        // red lights on whatever an aeroplane might meet
        if night > 0.1 {
            for (k, i) in [SLIP, YOURS, SHARD, LAST].into_iter().enumerate() {
                let p = shapes[i].tip;
                d.glow(p.0, p.1, 8.0 * u, 8.0 * u, BEACON, 0.55 * night, 2, Blend::Screen);
                d.circle(p.0, p.1, 1.8 * u, BEACON, night);
                out.blink.push(Blink { x: p.0, y: p.1, r: 13.0 * u, col: BEACON, every: 2.9, phase: k as f32 * 0.27, behind: Some(2) });
            }
        }

        // after dark the owners pick out their rooflines in light, until one in the morning
        let crown = smooth(1.0, -4.0, sc.el_eff) * if sc.t >= 12.0 || sc.t < 1.0 { 1.0 } else { 0.0 };
        if crown > 0.03 {
            for (i, tint) in [(1, AMBER), (SLIP, crate::color::SAGE), (8, ROSE), (SHARD, BLUE), (11, WARM[0]), (LAST, crate::color::SAGE)] {
                let s = &shapes[i];
                let [x0, x1, y] = s.roof;
                // along the slope where the roof has one, along the parapet where it is flat
                let (p, q) = if (s.tip.0 - x1).abs() < 1.0 && s.tip.1 < y - 2.0 {
                    ((x0, y), s.tip)
                } else if (s.tip.0 - x0).abs() < 1.0 && s.tip.1 < y - 2.0 {
                    (s.tip, (x1, y))
                } else {
                    ((x0, y), (x1, y))
                };
                d.line(p, q, 1.6 * u, mix(tint, WHITE, 0.12), 0.9 * crown);
                for k in 0..6 {
                    let f = (k as f32 + 0.5) / 6.0;
                    let (x, yy) = (p.0 + (q.0 - p.0) * f, p.1 + (q.1 - p.1) * f);
                    let r = (q.0 - p.0).abs() / 11.0 + 1.5 * u;
                    bloom.light(x - r, yy - 2.0 * u, x + r, yy + 2.0 * u, tint, 0.42 * crown * sc.p.bloom.max(0.5));
                }
            }
        }

        // the hotel's sign: five letters of rose neon on a frame
        let h = &shapes[HOTEL];
        let (sx0, sx1, sy) = (h.roof[0], h.roof[1], h.roof[2]);
        let glow = smooth(3.0, -3.0, sc.el_eff);
        d.rect(sx0, sy - 13.0 * u, sx0 + 1.2 * u, sy, col, 1.0);
        d.rect(sx1 - 1.2 * u, sy - 13.0 * u, sx1, sy, col, 1.0);
        d.rect(sx0, sy - 3.4 * u, sx1, sy - 2.2 * u, col, 1.0);
        let neon = mix(mix(col, ROSE, 0.35), mix(ROSE, WHITE, 0.25), glow);
        for k in 0..5 {
            let x = sx0 + (sx1 - sx0) * (0.14 + 0.18 * k as f32);
            d.rect(x - 2.0 * u, sy - 12.0 * u, x + 2.0 * u, sy - 4.6 * u, neon, 1.0);
            if glow > 0.05 {
                bloom.light(x - 3.0 * u, sy - 13.0 * u, x + 3.0 * u, sy - 3.6 * u, ROSE, 0.55 * glow * sc.p.bloom.max(0.5));
            }
        }

        // the tower going up: open floors of steel above the glass, and the crane on top
        let w = &shapes[WORKS];
        let (wx0, wx1, wy) = (w.roof[0], w.roof[1], w.roof[2]);
        let lw = (1.2 * u).max(0.9);
        for k in 0..=4 {
            let x = wx0 + (wx1 - wx0) * k as f32 / 4.0;
            d.line((x, wy + 1.0), (x, wy - lay.d(40.0)), lw, col, 1.0);
        }
        for k in 1..=4 {
            let y = wy - lay.d(10.0) * k as f32;
            d.line((wx0, y), (wx1, y), lw, col, 1.0);
        }
        d.line((wx0, wy - lay.d(10.0)), (wx0 + (wx1 - wx0) * 0.25, wy), lw * 0.8, col, 0.9);
        d.line((wx1 - (wx1 - wx0) * 0.25, wy - lay.d(40.0)), (wx1, wy - lay.d(30.0)), lw * 0.8, col, 0.9);
        let (cx, foot, top) = self.crane();
        props::crane_mast(d, cx, foot, top, u, col);
        if !sc.live {
            props::crane_jib(d, cx, top, u, col, props::crane_pose(sc, sc.ts));
        }
        // the hoist on the open floors, and its load
        let s = state.get(episode::LOAD);
        if s.visible && state.in_base(episode::LOAD) {
            episode::hoist(d, lay, self.bind.works, self.bind.pallet, s.damage, &self.story_tone(sc, Some(1)), s.pose);
        }
        if sc.dark > 0.3 {
            let apex = (cx, top - 30.0 * u);
            d.glow(apex.0, apex.1, 9.0 * u, 9.0 * u, BEACON, 0.6 * sc.dark, 2, Blend::Screen);
            d.circle(apex.0, apex.1, 2.2 * u, BEACON, 1.0);
            out.blink.push(Blink { x: apex.0, y: apex.1, r: 15.0 * u, col: BEACON, every: 2.86, phase: 0.6, behind: Some(2) });
            if sc.crane_works {
                // work lights on the open floors
                for k in 0..3 {
                    let x = wx0 + (wx1 - wx0) * (0.2 + 0.3 * k as f32);
                    bloom.light(x - 2.0 * u, wy - lay.d(28.0), x + 2.0 * u, wy - lay.d(24.0), [236.0, 240.0, 255.0], 0.7 * sc.dark);
                    d.rect(x - 1.4 * u, wy - lay.d(27.0), x + 1.4 * u, wy - lay.d(25.0), [236.0, 240.0, 255.0], sc.dark);
                }
            }
        }
    }

    /// The station: the lines of its glass roof, and the clock.
    fn station(&self, d: &mut Draw<Canvas>, sc: &Scene, col: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let slate = shade(col, -0.10);
        // ribs of the shed
        let (sx, sy, rx, ry) = (lay.dx(356.0), lay.dy(934.0), lay.d(78.0), lay.d(52.0));
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let pane = mix(mix(col, sc.sky[5], 0.30), WARM[1], 0.75 * dusk);
        for k in 0..7 {
            // the glass between the ribs glows from the platforms below
            let (a0, a1) = (std::f32::consts::PI * (0.14 + 0.105 * k as f32), std::f32::consts::PI * (0.14 + 0.105 * k as f32 + 0.07));
            let arc = |a: f32, s: f32| (sx - rx * s * a.cos(), sy - ry * s * a.sin());
            d.poly(&[arc(a0, 0.90), arc(a1, 0.90), arc(a1, 0.62), arc(a0, 0.62)], pane, 0.22 + 0.62 * dusk);
        }
        if dusk > 0.05 {
            bloom.light(sx - rx * 0.7, sy - ry * 0.85, sx + rx * 0.7, sy - ry * 0.3, WARM[1], 0.20 * dusk * sc.p.bloom);
        }
        // the clock tower
        let (x, top) = (lay.dx(CLOCK_X), lay.dy(CLOCK_TOP));
        let half = lay.d(15.0);
        d.rect(x - half * 1.38, top - lay.d(34.0) - 2.4 * u, x + half * 1.38, top - lay.d(34.0) + 1.0, slate, 1.0);
        d.poly(&[(x - half * 1.38, top - lay.d(34.0) - 2.0 * u), (x, top - lay.d(74.0)), (x + half * 1.38, top - lay.d(34.0) - 2.0 * u)], slate, 1.0);
        d.rect(x - half * 1.3, top - 1.4 * u, x + half * 1.3, top + 1.4 * u, slate, 1.0);
        let tip = top - lay.d(74.0);
        d.line((x, tip + 2.0 * u), (x, tip - 9.0 * u), 1.3 * u, col, 1.0);
        d.circle(x, tip - 10.0 * u, 2.0 * u, col, 1.0);
        for k in 0..3 {
            let y = top + (14.0 + 22.0 * k as f32) * u;
            d.rect(x - 2.0 * u, y, x + 2.0 * u, y + 9.0 * u, slate, 0.9);
        }
        let side = if light.x < x { -1.0 } else { 1.0 };
        let rim = mix(col, light.col, 0.62);
        let a = (light.k * 0.75).min(0.9);
        if a > 0.02 {
            d.rect(x + side * (half * 1.25 - 1.0 * u) - 0.9 * u, top - lay.d(34.0), x + side * (half * 1.25 - 1.0 * u) + 0.9 * u, top, rim, a);
            d.rect(x + side * (half - 1.0 * u) - 0.8 * u, top + 2.0 * u, x + side * (half - 1.0 * u) + 0.8 * u, lay.dy(DECK - 3.0), rim, a * 0.8);
        }
        props::clock_face(d, sc, x, top - lay.d(17.0), lay.d(13.4), u, col, bloom);
        out.perch = Some((x, tip));
        // the signal beyond the platform
        let s = state.get(episode::SIGNAL);
        if s.visible && state.in_base(episode::SIGNAL) {
            episode::signal_mast(d, lay, self.bind.signal_x + s.pose.dx, s.damage, &self.story_tone(sc, Some(3)));
        }
        for (id, at, kind) in [(episode::CONE, (self.bind.signal_x - 20.0, episode::DECK_Y), 0), (episode::BARRIER, (self.bind.signal_x - 70.0, episode::DECK_Y), 1), (episode::PART, (self.bind.pallet.0 + 6.0, self.bind.pallet.1 + 4.0), 2)] {
            let s = state.get(id);
            if s.visible && state.in_base(id) && s.damage != Damage::Missing {
                self.paint_thing(d, sc, kind, (at.0 + s.pose.dx, at.1 + s.pose.dy), s.pose.turn);
            }
        }
        // lamps along the parapet of the viaduct
        if dusk > 0.05 {
            let mut lx = 18.0;
            while lx < DW {
                if (lx - 356.0).abs() > 86.0 && (lx - CLOCK_X).abs() > 24.0 {
                    let (px, py) = (lay.dx(lx), lay.dy(DECK - 3.4));
                    d.dot(px, py, 1.3 * u, 1.3 * u, WARM[0], dusk, Blend::Over);
                    bloom.light(px - 2.0 * u, py - 2.0 * u, px + 2.0 * u, py + 2.0 * u, WARM[1], 0.42 * dusk);
                }
                lx += ARCH;
            }
        }
    }

    /// What is on the roofs nearest the window.
    fn roofs(&self, d: &mut Draw<Canvas>, sc: &Scene, light: &Light, bloom: &mut Bloom, out: &mut Anchors, state: &Snapshot) {
        let lay = &self.lay;
        let u = lay.u;
        let col = sc.layers[4];
        let dx = |v: f32| lay.dx(v);
        let dy = |v: f32| lay.dy(v);
        let rim = mix(col, light.col, 0.62);
        let a = (light.k * 0.95).min(0.9);
        let lit_left = |x: f32| light.x < dx(x);

        // the water tank: bands round the drum, braces between the legs, light down one side
        let slate = shade(col, -0.10);
        for y in [856.0, 868.0, 880.0] {
            d.rect(dx(92.0), dy(y), dx(150.0), dy(y) + 1.1 * u, slate, 0.9);
        }
        d.line((dx(100.0), dy(928.0)), (dx(142.0), dy(888.0)), 1.1 * u, col, 1.0);
        d.line((dx(142.0), dy(928.0)), (dx(100.0), dy(888.0)), 1.1 * u, col, 1.0);
        if a > 0.02 {
            let x = if lit_left(121.0) { 92.0 } else { 148.4 };
            d.rect(dx(x), dy(846.0), dx(x + 1.6), dy(886.0), rim, a);
            let (p, q) = if lit_left(121.0) { ((88.0, 846.0), (121.0, 828.0)) } else { ((121.0, 828.0), (154.0, 846.0)) };
            d.line((dx(p.0), dy(p.1)), (dx(q.0), dy(q.1)), 1.5 * u, rim, a);
            for (x0, x1, y) in [(-4.0, 243.0, 922.0), (259.0, 549.0, 964.0), (568.0, 904.0, 966.0), (922.0, 1222.0, 958.0), (1238.0, 1620.0, 918.0)] {
                d.rect(dx(x0), dy(y), dx(x1), dy(y) + 1.3 * u, rim, a * 0.5);
            }
        }
        // the door at the head of the stairs has a lamp over it
        let lamp = smooth(1.5, -3.5, sc.el_eff);
        d.rect(dx(200.0), dy(909.0), dx(212.0), dy(928.0), shade(col, 0.07), 0.9);
        if lamp > 0.05 {
            d.circle(dx(206.0), dy(905.5), 1.6 * u, mix(WARM[0], WHITE, 0.2), lamp);
            bloom.light(dx(201.0), dy(903.0), dx(211.0), dy(912.0), WARM[1], 0.7 * lamp * sc.p.bloom.max(0.5));
        }
        // a skylight, lit when someone is home under it
        let home = smooth(2.5, -3.0, sc.el_eff) * if (17.5..23.6).contains(&sc.t) || (6.2..7.8).contains(&sc.t) { 1.0 } else { 0.0 };
        let glass = mix(mix(col, sc.sky[5], 0.5), WARM[0], home);
        d.poly(&[(dx(304.0), dy(958.0)), (dx(352.0), dy(958.0)), (dx(346.0), dy(951.0)), (dx(310.0), dy(951.0))], glass, 0.35 + 0.65 * home.max(smooth(-5.0, 4.0, sc.el_eff) * 0.6));
        if home > 0.05 {
            bloom.light(dx(306.0), dy(948.0), dx(350.0), dy(958.0), WARM[1], 0.6 * home * sc.p.bloom);
        }
        // vents on that roof steam when it is cold
        if sc.smoke > 0.3 {
            let s = Smoke { x: dx(492.0), y: dy(950.0), scale: 1.05, col: mix(col, sc.sky[5], 0.72), behind: 9 };
            if !sc.live {
                props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, 2.0);
            }
            out.smoke.push(s);
        }
        // the terrace: planters, and string lights on the nights people stay out
        let leaf = mix(col, hex(sc.p.seasons[sc.season][1]), 0.25 + 0.45 * sc.day);
        for (x, big) in [(586.0, 1.0), (604.0, 1.5), (862.0, 1.2), (884.0, 0.9)] {
            d.ellipse(dx(x), dy(966.0) - (3.0 + 2.6 * big) * u, 6.0 * u, (3.0 + 2.6 * big) * u, leaf, 1.0);
        }
        d.rect(dx(696.0), dy(959.0), dx(730.0), dy(961.0), col, 1.0);
        d.rect(dx(711.0), dy(961.0), dx(715.0), dy(966.0), col, 1.0);
        if sc.festoon {
            for b in props::festoon(d, dx(620.0), dx(848.0), dy(966.0), u, col) {
                bloom.light(b.0 - 3.0 * u, b.1 - 3.0 * u, b.0 + 3.0 * u, b.1 + 3.0 * u, b.2, 0.5);
                out.twinkle.push((b.0, b.1, b.2, 9));
            }
        }
        // the pigeon loft
        d.rect(dx(1022.0), dy(942.0), dx(1030.0), dy(950.0), shade(col, 0.10), 0.8);
        d.rect(dx(1038.0), dy(942.0), dx(1046.0), dy(950.0), shade(col, 0.10), 0.8);
        d.line((dx(1170.0), dy(946.0)), (dx(1170.0), dy(912.0)), 1.1 * u, col, 1.0);
        for (at, half) in [(918.0, 5.0), (924.0, 8.0), (930.0, 11.0)] {
            d.line((dx(1170.0 - half), dy(at)), (dx(1170.0 + half), dy(at)), 1.0 * u, col, 1.0);
        }
        // the sign, seen from behind: the lattice that holds it, and a row of bulbs along the top
        for (x0, x1) in [(1452.0, 1506.0), (1506.0, 1560.0)] {
            d.line((dx(x0), dy(849.0)), (dx(x1), dy(884.0)), 1.0 * u, col, 1.0);
            d.line((dx(x1), dy(849.0)), (dx(x0), dy(884.0)), 1.0 * u, col, 1.0);
            d.line((dx(x0), dy(887.0)), (dx(x1), dy(918.0)), 1.0 * u, col, 1.0);
            d.line((dx(x1), dy(887.0)), (dx(x0), dy(918.0)), 1.0 * u, col, 1.0);
        }
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        for k in 0..12 {
            let (x, y) = (dx(1450.0 + 10.2 * k as f32), dy(844.0));
            let tint = [AMBER, WARM[0]][k % 2];
            d.circle(x, y, 1.7 * u, mix(mix(col, tint, 0.3), mix(tint, WHITE, 0.3), dusk), 1.0);
            if dusk > 0.05 {
                bloom.light(x - 2.6 * u, y - 2.6 * u, x + 2.6 * u, y + 2.6 * u, tint, 0.5 * dusk);
                out.twinkle.push((x, y, tint, 9));
            }
        }
        if !sc.live {
            if sc.laundry > 0 {
                props::laundry(d, sc, dx(376.0), dx(470.0), dy(964.0), u, col, 0, (sc.ts / 240.0) as f32);
            }
            if sc.cat_out && !sc.cat_asleep {
                let (x, y, face) = cat_place(sc);
                props::cat(d, dx(x), dy(y), u * 1.25, col, false, face, (sc.ts / 37.0) as f32);
            }
        }
        if sc.cat_out && sc.cat_asleep {
            props::cat(d, dx(1086.0), dy(958.0), u * 1.25, col, true, 1.0, 0.0);
        }
        // the junction box on the long low roof
        let s = state.get(episode::BOX);
        if s.visible && state.in_base(episode::BOX) {
            episode::junction_box(d, lay, (self.bind.box_at.0 + s.pose.dx, self.bind.box_at.1 + s.pose.dy), s.damage, &self.story_tone(sc, None));
        }
        // a window someone has opened
        for (id, r) in [(episode::WINDOW_A, self.bind.windows[0]), (episode::WINDOW_B, self.bind.windows[1])] {
            if state.get(id).open {
                let lit = mix(WARM[0], WHITE, 0.2);
                d.rect(dx(r[0]) - 0.6 * u, dy(r[1]) - 0.6 * u, dx(r[2]) + 0.6 * u, dy(r[3]) + 0.6 * u, lit, 0.9);
                bloom.light(dx(r[0]), dy(r[1]), dx(r[2]), dy(r[3]), WARM[1], 0.6 * (0.3 + 0.7 * sc.dark));
            }
        }
        out.leaves = true;
        out.glades.push([dx(570.0), dy(930.0), dx(900.0), dy(966.0)]);
    }

    /// A cone, a barrier or a steel plate, in the light of the deck.
    fn paint_thing<S: crate::gfx::Surface>(&self, d: &mut Draw<S>, sc: &Scene, kind: u8, at: (f32, f32), turn: f32) {
        let tone = self.story_tone(sc, Some(3));
        match kind {
            0 => crate::story::draw::cone(d, &self.lay, at, 0.55, &tone, 1.0),
            1 => crate::story::draw::barrier(d, &self.lay, at, 28.0, 0.55, &tone, 1.0),
            _ => crate::story::draw::crate_box(d, &self.lay, at, 0.35, turn, &tone, 1.0),
        }
    }

    /// Does the living scene want the crane held, as far as the last frame told?
    fn hold_wanted(&self) -> bool {
        self.caught.get()
    }
}

impl View for Downtown {
    fn id(&self) -> &'static str {
        "city"
    }

    fn about(&self) -> &'static str {
        "the middle of town, across the railway"
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(City::new(w, h, self.horizon()))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let week = [
            "the towers fill early; a train every three minutes at the rush, the crane at work",
            "a working day: offices lit until evening, a train every three minutes at the rush",
            "a working day: offices lit until evening, a train every three minutes at the rush",
            "a working day: offices lit until evening, a train every three minutes at the rush",
            "offices empty early; the terrace lights go on after dark",
            "the office towers stay dark; flats and the terrace are lit tonight, trains are few",
            "a quiet city: dark towers, few trains, an early night",
        ][sc.dow];
        let mut doing = vec![format!("{}% of windows lit", (sc.lit_share * 100.0).round())];
        doing.push(format!("the weather beacon {}", beacon(sc).2));
        doing.push(match headway(sc) {
            Some(gap) => format!("a train every {} minutes", (gap / 60.0).round()),
            None => "no trains until five".to_string(),
        });
        for (flag, text) in [
            (sc.crane_works, "the crane working"),
            (sc.festoon, "lights on the terrace"),
            (sc.laundry > 0, "washing on a roof"),
            (sc.balloon, "a balloon drifting over"),
            (!sc.planes.is_empty(), "a plane crossing"),
            (!sc.birds.is_empty(), "birds crossing"),
            (sc.smoke > 0.12, "the power station smoking"),
            (sc.traffic * sc.dark > 0.3, "rush-hour glow under the arches"),
            (sc.mist > 0.25, "mist in the streets"),
            (sc.frost > 0.3, "frost on the roofs"),
        ] {
            if flag {
                doing.push(text.to_string());
            }
        }
        if sc.cat_out {
            doing.push(if sc.cat_asleep { "the cat asleep by the loft" } else { "the cat out on a roof" }.to_string());
        }
        let house = Yours { lamp: "the lantern on your tower", containers: ("blue floor", "blue floors"), smoke: "steam off its roof" };
        vec![("week", week.to_string()), ("city", doing.join(", ")), ("yours", yours(sc, &house))]
    }
}
