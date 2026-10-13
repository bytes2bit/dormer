//! Colour, and the few small tools everything else leans on.
//!
//! The tables run night, deep twilight, civil twilight, sun on the horizon,
//! golden hour, day; `KEYS` gives the sun elevation each row belongs to and
//! the `key_*` functions blend between them. The first sky stop of every row
//! is the colour of your bar, so the bar never shows an edge.

pub type Rgb = [f32; 3];

pub const STOPS: [f32; 7] = [0.00, 0.22, 0.46, 0.62, 0.72, 0.79, 1.00];
pub const KEYS: [f32; 6] = [-16.0, -9.0, -3.0, 1.0, 8.0, 22.0];

pub const fn hex(c: u32) -> Rgb {
    [((c >> 16) & 255) as f32, ((c >> 8) & 255) as f32, (c & 255) as f32]
}

/// "#1A2027" or "1a2027".
pub fn parse_hex(s: &str) -> Option<Rgb> {
    let s = s.trim().trim_start_matches('#');
    if s.len() != 6 {
        return None;
    }
    u32::from_str_radix(s, 16).ok().map(hex)
}

#[inline]
pub fn mix(a: Rgb, b: Rgb, t: f32) -> Rgb {
    let t = t.clamp(0.0, 1.0);
    [a[0] + (b[0] - a[0]) * t, a[1] + (b[1] - a[1]) * t, a[2] + (b[2] - a[2]) * t]
}

/// Lighter (k > 0) or darker (k < 0), as a share of the way to white or black.
#[inline]
pub fn shade(c: Rgb, k: f32) -> Rgb {
    if k >= 0.0 {
        mix(c, WHITE, k)
    } else {
        mix(c, [0.0; 3], -k)
    }
}

#[inline]
pub fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// 0 at `a`, 1 at `b`, eased; `a` may be larger than `b`.
#[inline]
pub fn smooth(a: f32, b: f32, x: f32) -> f32 {
    let t = ((x - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

#[inline]
pub fn gauss(x: f32, mu: f32, sig: f32) -> f32 {
    let d = (x - mu) / sig;
    (-d * d / 2.0).exp()
}

fn key_at(el: f32) -> (usize, f32) {
    if el <= KEYS[0] {
        return (0, 0.0);
    }
    for i in 0..KEYS.len() - 1 {
        if el <= KEYS[i + 1] {
            return (i, (el - KEYS[i]) / (KEYS[i + 1] - KEYS[i]));
        }
    }
    (KEYS.len() - 2, 1.0)
}

pub fn key_f(rows: &[f32; 6], el: f32) -> f32 {
    let (i, f) = key_at(el);
    lerp(rows[i], rows[i + 1], f)
}

pub fn key_c(rows: &[u32; 6], el: f32) -> Rgb {
    let (i, f) = key_at(el);
    mix(hex(rows[i]), hex(rows[i + 1]), f)
}

pub fn key_row<const N: usize>(rows: &[[u32; N]; 6], el: f32) -> [Rgb; N] {
    let (i, f) = key_at(el);
    let mut out = [[0.0; 3]; N];
    for k in 0..N {
        out[k] = mix(hex(rows[i][k]), hex(rows[i + 1][k]), f);
    }
    out
}

pub struct Palette {
    pub sky: [[u32; 7]; 6],
    pub rose: u32,
    pub layers: [[u32; 5]; 6],
    pub haze: [u32; 6],
    pub haze_gain: [f32; 6],
    pub haze_alpha: [f32; 5],
    pub cloud: [u32; 6],
    pub cloud_lit: [u32; 6],
    pub cloud_alpha: [f32; 6],
    pub glow: [u32; 6],
    pub glow_gain: [f32; 6],
    pub glow_alpha: f32,
    pub sun: [u32; 2],
    pub grey: [u32; 2],
    pub storm: u32,
    pub star: u32,
    pub moon: u32,
    pub ink: u32,
    pub trail: u32,
    pub rain: u32,
    pub snow: u32,
    pub bird: u32,
    pub tree: [u32; 2],
    pub tree_night: u32,
    pub tree_light: f32,
    pub trunk: u32,
    /// winter, spring, summer, autumn
    pub seasons: [[u32; 3]; 4],
    pub grain: f32,
    /// how much lit windows bloom
    pub bloom: f32,
}

pub static DARK: Palette = Palette {
    sky: [
        [0x1A2027, 0x1A212A, 0x1B2531, 0x1D2A38, 0x20303F, 0x243544, 0x27394A],
        [0x1A2027, 0x1C2531, 0x213245, 0x2D4159, 0x4A4C63, 0x7C5E64, 0x9C7060],
        [0x1A2027, 0x1E2833, 0x293C4D, 0x44525F, 0x7D6862, 0xC08A58, 0xD6A062],
        [0x1A2027, 0x1F2A35, 0x2C4152, 0x4A5A68, 0x8A7468, 0xD39A5C, 0xE3B06A],
        [0x1A2027, 0x223141, 0x335069, 0x5A7B91, 0x9FA39B, 0xDDB98A, 0xEBCB9C],
        [0x1A2027, 0x24394E, 0x2F5878, 0x477EA3, 0x639AB9, 0x82B0C8, 0x98C0D2],
    ],
    rose: 0xC58A9E,
    layers: [
        [0x2B3643, 0x232D38, 0x1C252E, 0x151D24, 0x0F151A],
        [0x3C4856, 0x2D3945, 0x222C35, 0x181F27, 0x10171B],
        [0x566472, 0x3B4854, 0x28333C, 0x1A2329, 0x11181C],
        [0x5B6876, 0x3F4C58, 0x2B363F, 0x1C252C, 0x12191D],
        [0x6F7F8C, 0x4E5E6B, 0x36444F, 0x232F37, 0x162027],
        [0x86A0B2, 0x67849A, 0x4A667C, 0x31495C, 0x203240],
    ],
    haze: [0x2A3A4A, 0x4A4A5E, 0xC98F62, 0xD9A05E, 0xD9B182, 0xA9C8D6],
    haze_gain: [0.5, 0.7, 1.0, 1.0, 0.9, 0.8],
    haze_alpha: [70.0, 55.0, 40.0, 0.0, 0.0],
    cloud: [0x263039, 0x2B313F, 0x4B4654, 0x6C5F66, 0x9DA3AB, 0xA9C2D3],
    cloud_lit: [0x34404C, 0x8A5F66, 0xE9A873, 0xF7C98D, 0xFBE4BF, 0xE6EEF1],
    cloud_alpha: [150.0, 165.0, 185.0, 195.0, 205.0, 210.0],
    glow: [0x2A3A4A, 0xB9785A, 0xE8A968, 0xF4C987, 0xF8DCAE, 0xFFF4DC],
    glow_gain: [0.0, 0.35, 0.85, 1.0, 0.8, 0.5],
    glow_alpha: 150.0,
    sun: [0xF6B458, 0xFFF0CC],
    grey: [0x1D232A, 0x5E6B76],
    storm: 0x232A32,
    star: 0xEBE8DC,
    moon: 0xEDE7D6,
    ink: 0xE6DFCF,
    trail: 0xF2EEE3,
    rain: 0xC9D6DE,
    snow: 0xE9EEF0,
    bird: 0x141A20,
    tree: [0x15221D, 0x1B2B25],
    tree_night: 0x0F1714,
    tree_light: 0.40,
    trunk: 0x1A1F1E,
    seasons: [
        [0x2A3531, 0x313C37, 0x252F2C],
        [0x3A6A4B, 0x447A57, 0x2F5A41],
        [0x2A5637, 0x346542, 0x234A2F],
        [0x8A5630, 0x9A7A36, 0x3A5A40],
    ],
    grain: 2.6,
    bloom: 1.0,
};

pub static LIGHT: Palette = Palette {
    sky: [
        [0xECEFEE, 0xDEE2EA, 0xC6CCDC, 0xAFB7CE, 0xA3ABC6, 0xA0A6C0, 0xA4A8C0],
        [0xECEFEE, 0xDFE3EA, 0xC9CEDD, 0xB6BCD0, 0xB8B3C8, 0xC9AFBA, 0xD7B5AE],
        [0xECEFEE, 0xE2E6EA, 0xD2DAE2, 0xC9CED8, 0xD9C6C6, 0xEEC7A6, 0xF4D0A6],
        [0xECEFEE, 0xE4E9EB, 0xD8E1E6, 0xD6D9DC, 0xE8D3C4, 0xF5D2A8, 0xF8DDB6],
        [0xECEFEE, 0xE3EBEC, 0xDCE6E8, 0xE2E3DC, 0xEFE0CC, 0xF6DFC2, 0xF8E6CC],
        [0xECEFEE, 0xDDEAF0, 0xC9DFEB, 0xBAD6E7, 0xBDD9E8, 0xCBE1EA, 0xD7E8EC],
    ],
    rose: 0xE9B9C0,
    layers: [
        [0xA9B0C6, 0x8C96AE, 0x6F7B96, 0x55617C, 0x404B63],
        [0xB4B6C8, 0x959BAF, 0x767F97, 0x5A657E, 0x444F66],
        [0xC9BFC0, 0xA9A3AC, 0x86899A, 0x646C80, 0x4A5368],
        [0xCFC3BE, 0xADA6AA, 0x898C99, 0x666E80, 0x4B5568],
        [0xCDCBC8, 0xACB2B6, 0x87949C, 0x62717B, 0x485862],
        [0xC9D3D7, 0xA9B8BF, 0x8497A0, 0x5F747E, 0x465A63],
    ],
    haze: [0xC9CDE0, 0xD9C9D6, 0xF6D9BC, 0xFBE3C3, 0xFBEBD3, 0xEEF4F5],
    haze_gain: [0.6, 0.7, 1.0, 1.0, 1.0, 0.9],
    haze_alpha: [90.0, 70.0, 45.0, 0.0, 0.0],
    cloud: [0xC3C8D8, 0xC9C4D4, 0xD9C8CC, 0xE6D4CC, 0xECEEF0, 0xF4F7F8],
    cloud_lit: [0xD9DDE9, 0xEFC9C4, 0xFBDDB8, 0xFFEBCD, 0xFFF8EA, 0xFFFFFF],
    cloud_alpha: [170.0, 180.0, 195.0, 205.0, 215.0, 225.0],
    glow: [0xC9CDE0, 0xE8B9A6, 0xF7D2A4, 0xFFE9C4, 0xFFF3DC, 0xFFFBEF],
    glow_gain: [0.0, 0.4, 0.9, 1.0, 0.9, 0.6],
    glow_alpha: 190.0,
    sun: [0xF2B45A, 0xFBE7B0],
    grey: [0xB9BEC8, 0xD5DADD],
    storm: 0x8E98A6,
    star: 0xFFFFFF,
    moon: 0xFFFFFF,
    ink: 0x3A4656,
    trail: 0xFFFFFF,
    rain: 0x5F747E,
    snow: 0xFFFFFF,
    bird: 0x46525E,
    tree: [0x52705F, 0x5C7A68],
    tree_night: 0x3D4A5C,
    tree_light: 0.85,
    trunk: 0x4A5560,
    seasons: [
        [0x6F7F7A, 0x7B8A84, 0x657570],
        [0x6FA77F, 0x7DB58B, 0x63997A],
        [0x52705F, 0x5C7A68, 0x4A6857],
        [0xC08A52, 0xC9A85A, 0x6E8A6A],
    ],
    grain: 2.0,
    bloom: 0.45,
};

pub fn palette(mode: &str) -> &'static Palette {
    if mode == "light" {
        &LIGHT
    } else {
        &DARK
    }
}

/// windows at home
pub const WARM: [Rgb; 4] = [hex(0xFFD98A), hex(0xF2C46D), hex(0xE8A94F), hex(0xF7B45A)];
/// office strip lights
pub const COOL: [Rgb; 4] = [hex(0xE9E4CF), hex(0xDDE3DA), hex(0xF1E6C4), hex(0xE4E8E0)];
/// the ones still up at three, lit by a monitor
pub const SCREEN: Rgb = hex(0x9FC3E0);
pub const AMBER: Rgb = hex(0xE0A951);
pub const ROSE: Rgb = hex(0xE07A70);
pub const SAGE: Rgb = hex(0x8FB8A0);
pub const BLUE: Rgb = hex(0x86AED0);
pub const CREAM: Rgb = hex(0xE6DFCF);
pub const BEACON: Rgb = hex(0xE5736B);
pub const WHITE: Rgb = [255.0; 3];
