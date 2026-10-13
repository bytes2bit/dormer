//! Sun, moon, planets, stars and weather codes: what the sky is doing over one place.
//!
//! Low-precision formulas throughout: good to a fraction of a degree, which
//! is a few pixels here.

use std::f64::consts::PI;

use crate::rng::PyRng;

/// Where the sun meets the far roofs, as a fraction of the height, in the view the sky was first
/// drawn for. A view may put its horizon elsewhere; the arc and the dome stretch to match.
pub const HORIZON: f64 = 0.770;
/// How far above that the highest summer sun climbs.
pub const ARC: f64 = 0.30;
/// How far above the horizon the zenith sits, for the stars.
pub const DOME: f64 = 0.74;
/// Degrees of compass across the whole width.
pub const SPAN: f64 = 250.0;

#[derive(Clone, Copy, Default, Debug)]
pub struct Body {
    pub el: f64,
    pub az: f64,
    /// hour angle, radians
    pub ha: f64,
    /// hour angle at which it sets
    pub h0: f64,
    /// place on the picture, as fractions of its width and height
    pub x: f64,
    pub y: f64,
    pub rise: f64,
    pub set: f64,
    pub ra: f64,
    pub dec: f64,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Moon {
    pub body: Body,
    pub lit: f64,
    pub waxing: bool,
    pub elong: f64,
}

#[derive(Clone, Copy, Debug)]
pub struct Planet {
    pub name: &'static str,
    pub body: Body,
    pub mag: f64,
    /// degrees from the sun
    pub elong: f64,
}

pub struct Ephemeris {
    pub sun: Body,
    pub moon: Moon,
    pub planets: Vec<Planet>,
    /// local sidereal time, radians
    pub lst: f64,
    pub lat: f64,
    /// where this view's horizon lies, as a fraction of the height
    pub horizon: f64,
}

fn rad(d: f64) -> f64 {
    d * PI / 180.0
}

fn sn(d: f64) -> f64 {
    rad(d).sin()
}

fn wrap(a: f64) -> f64 {
    (a + PI).rem_euclid(2.0 * PI) - PI
}

/// Heliocentric ecliptic position (AU) from mean elements at J2000 and their change per century.
fn orbit(e: &[f64; 12], t: f64) -> [f64; 3] {
    let a = e[0] + e[6] * t;
    let ecc = e[1] + e[7] * t;
    let inc = rad(e[2] + e[8] * t);
    let l = e[3] + e[9] * t;
    let peri = e[4] + e[10] * t;
    let node = e[5] + e[11] * t;
    let m = rad((l - peri).rem_euclid(360.0));
    let mut big = m;
    for _ in 0..6 {
        big -= (big - ecc * big.sin() - m) / (1.0 - ecc * big.cos());
    }
    let (xp, yp) = (a * (big.cos() - ecc), a * (1.0 - ecc * ecc).sqrt() * big.sin());
    let (w, o) = (rad(peri - node), rad(node));
    let (cw, sw, co, so, ci, si) = (w.cos(), w.sin(), o.cos(), o.sin(), inc.cos(), inc.sin());
    [
        (cw * co - sw * so * ci) * xp + (-sw * co - cw * so * ci) * yp,
        (cw * so + sw * co * ci) * xp + (-sw * so + cw * co * ci) * yp,
        (sw * si) * xp + (cw * si) * yp,
    ]
}

const EARTH: [f64; 12] = [
    1.00000261, 0.01671123, -0.00001531, 100.46457166, 102.93768193, 0.0, 0.00000562, -0.00004392, -0.01294668,
    35999.37244981, 0.32327364, 0.0,
];
/// name, elements, magnitude at one AU from sun and earth
const PLANETS: [(&str, [f64; 12], f64); 4] = [
    (
        "Venus",
        [
            0.72333566, 0.00677672, 3.39467605, 181.97909950, 131.60246718, 76.67984255, 0.00000390, -0.00004107,
            -0.00078890, 58517.81538729, 0.00268329, -0.27769418,
        ],
        -4.40,
    ),
    (
        "Mars",
        [
            1.52371034, 0.09339410, 1.84969142, -4.55343205, -23.94362959, 49.55953891, 0.00001847, 0.00007882,
            -0.00813131, 19140.30268499, 0.44441088, -0.29257343,
        ],
        -1.52,
    ),
    (
        "Jupiter",
        [
            5.20288700, 0.04838624, 1.30439695, 34.39644051, 14.72847983, 100.47390909, -0.00011607, -0.00013253,
            -0.00183714, 3034.74612775, 0.21252668, 0.20469106,
        ],
        -9.40,
    ),
    (
        "Saturn",
        [
            9.53667594, 0.05386179, 2.48599187, 49.95424423, 92.59887831, 113.66242448, -0.00125060, -0.00050991,
            0.00193609, 1222.49362201, -0.41897216, -0.28867794,
        ],
        -8.88,
    ),
];

pub fn ephemeris(ts: f64, lat_deg: f64, lon_deg: f64, horizon: f64) -> Ephemeris {
    let stretch = horizon / HORIZON;
    let d = ts / 86400.0 - 10957.5; // days since J2000.0
    let g = rad((357.529 + 0.98560028 * d).rem_euclid(360.0));
    let lam_s = (280.459 + 0.98564736 * d + 1.915 * g.sin() + 0.020 * (2.0 * g).sin()).rem_euclid(360.0);
    let eps = rad(23.439 - 0.00000036 * d);
    let ls = rad(lam_s);
    let s_ra = (eps.cos() * ls.sin()).atan2(ls.cos());
    let s_dec = (eps.sin() * ls.sin()).asin();
    let t = d / 36525.0;
    let lam_m = (218.32 + 481267.881 * t + 6.29 * sn(135.0 + 477198.87 * t) - 1.27 * sn(259.3 - 413335.36 * t)
        + 0.66 * sn(235.7 + 890534.22 * t)
        + 0.21 * sn(269.9 + 954397.74 * t)
        - 0.19 * sn(357.5 + 35999.05 * t)
        - 0.11 * sn(186.5 + 966404.03 * t))
        .rem_euclid(360.0);
    let beta = rad(5.13 * sn(93.3 + 483202.02 * t) + 0.28 * sn(228.2 + 960400.89 * t)
        - 0.28 * sn(318.3 + 6003.15 * t)
        - 0.17 * sn(217.6 - 407332.21 * t));
    let lm = rad(lam_m);
    let equatorial = |lon: f64, lat: f64| -> (f64, f64) {
        (
            (eps.cos() * lat.cos() * lon.sin() - eps.sin() * lat.sin()).atan2(lat.cos() * lon.cos()),
            (eps.sin() * lat.cos() * lon.sin() + eps.cos() * lat.sin()).asin(),
        )
    };
    let (m_ra, m_dec) = equatorial(lm, beta);
    let lst = rad((280.46061837 + 360.98564736629 * d).rem_euclid(360.0)) + rad(lon_deg);
    let lat = rad(lat_deg);
    let side = if lat_deg >= 0.0 { 1.0 } else { -1.0 }; // south of the equator the sun crosses the north

    let body = |ra: f64, dec: f64, dip: f64| -> Body {
        let ha = wrap(lst - ra);
        let el = (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos()).asin();
        let az = (-ha.sin()).atan2(dec.tan() * lat.cos() - lat.sin() * ha.cos()).rem_euclid(2.0 * PI);
        let c = (rad(dip).sin() - lat.sin() * dec.sin()) / (lat.cos() * dec.cos());
        let h0 = c.clamp(-1.0, 1.0).acos().max(0.35);
        let reference = rad((90.0 - lat_deg.abs() + 23.44).min(89.0)).sin();
        Body {
            el: el.to_degrees(),
            az: az.to_degrees(),
            ha,
            h0,
            x: 0.5 + side * (ha / h0) * 0.36,
            y: horizon - ARC * stretch * el.sin() / reference,
            rise: ts + (-h0 - ha) / (2.0 * PI) * 86400.0,
            set: ts + (h0 - ha) / (2.0 * PI) * 86400.0,
            ra,
            dec,
        }
    };
    let sun = body(s_ra, s_dec, -0.833);
    let elong = (lam_m - lam_s).rem_euclid(360.0);
    let moon = Moon { body: body(m_ra, m_dec, 0.125), lit: (1.0 - rad(elong).cos()) / 2.0, waxing: elong < 180.0, elong };

    let earth = orbit(&EARTH, t);
    let mut planets = Vec::new();
    for (name, elements, h) in PLANETS.iter() {
        let p = orbit(elements, t);
        let (x, y, z) = (p[0] - earth[0], p[1] - earth[1], p[2] - earth[2]);
        let (r, delta) = ((p[0] * p[0] + p[1] * p[1] + p[2] * p[2]).sqrt(), (x * x + y * y + z * z).sqrt());
        let (lon, blat) = (y.atan2(x), (z / delta).asin());
        let (ra, dec) = equatorial(lon, blat);
        let apart = wrap(lon - ls).to_degrees().abs();
        let mut mag = h + 5.0 * (r * delta).log10();
        if *name == "Venus" {
            // a thin crescent near the sun is fainter than its distance alone would say
            let phase = ((r * r + delta * delta - 1.0) / (2.0 * r * delta)).clamp(-1.0, 1.0).acos().to_degrees();
            mag += 0.0009 * phase + 0.000239 * phase * phase - 0.00000065 * phase * phase * phase;
        }
        planets.push(Planet { name, body: body(ra, dec, -0.57), mag, elong: apart });
    }
    Ephemeris { sun, moon, planets, lst, lat, horizon }
}

/// Where a fixed star is: (x, y) on the picture as fractions, and its elevation in degrees.
/// Stars use the whole dome, compass bearing across and height up, so constellations keep their shape.
pub fn star_at(eph: &Ephemeris, ra: f64, dec: f64) -> (f64, f64, f64) {
    let ha = wrap(eph.lst - ra);
    let lat = eph.lat;
    let el = (lat.sin() * dec.sin() + lat.cos() * dec.cos() * ha.cos()).asin();
    let az = (-ha.sin()).atan2(dec.tan() * lat.cos() - lat.sin() * ha.cos());
    let off = if lat >= 0.0 { wrap(az - PI) } else { wrap(az) };
    (0.5 + off.to_degrees() / SPAN, eph.horizon - el.to_degrees() / 90.0 * DOME * eph.horizon / HORIZON, el.to_degrees())
}

/// The reverse: right ascension and declination of a point on the picture.
pub fn sky_at(eph: &Ephemeris, x: f64, y: f64) -> (f64, f64) {
    let lat = eph.lat;
    let off = rad((x - 0.5) * SPAN);
    let az = if lat >= 0.0 { off + PI } else { off };
    let el = rad(((eph.horizon - y) / (DOME * eph.horizon / HORIZON) * 90.0).clamp(-10.0, 90.0));
    let dec = (lat.sin() * el.sin() + lat.cos() * el.cos() * az.cos()).clamp(-1.0, 1.0).asin();
    let ha = (-az.sin() * el.cos()).atan2(el.sin() * lat.cos() - el.cos() * lat.sin() * az.cos());
    (eph.lst - ha, dec)
}

/// Galactic longitude and latitude, radians: where the Milky Way lies.
pub fn galactic(ra: f64, dec: f64) -> (f64, f64) {
    let (pole_ra, pole_dec, node) = (rad(192.8595), rad(27.1284), rad(122.932));
    let b = (dec.sin() * pole_dec.sin() + dec.cos() * pole_dec.cos() * (ra - pole_ra).cos()).clamp(-1.0, 1.0).asin();
    let l = node
        - (dec.cos() * (ra - pole_ra).sin()).atan2(dec.sin() * pole_dec.cos() - dec.cos() * pole_dec.sin() * (ra - pole_ra).cos());
    (wrap(l), b)
}

pub struct Star {
    pub ra: f64,
    pub dec: f64,
    pub mag: f32,
    /// -1 blue-white … 0 white … 1 orange
    pub tint: f32,
    pub name: &'static str,
}

/// The brightest stars: name, right ascension (hours), declination (degrees), magnitude, tint.
const BRIGHT: &[(&str, f64, f64, f32, f32)] = &[
    ("Sirius", 6.752, -16.72, -1.46, -0.5),
    ("Canopus", 6.399, -52.70, -0.74, 0.0),
    ("Arcturus", 14.261, 19.18, -0.05, 0.9),
    ("Vega", 18.616, 38.78, 0.03, -0.5),
    ("Capella", 5.278, 46.00, 0.08, 0.4),
    ("Rigel", 5.242, -8.20, 0.13, -0.8),
    ("Procyon", 7.655, 5.22, 0.34, 0.1),
    ("Betelgeuse", 5.919, 7.41, 0.42, 1.0),
    ("Achernar", 1.629, -57.24, 0.46, -0.7),
    ("Hadar", 14.064, -60.37, 0.61, -0.8),
    ("Altair", 19.846, 8.87, 0.76, -0.2),
    ("Acrux", 12.443, -63.10, 0.76, -0.8),
    ("Aldebaran", 4.599, 16.51, 0.86, 1.0),
    ("Antares", 16.490, -26.43, 0.96, 1.0),
    ("Spica", 13.420, -11.16, 0.97, -0.9),
    ("Pollux", 7.755, 28.03, 1.14, 0.7),
    ("Fomalhaut", 22.961, -29.62, 1.16, -0.3),
    ("Deneb", 20.690, 45.28, 1.25, -0.4),
    ("Mimosa", 12.795, -59.69, 1.25, -0.9),
    ("Regulus", 10.139, 11.97, 1.35, -0.6),
    ("Adhara", 6.977, -28.97, 1.50, -0.8),
    ("Castor", 7.577, 31.89, 1.58, -0.4),
    ("Shaula", 17.560, -37.10, 1.62, -0.8),
    ("Gacrux", 12.519, -57.11, 1.63, 1.0),
    ("Bellatrix", 5.419, 6.35, 1.64, -0.8),
    ("Elnath", 5.438, 28.61, 1.65, -0.6),
    ("Miaplacidus", 9.220, -69.72, 1.67, -0.3),
    ("Alnilam", 5.604, -1.20, 1.69, -0.8),
    ("Alnair", 22.137, -46.96, 1.74, -0.6),
    ("Alnitak", 5.679, -1.94, 1.77, -0.8),
    ("Alioth", 12.900, 55.96, 1.77, -0.4),
    ("Dubhe", 11.062, 61.75, 1.79, 0.7),
    ("Mirfak", 3.405, 49.86, 1.79, 0.2),
    ("Wezen", 7.140, -26.39, 1.83, 0.3),
    ("Kaus Australis", 18.403, -34.38, 1.85, -0.5),
    ("Sargas", 17.622, -43.00, 1.86, 0.2),
    ("Avior", 8.375, -59.51, 1.86, 0.8),
    ("Alkaid", 13.792, 49.31, 1.86, -0.8),
    ("Menkalinan", 5.992, 44.95, 1.90, -0.4),
    ("Atria", 16.811, -69.03, 1.91, 0.9),
    ("Alhena", 6.629, 16.40, 1.93, -0.4),
    ("Peacock", 20.427, -56.74, 1.94, -0.8),
    ("Alsephina", 8.745, -54.71, 1.96, -0.4),
    ("Mirzam", 6.378, -17.96, 1.98, -0.9),
    ("Alphard", 9.460, -8.66, 1.98, 0.9),
    ("Polaris", 2.530, 89.26, 1.98, 0.3),
    ("Hamal", 2.120, 23.46, 2.00, 0.8),
    ("Diphda", 0.727, -17.99, 2.02, 0.7),
    ("Mizar", 13.399, 54.93, 2.04, -0.4),
    ("Nunki", 18.921, -26.30, 2.05, -0.8),
    ("Mirach", 1.162, 35.62, 2.05, 1.0),
    ("Menkent", 14.111, -36.37, 2.06, 0.7),
    ("Alpheratz", 0.140, 29.09, 2.06, -0.6),
    ("Rasalhague", 17.582, 12.56, 2.07, -0.3),
    ("Kochab", 14.845, 74.16, 2.08, 0.9),
    ("Saiph", 5.796, -9.67, 2.09, -0.8),
    ("Tiaki", 22.711, -46.88, 2.10, 1.0),
    ("Algol", 3.136, 40.96, 2.12, -0.6),
    ("Denebola", 11.818, 14.57, 2.13, -0.3),
    ("Muhlifain", 12.692, -48.96, 2.17, -0.4),
    ("Sadr", 20.370, 40.26, 2.20, 0.3),
    ("Suhail", 9.133, -43.43, 2.21, 0.9),
    ("Mintaka", 5.533, -0.30, 2.23, -0.8),
    ("Schedar", 0.675, 56.54, 2.23, 0.8),
    ("Eltanin", 17.943, 51.49, 2.23, 0.9),
    ("Alphecca", 15.578, 26.71, 2.23, -0.4),
    ("Naos", 8.060, -40.00, 2.25, -0.9),
    ("Aspidiske", 9.285, -59.28, 2.25, 0.0),
    ("Almach", 2.065, 42.33, 2.26, 0.8),
    ("Caph", 0.153, 59.15, 2.27, 0.1),
    ("Larawag", 16.836, -34.29, 2.29, 0.8),
    ("Dschubba", 16.006, -22.62, 2.32, -0.8),
    ("Merak", 11.031, 56.38, 2.37, -0.4),
    ("Izar", 14.750, 27.07, 2.37, 0.7),
    ("Ankaa", 0.438, -42.31, 2.38, 0.8),
    ("Enif", 21.736, 9.88, 2.39, 0.9),
    ("Scheat", 23.063, 28.08, 2.42, 1.0),
    ("Sabik", 17.173, -15.72, 2.43, -0.4),
    ("Phecda", 11.897, 53.69, 2.44, -0.4),
    ("Aludra", 7.401, -29.30, 2.45, -0.8),
    ("Navi", 0.945, 60.72, 2.47, -0.8),
    ("Markab", 23.079, 15.21, 2.48, -0.6),
    ("Aljanah", 20.770, 33.97, 2.48, 0.8),
    ("Acrab", 16.091, -19.81, 2.62, -0.8),
    ("Zubeneschamali", 15.283, -9.38, 2.61, -0.6),
    ("Unukalhai", 15.738, 6.43, 2.63, 0.8),
    ("Sheratan", 1.911, 20.81, 2.64, -0.4),
    ("Kraz", 12.573, -23.40, 2.65, 0.7),
    ("Ruchbah", 1.430, 60.24, 2.68, -0.3),
    ("Meissa", 5.585, 9.93, 3.33, -0.8),
    ("Tarazed", 19.771, 10.61, 2.72, 0.9),
    ("Albireo", 19.512, 27.96, 3.05, 0.8),
];

/// Every star dormer knows: the bright ones by name, then a field of faint ones scattered over the
/// whole sphere, thicker along the Milky Way. They all turn with the real sky.
pub fn stars(seed: u64) -> Vec<Star> {
    let mut out: Vec<Star> = BRIGHT
        .iter()
        .map(|&(name, ra, dec, mag, tint)| Star { ra: rad(ra * 15.0), dec: rad(dec), mag, tint, name })
        .collect();
    let mut r = PyRng::new(seed * 31 + 1);
    while out.len() < 2300 {
        let ra = r.uniform(0.0, 2.0 * PI);
        let dec = r.uniform(-1.0, 1.0).asin();
        let (keep, m, tint) = (r.random(), r.random(), r.random());
        let (_, b) = galactic(ra, dec);
        let band = (-(b.to_degrees() / 13.0).powi(2)).exp();
        if keep > 0.30 + 0.70 * band {
            continue;
        }
        // many faint ones, few bright: magnitudes 2.8 to 6.2
        let mag = 2.8 + 3.4 * (1.0 - (1.0 - m).powf(2.6)).powf(0.55);
        let tint = if tint < 0.12 { 0.8 } else if tint > 0.88 { -0.8 } else { 0.0 };
        out.push(Star { ra, dec, mag: mag as f32, tint, name: "" });
    }
    out
}

/// The nights when the earth crosses a comet's trail: (month, day, name, how many times the usual count).
pub const SHOWERS: [(i32, i32, &str, f32); 8] = [
    (1, 3, "Quadrantids", 7.0),
    (4, 22, "Lyrids", 3.0),
    (5, 6, "Eta Aquariids", 4.0),
    (8, 12, "Perseids", 8.0),
    (10, 21, "Orionids", 3.0),
    (11, 17, "Leonids", 3.0),
    (12, 14, "Geminids", 9.0),
    (12, 22, "Ursids", 2.0),
];

// ── weather ──────────────────────────────────────────────────────────────────

#[derive(Clone, Debug)]
pub struct Weather {
    pub kind: String,
    pub cover: f64,
    pub rain: f64,
    pub snow: f64,
    pub fog: f64,
    pub storm: bool,
    pub temp: f64,
    pub wind: f64,
    pub wdir: f64,
    pub humidity: f64,
    /// was it really measured
    pub known: bool,
    pub source: String,
    /// "", "pretend" or "dev"
    pub note: &'static str,
    /// hours until rain is forecast, when that is soon
    pub rain_in: Option<f64>,
}

/// Open-Meteo's weather code as the handful of things this picture can show.
pub fn from_wmo(code: i64) -> Weather {
    let (kind, cover, rain, snow, fog, storm) = match code {
        1 => ("mostly clear", 0.22, 0.0, 0.0, 0.0, false),
        2 => ("partly cloudy", 0.5, 0.0, 0.0, 0.0, false),
        3 => ("overcast", 0.95, 0.0, 0.0, 0.0, false),
        45 | 48 => ("fog", 0.75, 0.0, 0.0, 1.0, false),
        51 => ("drizzle", 0.92, 0.2, 0.0, 0.0, false),
        53 => ("drizzle", 0.92, 0.3, 0.0, 0.0, false),
        55 | 56 | 57 => ("drizzle", 0.92, 0.4, 0.0, 0.0, false),
        61 | 66 | 80 => ("light rain", 1.0, 0.55, 0.0, 0.0, false),
        63 | 67 | 81 => ("rain", 1.0, 0.75, 0.0, 0.0, false),
        65 | 82 => ("heavy rain", 1.0, 1.0, 0.0, 0.0, false),
        71 => ("snow", 0.95, 0.0, 0.4, 0.0, false),
        77 => ("snow", 0.95, 0.0, 0.3, 0.0, false),
        85 => ("snow", 0.95, 0.0, 0.5, 0.0, false),
        73 => ("snow", 0.95, 0.0, 0.7, 0.0, false),
        75 | 86 => ("snow", 0.95, 0.0, 1.0, 0.0, false),
        95 | 96 | 99 => ("thunderstorm", 1.0, 0.9, 0.0, 0.0, true),
        _ => ("clear", 0.05, 0.0, 0.0, 0.0, false),
    };
    Weather {
        kind: kind.into(),
        cover,
        rain,
        snow,
        fog,
        storm,
        temp: 18.0,
        wind: 8.0,
        wdir: 270.0,
        humidity: 60.0,
        known: true,
        source: "none".into(),
        note: "",
        rain_in: None,
    }
}

pub fn is_wet(code: i64) -> bool {
    matches!(code, 51..=67 | 80..=82 | 95..=99)
}

pub const PRESETS: [(&str, i64); 10] = [
    ("clear", 0),
    ("fair", 1),
    ("partly", 2),
    ("overcast", 3),
    ("fog", 45),
    ("drizzle", 53),
    ("rain", 63),
    ("heavy", 65),
    ("snow", 73),
    ("storm", 95),
];

/// A named kind of weather, keeping the wind and temperature of `like`.
pub fn preset(name: &str, like: &Weather) -> Option<Weather> {
    let code = PRESETS.iter().find(|p| p.0 == name)?.1;
    let mut w = from_wmo(code);
    w.temp = like.temp;
    w.wind = like.wind;
    w.wdir = like.wdir;
    w.humidity = like.humidity;
    w.source = like.source.clone();
    Some(w)
}

/// A name for weather given as numbers (the dev panel's sliders).
pub fn describe(w: &Weather) -> &'static str {
    if w.storm {
        "thunderstorm"
    } else if w.snow > 0.05 {
        "snow"
    } else if w.rain > 0.05 {
        if w.rain < 0.45 {
            "drizzle"
        } else if w.rain < 0.9 {
            "rain"
        } else {
            "heavy rain"
        }
    } else if w.fog > 0.3 {
        "fog"
    } else if w.cover < 0.15 {
        "clear"
    } else if w.cover < 0.38 {
        "mostly clear"
    } else if w.cover < 0.75 {
        "partly cloudy"
    } else {
        "overcast"
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_sky_over_tangier() {
        // 13 October 2026, 12:20 UTC: close to local noon
        let e = ephemeris(1791894000.0, 35.7758, -5.7909, HORIZON);
        assert!((44.0..48.0).contains(&e.sun.el), "sun {}", e.sun.el);
        assert!((0.45..0.55).contains(&e.sun.x), "sun x {}", e.sun.x);
        assert!(e.moon.waxing && (0.03..0.20).contains(&e.moon.lit), "moon {}", e.moon.lit);
        // Saturn was at opposition at the start of the month; Venus is days from passing the sun
        let far = |name: &str| e.planets.iter().find(|p| p.name == name).unwrap().elong;
        assert!(far("Saturn") > 160.0, "Saturn {}", far("Saturn"));
        assert!(far("Venus") < 25.0, "Venus {}", far("Venus"));
    }

    #[test]
    fn the_dome_maps_there_and_back() {
        for horizon in [HORIZON, 0.70] {
            let e = ephemeris(1791928800.0, 35.7758, -5.7909, horizon);
            let (ra, dec) = sky_at(&e, 0.31, 0.42);
            let (x, y, _) = star_at(&e, ra, dec);
            assert!((x - 0.31).abs() < 1e-6 && (y - 0.42).abs() < 1e-6, "{} {}", x, y);
        }
    }
}
