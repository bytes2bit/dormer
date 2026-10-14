//! One moment, worked out: everything the painter and the live layer need to know.

use crate::color::{gauss, key_c, key_f, key_row, mix, palette, smooth, Palette, Rgb};
use crate::rng::PyRng;
use crate::sky::{ephemeris, Ephemeris, Weather, SHOWERS};
use crate::tm::{self, Local};

#[derive(Clone, Debug)]
pub struct Place {
    pub lat: f64,
    pub lon: f64,
    pub name: String,
}

/// What this computer is doing, as far as the picture cares.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Machine {
    /// AI coding agents running: the lamp in the roof room
    pub agents: u32,
    /// running containers: the blue windows under it
    pub containers: u32,
    /// how hard the processor is working, 0 to 1: smoke from the house chimney
    pub load: f32,
    /// charge as 0 to 1 while running on battery
    pub battery: Option<f32>,
    /// sound is playing: music drifts out of the roof room
    pub audio: bool,
}

/// How often the live layer's visitors turn up, as multiples of the usual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rates {
    pub birds: f32,
    pub meteors: f32,
    pub planes: f32,
}

impl Default for Rates {
    fn default() -> Self {
        Rates { birds: 1.0, meteors: 1.0, planes: 1.0 }
    }
}

pub struct Scene {
    pub ts: f64,
    pub mode: String,
    pub p: &'static Palette,
    pub wx: Weather,
    pub place: Place,
    pub machine: Machine,
    pub rates: Rates,
    pub lt: Local,
    /// local time as hours, 0 to 24
    pub t: f64,
    pub dow: usize,
    /// the day of the week the households keep: New Year's Eve is a Saturday night whatever the calendar says
    pub home_dow: usize,
    pub day_id: i64,
    pub slot: i64,
    pub eph: Ephemeris,
    /// the animated things are left for the live layer to draw
    pub live: bool,

    pub am: bool,
    pub cover: f32,
    pub rain: f32,
    pub snow: f32,
    pub fog: f32,
    pub storm: bool,
    pub temp: f32,
    pub wind: f32,
    /// -1.6 to 1.6: how hard, and which way across the picture, the wind blows
    pub wind_x: f32,
    pub drift: f32,
    pub gloom: f32,
    /// how bright it feels down in the street, as a sun elevation
    pub el_eff: f32,
    pub day: f32,
    pub dark: f32,
    pub night: f32,
    pub twilight: f32,
    pub veil: f32,
    pub mist: f32,
    pub frost: f32,
    /// rain on the way: a bank of cloud builds on the windward horizon
    pub front: f32,
    pub moonlight: f32,

    pub sky: [Rgb; 7],
    pub layers: [Rgb; 5],
    pub haze: Rgb,
    pub haze_alpha: [f32; 5],
    pub cloud: Rgb,
    pub cloud_lit: Rgb,
    pub cloud_alpha: f32,
    pub glow: Rgb,
    pub glow_alpha: f32,
    pub sun_col: Rgb,
    pub sun_vis: f32,

    pub busy: f32,
    pub traffic: f32,
    pub sleepers: usize,
    pub laundry: usize,
    pub kite: bool,
    pub balloon: bool,
    pub festoon: bool,
    pub crane_works: bool,
    pub fireworks: bool,
    pub cat_asleep: bool,
    pub cat_out: bool,
    pub smoke: f32,
    /// 0 winter … 3 autumn
    pub season: usize,
    pub shower: Option<(&'static str, f32)>,
    /// x, y, way, slope
    pub planes: Vec<[f32; 4]>,
    /// x, y, way
    pub birds: Vec<[f32; 3]>,
    /// filled in by the painter
    pub lit_share: f32,
    /// how high the sea stands, -1 at the lowest ebb to 1 at the top of a spring tide. Worked out
    /// from where the moon is, which is roughly right for an open coast and no tide table.
    pub tide: f32,
    /// what the dev panel is holding this view's own knobs to
    pub knobs: Vec<(String, f64)>,
    /// the view has no air: nothing falls, flies or blows there, and the weather is news from home
    pub void: bool,
}

impl Scene {
    /// A knob of the view, if the dev panel is holding it.
    pub fn knob(&self, key: &str) -> Option<f32> {
        self.knobs.iter().find(|k| k.0 == key).map(|k| k.1 as f32)
    }
}

pub struct Inputs<'a> {
    pub ts: f64,
    pub wx: &'a Weather,
    pub mode: &'a str,
    pub machine: Machine,
    pub place: &'a Place,
    pub drift: Option<f32>,
    /// the bar's colour
    pub top: Option<Rgb>,
    pub rates: Rates,
    /// where the view puts its horizon
    pub horizon: f64,
    pub knobs: &'a [(String, f64)],
    pub void: bool,
}

/// Cloud drift is kept below this, so it never loses precision.
pub const DRIFT_WRAP: f64 = 510.0;

/// How hard, and which way across the picture, the wind blows: -1.6 to 1.6.
/// Looking south, west is on the right: a wind from the west carries things leftwards.
pub fn wind_x(wx: &Weather, place: &Place) -> f32 {
    let side = if place.lat >= 0.0 { 1.0 } else { -1.0 };
    side * (wx.wdir as f32).to_radians().sin() * (wx.wind as f32 / 16.0).min(1.6)
}

pub fn scene(inp: Inputs) -> Scene {
    let Inputs { ts, wx, mode, machine, place, drift, top, rates, horizon, knobs, void } = inp;
    let p = palette(mode);
    let lt = tm::local(ts);
    let t = lt.hour as f64 + lt.min as f64 / 60.0 + lt.sec as f64 / 3600.0;
    let (dow, month, mday) = (lt.dow, lt.month, lt.mday);
    let day_id = lt.year as i64 * 1000 + lt.yday as i64;
    let eph = ephemeris(ts, place.lat, place.lon, horizon);
    let el = eph.sun.el as f32;
    let am = eph.sun.ha < 0.0;

    let (cover, rain, snow, fog, storm) = (wx.cover as f32, wx.rain as f32, wx.snow as f32, wx.fog as f32, wx.storm);
    let (temp, wind) = (wx.temp as f32, wx.wind as f32);
    let wind_x = wind_x(wx, place);
    let drift = drift.unwrap_or((ts / 3600.0 * wind_x as f64 * 0.19).rem_euclid(DRIFT_WRAP) as f32);
    let overcast = smooth(0.45, 1.0, cover);
    let gloom = (overcast * 0.30).max(rain * 0.75).max(fog * 0.5).max(snow * 0.4).max(if storm { 0.92 } else { 0.0 });
    let el_eff = el * (1.0 - 0.85 * gloom) - 9.0 * gloom;
    let day = smooth(-6.0, 14.0, el);
    let dark = 1.0 - smooth(-7.0, 3.0, el_eff);
    let night = smooth(-5.0, -15.0, el) * (1.0 - overcast) * (1.0 - fog);
    let twilight = (1.0 - (el + 1.0).abs() / 11.0).max(0.0);

    let mut sky = key_row(&p.sky, el);
    if let Some(top) = top {
        sky[0] = top;
    }
    if am {
        // dawn is rosier than dusk
        for (i, wgt) in [(3, 0.10), (4, 0.30), (5, 0.38), (6, 0.30)] {
            sky[i] = mix(sky[i], crate::color::hex(p.rose), wgt * twilight);
        }
    }
    let hx = crate::color::hex;
    let mut grey = mix(hx(p.grey[0]), hx(p.grey[1]), day);
    if storm {
        grey = mix(grey, hx(p.storm), 0.7);
    }
    let veil = (overcast * 0.78).max(rain * 0.85).max(fog * 0.7).max(snow * 0.6).max(if storm { 0.9 } else { 0.0 });
    for c in sky.iter_mut().skip(1) {
        *c = mix(*c, mix(grey, *c, 0.30), veil);
    }
    let mut layers = key_row(&p.layers, el);
    for c in layers.iter_mut() {
        *c = mix(*c, mix(grey, *c, 0.55), veil * 0.5);
    }
    let haze = mix(key_c(&p.haze, el), grey, veil * 0.7);
    let gain = key_f(&p.haze_gain, el);
    let mut haze_alpha = p.haze_alpha;
    for a in haze_alpha.iter_mut() {
        *a *= gain;
    }
    let dim = if mode == "dark" { 0.5 } else { 0.2 };
    let cloud = mix(key_c(&p.cloud, el), mix(grey, hx(p.storm), dim), gloom * 0.8);
    let cloud_lit = mix(key_c(&p.cloud_lit, el), cloud, (gloom * 1.1).min(0.9));
    let cloud_alpha = key_f(&p.cloud_alpha, el);
    let glow = key_c(&p.glow, el);
    let glow_alpha = p.glow_alpha * key_f(&p.glow_gain, el) * (1.0 - 0.8 * veil);
    let sun_col = mix(hx(p.sun[0]), hx(p.sun[1]), smooth(0.0, 24.0, el));
    let sun_vis = (1.0 - smooth(0.55, 0.92, cover)) * (1.0 - fog * 0.8);

    // the working week, as a number: how much the city has on
    let tf = t as f32;
    let busy = [1.0, 0.8, 0.8, 0.8, 0.9, 0.35, 0.2][dow];
    let traffic =
        busy * (gauss(tf, 8.1, 0.8) + gauss(tf, 18.2, 1.0)) + 0.22 * smooth(18.5, 20.0, tf) * smooth(24.0, 22.5, tf);
    let dry = rain < 0.05 && snow < 0.05 && !storm;
    let sleepers = if dow == 6 && t < 10.75 { 4 } else if dow == 5 && t < 9.0 { 1 } else { 0 };
    let laundry = if dow >= 5 && (9.5..=18.5).contains(&t) && dry && el > 2.0 && fog < 0.5 {
        if dow == 6 { 2 } else { 1 }
    } else {
        0
    };
    let kite = dow >= 5 && (14.0..18.6).contains(&t) && el > 3.0 && (7.0..=42.0).contains(&wind) && dry && fog < 0.5;
    let balloon = dow >= 5 && (7.5..11.5).contains(&t) && el > 2.0 && wind < 22.0 && cover < 0.7 && dry && fog < 0.5;
    // string lights on Friday and Saturday nights, and every night through the second half of December
    let festive = (month == 12 && mday >= 12) || (month == 1 && mday <= 2);
    let festoon = (((dow == 4 || dow == 5 || festive) && t >= 17.0) || ((dow == 5 || dow == 6 || festive) && t < 1.5)) && el_eff < 1.0 && dry;
    let crane_works = dow < 5 && ((8.0..12.0).contains(&t) || (13.0..17.75).contains(&t)) && !storm;
    let fireworks = (month == 12 && mday == 31 && t >= 21.5) || (month == 1 && mday == 1 && t < 1.0);
    let cat_asleep = t >= 22.5 || t < 6.5 || (dow == 6 && t < 10.5);
    let cat_out = rain < 0.2 && snow < 0.2 && !storm;
    let meal = (gauss(tf, 7.6, 0.7) * if dow == 6 { 0.3 } else { 1.0 }).max(gauss(tf, 13.4, 0.8)).max(gauss(tf, 20.3, 0.9));
    let smoke = (((15.0 - temp) / 16.0).max(0.0) + if meal > 0.45 { 0.10 } else { 0.0 }).min(0.75);
    let season = (((month % 12) / 3 + if place.lat >= 0.0 { 0 } else { 2 }) % 4) as usize;

    // Still, damp air near sunrise leaves mist lying between the roofs.
    let calm = 1.0 - smooth(5.0, 16.0, wind);
    let early = smooth(14.0, 4.0, el) * smooth(-30.0, -8.0, el).max(if am { 1.0 } else { 0.35 });
    let mist = (smooth(84.0, 97.0, wx.humidity as f32) * calm * early * (1.0 - fog)).min(1.0) * if dry { 1.0 } else { 0.3 };
    let frost = smooth(1.5, -2.0, temp) * smooth(12.0, 2.0, el) * if snow > 0.3 { 0.0 } else { 1.0 };
    let front = match wx.rain_in {
        Some(h) if rain < 0.05 && !storm => (1.0 - h as f32 / 3.5).clamp(0.0, 1.0),
        _ => 0.0,
    };
    let moonlight = (eph.moon.lit as f32) * smooth(-1.0, 18.0, eph.moon.body.el as f32) * (1.0 - smooth(0.5, 0.95, cover));

    // High water follows the moon across the meridian by an hour or so, twice a day; the range is
    // widest at new and full moon, when the sun pulls the same way.
    let range = 0.72 + 0.28 * (2.0 * (eph.moon.elong as f32).to_radians()).cos();
    let tide = range * (2.0 * (eph.moon.body.ha as f32 - 0.45)).cos();

    // On a handful of nights each year the earth crosses a comet's trail.
    let mut shower = None;
    for &(m, d, name, rate) in SHOWERS.iter() {
        let peak = tm::stamp(lt.year, m, d, 12, 0, 0);
        let off = ((ts - peak) / 86400.0).abs();
        if off < 1.6 {
            shower = Some((name, 1.0 + (rate - 1.0) * (1.0 - (off / 1.6) as f32)));
        }
    }

    // planes: each day has its own timetable, heavier on working days
    let mut planes = Vec::new();
    let mut r = PyRng::new((day_id * 13 + 5) as u64);
    for _ in 0..[46, 30, 30, 30, 36, 18, 10][dow] {
        let start = r.uniform(5.5, 23.5);
        let dur = r.uniform(9.0, 15.0) / 60.0;
        let way = r.pick(&[-1.0f32, 1.0]);
        let y0 = r.uniform(0.08, 0.30);
        let slope = r.uniform(-0.05, 0.05);
        if start <= t && t < start + dur && cover < 0.8 && fog < 0.5 {
            let f = (t - start) / dur;
            let x = if way > 0.0 { -0.12 + 1.24 * f } else { 1.12 - 1.24 * f };
            planes.push([x as f32, (y0 + slope * f) as f32, way, slope as f32]);
        }
    }
    // birds cross at each end of the day
    let mut birds = Vec::new();
    if -3.0 < el && el < 13.0 && dry && fog < 0.5 {
        let way = if am { 1.0 } else { -1.0 };
        let f = ((ts / 300.0 * 0.061).rem_euclid(1.3) - 0.15) as f32;
        birds.push([if way > 0.0 { f } else { 1.0 - f }, 0.50 + 0.07 * (ts / 2100.0).sin() as f32, way]);
    }

    Scene {
        ts,
        mode: mode.to_string(),
        p,
        wx: wx.clone(),
        place: place.clone(),
        machine,
        rates,
        lt,
        t,
        dow,
        home_dow: if (month == 12 && mday == 31 && t >= 12.0) || (month == 1 && mday == 1 && t < 6.0) { 5 } else { dow },
        day_id,
        slot: (ts / 300.0).floor() as i64,
        eph,
        live: false,
        am,
        cover,
        rain,
        snow,
        fog,
        storm,
        temp,
        wind,
        wind_x,
        drift,
        gloom,
        el_eff,
        day,
        dark,
        night,
        twilight,
        veil,
        mist,
        frost,
        front,
        moonlight,
        sky,
        layers,
        haze,
        haze_alpha,
        cloud,
        cloud_lit,
        cloud_alpha,
        glow,
        glow_alpha,
        sun_col,
        sun_vis,
        busy,
        traffic,
        sleepers,
        laundry,
        kite,
        balloon,
        festoon,
        crane_works,
        fireworks,
        cat_asleep,
        cat_out,
        smoke,
        season,
        shower,
        planes,
        birds,
        lit_share: 0.0,
        tide,
        knobs: knobs.to_vec(),
        void,
    }
}
