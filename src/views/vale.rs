//! Vale: a valley in another world, under the sky of this one.
//!
//! Rocks hang in the air here and a second, smaller moon keeps the calendar,
//! but the weather, the light and the seasons are the ones outside your
//! window.
//!
//!   the tower on the crag    yours: the lamp in its top room, the flue that smokes violet
//!   the standing stones      a rune lit blue for each running container
//!   the little moon          crosses the sky once a week: left on Monday, right on Sunday
//!   the falls                run high in rain, thin in a dry heat, and freeze in a hard frost
//!   the sky-ship             crosses on the hour and takes nine minutes over it
//!   the great tree           keeps the season; its lanterns are all lit on Friday and Saturday nights

use std::f32::consts::{PI, TAU};

use crate::color::{hex, mix, shade, smooth, Rgb, AMBER, BLUE, CREAM, ROSE, SAGE, WARM, WHITE};
use crate::gfx::{Blend, Bloom, Canvas, Depth, Draw, Frame, Shaper, Surface, P};
use crate::paint::{band, lit_disc, Light};
use crate::props::{self, Layout};
use crate::rng::hash2;
use crate::scene::Scene;
use crate::sky::Star;
use crate::tm::DAYS;
use crate::views::kit::{curve, grass, height_at, to_px};
use crate::views::{lamp_colours, yours, Anchors, Breath, Knob, Lights, Smoke, Stage, View, Yours};

pub struct Vale;

const HORIZON: f64 = 0.660;
const HZ: f32 = 660.0;

/// The far range, peak by peak. The Horn stands over the right of it.
const TEETH: &[P] = &[
    (-40.0, 600.0),
    (36.0, 562.0),
    (104.0, 592.0),
    (178.0, 530.0),
    (246.0, 578.0),
    (318.0, 548.0),
    (396.0, 598.0),
    (470.0, 572.0),
    (540.0, 612.0),
    (616.0, 590.0),
    (700.0, 624.0),
    (786.0, 604.0),
    (868.0, 630.0),
    (950.0, 606.0),
    (1038.0, 562.0),
    (1108.0, 592.0),
    (1182.0, 522.0),
    (1252.0, 568.0),
    (1318.0, 540.0),
    (1372.0, 452.0),
    (1404.0, 380.0),
    (1426.0, 356.0),
    (1440.0, 388.0),
    (1456.0, 474.0),
    (1494.0, 542.0),
    (1544.0, 502.0),
    (1602.0, 548.0),
    (1650.0, 522.0),
];
/// Which of those are high enough for snow: their place in the list.
const SNOWY: &[usize] = &[1, 3, 5, 14, 16, 21, 25];
/// The peaks: where in the list the tip is, and the valley on each side of it.
const PEAKS: &[(usize, usize, usize)] = &[(1, 0, 2), (3, 2, 4), (5, 4, 6), (7, 6, 8), (9, 8, 10), (11, 10, 12), (14, 12, 15), (16, 15, 17), (21, 17, 24), (25, 24, 27)];

/// The wooded hills that close the valley in, and the floor of it in front of them.
const HILLS: &[P] = &[
    (-40.0, 612.0),
    (90.0, 606.0),
    (220.0, 622.0),
    (340.0, 650.0),
    (450.0, 676.0),
    (560.0, 694.0),
    (690.0, 706.0),
    (820.0, 712.0),
    (950.0, 708.0),
    (1080.0, 698.0),
    (1200.0, 680.0),
    (1320.0, 660.0),
    (1440.0, 646.0),
    (1560.0, 636.0),
    (1640.0, 630.0),
];

const LAKE: &[P] = &[
    (560.0, 728.0),
    (640.0, 718.0),
    (760.0, 714.0),
    (900.0, 714.0),
    (1040.0, 717.0),
    (1150.0, 723.0),
    (1232.0, 735.0),
    (1250.0, 750.0),
    (1190.0, 764.0),
    (1060.0, 772.0),
    (900.0, 776.0),
    (740.0, 772.0),
    (620.0, 762.0),
    (556.0, 746.0),
    (560.0, 728.0),
];

/// The crag the tower stands on.
const CRAG: &[P] = &[
    (392.0, 816.0),
    (412.0, 770.0),
    (404.0, 730.0),
    (432.0, 690.0),
    (446.0, 650.0),
    (478.0, 628.0),
    (500.0, 601.0),
    (596.0, 601.0),
    (606.0, 640.0),
    (634.0, 668.0),
    (630.0, 710.0),
    (664.0, 748.0),
    (674.0, 790.0),
    (712.0, 816.0),
];
/// The tower is drawn this much bigger than its numbers say, about its foot.
const TALL: f32 = 1.25;
const TOWER_X: f32 = 548.0;
const TOWER_FOOT: f32 = 601.0;
const LAMP: P = (548.0, 462.0);
const FLUE: P = (509.0, 448.0);
const ROOF_TIP: P = (552.0, 382.0);

/// A rock that hangs in the air: along its top from left to right, then back along its underside.
struct Isle {
    top: &'static [P],
    under: &'static [P],
    /// trees on it: x, and how tall
    trees: &'static [(f32, f32)],
    /// how far off it is, 0 near to 1 lost in the air
    far: f32,
}

const ISLES: [Isle; 3] = [
    Isle {
        top: &[(1022.0, 393.0), (1050.0, 379.0), (1090.0, 371.0), (1140.0, 367.0), (1190.0, 371.0), (1222.0, 381.0), (1240.0, 393.0)],
        under: &[
            (1240.0, 393.0),
            (1226.0, 417.0),
            (1204.0, 431.0),
            (1196.0, 462.0),
            (1176.0, 470.0),
            (1160.0, 512.0),
            (1146.0, 498.0),
            (1128.0, 534.0),
            (1112.0, 488.0),
            (1090.0, 474.0),
            (1078.0, 441.0),
            (1050.0, 433.0),
            (1036.0, 411.0),
            (1022.0, 393.0),
        ],
        trees: &[(1092.0, 26.0), (1166.0, 32.0), (1202.0, 22.0)],
        far: 0.0,
    },
    Isle {
        top: &[(858.0, 513.0), (880.0, 505.0), (910.0, 503.0), (934.0, 509.0), (948.0, 517.0)],
        under: &[(948.0, 517.0), (936.0, 535.0), (918.0, 543.0), (910.0, 568.0), (898.0, 549.0), (880.0, 541.0), (866.0, 529.0), (858.0, 513.0)],
        trees: &[(904.0, 20.0)],
        far: 0.25,
    },
    Isle {
        top: &[(1462.0, 259.0), (1484.0, 253.0), (1508.0, 255.0), (1524.0, 263.0)],
        under: &[(1524.0, 263.0), (1512.0, 277.0), (1500.0, 296.0), (1490.0, 281.0), (1474.0, 275.0), (1462.0, 259.0)],
        trees: &[(1496.0, 11.0)],
        far: 0.6,
    },
];
/// Where the water leaves the big rock, and where it reaches the lake.
const FALL: (f32, f32, f32) = (1040.0, 388.0, 724.0);

/// The knoll the great tree stands on, and the bank the stones stand on.
const KNOLL: &[P] = &[(990.0, 1012.0), (1080.0, 962.0), (1170.0, 922.0), (1260.0, 892.0), (1350.0, 874.0), (1450.0, 864.0), (1550.0, 860.0), (1640.0, 862.0)];
const BANK: &[P] = &[(-40.0, 842.0), (100.0, 834.0), (240.0, 840.0), (380.0, 858.0), (500.0, 890.0), (600.0, 934.0), (680.0, 980.0), (730.0, 1014.0)];

/// The limbs of the great tree, each from the trunk outwards, and how thick it starts.
const LIMBS: &[(&[P], f32)] = &[
    (&[(1430.0, 730.0), (1376.0, 676.0), (1326.0, 640.0), (1270.0, 604.0)], 15.0),
    (&[(1432.0, 706.0), (1412.0, 640.0), (1396.0, 590.0), (1378.0, 536.0)], 14.0),
    (&[(1438.0, 704.0), (1464.0, 648.0), (1486.0, 606.0), (1520.0, 556.0)], 14.0),
    (&[(1440.0, 736.0), (1492.0, 706.0), (1546.0, 680.0), (1610.0, 652.0)], 13.0),
    (&[(1326.0, 640.0), (1310.0, 606.0), (1300.0, 574.0)], 7.0),
    (&[(1486.0, 606.0), (1478.0, 570.0), (1470.0, 534.0)], 7.0),
    (&[(1396.0, 590.0), (1428.0, 548.0), (1440.0, 508.0)], 7.0),
    (&[(1546.0, 680.0), (1570.0, 640.0), (1580.0, 604.0)], 6.0),
];
/// The crown, clump by clump: x, y, half-width, half-height.
const CROWN: &[(f32, f32, f32, f32)] = &[
    (1300.0, 652.0, 58.0, 36.0),
    (1612.0, 644.0, 62.0, 44.0),
    (1268.0, 584.0, 70.0, 48.0),
    (1574.0, 572.0, 72.0, 50.0),
    (1340.0, 524.0, 76.0, 52.0),
    (1504.0, 512.0, 78.0, 52.0),
    (1420.0, 486.0, 82.0, 54.0),
    (1384.0, 600.0, 72.0, 44.0),
    (1530.0, 622.0, 70.0, 42.0),
    (1452.0, 572.0, 92.0, 56.0),
];
/// Lanterns hung from it.
const LANTERNS: [P; 9] = [(1262.0, 656.0), (1300.0, 706.0), (1342.0, 672.0), (1384.0, 668.0), (1470.0, 660.0), (1510.0, 688.0), (1552.0, 716.0), (1596.0, 706.0), (1632.0, 720.0)];

/// The standing stones: x, height, width, and how far each leans.
const STONES: [(f32, f32, f32, f32); 8] = [
    (56.0, 68.0, 28.0, -2.0),
    (110.0, 55.0, 25.0, 1.5),
    (164.0, 78.0, 30.0, 0.5),
    (220.0, 60.0, 26.0, -3.0),
    (274.0, 72.0, 29.0, 2.5),
    (328.0, 52.0, 24.0, -1.0),
    (380.0, 64.0, 27.0, 3.0),
    (430.0, 48.0, 23.0, -2.5),
];
/// What is cut into each: strokes in a box one wide and one tall, up from the bottom.
const RUNES: [&[&[P]]; 8] = [
    &[&[(0.0, 0.0), (0.0, 1.0)], &[(0.0, 0.5), (0.45, 0.8)], &[(0.0, 0.22), (0.45, 0.52)]],
    &[&[(0.0, 0.05), (0.42, 0.5), (0.0, 0.95), (-0.42, 0.5), (0.0, 0.05)]],
    &[&[(-0.36, 0.95), (0.36, 0.95), (-0.36, 0.05), (0.36, 0.05)]],
    &[&[(0.0, 0.0), (0.0, 1.0)], &[(-0.42, 0.58), (0.0, 1.0), (0.42, 0.58)]],
    &[&[(-0.4, 0.05), (0.4, 0.95)], &[(0.4, 0.05), (-0.4, 0.95)]],
    &[&[(-0.38, 0.0), (-0.38, 1.0)], &[(-0.38, 1.0), (0.38, 0.72), (-0.38, 0.46), (0.38, 0.0)]],
    &[&[(-0.42, 0.0), (0.0, 1.0), (0.42, 0.0)], &[(-0.22, 0.42), (0.22, 0.42)]],
    &[&[(-0.4, 0.15), (0.4, 0.15)], &[(-0.4, 0.5), (0.4, 0.5)], &[(-0.4, 0.85), (0.4, 0.85)]],
];

/// Small trees about the valley: x, y of the foot, and how tall.
const COPSE: &[(f32, f32, f32)] = &[
    (96.0, 668.0, 30.0),
    (132.0, 676.0, 22.0),
    (214.0, 700.0, 34.0),
    (252.0, 706.0, 24.0),
    (318.0, 742.0, 28.0),
    (1268.0, 760.0, 30.0),
    (1300.0, 768.0, 22.0),
    (1342.0, 752.0, 26.0),
    (1500.0, 720.0, 32.0),
    (1540.0, 728.0, 24.0),
    (1130.0, 800.0, 20.0),
    (792.0, 826.0, 18.0),
    (1010.0, 836.0, 22.0),
];

/// The hamlet across the lake: x, width, height of the wall, and which colour of roof.
const HOUSES: [(f32, f32, f32, usize); 7] = [(724.0, 15.0, 8.0, 0), (744.0, 12.0, 7.0, 1), (760.0, 17.0, 9.0, 2), (782.0, 13.0, 7.0, 0), (800.0, 15.0, 8.0, 1), (822.0, 12.0, 7.0, 2), (840.0, 14.0, 7.5, 0)];
const SHORE: f32 = 714.0;

/// Seconds the sky-ship takes to cross.
const CROSSING: f64 = 540.0;

const FAR: usize = 0;
const CRAGS: usize = 2;
const NEAR: usize = 4;

struct Dale {
    lay: Layout,
    /// the layout the tower is drawn in
    tw: Layout,
    masks: Vec<(usize, Vec<u8>)>,
    depth: Depth,
    hills: Vec<P>,
    knoll: Vec<P>,
    bank: Vec<P>,
}

/// The outline of the tower, for the masks.
fn tower_shape(sh: &mut Shaper, lay: &Layout) {
    let x = TOWER_X;
    sh.poly(&to_px(lay, &[(x - 23.0, TOWER_FOOT + 2.0), (x - 19.0, 486.0), (x + 19.0, 486.0), (x + 23.0, TOWER_FOOT + 2.0)]));
    sh.poly(&to_px(lay, &[(x - 31.0, 486.0), (x - 31.0, 444.0), (x + 31.0, 444.0), (x + 31.0, 486.0)]));
    sh.poly(&to_px(lay, &[(x - 38.0, 447.0), (ROOF_TIP.0, ROOF_TIP.1), (x + 38.0, 447.0)]));
    sh.poly(&to_px(lay, &[(x + 21.0, 562.0), (x + 21.0, 520.0), (x + 32.0, 500.0), (x + 43.0, 520.0), (x + 43.0, 562.0)]));
}

/// The outline of one hanging rock, and of the trees on it.
fn isle_shape(sh: &mut Shaper, lay: &Layout, isle: &Isle) {
    let mut p = to_px(lay, &curve(isle.top, 6));
    p.extend(to_px(lay, isle.under));
    sh.poly(&p);
    for &(x, tall) in isle.trees {
        let y = height_at(isle.top, x);
        sh.ellipse(lay.dx(x), lay.dy(y - tall * 0.62), lay.d(tall * 0.44), lay.d(tall * 0.46));
    }
}

impl Dale {
    fn new(w: usize, h: usize) -> Dale {
        let lay = Layout::new(w, h, HORIZON);
        let tw = lay.scaled(TALL, (TOWER_X, TOWER_FOOT), (TOWER_X, TOWER_FOOT));
        let mut sh = Shaper::new();
        let (hills, knoll, bank) = (curve(HILLS, 8), curve(KNOLL, 8), curve(BANK, 8));
        let mut masks = Vec::new();
        let mut mask = |sh: &mut Shaper, top: f32, f: &mut dyn FnMut(&mut Shaper)| {
            if sh.begin(0.0, lay.dy(top).max(0.0), lay.w, lay.h, w, h) {
                f(sh);
                masks.push(sh.mask(w));
            } else {
                masks.push((h, Vec::new()));
            }
        };
        let down = |line: &[P]| {
            let mut p = to_px(&lay, line);
            p.extend([(lay.w + 2.0, lay.h + 2.0), (-2.0, lay.h + 2.0)]);
            p
        };
        mask(&mut sh, 350.0, &mut |sh| sh.poly(&down(TEETH)));
        mask(&mut sh, 600.0, &mut |sh| sh.poly(&down(&hills)));
        mask(&mut sh, 300.0, &mut |sh| {
            sh.poly(&to_px(&lay, CRAG));
            tower_shape(sh, &tw);
        });
        mask(&mut sh, 236.0, &mut |sh| {
            for isle in &ISLES {
                isle_shape(sh, &lay, isle);
            }
        });
        mask(&mut sh, 424.0, &mut |sh| {
            let mut p = to_px(&lay, &knoll);
            p.extend([(lay.w + 2.0, lay.h + 2.0), (lay.dx(990.0), lay.h + 2.0)]);
            sh.poly(&p);
            let mut p = to_px(&lay, &bank);
            p.extend([(-2.0, lay.h + 2.0)]);
            sh.poly(&p);
            for &(x, y, rx, ry) in CROWN {
                sh.ellipse(lay.dx(x), lay.dy(y), lay.d(rx), lay.d(ry));
            }
            sh.poly(&to_px(&lay, &[(1396.0, 872.0), (1420.0, 700.0), (1446.0, 700.0), (1470.0, 872.0)]));
        });
        let depth = Depth::stack(&masks, w, h);
        Dale { lay, tw, masks, depth, hills, knoll, bank }
    }
}

/// Is the valley awake? It keeps village hours.
fn awake(sc: &Scene) -> f32 {
    smooth(1.0, -4.0, sc.el_eff) * if sc.t >= 16.0 || sc.t < 0.3 { 1.0 } else if sc.t < 1.5 { 0.4 } else if (5.6..8.5).contains(&sc.t) { 0.4 } else { 0.1 }
}

/// How hard the falls are running, 0 to 1. Nothing at all means they have frozen.
fn flow(sc: &Scene) -> f32 {
    if let Some(v) = sc.knob("falls") {
        return v.clamp(0.0, 1.0);
    }
    if sc.temp < -1.5 {
        return 0.0;
    }
    (0.45 + 0.55 * sc.rain.max(if sc.storm { 1.0 } else { 0.0 }) - 0.25 * smooth(26.0, 36.0, sc.temp)).clamp(0.12, 1.0)
}

/// How far through the week it is, 0 to 1 from the first minute of Monday.
fn week(sc: &Scene) -> f32 {
    (sc.knob("week").map_or(sc.dow as f32 + sc.t as f32 / 24.0, |v| v) / 7.0).clamp(0.0, 1.0)
}

/// Where the little moon is: fractions of the width, and of the height the sky was first drawn at.
fn wanderer(sc: &Scene) -> (f32, f32) {
    let p = week(sc);
    (0.05 + 0.90 * p, 0.71 - 0.50 * (PI * p).sin().powf(0.85))
}

fn snow_col(sc: &Scene) -> Rgb {
    mix(hex(sc.p.snow), sc.sky[4], 0.16 + 0.44 * (1.0 - sc.day))
}

/// The colours the great tree wears: the shade under it and the light on it. None in winter.
fn foliage(sc: &Scene) -> Option<(Rgb, Rgb)> {
    let k = 0.20 + 0.62 * sc.day;
    let (deep, bright): (Rgb, Rgb) = match sc.season {
        0 => return None,
        1 => ([190.0, 116.0, 150.0], [247.0, 204.0, 218.0]),
        2 => ([36.0, 104.0, 92.0], [104.0, 172.0, 128.0]),
        _ => ([164.0, 82.0, 54.0], [232.0, 160.0, 76.0]),
    };
    Some((mix(sc.layers[4], deep, k), mix(sc.layers[3], bright, k)))
}

/// The water coming off the big rock. The still picture has the whole fall; given a time, only what moves in it.
fn falls<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, t: Option<f32>) {
    let (x, y0, y1) = FALL;
    let run = flow(sc);
    let u = lay.u;
    let white = mix(snow_col(sc), WHITE, 0.3);
    let sway = sc.wind_x.clamp(-1.2, 1.2) * 12.0;
    let mid = |f: f32| x + sway * f * f;
    let wide = |f: f32| (3.0 + 6.0 * f) * (0.45 + 0.75 * run.max(0.25));
    if run <= 0.0 {
        // frozen: a column of ice, with the light in it
        if t.is_none() {
            let ice = mix(white, [170.0, 214.0, 240.0], 0.45);
            for k in 0..6 {
                let (f0, f1) = (k as f32 / 6.0, (k + 1) as f32 / 6.0);
                let (ya, yb) = (y0 + (y1 - y0) * f0, y0 + (y1 - y0) * f1);
                d.poly(&[(lay.dx(x - 3.0 - 4.0 * f0), lay.dy(ya)), (lay.dx(x + 3.0 + 4.0 * f0), lay.dy(ya)), (lay.dx(x + 2.0 + 4.0 * f1), lay.dy(yb)), (lay.dx(x - 2.0 - 4.0 * f1), lay.dy(yb))], ice, 0.80 - 0.08 * k as f32);
            }
            d.line((lay.dx(x - 1.0), lay.dy(y0 + 10.0)), (lay.dx(x - 2.0), lay.dy(y1 - 30.0)), 1.2 * u, WHITE, 0.5);
        }
        return;
    }
    match t {
        None => {
            // it thins as it falls, and goes to spray before it lands
            const N: usize = 9;
            for k in 0..N {
                let (f0, f1) = (k as f32 / N as f32, (k + 1) as f32 / N as f32);
                let (ya, yb) = (y0 + (y1 - y0) * f0, y0 + (y1 - y0) * f1);
                d.poly(
                    &[(lay.dx(mid(f0) - wide(f0)), lay.dy(ya)), (lay.dx(mid(f0) + wide(f0)), lay.dy(ya)), (lay.dx(mid(f1) + wide(f1)), lay.dy(yb)), (lay.dx(mid(f1) - wide(f1)), lay.dy(yb))],
                    white,
                    (0.74 - 0.46 * f0) * (0.55 + 0.45 * run),
                );
            }
            for (dx, dy, r, a) in [(0.0, -6.0, 26.0, 0.34), (-16.0, 0.0, 18.0, 0.26), (18.0, 2.0, 20.0, 0.26), (2.0, -26.0, 16.0, 0.16)] {
                d.puff(lay.dx(mid(1.0) + dx), lay.dy(y1 + dy), lay.d(r * (0.7 + 0.5 * run)), lay.d(r * 0.5), white, a * (0.5 + 0.5 * run), 0.1);
            }
        }
        Some(t) => {
            let n = (10.0 + 22.0 * run) as u32;
            for i in 0..n {
                let f = (t * (0.30 + 0.16 * hash2(i, 311)) + hash2(i, 313)).fract();
                let fall = f * f * 0.75 + f * 0.25;
                let px = lay.dx(mid(fall) + (hash2(i, 317) - 0.5) * 2.0 * wide(fall));
                let py = lay.dy(y0 + (y1 - y0) * fall);
                let len = lay.d(8.0 + 26.0 * f);
                d.rect(px - 0.7 * u, py, px + 0.7 * u, py + len, WHITE, 0.55 * (1.0 - 0.6 * fall) * (0.5 + 0.5 * run));
            }
        }
    }
}

/// The sky-ship, if it is crossing. `t` makes it ride the air.
fn ship<S: Surface>(d: &mut Draw<S>, sc: &Scene, lay: &Layout, now: f64, t: f32) {
    let since = now.rem_euclid(3600.0);
    if since >= CROSSING || sc.fog > 0.7 || sc.storm {
        return;
    }
    let f = (since / CROSSING) as f32;
    let way = if (now / 3600.0).floor() as i64 % 2 == 0 { 1.0 } else { -1.0 };
    let x = lay.dx(if way > 0.0 { -90.0 + 1780.0 * f } else { 1690.0 - 1780.0 * f });
    let y = lay.dy(466.0 + 16.0 * (f * TAU * 1.4).sin()) + (t * 0.7).sin() * 1.6 * lay.u;
    let (s, u) = (lay.d(1.0), lay.u);
    let at = |a: f32, b: f32| (x + a * way * s, y + b * s);
    let night = smooth(1.0, -5.0, sc.el_eff);
    let bag = mix(sc.layers[1], CREAM, 0.16 + 0.60 * sc.day);
    let wood = mix(sc.layers[3], AMBER, 0.10 + 0.26 * sc.day);
    // the ropes, then the hull under the bag
    for a in [-15.0f32, -5.0, 6.0, 16.0] {
        d.line(at(a * 0.8, -3.0), at(a * 1.5, -17.0), 0.8 * u, wood, 0.9);
    }
    d.poly(&[at(-21.0, -4.0), at(22.0, -4.0), at(30.0, -6.5), at(19.0, 5.0), at(-15.0, 5.0)], wood, 1.0);
    d.poly(&[at(-21.0, -4.0), at(-30.0, -13.0), at(-17.0, -9.0)], mix(wood, ROSE, 0.5), 1.0);
    d.rect(x - 9.0 * s, y - 9.0 * s, x + 7.0 * s, y - 4.0 * s, shade(wood, 0.10), 1.0);
    let (bx, by) = at(1.0, -27.0);
    d.ellipse(bx, by, 37.0 * s, 12.0 * s, bag, 1.0);
    d.ellipse(bx, by + 3.0 * s, 35.0 * s, 8.6 * s, shade(bag, -0.10), 0.55);
    d.ellipse(bx, by - 2.6 * s, 37.0 * s, 3.0 * s, mix(bag, ROSE, 0.62), 0.9);
    d.poly(&[at(-33.0, -27.0), at(-46.0, -35.0), at(-44.0, -20.0)], mix(bag, ROSE, 0.62), 1.0);
    if night > 0.1 {
        for k in 0..3 {
            let (wx, wy) = at(-6.0 + 5.0 * k as f32, -6.4);
            d.dot(wx, wy, 0.9 * u, 0.9 * u, WARM[0], night, Blend::Over);
        }
        let (lx, ly) = at(30.0, -9.0);
        d.glow(lx, ly, 13.0 * u, 13.0 * u, WARM[1], 0.7 * night, 2, Blend::Screen);
        d.dot(lx, ly, 1.7 * u, 1.7 * u, WARM[0], 1.0, Blend::Over);
    }
}

fn cat_place(sc: &Scene) -> (f32, f32, f32) {
    // on a stone, on the bank, at the foot of the tree
    const SPOTS: [(f32, f32); 4] = [(164.0, 764.0), (476.0, 884.0), (1214.0, 904.0), (274.0, 775.0)];
    let k = (hash2((sc.day_id % 100000) as u32, (sc.t / 1.5) as u32 + 9) * SPOTS.len() as f32) as usize % SPOTS.len();
    (SPOTS[k].0, SPOTS[k].1, if hash2(k as u32 + 5, (sc.t / 1.5) as u32) < 0.5 { -1.0 } else { 1.0 })
}

const BANK_GRASS: (f32, f32, u32) = (-10.0, 700.0, 70);
const KNOLL_GRASS: (f32, f32, u32) = (1010.0, 1610.0, 64);

impl Stage for Dale {
    fn lay(&self) -> &Layout {
        &self.lay
    }

    fn depth(&self) -> &Depth {
        &self.depth
    }

    fn lights(&self, sc: &Scene) -> Lights {
        Lights { share: 0.20 * awake(sc), on: Vec::new() }
    }

    fn heavens(&self, d: &mut Draw<Canvas>, sc: &Scene, _stars: &[Star], _out: &mut Anchors) {
        // the little moon, lit by the same sun as the big one
        let lay = &self.lay;
        let (fx, fy) = wanderer(sc);
        let (x, y) = (lay.x(fx), lay.sy(fy));
        let vis = (0.30 + 0.70 * (1.0 - sc.day)) * (1.0 - 0.75 * smooth(0.5, 0.95, sc.cover)) * (1.0 - 0.8 * sc.fog);
        if vis < 0.05 {
            return;
        }
        let col = mix(hex(sc.p.moon), [150.0, 232.0, 206.0], 0.70);
        let apart = ((fx - sc.eph.sun.x as f32).abs() / 0.9).clamp(0.0, 1.0);
        let lit = ((1.0 - (PI * apart).cos()) / 2.0).max(0.14);
        let r = 11.0 * lay.u;
        d.glow(x, y, r * 4.4, r * 4.4, col, 0.20 * lit * vis * (1.0 - sc.day), 2, Blend::Screen);
        // it wears a ring, tipped over: the far half goes behind it
        let ring = |from: f32| -> Vec<P> {
            (0..=14).map(|i| from + PI * i as f32 / 14.0).map(|a| (a.cos() * r * 2.15, a.sin() * r * 0.52)).map(|(a, b)| (x + a * 0.94 - b * -0.34, y + a * -0.34 + b * 0.94)).collect()
        };
        d.path(&ring(PI), 1.3 * lay.u, col, 0.55 * vis);
        lit_disc(d, (x, y), r, col, lit, (lay.x(sc.eph.sun.x as f32), lay.y(sc.eph.sun.y as f32)), vis);
        d.path(&ring(0.0), 1.3 * lay.u, col, 0.80 * vis);
    }

    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let w = lay.w as usize;
        let light = Light::of(sc, lay);
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let leaf = hex(sc.p.seasons[sc.season][1]);
        let cold = smooth(16.0, 0.0, sc.temp).max(sc.snow).max(if sc.season == 0 { 0.6 } else { 0.0 });
        let snow = snow_col(sc);

        // ── the far range ──
        let far = mix(mix(sc.layers[0], [156.0, 138.0, 206.0], 0.30 + 0.26 * sc.day), sc.sky[5], 0.20);
        let (r0, r1) = (lay.dy(380.0), lay.dy(HZ));
        d.s.stamp(&self.masks[FAR].1, self.masks[FAR].0 as i32, 0, |y| (mix(far, mix(far, sc.sky[5], 0.26), smooth(r0, r1, y as f32)), 1.0));
        bloom.hide(&self.masks[FAR].1, w, self.masks[FAR].0);
        // each peak has a face turned to the light and one turned from it, meeting along a crooked ridge
        let from_left = light.x < lay.w / 2.0;
        for &(tip, l, r) in PEAKS {
            let t = TEETH[tip];
            let foot = HZ + 6.0;
            let face = |right: bool| -> Vec<P> {
                let mut p: Vec<P> = if right { TEETH[tip..=r].to_vec() } else { TEETH[l..=tip].iter().rev().copied().collect() };
                let end = p[p.len() - 1];
                let dir = if right { 1.0 } else { -1.0 };
                p.extend([(end.0, foot), (t.0 + dir * (end.0 - t.0).abs() * 0.12, foot), (t.0 - dir * 4.0, t.1 + (foot - t.1) * 0.55)]);
                to_px(lay, &p)
            };
            d.poly(&face(from_left), shade(far, -0.24), 0.42);
            if light.k > 0.03 {
                d.poly(&face(!from_left), mix(far, light.col, 0.6), (0.7 * light.k).min(0.4));
            }
        }
        // snow on the highest of them, lower down the colder it is
        for &i in SNOWY {
            let (l, tip, r) = (TEETH[i - 1], TEETH[i], TEETH[i + 1]);
            let deep = (16.0 + 50.0 * cold).min((l.1 - tip.1).min(r.1 - tip.1) * 0.86);
            let (fl, fr) = (deep / (l.1 - tip.1), deep / (r.1 - tip.1));
            let (pl, pr) = ((tip.0 + (l.0 - tip.0) * fl, tip.1 + deep), (tip.0 + (r.0 - tip.0) * fr, tip.1 + deep));
            let span = pr.0 - pl.0;
            let cap = [tip, pr, (pl.0 + span * 0.78, pr.1 - deep * 0.30), (pl.0 + span * 0.60, pr.1 + deep * 0.10), (pl.0 + span * 0.42, pr.1 - deep * 0.34), (pl.0 + span * 0.24, pr.1 + deep * 0.04), pl];
            d.poly(&to_px(lay, &cap), snow, 0.92);
            let shady = if from_left { [tip, pr, (pl.0 + span * 0.78, pr.1 - deep * 0.30), (tip.0 + 1.0, tip.1 + deep * 0.5)] } else { [tip, (tip.0 - 1.0, tip.1 + deep * 0.5), (pl.0 + span * 0.24, pr.1 + deep * 0.04), pl] };
            d.poly(&to_px(lay, &shady), mix(far, sc.sky[2], 0.4), 0.36);
        }
        band(d.s, lay.dy(624.0), lay.dy(HZ + 30.0), 22.0 * u, sc.haze, sc.haze_alpha[0] / 255.0 * 0.45);

        // ── the hills, the floor of the valley, and the lake ──
        let hill = mix(mix(sc.layers[1], leaf, 0.16 + 0.26 * sc.day), sc.sky[5], 0.10);
        let meadow = mix(hex(sc.p.seasons[sc.season][2]), hex(sc.p.seasons[sc.season][1]), 0.22);
        let floor = mix(sc.layers[2], meadow, 0.26 + 0.46 * sc.day);
        let (m0, m1) = (lay.dy(700.0), lay.dy(830.0));
        d.s.stamp(&self.masks[1].1, self.masks[1].0 as i32, 0, |y| (mix(hill, floor, smooth(m0, m1, y as f32)), 1.0));
        bloom.hide(&self.masks[1].1, w, self.masks[1].0);
        // firs along the skyline
        let mut x = -20.0;
        let mut k = 0u32;
        while x < 1620.0 {
            let (y, tall) = (height_at(&self.hills, x) + 2.0, 7.0 + 9.0 * hash2(k, 401));
            d.poly(&[(lay.dx(x - tall * 0.34), lay.dy(y)), (lay.dx(x), lay.dy(y - tall)), (lay.dx(x + tall * 0.34), lay.dy(y))], hill, 1.0);
            x += 6.0 + 9.0 * hash2(k, 403);
            k += 1;
        }
        if sc.snow > 0.2 || sc.frost > 0.4 {
            band(d.s, lay.dy(640.0), lay.dy(720.0), 20.0 * u, snow, 0.20 * sc.snow.max(sc.frost));
        }
        // where the sun gets down into the valley
        if sc.day > 0.1 {
            let sunny = mix(floor, [176.0, 204.0, 124.0], 0.5);
            for (x, y, rx, ry) in [(960.0f32, 812.0f32, 300.0f32, 30.0f32), (300.0, 790.0, 240.0, 26.0), (1180.0, 790.0, 200.0, 20.0)] {
                d.glow(lay.dx(x), lay.dy(y), lay.d(rx), lay.d(ry), sunny, 0.40 * sc.day * sc.sun_vis.max(0.3), 1, Blend::Over);
            }
        }
        self.lake(d, sc, lights, &light, bloom, out);
        // the fall itself is always there; the live layer adds the water moving in it
        falls(d, sc, lay, None);
        if !sc.live {
            ship(d, sc, lay, sc.ts, 0.0);
        }
        if sc.fog > 0.0 {
            band(d.s, lay.dy(600.0), lay.h * 1.3, 80.0 * u, mix(sc.sky[4], sc.haze, 0.5), 0.5 * sc.fog);
        }
        if sc.mist > 0.02 {
            band(d.s, lay.dy(690.0), lay.dy(790.0), 34.0 * u, mix(sc.sky[5], sc.haze, 0.4), 0.36 * sc.mist);
        }
        // small trees here and there, in the same leaf as the great one
        let from_left = light.x < lay.w / 2.0;
        for &(x, y, tall) in COPSE {
            let trunk = shade(floor, -0.34);
            d.line((lay.dx(x), lay.dy(y)), (lay.dx(x), lay.dy(y - tall * 0.5)), (tall * 0.07 * u).max(0.9), trunk, 1.0);
            d.ellipse(lay.dx(x + 2.0), lay.dy(y + 1.0), lay.d(tall * 0.5), lay.d(tall * 0.09), shade(floor, -0.22), 0.5);
            match foliage(sc) {
                Some((deep, bright)) => {
                    let (deep, bright) = (mix(deep, floor, 0.25), mix(bright, floor, 0.25));
                    d.ellipse(lay.dx(x), lay.dy(y - tall * 0.66), lay.d(tall * 0.40), lay.d(tall * 0.42), deep, 1.0);
                    let side = if from_left { -1.0 } else { 1.0 };
                    d.ellipse(lay.dx(x + side * tall * 0.10), lay.dy(y - tall * 0.76), lay.d(tall * 0.26), lay.d(tall * 0.26), bright, 0.30 + 0.5 * sc.day);
                }
                None => {
                    for a in [-0.7f32, -0.25, 0.25, 0.7] {
                        d.line((lay.dx(x), lay.dy(y - tall * 0.45)), (lay.dx(x + a * tall * 0.36), lay.dy(y - tall * (0.98 - 0.2 * a.abs()))), (0.9 * u).max(0.8), trunk, 0.9);
                    }
                }
            }
        }
        // the river out of the lake, and the path up to the crag
        let ribbon = |way: &[P], w0: f32, w1: f32| -> Vec<P> {
            let n = way.len();
            let side = |k: usize, sign: f32| {
                let (a, b) = (way[k.saturating_sub(1)], way[(k + 1).min(n - 1)]);
                let (dx, dy) = (b.0 - a.0, b.1 - a.1);
                let len = (dx * dx + dy * dy).sqrt().max(1e-3);
                let half = (w0 + (w1 - w0) * (k as f32 / n as f32).powf(1.5)) * sign;
                (lay.dx(way[k].0 - dy / len * half), lay.dy(way[k].1 + dx / len * half))
            };
            (0..n).map(|k| side(k, 1.0)).chain((0..n).rev().map(|k| side(k, -1.0))).collect()
        };
        let water = mix(mix(sc.layers[2], sc.sky[4], 0.62), sc.sky[5], 0.2);
        d.poly(&ribbon(&curve(&[(936.0, 772.0), (962.0, 800.0), (940.0, 832.0), (884.0, 860.0), (856.0, 902.0), (884.0, 954.0), (950.0, 1010.0)], 8), 3.0, 22.0), water, 0.95);
        let track = mix(floor, CREAM, 0.10 + 0.26 * sc.day);
        d.poly(&ribbon(&curve(&[(676.0, 806.0), (664.0, 826.0), (704.0, 858.0), (738.0, 902.0), (712.0, 952.0), (732.0, 1010.0)], 8), 1.6, 8.0), track, 0.5);

        // ── the crag and the tower ──
        let rock = mix(sc.layers[3], [152.0, 122.0, 146.0], 0.12 + 0.22 * sc.day);
        let (c0, c1) = (lay.dy(600.0), lay.dy(816.0));
        d.s.stamp(&self.masks[CRAGS].1, self.masks[CRAGS].0 as i32, 0, |y| (mix(rock, mix(rock, floor, 0.55), smooth(c0, c1, y as f32).powi(2)), 1.0));
        bloom.hide(&self.masks[CRAGS].1, w, self.masks[CRAGS].0);
        self.crag(d, sc, rock, &light);
        self.tower(d, sc, rock, lights, &light, bloom, out);

        // ── the rocks that hang in the air ──
        for (k, isle) in ISLES.iter().enumerate() {
            self.isle(d, sc, isle, k, &light, out);
        }

        // ── the knoll, the bank and what stands on them ──
        let near = mix(sc.layers[4], meadow, 0.12 + 0.34 * sc.day);
        self.tree(d, sc, near, &light, bloom, out);
        // the mask has the crown in it too, so the ground is painted by its own outline
        for (line, x0) in [(&self.knoll, 990.0f32), (&self.bank, -42.0)] {
            let mut p = to_px(lay, line);
            if x0 > 0.0 {
                p.extend([(lay.w + 2.0, lay.h + 2.0), (lay.dx(x0), lay.h + 2.0)]);
            } else {
                p.extend([(-2.0, lay.h + 2.0)]);
            }
            d.poly(&p, near, 1.0);
        }
        bloom.hide(&self.masks[NEAR].1, w, self.masks[NEAR].0);
        self.stones(d, sc, near, &light, bloom, out);
        // flowers in the grass while there is light to see them by
        if sc.season != 0 && sc.day > 0.25 && sc.snow < 0.1 {
            for i in 0..70u32 {
                let left = i % 2 == 0;
                let x = if left { 20.0 + 560.0 * hash2(i, 431) } else { 1080.0 + 500.0 * hash2(i, 431) };
                let y = height_at(if left { &self.bank } else { &self.knoll }, x) + 8.0 + 60.0 * hash2(i, 433);
                d.dot(lay.dx(x), lay.dy(y), 1.5 * u, 1.5 * u, [ROSE, CREAM, AMBER, [196.0, 170.0, 232.0]][i as usize % 4], 0.75 * sc.day, Blend::Over);
            }
        }
        if !sc.live {
            grass(d, sc, lay, &self.bank, BANK_GRASS, (sc.ts / 60.0) as f32, near);
            grass(d, sc, lay, &self.knoll, KNOLL_GRASS, (sc.ts / 60.0) as f32, near);
        }
        if sc.cat_out {
            if sc.cat_asleep {
                props::cat(d, lay.dx(1214.0), lay.dy(904.0), u * 1.1, shade(near, -0.25), true, 1.0, 0.0);
            } else if !sc.live {
                let (x, y, face) = cat_place(sc);
                props::cat(d, lay.dx(x), lay.dy(y), u * 1.1, shade(near, -0.25), false, face, (sc.ts / 37.0) as f32);
            }
        }
        let _ = dusk;
        out.glades.push([lay.dx(40.0), lay.dy(780.0), lay.dx(640.0), lay.dy(900.0)]);
        out.glades.push([lay.dx(1040.0), lay.dy(820.0), lay.dx(1580.0), lay.dy(920.0)]);
        out.leaves = true;
    }

    fn targets(&self, sc: &Scene, a: &Anchors) -> Vec<crate::interaction::Target> {
        use crate::interaction::{common_targets, Action, Target};
        let mut out = common_targets(sc, &self.lay, a);
        out.push(Target::new(100, [570.0, SHORE + 5.0, 1230.0, 766.0], (930.0, 741.0), Action::Ripple, mix(sc.sky[4], BLUE, 0.25), Some(CRAGS)));
        out.push(Target::new(101, [1300.0, 565.0, 1570.0, 880.0], (1430.0, 735.0), Action::Sparks, AMBER, None));
        out.push(Target::near(102, (1220.0, 856.0), 65.0, Action::Sparks, BLUE, None));
        out
    }

    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, _a: &Anchors, t: f32, now: f64) {
        let lay = &self.lay;
        let u = lay.u;
        d.s.behind = Some(CRAGS);
        ship(d, sc, lay, now, t);
        d.s.behind = Some(NEAR);
        falls(d, sc, lay, Some(t));
        d.s.behind = None;
        let rock = shade(mix(sc.layers[3], [152.0, 122.0, 146.0], 0.12 + 0.22 * sc.day), -0.25);
        let tw = &self.tw;
        props::pennant(d, sc, tw.dx(ROOF_TIP.0), tw.dy(ROOF_TIP.1), tw.dy(ROOF_TIP.1 - 20.0), u, rock, t * (2.0 + 3.0 * sc.wind_x.abs().min(1.5)));
        let near = mix(sc.layers[4], mix(hex(sc.p.seasons[sc.season][2]), hex(sc.p.seasons[sc.season][1]), 0.22), 0.12 + 0.34 * sc.day);
        grass(d, sc, lay, &self.bank, BANK_GRASS, t, near);
        grass(d, sc, lay, &self.knoll, KNOLL_GRASS, t, near);
        if sc.cat_out && !sc.cat_asleep {
            let (x, y, face) = cat_place(sc);
            let tail = if (t * 0.11).sin() > -0.3 { t * 1.4 } else { 0.0 };
            props::cat(d, lay.dx(x), lay.dy(y), u * 1.1, shade(near, -0.25), false, face, tail);
        }
    }
}

impl Dale {
    /// The lake: the sky lying in it, the hamlet on its far shore, and whatever is bright overhead.
    fn lake(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let shore = to_px(lay, &curve(LAKE, 6));
        let deep = mix(mix(sc.layers[2], sc.sky[3], 0.55), sc.sky[5], 0.15);
        let bright = mix(deep, sc.sky[5], 0.62);
        let (y0, y1) = (lay.dy(SHORE), lay.dy(776.0));
        if d.sh.begin(lay.dx(550.0), y0 - 2.0, lay.dx(1256.0), y1 + 2.0, d.s.w, d.s.h) {
            d.sh.poly(&shore);
            let water = d.sh.take();
            let frozen = sc.temp < -3.0;
            let ice = mix(snow_col(sc), sc.sky[5], 0.3);
            water.paint(d.s, Blend::Over, |y| (if frozen { ice } else { mix(bright, deep, smooth(y0, y1, y).powf(0.7)) }, 1.0));
            // the hills stand in it, upside down and dim
            if d.sh.begin(lay.dx(550.0), y0 - 2.0, lay.dx(1256.0), y1 + 2.0, d.s.w, d.s.h) {
                let mirror: Vec<P> = self.hills.iter().filter(|p| p.0 > 540.0 && p.0 < 1270.0).map(|p| (lay.dx(p.0), lay.dy(SHORE + (SHORE - p.1) * 0.9 + 6.0))).collect();
                let mut m = vec![(lay.dx(540.0), y0), (lay.dx(1270.0), y0)];
                m.extend(mirror.into_iter().rev());
                d.sh.poly(&m);
                let mut hills = d.sh.take();
                hills.clip(&water);
                hills.paint(d.s, Blend::Over, |_| (shade(deep, -0.18), if frozen { 0.12 } else { 0.34 }));
            }
            // the sun or the moon, drawn down the water
            let (x, col, k) = if sc.sun_vis * smooth(-1.5, 3.0, sc.eph.sun.el as f32) > 0.2 {
                (lay.x(sc.eph.sun.x as f32), mix(sc.sun_col, sc.glow, 0.35), sc.sun_vis * (1.0 - 0.7 * smooth(14.0, 48.0, sc.eph.sun.el as f32)))
            } else {
                (lay.x(sc.eph.moon.body.x as f32), hex(sc.p.moon), sc.moonlight * (1.0 - sc.day) * 0.8)
            };
            if k > 0.05 && x > lay.dx(570.0) && x < lay.dx(1230.0) && !frozen {
                for i in 0..26u32 {
                    let f = hash2(i, 441);
                    let y = y0 + (y1 - y0) * (0.08 + 0.84 * f);
                    let half = lay.d(4.0 + 22.0 * f) * (0.4 + 0.6 * hash2(i, 443));
                    let gx = x + (hash2(i, 447) - 0.5) * lay.d(14.0 + 40.0 * f);
                    d.rect_mode(gx - half, y, gx + half, y + (1.0 + 0.8 * f) * u, col, 0.55 * k, Blend::Screen);
                }
            }
            // a few long ripples
            for i in 0..14u32 {
                let f = hash2(i, 451);
                let (x, y) = (lay.dx(600.0 + 600.0 * hash2(i, 453)), y0 + (y1 - y0) * (0.2 + 0.75 * f));
                d.rect(x, y, x + lay.d(24.0 + 60.0 * f), y + 1.0 * u, bright, 0.22);
            }
        }
        // the hamlet
        let wall = mix(sc.layers[2], CREAM, 0.10 + 0.50 * sc.day);
        let roofs = [mix(sc.layers[2], ROSE, 0.2 + 0.5 * sc.day), mix(sc.layers[2], [110.0, 118.0, 180.0], 0.2 + 0.5 * sc.day), mix(sc.layers[2], AMBER, 0.2 + 0.5 * sc.day)];
        let village = lights.share / 0.20;
        for (k, (x, wide, tall, roof)) in HOUSES.into_iter().enumerate() {
            let p = |a: f32, b: f32| (lay.dx(a), lay.dy(b));
            d.rect(lay.dx(x - wide / 2.0), lay.dy(SHORE - tall), lay.dx(x + wide / 2.0), lay.dy(SHORE + 1.0), wall, 1.0);
            d.poly(&[p(x - wide / 2.0 - 1.5, SHORE - tall + 0.5), p(x, SHORE - tall - wide * 0.46), p(x + wide / 2.0 + 1.5, SHORE - tall + 0.5)], roofs[roof], 1.0);
            if light.k > 0.03 {
                let side = if light.x < lay.dx(x) { -1.0 } else { 1.0 };
                d.rect(lay.dx(x + side * (wide / 2.0 - 1.2)), lay.dy(SHORE - tall), lay.dx(x + side * wide / 2.0), lay.dy(SHORE + 1.0), mix(wall, light.col, 0.6), (light.k * 0.9).min(0.8));
            }
            if hash2(k as u32, 461 + (sc.t * 2.0) as u32) < village {
                let (wx, wy) = p(x - wide * 0.12, SHORE - tall * 0.45);
                d.rect(wx - 1.3 * u, wy - 1.5 * u, wx + 1.3 * u, wy + 1.5 * u, WARM[k % 4], 1.0);
                bloom.light(wx - 2.5 * u, wy - 2.5 * u, wx + 2.5 * u, wy + 2.5 * u, WARM[1], 0.6 * sc.p.bloom);
                d.rect_mode(wx - 0.9 * u, lay.dy(SHORE + 4.0), wx + 0.9 * u, lay.dy(SHORE + 10.0), WARM[1], 0.20, Blend::Screen);
            }
        }
        // a spire among them, and a jetty
        let dark = shade(mix(sc.layers[2], sc.layers[3], 0.5), -0.05);
        d.poly(&to_px(lay, &[(772.0, SHORE - 9.0), (774.0, SHORE - 30.0), (776.0, SHORE - 9.0)]), roofs[1], 1.0);
        d.rect(lay.dx(852.0), lay.dy(SHORE + 2.0), lay.dx(884.0), lay.dy(SHORE + 3.6), dark, 1.0);
        if sc.smoke > 0.08 {
            let col = mix(sc.layers[1], sc.sky[5], 0.7);
            let s = Smoke { x: lay.dx(763.0), y: lay.dy(SHORE - 16.0), scale: 0.5, col, behind: CRAGS };
            if !sc.live {
                props::smoke_still(d, sc, s.x, s.y, u * s.scale, s.col, 2.0);
            }
            out.smoke.push(s);
        }
    }

    /// Beds of rock in the crag, the stair cut up it, and the lamps along the stair.
    fn crag(&self, d: &mut Draw<Canvas>, sc: &Scene, rock: Rgb, light: &Light) {
        let lay = &self.lay;
        let u = lay.u;
        let from_left = light.x < lay.dx(548.0);
        // a ridge runs down the middle of it: one side takes the light, the other is in its own shade
        let ridge = [(548.0, 601.0), (538.0, 672.0), (556.0, 748.0), (546.0, 816.0)];
        let flank = |left: bool| -> Vec<P> {
            let mut p: Vec<P> = if left { CRAG[..7].to_vec() } else { CRAG[7..].iter().rev().copied().collect() };
            p.extend(ridge);
            to_px(lay, &p)
        };
        if light.k > 0.03 {
            d.poly(&flank(from_left), mix(rock, light.col, 0.55), (0.6 * light.k).min(0.5));
        }
        d.poly(&flank(!from_left), shade(rock, -0.22), 0.40);
        let dark = shade(rock, -0.18);
        for (a, b, wide) in [((474.0, 640.0), (452.0, 780.0), 7.0), ((516.0, 612.0), (498.0, 800.0), 9.0), ((574.0, 610.0), (590.0, 790.0), 8.0), ((612.0, 660.0), (640.0, 800.0), 6.0), ((430.0, 716.0), (420.0, 806.0), 5.0)] {
            d.line((lay.dx(a.0), lay.dy(a.1)), (lay.dx(b.0), lay.dy(b.1)), lay.d(wide), dark, 0.20);
        }
        // the stair, back and forth up the east face, with a lamp at each turn
        let stair = [(676.0, 800.0), (640.0, 762.0), (654.0, 728.0), (622.0, 700.0), (628.0, 664.0), (602.0, 634.0), (586.0, 603.0)];
        d.path(&to_px(lay, &stair), 1.5 * u, mix(rock, CREAM, 0.16 + 0.3 * sc.day), 0.7);
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        if dusk > 0.05 {
            for (k, p) in stair.iter().enumerate().skip(1).step_by(2) {
                let (lx, ly) = (lay.dx(p.0), lay.dy(p.1 - 5.0));
                d.glow(lx, ly, 9.0 * u, 9.0 * u, WARM[1], 0.6 * dusk, 2, Blend::Screen);
                d.dot(lx, ly, 1.4 * u, 1.4 * u, WARM[k % 4], dusk, Blend::Over);
            }
        }
        // turf on the ledges, and where the crag meets the meadow
        let turf = mix(rock, mix(hex(sc.p.seasons[sc.season][2]), hex(sc.p.seasons[sc.season][1]), 0.22), 0.24 + 0.44 * sc.day);
        for (x, y, rx, ry) in [(548.0, 603.0, 50.0, 4.0), (440.0, 690.0, 12.0, 3.4), (634.0, 668.0, 11.0, 3.2), (410.0, 772.0, 12.0, 3.6), (552.0, 818.0, 168.0, 7.0)] {
            d.ellipse(lay.dx(x), lay.dy(y), lay.d(rx), lay.d(ry), turf, 0.9);
        }
    }

    /// The wizard's tower: yours.
    fn tower(&self, d: &mut Draw<Canvas>, sc: &Scene, rock: Rgb, lights: &Lights, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.tw;
        let u = lay.u * 1.15;
        let x = TOWER_X;
        let p = |a: f32, b: f32| (lay.dx(a), lay.dy(b));
        let r = |d: &mut Draw<Canvas>, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, a: f32| d.rect(lay.dx(x0), lay.dy(y0), lay.dx(x1), lay.dy(y1), col, a);
        let stone = mix(rock, CREAM, 0.10 + 0.56 * sc.day);
        let roof = mix(rock, [112.0, 104.0, 190.0], 0.24 + 0.56 * sc.day);
        let dark = shade(rock, -0.26);
        let side = if light.x < lay.dx(x) { -1.0 } else { 1.0 };
        let lit = mix(stone, light.col, 0.65);
        // the turret on its side
        d.poly(&[p(x + 21.0, 562.0), p(x + 21.0, 520.0), p(x + 43.0, 520.0), p(x + 43.0, 562.0), p(x + 32.0, 572.0)], shade(stone, -0.06), 1.0);
        d.poly(&[p(x + 17.0, 522.0), p(x + 32.0, 498.0), p(x + 47.0, 522.0)], roof, 1.0);
        r(d, x + 29.5, 532.0, x + 34.5, 541.0, dark, 0.9);
        // the shaft, narrowing as it goes up
        let half = |y: f32| 19.0 + 4.0 * (y - 486.0) / (TOWER_FOOT - 486.0);
        d.poly(&[p(x - half(TOWER_FOOT), TOWER_FOOT + 2.0), p(x - 19.0, 486.0), p(x + 19.0, 486.0), p(x + half(TOWER_FOOT), TOWER_FOOT + 2.0)], stone, 1.0);
        d.poly(&[p(x - side * half(TOWER_FOOT), TOWER_FOOT + 2.0), p(x - side * 19.0, 486.0), p(x - side * 8.0, 486.0), p(x - side * (half(TOWER_FOOT) - 12.0), TOWER_FOOT + 2.0)], shade(rock, -0.1), 0.18 + 0.22 * sc.day);
        if light.k > 0.03 {
            d.poly(&[p(x + side * half(TOWER_FOOT), TOWER_FOOT + 2.0), p(x + side * 19.0, 486.0), p(x + side * 16.4, 486.0), p(x + side * (half(TOWER_FOOT) - 2.8), TOWER_FOOT + 2.0)], lit, (light.k * 0.9).min(0.85));
        }
        // courses of stone, slit windows, the door
        for y in [512.0f32, 540.0, 568.0] {
            r(d, x - half(y), y, x + half(y), y + 1.0, dark, 0.22);
        }
        let up = lights.share / 0.20;
        for (k, (wx, wy)) in [(x - 6.0, 500.0f32), (x + 7.0, 528.0), (x - 5.0, 556.0)].into_iter().enumerate() {
            if hash2(k as u32 + 20, (sc.t * 2.0) as u32) < up {
                r(d, wx - 1.6, wy, wx + 1.6, wy + 8.0, WARM[k % 4], 1.0);
                bloom.light(lay.dx(wx - 3.0), lay.dy(wy - 1.0), lay.dx(wx + 3.0), lay.dy(wy + 9.0), WARM[1], 0.6 * sc.p.bloom);
            } else {
                r(d, wx - 1.6, wy, wx + 1.6, wy + 8.0, dark, 0.85);
            }
        }
        d.poly(&[p(x - 5.0, TOWER_FOOT + 1.0), p(x - 5.0, 588.0), p(x, 583.0), p(x + 5.0, 588.0), p(x + 5.0, TOWER_FOOT + 1.0)], dark, 0.95);
        // the top room stands out over the shaft on corbels
        d.poly(&[p(x - 19.0, 494.0), p(x - 31.0, 482.0), p(x + 31.0, 482.0), p(x + 19.0, 494.0)], shade(stone, -0.12), 1.0);
        r(d, x - 31.0, 444.0, x + 31.0, 483.0, stone, 1.0);
        r(d, x - side * 31.0, 444.0, x - side * 20.0, 483.0, shade(rock, -0.1), 0.18 + 0.22 * sc.day);
        if light.k > 0.03 {
            r(d, x + side * 28.6, 444.0, x + side * 31.0, 483.0, lit, (light.k * 0.9).min(0.85));
        }
        r(d, x - 33.0, 480.0, x + 33.0, 484.0, dark, 0.9);
        // the flue the smoke of the work goes up
        let (fx, fy) = p(FLUE.0, FLUE.1);
        d.path(&[p(x - 31.0, 470.0), p(FLUE.0, 470.0), p(FLUE.0, FLUE.1)], 3.0 * u, dark, 1.0);
        d.rect(fx - 3.0 * u, fy - 2.0 * u, fx + 3.0 * u, fy, dark, 1.0);
        let fume = mix(mix(rock, sc.sky[5], 0.6), [176.0, 132.0, 226.0], 0.5);
        out.stove = Some(Smoke { x: fx, y: fy - 2.0 * u, scale: 0.7, col: fume, behind: 9 });
        if sc.machine.load > 0.35 && !sc.live {
            props::smoke_still(d, sc, fx, fy - 2.0 * u, u * 0.7, fume, 1.3);
        }
        // the roof, a little crooked, with a gold ball on it
        d.poly(&[p(x - 38.0, 447.0), p(x - 12.0, 410.0), p(ROOF_TIP.0, ROOF_TIP.1), p(x + 14.0, 412.0), p(x + 38.0, 447.0)], roof, 1.0);
        d.poly(&[p(x - side * 38.0, 447.0), p(x - side * 12.0, 410.0), p(ROOF_TIP.0, ROOF_TIP.1), p(x - side * 4.0, 447.0)], shade(roof, -0.18), 0.5);
        if light.k > 0.03 {
            d.line(p(ROOF_TIP.0, ROOF_TIP.1), p(x + side * 38.0, 447.0), 1.6 * u, mix(roof, light.col, 0.65), (light.k * 0.9).min(0.85));
        }
        d.circle(lay.dx(ROOF_TIP.0), lay.dy(ROOF_TIP.1 - 1.0), 2.4 * u, mix(rock, AMBER, 0.4 + 0.5 * sc.day), 1.0);
        // the window of the top room, with the lamp in it
        let (wx0, wy0, wx1, wy1) = (LAMP.0 - 9.0, LAMP.1 - 13.0, LAMP.0 + 9.0, LAMP.1 + 11.0);
        let arch: Vec<P> = [p(wx0, wy1), p(wx0, wy0 + 8.0)].into_iter().chain((0..=10).map(|i| PI * i as f32 / 10.0).map(|a| p(LAMP.0 - 9.0 * a.cos(), wy0 + 8.0 - 8.0 * a.sin()))).chain([p(wx1, wy1)]).collect();
        d.poly(&arch.iter().map(|q| (q.0 + (q.0 - lay.dx(LAMP.0)) * 0.22, q.1 + (q.1 - lay.dy(LAMP.1)) * 0.14)).collect::<Vec<P>>(), dark, 1.0);
        if sc.machine.agents > 0 {
            let (lamp, glow, low) = lamp_colours(sc);
            d.poly(&arch, lamp, 1.0);
            bloom.light(lay.dx(wx0 - 4.0), lay.dy(wy0 - 4.0), lay.dx(wx1 + 4.0), lay.dy(wy1 + 4.0), glow, (0.9 + 0.9 * sc.dark) * sc.p.bloom.max(0.6));
            let (cx, cy) = p(LAMP.0, LAMP.1);
            out.breath.push(Breath { x: cx, y: cy, rx: lay.d(52.0), ry: lay.d(42.0), col: glow, strength: 0.45 + 0.55 * sc.dark, low, behind: Some(3) });
        } else {
            d.poly(&arch, mix(rock, sc.sky[5], 0.12 + 0.36 * sc.day), 1.0);
        }
        d.line(p(LAMP.0, wy0), p(LAMP.0, wy1), 1.3 * u, dark, 1.0);
        d.line(p(wx0, LAMP.1 + 1.0), p(wx1, LAMP.1 + 1.0), 1.1 * u, dark, 1.0);
        out.music = Some((lay.dx(x + 36.0), lay.dy(452.0)));
        out.perch = Some(p(ROOF_TIP.0, ROOF_TIP.1 + 20.0));
        if !sc.live {
            props::pennant(d, sc, lay.dx(ROOF_TIP.0), lay.dy(ROOF_TIP.1), lay.dy(ROOF_TIP.1 - 20.0), self.lay.u, dark, (sc.ts / 200.0) as f32);
            if sc.machine.audio {
                let ink = mix(hex(sc.p.ink), AMBER, 0.4);
                props::note(d, lay.dx(x + 44.0), lay.dy(446.0), u, ink, 0.8, false);
                props::note(d, lay.dx(x + 34.0), lay.dy(428.0), u, ink, 0.55, true);
            }
        }
    }

    fn isle(&self, d: &mut Draw<Canvas>, sc: &Scene, isle: &Isle, k: usize, light: &Light, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let air = sc.sky[4];
        let rock = mix(mix(sc.layers[3], [146.0, 108.0, 96.0], 0.14 + 0.30 * sc.day), air, 0.55 * isle.far);
        let turf = mix(mix(sc.layers[2], hex(sc.p.seasons[sc.season][1]), 0.24 + 0.46 * sc.day), air, 0.5 * isle.far);
        let top = curve(isle.top, 6);
        let (x0, x1) = (isle.top[0].0, isle.top[isle.top.len() - 1].0);
        let mid = (x0 + x1) / 2.0;
        // the rock, darker towards its point, with the side that faces the light picked out
        let mut body = to_px(lay, &top);
        body.extend(to_px(lay, isle.under));
        d.poly(&body, rock, 1.0);
        let from_left = light.x < lay.dx(mid);
        let n = isle.under.len();
        let half: Vec<P> = isle.under.iter().copied().filter(|p| (p.0 < mid) != from_left || (p.0 - mid).abs() < 8.0).collect();
        if half.len() > 2 {
            let mut lee = half.clone();
            lee.push((mid, isle.under[0].1));
            d.poly(&to_px(lay, &lee), shade(rock, -0.24), 0.46);
        }
        let _ = n;
        // roots and shards hanging under it
        for i in 0..(3 + (x1 - x0) as u32 / 40) {
            let x = x0 + (x1 - x0) * (0.2 + 0.6 * hash2(i, 471 + k as u32));
            let y = isle.under.iter().filter(|p| (p.0 - x).abs() < 30.0).map(|p| p.1).fold(isle.top[0].1 + 20.0, f32::max);
            d.line((lay.dx(x), lay.dy(y - 8.0)), (lay.dx(x + 2.0), lay.dy(y + 6.0 + 12.0 * hash2(i, 473))), (1.0 * u).max(0.8), shade(rock, -0.2), 0.7 * (1.0 - isle.far));
        }
        // turf over the top, hanging a little over the edge
        let mut cap = to_px(lay, &top);
        cap.extend(top.iter().rev().map(|p| (lay.dx(p.0), lay.dy(p.1 + (7.0 + 3.0 * (p.0 * 0.11).sin()) * (1.0 - 0.5 * isle.far)))));
        d.poly(&cap, turf, 1.0);
        if light.k > 0.03 {
            d.path(&to_px(lay, &top), 1.4 * u, mix(turf, light.col, 0.6), (0.8 * light.k).min(0.7));
        }
        for &(x, tall) in isle.trees {
            let y = height_at(isle.top, x);
            let crown = foliage(sc).map_or(shade(rock, -0.1), |c| mix(c.0, air, 0.5 * isle.far));
            d.line((lay.dx(x), lay.dy(y + 1.0)), (lay.dx(x), lay.dy(y - tall * 0.5)), (1.4 * u * (1.0 - 0.5 * isle.far)).max(0.8), shade(rock, -0.25), 1.0);
            if sc.season != 0 {
                d.ellipse(lay.dx(x), lay.dy(y - tall * 0.62), lay.d(tall * 0.44), lay.d(tall * 0.46), crown, 1.0);
                if let Some(c) = foliage(sc) {
                    let side = if from_left { -1.0 } else { 1.0 };
                    d.ellipse(lay.dx(x + side * tall * 0.12), lay.dy(y - tall * 0.72), lay.d(tall * 0.28), lay.d(tall * 0.28), mix(c.1, air, 0.5 * isle.far), 0.8);
                }
            } else {
                for a in [-0.7f32, -0.25, 0.25, 0.7] {
                    d.line((lay.dx(x), lay.dy(y - tall * 0.45)), (lay.dx(x + a * tall * 0.4), lay.dy(y - tall * (0.95 - 0.2 * a.abs()))), (0.9 * u).max(0.8), shade(rock, -0.25), 0.9);
                }
            }
        }
        if k == 0 {
            // what is left of an arch, on the big one
            let stone = mix(mix(sc.layers[2], CREAM, 0.10 + 0.5 * sc.day), air, 0.1);
            for x in [1118.0f32, 1140.0] {
                d.rect(lay.dx(x - 2.4), lay.dy(height_at(&top, x) - 24.0), lay.dx(x + 2.4), lay.dy(height_at(&top, x) + 1.0), stone, 1.0);
            }
            let span: Vec<P> = (0..=8).map(|i| PI * i as f32 / 8.0).map(|a| (lay.dx(1129.0 - 13.4 * a.cos()), lay.dy(344.0 - 9.0 * a.sin()))).collect();
            d.path(&span[..7], 3.6 * u, stone, 1.0);
            // crystals in its underside, that hold a little light after dark
            let glow = smooth(2.0, -5.0, sc.el_eff);
            for (j, (x, y, len)) in [(1126.0f32, 520.0f32, 14.0f32), (1150.0, 494.0, 10.0), (1100.0, 478.0, 9.0), (1186.0, 460.0, 8.0)].into_iter().enumerate() {
                let c: Rgb = [130.0, 226.0, 232.0];
                d.poly(&[(lay.dx(x - 2.6), lay.dy(y - len)), (lay.dx(x + 2.6), lay.dy(y - len)), (lay.dx(x + 0.6), lay.dy(y + 2.0))], mix(rock, c, 0.35 + 0.5 * glow), 1.0);
                if glow > 0.1 {
                    d.glow(lay.dx(x), lay.dy(y - len * 0.5), 10.0 * u, 12.0 * u, c, 0.5 * glow, 2, Blend::Screen);
                    out.twinkle.push((lay.dx(x), lay.dy(y - len * 0.5), c, if j == 0 { 9 } else { NEAR }));
                }
            }
        }
    }

    /// The great tree on the knoll, and the lanterns in it.
    fn tree(&self, d: &mut Draw<Canvas>, sc: &Scene, near: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let bark = mix(shade(near, -0.30), [92.0, 70.0, 66.0], 0.10 + 0.34 * sc.day);
        let from_left = light.x < lay.dx(1436.0);
        let side = if from_left { -1.0 } else { 1.0 };
        let leaves = foliage(sc);
        // the trunk: wide in the root, with one side in the light
        let trunk = [(1384.0, 880.0), (1404.0, 846.0), (1416.0, 790.0), (1420.0, 720.0), (1422.0, 690.0), (1446.0, 690.0), (1448.0, 724.0), (1454.0, 790.0), (1466.0, 846.0), (1486.0, 880.0)];
        d.poly(&to_px(lay, &trunk), bark, 1.0);
        for &(limb, thick) in LIMBS {
            let line = curve(limb, 5);
            let n = line.len();
            for (i, seg) in line.windows(2).enumerate() {
                let wide = lay.d(thick * (1.0 - 0.72 * i as f32 / n as f32));
                d.line((lay.dx(seg[0].0), lay.dy(seg[0].1)), (lay.dx(seg[1].0), lay.dy(seg[1].1)), wide.max(1.0), bark, 1.0);
            }
        }
        if light.k > 0.03 {
            let edge: Vec<P> = if from_left { trunk[..5].to_vec() } else { trunk[5..].to_vec() };
            d.path(&to_px(lay, &edge), 2.2 * u, mix(bark, light.col, 0.6), (0.9 * light.k).min(0.8));
        }
        d.poly(&to_px(lay, &[(1432.0, 880.0), (1438.0, 800.0), (1436.0, 730.0), (1446.0, 724.0), (1454.0, 790.0), (1466.0, 846.0), (1486.0, 880.0)]), shade(bark, -0.3), if from_left { 0.34 } else { 0.0 });
        match leaves {
            Some((deep, bright)) => {
                let (bx0, by0, bx1, by1) = (lay.dx(1190.0), lay.dy(420.0), lay.dx(1690.0), lay.dy(706.0));
                let clumps = |sh: &mut Shaper, dx: f32, dy: f32, grow: f32| {
                    for &(x, y, rx, ry) in CROWN {
                        sh.ellipse(lay.dx(x + dx * rx), lay.dy(y + dy * ry), lay.d(rx * grow), lay.d(ry * grow));
                    }
                };
                if d.sh.begin(bx0, by0, bx1, by1, d.s.w, d.s.h) {
                    clumps(d.sh, 0.0, 0.0, 1.0);
                    let body = d.sh.take();
                    let under = shade(deep, -0.16);
                    body.paint(d.s, Blend::Over, |y| (mix(deep, under, smooth(by0, by1, y)), 1.0));
                    // the tops of the clumps catch the light, from the side it comes
                    let shine = 0.22 + 0.78 * sc.day.max(sc.moonlight * 0.5).max(sc.twilight * 0.8);
                    for (dx, dy, grow, col, a) in [(side * 0.22, -0.34, 0.80, bright, 0.80), (side * 0.36, -0.56, 0.52, mix(bright, light.col, 0.35), 0.55)] {
                        if d.sh.begin(bx0, by0, bx1, by1, d.s.w, d.s.h) {
                            clumps(d.sh, dx, dy, grow);
                            let mut lit = d.sh.take();
                            lit.clip(&body);
                            lit.paint(d.s, Blend::Over, |_| (col, a * shine));
                        }
                    }
                    if sc.snow > 0.2 {
                        if d.sh.begin(bx0, by0, bx1, by1, d.s.w, d.s.h) {
                            clumps(d.sh, 0.0, -0.66, 0.60);
                            let mut white = d.sh.take();
                            white.clip(&body);
                            white.paint(d.s, Blend::Over, |_| (snow_col(sc), 0.75 * sc.snow));
                        }
                    }
                }
            }
            None => {
                // bare: twigs at the end of every limb, and snow lying along them when there is any
                for (k, &(limb, _)) in LIMBS.iter().enumerate() {
                    let tip = limb[limb.len() - 1];
                    let before = limb[limb.len() - 2];
                    let aim = (tip.1 - before.1).atan2(tip.0 - before.0);
                    for j in 0..5 {
                        let a = aim + (j as f32 - 2.0) * 0.36;
                        let len = 34.0 + 22.0 * hash2(k as u32, 481 + j);
                        let end = (tip.0 + a.cos() * len, tip.1 + a.sin() * len);
                        d.line((lay.dx(tip.0), lay.dy(tip.1)), (lay.dx(end.0), lay.dy(end.1)), 1.5 * u, bark, 1.0);
                        let fork = (end.0 + (a + 0.5).cos() * len * 0.4, end.1 + (a + 0.5).sin() * len * 0.4);
                        d.line((lay.dx(end.0), lay.dy(end.1)), (lay.dx(fork.0), lay.dy(fork.1)), 1.0 * u, bark, 0.9);
                    }
                    if sc.snow > 0.1 || sc.frost > 0.4 {
                        let line = to_px(lay, &curve(limb, 4).iter().map(|p| (p.0, p.1 - 3.0)).collect::<Vec<P>>());
                        d.path(&line, 2.0 * u, snow_col(sc), 0.8);
                    }
                }
            }
        }
        // lanterns: three on any night, all of them when the week is over
        let dusk = smooth(2.0, -4.0, sc.el_eff);
        let tints = [AMBER, ROSE, WARM[0], SAGE, WARM[1], BLUE, [200.0, 160.0, 236.0], AMBER, ROSE];
        for (k, (x, y)) in LANTERNS.into_iter().enumerate() {
            let (px, py) = (lay.dx(x), lay.dy(y));
            d.line((px, py - lay.d(22.0)), (px, py - 3.0 * u), (0.8 * u).max(0.8), bark, 0.9);
            let lit = dusk > 0.1 && (sc.festoon || k % 3 == 1);
            let paper = mix(shade(near, -0.1), tints[k], 0.30 + 0.40 * sc.day);
            if lit {
                d.glow(px, py, 12.0 * u, 12.0 * u, mix(tints[k], WARM[1], 0.5), 0.6 * dusk, 2, Blend::Screen);
                d.ellipse(px, py, 2.8 * u, 3.6 * u, mix(tints[k], WHITE, 0.3), 1.0);
                bloom.light(px - 4.0 * u, py - 5.0 * u, px + 4.0 * u, py + 5.0 * u, mix(tints[k], WARM[1], 0.5), 0.8 * dusk * sc.p.bloom);
                out.twinkle.push((px, py, tints[k], 9));
            } else {
                d.ellipse(px, py, 2.8 * u, 3.6 * u, paper, 1.0);
            }
        }
    }

    /// The standing stones on the bank, and the runes cut in them.
    fn stones(&self, d: &mut Draw<Canvas>, sc: &Scene, near: Rgb, light: &Light, bloom: &mut Bloom, out: &mut Anchors) {
        let lay = &self.lay;
        let u = lay.u;
        let stone = mix(sc.layers[3], [150.0, 148.0, 170.0], 0.14 + 0.34 * sc.day);
        for (k, (x, tall, wide, lean)) in STONES.into_iter().enumerate() {
            let foot = height_at(&self.bank, x) + 5.0;
            let at = |a: f32, b: f32| (lay.dx(x + a + lean * b / tall * 3.0), lay.dy(foot - b));
            let (h, w) = (tall, wide / 2.0);
            let body = [at(-w, 0.0), at(-w + 1.5, h * 0.62), at(-w * 0.5, h), at(w * 0.45, h - 3.0), at(w - 1.0, h * 0.56), at(w, 0.0)];
            d.poly(&body, stone, 1.0);
            let side = if light.x < lay.dx(x) { -1.0 } else { 1.0 };
            d.poly(&[at(-side * w, 0.0), at(-side * (w - 1.5), h * 0.6), at(-side * w * 0.5, h - 1.5), at(-side * w * 0.1, h * 0.5), at(-side * w * 0.3, 0.0)], shade(stone, -0.24), 0.5);
            if light.k > 0.03 {
                d.line(at(side * w, 0.0), at(side * (w - 1.0), h * 0.58), 1.6 * u, mix(stone, light.col, 0.65), (0.9 * light.k).min(0.8));
            }
            // moss at its foot
            d.ellipse(lay.dx(x), lay.dy(foot), lay.d(w * 1.5), lay.d(4.0), near, 1.0);
            let on = k < sc.machine.containers as usize;
            let ink = if on { mix(BLUE, WHITE, 0.45) } else { shade(stone, -0.30) };
            let (box_w, box_h, lift) = (wide * 0.56, tall * 0.44, tall * 0.26);
            for stroke in RUNES[k] {
                let line: Vec<P> = stroke.iter().map(|q| at(q.0 * box_w, lift + q.1 * box_h)).collect();
                d.path(&line, if on { 1.9 * u } else { 1.5 * u }, ink, if on { 1.0 } else { 0.7 });
            }
            if on {
                let (cx, cy) = at(0.0, lift + box_h * 0.5);
                let strength = 0.45 + 0.55 * sc.dark;
                d.glow(cx, cy, lay.d(wide * 0.9), lay.d(tall * 0.5), BLUE, 0.34 * strength, 2, Blend::Screen);
                bloom.light(cx - lay.d(box_w), cy - lay.d(box_h * 0.6), cx + lay.d(box_w), cy + lay.d(box_h * 0.6), BLUE, 0.42 * strength * sc.p.bloom.max(0.6));
                out.breath.push(Breath { x: cx, y: cy, rx: lay.d(wide * 0.8), ry: lay.d(tall * 0.42), col: BLUE, strength: 0.6 * strength, low: false, behind: None });
            }
        }
    }
}

fn show_falls(v: f64) -> String {
    let word = if v < 0.02 {
        "frozen"
    } else if v < 0.3 {
        "a trickle"
    } else if v < 0.7 {
        "running"
    } else {
        "in spate"
    };
    format!("{:.2}  {}", v, word)
}

fn show_week(v: f64) -> String {
    let day = (v.floor() as usize).min(6);
    format!("{} {:02}:00", &DAYS[day][..3], ((v - day as f64) * 24.0) as u32)
}

const KNOBS: &[Knob] = &[
    Knob { key: "falls", label: "the falls", lo: 0.0, hi: 1.0, step: 0.05, big: 0.25, show: show_falls },
    Knob { key: "week", label: "little moon", lo: 0.0, hi: 7.0, step: 0.125, big: 1.0, show: show_week },
];

impl View for Vale {
    fn id(&self) -> &'static str {
        "vale"
    }

    fn about(&self) -> &'static str {
        "a valley of hanging rocks, a wizard's tower and a great tree"
    }

    fn horizon(&self) -> f64 {
        HORIZON
    }

    fn knobs(&self) -> &'static [Knob] {
        KNOBS
    }

    fn real(&self, key: &str, sc: &Scene) -> f64 {
        match key {
            "falls" => flow(sc) as f64,
            "week" => sc.dow as f64 + sc.t / 24.0,
            _ => 0.0,
        }
    }

    fn stage(&self, _seed: u64, w: usize, h: usize) -> Box<dyn Stage> {
        Box::new(Dale::new(w, h))
    }

    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)> {
        let p = week(sc);
        let place = if p < 0.2 {
            "low in the left of the sky"
        } else if p < 0.42 {
            "climbing on the left"
        } else if p < 0.58 {
            "at the top of its week"
        } else if p < 0.8 {
            "going down on the right"
        } else {
            "low in the right of the sky"
        };
        let moon = format!("the little moon {}: {}", place, DAYS[sc.dow]);
        let run = flow(sc);
        let mut vale = vec![if run <= 0.0 {
            "the falls frozen".to_string()
        } else if run < 0.3 {
            "the falls down to a trickle".to_string()
        } else if run < 0.7 {
            "the falls running".to_string()
        } else {
            "the falls in spate".to_string()
        }];
        let since = sc.ts.rem_euclid(3600.0);
        for (flag, text) in [
            (since < CROSSING && sc.fog < 0.7 && !sc.storm, "the sky-ship crossing"),
            (sc.festoon, "every lantern in the tree lit"),
            (!sc.festoon && sc.el_eff < -1.0, "three lanterns in the tree"),
            (awake(sc) > 0.5, "lights in the hamlet"),
            (sc.season == 0, "the tree bare"),
            (sc.season == 1, "the tree in blossom"),
            (sc.season == 3, "the tree turned"),
            (sc.temp < -3.0, "the lake frozen"),
            (sc.snow > 0.2 || sc.temp < 2.0, "snow far down the peaks"),
            (sc.mist > 0.25, "mist on the lake"),
            (sc.smoke > 0.08, "smoke from the hamlet"),
            (!sc.birds.is_empty(), "birds crossing"),
        ] {
            if flag {
                vale.push(text.to_string());
            }
        }
        if sc.cat_out {
            vale.push(if sc.cat_asleep { "the cat asleep under the tree" } else { "the cat about" }.to_string());
        }
        let house = Yours { lamp: "the lamp in the tower's top room", containers: ("rune lit blue on the stones", "runes lit blue on the stones"), smoke: "violet smoke from the flue" };
        vec![("week", moon), ("vale", vale.join(", ")), ("yours", yours(sc, &house))]
    }
}
