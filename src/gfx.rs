//! dormer's own small renderer.
//!
//! Shapes are rasterised by accumulating signed edge coverage, the way font
//! renderers do: exact antialiasing in one pass, no supersampling. The still
//! picture is composed in floating point on a `Canvas` and only turned into
//! eight-bit pixels at the very end, with a little fixed grain that also
//! hides the banding a dark gradient would otherwise show. The live layer
//! draws the same shapes straight onto screen pixels through `Frame`.

use std::fs::File;
use std::io::BufWriter;

use crate::color::Rgb;
use crate::rng::hash2;

pub type P = (f32, f32);

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Blend {
    /// paint over
    Over,
    /// add light, never past white
    Screen,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Rect {
    pub x0: i32,
    pub y0: i32,
    pub x1: i32,
    pub y1: i32,
}

impl Rect {
    pub fn area(&self) -> i64 {
        (self.x1 - self.x0).max(0) as i64 * (self.y1 - self.y0).max(0) as i64
    }

    pub fn join(&self, o: &Rect) -> Rect {
        Rect { x0: self.x0.min(o.x0), y0: self.y0.min(o.y0), x1: self.x1.max(o.x1), y1: self.y1.max(o.y1) }
    }

    pub fn touches(&self, o: &Rect, pad: i32) -> bool {
        self.x0 - pad < o.x1 && o.x0 - pad < self.x1 && self.y0 - pad < o.y1 && o.y0 - pad < self.y1
    }
}

pub trait Surface {
    fn w(&self) -> usize;
    fn h(&self) -> usize;
    /// Blend `col` into row `y` from `x`, by `cov` (0 to 1 for each pixel) times `a`.
    fn span(&mut self, x: usize, y: usize, cov: &[f32], col: Rgb, a: f32, mode: Blend);
    /// Told about every rectangle that is drawn on, for surfaces that keep track of damage.
    fn touch(&mut self, _r: Rect) {}
}

// ── the canvas the still picture is composed on ──────────────────────────────

pub struct Canvas {
    pub w: usize,
    pub h: usize,
    /// red, green, blue for each pixel, 0 to 255
    pub px: Vec<f32>,
}

impl Canvas {
    pub fn new(w: usize, h: usize) -> Canvas {
        Canvas { w, h, px: vec![0.0; w * h * 3] }
    }

    /// Fill the whole canvas, one colour to a row.
    pub fn rows(&mut self, colours: &[Rgb]) {
        let w = self.w;
        let band = (self.h + threads() - 1) / threads();
        std::thread::scope(|sc| {
            for (dst, cols) in self.px.chunks_mut(band * w * 3).zip(colours.chunks(band)) {
                sc.spawn(move || {
                    for (row, c) in dst.chunks_mut(w * 3).zip(cols) {
                        for p in row.chunks_exact_mut(3) {
                            p.copy_from_slice(c);
                        }
                    }
                });
            }
        });
    }

    /// Wash rows `y0..y1` towards `col`, by an amount for each row.
    pub fn wash(&mut self, y0: i32, y1: i32, col: Rgb, mut alpha: impl FnMut(f32) -> f32) {
        for y in y0.max(0)..y1.min(self.h as i32) {
            let a = alpha(y as f32 + 0.5).clamp(0.0, 1.0);
            if a < 0.002 {
                continue;
            }
            let y = y as usize;
            for p in self.px[y * self.w * 3..(y + 1) * self.w * 3].chunks_exact_mut(3) {
                p[0] += (col[0] - p[0]) * a;
                p[1] += (col[1] - p[1]) * a;
                p[2] += (col[2] - p[2]) * a;
            }
        }
    }

    /// Composite a coverage mask (one byte a pixel, rows from `y0`) in a colour for each row.
    pub fn stamp(&mut self, mask: &[u8], y0: i32, dy: i32, mut colour: impl FnMut(i32) -> (Rgb, f32)) {
        let w = self.w;
        let rows = mask.len() / w;
        for r in 0..rows {
            let y = y0 + r as i32 + dy;
            if y < 0 || y >= self.h as i32 {
                continue;
            }
            let (c, a) = colour(y);
            let src = &mask[r * w..(r + 1) * w];
            let dst = &mut self.px[y as usize * w * 3..(y as usize + 1) * w * 3];
            let full = a >= 0.999;
            for (p, &m) in dst.chunks_exact_mut(3).zip(src) {
                if m == 0 {
                    continue;
                }
                if m == 255 && full {
                    p.copy_from_slice(&c);
                } else {
                    let k = m as f32 * (1.0 / 255.0) * a;
                    p[0] += (c[0] - p[0]) * k;
                    p[1] += (c[1] - p[1]) * k;
                    p[2] += (c[2] - p[2]) * k;
                }
            }
        }
    }

    /// Eight-bit pixels (0x00RRGGBB), with the grain laid on.
    pub fn finish(&self, grain: f32) -> Vec<u32> {
        let tile = grain_tile();
        let mut out = vec![0u32; self.w * self.h];
        let w = self.w;
        let band = (self.h + threads() - 1) / threads();
        std::thread::scope(|sc| {
            for (k, (dst, src)) in out.chunks_mut(band * w).zip(self.px.chunks(band * w * 3)).enumerate() {
                sc.spawn(move || {
                    for (r, (drow, srow)) in dst.chunks_mut(w).zip(src.chunks(w * 3)).enumerate() {
                        let y = k * band + r;
                        let trow = &tile[(y & 255) * 256..(y & 255) * 256 + 256];
                        let shift = (hash2(y as u32 >> 8, 7) * 256.0) as usize;
                        for (x, (d, s)) in drow.iter_mut().zip(srow.chunks_exact(3)).enumerate() {
                            let n = trow[(x + shift) & 255];
                            let lum = (s[0] + 2.0 * s[1] + s[2]) * (1.0 / 1020.0);
                            let g = n * grain * (0.55 + 0.9 * lum) + 0.5;
                            let q = |v: f32| (v + g).clamp(0.0, 255.0) as u32;
                            *d = 0xFF00_0000 | (q(s[0]) << 16) | (q(s[1]) << 8) | q(s[2]);
                        }
                    }
                });
            }
        });
        out
    }
}

impl Surface for Canvas {
    fn w(&self) -> usize {
        self.w
    }

    fn h(&self) -> usize {
        self.h
    }

    #[inline]
    fn span(&mut self, x: usize, y: usize, cov: &[f32], col: Rgb, a: f32, mode: Blend) {
        let at = (y * self.w + x) * 3;
        let row = &mut self.px[at..at + cov.len() * 3];
        match mode {
            Blend::Over => {
                for (p, &c) in row.chunks_exact_mut(3).zip(cov) {
                    let k = c * a;
                    p[0] += (col[0] - p[0]) * k;
                    p[1] += (col[1] - p[1]) * k;
                    p[2] += (col[2] - p[2]) * k;
                }
            }
            Blend::Screen => {
                let c = [col[0] / 255.0, col[1] / 255.0, col[2] / 255.0];
                for (p, &v) in row.chunks_exact_mut(3).zip(cov) {
                    let k = v * a;
                    p[0] += (255.0 - p[0]).max(0.0) * c[0] * k;
                    p[1] += (255.0 - p[1]).max(0.0) * c[1] * k;
                    p[2] += (255.0 - p[2]).max(0.0) * c[2] * k;
                }
            }
        }
    }
}

fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1).clamp(1, 4)
}

/// A fixed tile of noise in -1 to 1, bunched towards 0: the same grain on every frame.
fn grain_tile() -> &'static [f32] {
    use std::sync::OnceLock;
    static TILE: OnceLock<Vec<f32>> = OnceLock::new();
    TILE.get_or_init(|| {
        (0..256 * 256u32).map(|i| hash2(i & 255, (i >> 8) + 1000) + hash2((i & 255) + 4000, i >> 8) - 1.0).collect()
    })
}

// ── screen pixels, for the live layer ────────────────────────────────────────

/// How much of each pixel the nearer rows of roofs cover, so that live things can pass behind them.
pub struct Depth {
    pub w: usize,
    pub y0: usize,
    /// `front[i]`: layer `i` and everything nearer
    pub front: Vec<Vec<u8>>,
}

impl Depth {
    /// From a view's layers, far to near, each a mask of whole rows from its first row.
    pub fn stack(masks: &[(usize, Vec<u8>)], w: usize, h: usize) -> Depth {
        let y0 = masks.iter().map(|m| m.0).min().unwrap_or(h).min(h);
        let rows = h - y0;
        let mut front = vec![vec![0u8; w * rows]; masks.len()];
        for i in (0..masks.len()).rev() {
            if i + 1 < masks.len() {
                let (near, far) = front.split_at_mut(i + 1);
                near[i].copy_from_slice(&far[0]);
            }
            let (my0, m) = &masks[i];
            for (r, row) in m.chunks(w).enumerate() {
                let at = (my0 + r - y0) * w;
                if at + w > front[i].len() {
                    break;
                }
                for (d, &s) in front[i][at..at + w].iter_mut().zip(row) {
                    *d = (*d).max(s);
                }
            }
        }
        Depth { w, y0, front }
    }
}

pub struct Frame<'a> {
    pub w: usize,
    pub h: usize,
    pub px: &'a mut [u32],
    pub dirty: &'a mut Vec<Rect>,
    pub depth: Option<&'a Depth>,
    /// draw behind this layer and everything nearer (0: behind all of the ground)
    pub behind: Option<usize>,
}

impl Surface for Frame<'_> {
    fn w(&self) -> usize {
        self.w
    }

    fn h(&self) -> usize {
        self.h
    }

    #[inline]
    fn span(&mut self, x: usize, y: usize, cov: &[f32], col: Rgb, a: f32, mode: Blend) {
        let at = y * self.w + x;
        let row = &mut self.px[at..at + cov.len()];
        let hide: Option<&[u8]> = match (self.behind, self.depth) {
            (Some(i), Some(d)) if y >= d.y0 && i < d.front.len() => {
                let o = (y - d.y0) * d.w + x;
                d.front[i].get(o..o + cov.len())
            }
            _ => None,
        };
        let (cr, cg, cb) = (col[0] as i32, col[1] as i32, col[2] as i32);
        let a256 = a * 256.0;
        for (i, (p, &c)) in row.iter_mut().zip(cov).enumerate() {
            let mut k = (c * a256) as i32;
            if k <= 0 {
                continue;
            }
            if let Some(h) = hide {
                k = (k * (255 - h[i] as i32)) >> 8;
                if k <= 0 {
                    continue;
                }
            }
            let k = k.min(256);
            let (r, g, b) = (((*p >> 16) & 255) as i32, ((*p >> 8) & 255) as i32, (*p & 255) as i32);
            let (r, g, b) = match mode {
                Blend::Over => (r + (((cr - r) * k) >> 8), g + (((cg - g) * k) >> 8), b + (((cb - b) * k) >> 8)),
                Blend::Screen => (
                    r + (((255 - r) * cr * k) >> 16),
                    g + (((255 - g) * cg * k) >> 16),
                    b + (((255 - b) * cb * k) >> 16),
                ),
            };
            *p = 0xFF00_0000 | ((r as u32) << 16) | ((g as u32) << 8) | b as u32;
        }
    }

    fn touch(&mut self, r: Rect) {
        let r = Rect { x0: r.x0.max(0), y0: r.y0.max(0), x1: r.x1.min(self.w as i32), y1: r.y1.min(self.h as i32) };
        if r.x1 > r.x0 && r.y1 > r.y0 {
            self.dirty.push(r);
        }
    }
}

// ── shapes ───────────────────────────────────────────────────────────────────

/// The rasteriser. `begin` a bounding box, add outlines, then `fill`.
#[derive(Default)]
pub struct Shaper {
    acc: Vec<f32>,
    cov: Vec<f32>,
    ox: i32,
    oy: i32,
    w: usize,
    h: usize,
    stride: usize,
    ymin: usize,
    ymax: usize,
    open: bool,
}

impl Shaper {
    pub fn new() -> Shaper {
        Shaper::default()
    }

    /// Start a shape somewhere inside this box. False when none of it is on the surface.
    pub fn begin(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, sw: usize, sh: usize) -> bool {
        self.open = false;
        if !(x0.is_finite() && y0.is_finite() && x1.is_finite() && y1.is_finite()) {
            return false;
        }
        let ix0 = (x0.floor().max(-1e6) as i32).max(0);
        let iy0 = (y0.floor().max(-1e6) as i32).max(0);
        let ix1 = (x1.ceil().min(1e6) as i32 + 1).min(sw as i32);
        let iy1 = (y1.ceil().min(1e6) as i32 + 1).min(sh as i32);
        if ix1 <= ix0 || iy1 <= iy0 {
            return false;
        }
        self.ox = ix0;
        self.oy = iy0;
        self.w = (ix1 - ix0) as usize;
        self.h = (iy1 - iy0) as usize;
        self.stride = self.w + 2;
        self.acc.clear();
        self.acc.resize(self.stride * self.h, 0.0);
        self.ymin = self.h;
        self.ymax = 0;
        self.open = true;
        true
    }

    fn line(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) {
        let (ox, oy) = (self.ox as f32, self.oy as f32);
        let (mut x0, mut y0, mut x1, mut y1) = (x0 - ox, y0 - oy, x1 - ox, y1 - oy);
        if y0 == y1 {
            return;
        }
        let dir = if y0 < y1 { 1.0 } else { -1.0 };
        if y0 > y1 {
            std::mem::swap(&mut x0, &mut x1);
            std::mem::swap(&mut y0, &mut y1);
        }
        let (hf, wf) = (self.h as f32, self.w as f32);
        if y1 <= 0.0 || y0 >= hf {
            return;
        }
        let dxdy = (x1 - x0) / (y1 - y0);
        let mut x = x0;
        let first = if y0 < 0.0 {
            x -= y0 * dxdy;
            0
        } else {
            y0 as usize
        };
        let last = (y1.ceil() as usize).min(self.h);
        self.ymin = self.ymin.min(first);
        self.ymax = self.ymax.max(last);
        for y in first..last {
            let row = y * self.stride;
            let dy = ((y + 1) as f32).min(y1) - (y as f32).max(y0);
            let xnext = x + dxdy * dy;
            let d = dy * dir;
            let (xa, xb) = if x < xnext { (x, xnext) } else { (xnext, x) };
            let (xa, xb) = (xa.clamp(0.0, wf), xb.clamp(0.0, wf));
            let xaf = xa.floor();
            let xai = xaf as usize;
            let xbc = xb.ceil();
            let xbi = xbc as usize;
            let acc = &mut self.acc[row..row + self.stride];
            if xbi <= xai + 1 {
                let xmf = 0.5 * (xa + xb) - xaf;
                acc[xai] += d - d * xmf;
                acc[xai + 1] += d * xmf;
            } else {
                let s = 1.0 / (xb - xa);
                let x0f = xa - xaf;
                let a0 = 0.5 * s * (1.0 - x0f) * (1.0 - x0f);
                let x1f = xb - xbc + 1.0;
                let am = 0.5 * s * x1f * x1f;
                acc[xai] += d * a0;
                if xbi == xai + 2 {
                    acc[xai + 1] += d * (1.0 - a0 - am);
                } else {
                    let a1 = s * (1.5 - x0f);
                    acc[xai + 1] += d * (a1 - a0);
                    for v in acc[xai + 2..xbi - 1].iter_mut() {
                        *v += d * s;
                    }
                    let a2 = a1 + (xbi - xai - 3) as f32 * s;
                    acc[xbi - 1] += d * (1.0 - a2 - am);
                }
                acc[xbi] += d * am;
            }
            x = xnext;
        }
    }

    /// A closed outline. Outlines added to one shape are joined, whichever way round they run.
    pub fn poly(&mut self, pts: &[P]) {
        if !self.open || pts.len() < 3 {
            return;
        }
        let n = pts.len();
        let mut area = 0.0f64;
        for i in 0..n {
            let (a, b) = (pts[i], pts[(i + 1) % n]);
            area += (b.0 as f64).clamp(-1e5, 1e5) * (a.1 as f64).clamp(-1e5, 1e5)
                - (a.0 as f64).clamp(-1e5, 1e5) * (b.1 as f64).clamp(-1e5, 1e5);
        }
        for i in 0..n {
            let (a, b) = if area >= 0.0 { (pts[i], pts[(i + 1) % n]) } else { (pts[(i + 1) % n], pts[i]) };
            self.line(a.0, a.1, b.0, b.1);
        }
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32) {
        if !self.open || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let n = ((rx.max(ry) * 1.4) as usize + 10).min(72);
        let step = std::f32::consts::TAU / n as f32;
        let mut prev = (cx + rx, cy);
        for i in 1..=n {
            let a = step * i as f32;
            let next = if i == n { (cx + rx, cy) } else { (cx + rx * a.cos(), cy + ry * a.sin()) };
            self.line(next.0, next.1, prev.0, prev.1);
            prev = next;
        }
    }

    /// A straight stroke of this width, with square ends.
    pub fn stroke(&mut self, a: P, b: P, width: f32) {
        let (dx, dy) = (b.0 - a.0, b.1 - a.1);
        let len = (dx * dx + dy * dy).sqrt();
        if len < 1e-4 {
            return;
        }
        let (nx, ny) = (-dy / len * width * 0.5, dx / len * width * 0.5);
        self.poly(&[(a.0 + nx, a.1 + ny), (b.0 + nx, b.1 + ny), (b.0 - nx, b.1 - ny), (a.0 - nx, a.1 - ny)]);
    }

    fn resolve(&mut self, y: usize) -> Option<(usize, usize)> {
        let row = &self.acc[y * self.stride..y * self.stride + self.w];
        if self.cov.len() < self.w {
            self.cov.resize(self.w, 0.0);
        }
        let (mut s, mut first, mut last) = (0.0f32, usize::MAX, 0);
        for (x, (&v, c)) in row.iter().zip(self.cov.iter_mut()).enumerate() {
            s += v;
            let k = s.abs().min(1.0);
            *c = k;
            if k > 0.003 {
                if first == usize::MAX {
                    first = x;
                }
                last = x;
            }
        }
        if first == usize::MAX {
            None
        } else {
            Some((first, last + 1))
        }
    }

    /// Paint the shape, in one colour.
    pub fn fill<S: Surface>(&mut self, s: &mut S, col: Rgb, a: f32, mode: Blend) {
        self.fill_rows(s, mode, |_| (col, a));
    }

    /// Paint the shape, with a colour and strength for each row.
    pub fn fill_rows<S: Surface>(&mut self, s: &mut S, mode: Blend, mut colour: impl FnMut(f32) -> (Rgb, f32)) {
        if !self.open {
            return;
        }
        self.open = false;
        let (y0, y1) = (self.ymin, self.ymax.min(self.h));
        if y1 <= y0 {
            return;
        }
        for y in y0..y1 {
            if let Some((a, b)) = self.resolve(y) {
                let (col, alpha) = colour((self.oy + y as i32) as f32 + 0.5);
                if alpha > 0.002 {
                    s.span(self.ox as usize + a, self.oy as usize + y, &self.cov[a..b], col, alpha, mode);
                }
            }
        }
        s.touch(Rect { x0: self.ox, y0: self.oy + y0 as i32, x1: self.ox + self.w as i32, y1: self.oy + y1 as i32 });
    }

    /// The shape as a mask, one byte a pixel, `w` wide, covering whole rows of the surface from `y0`.
    pub fn mask(&mut self, w: usize) -> (usize, Vec<u8>) {
        let mut out = vec![0u8; w * self.h];
        if self.open {
            for y in self.ymin..self.ymax.min(self.h) {
                if let Some((a, b)) = self.resolve(y) {
                    let at = y * w + self.ox as usize;
                    for (d, c) in out[at + a..at + b].iter_mut().zip(&self.cov[a..b]) {
                        *d = (c * 255.0 + 0.5) as u8;
                    }
                }
            }
        }
        self.open = false;
        (self.oy as usize, out)
    }

    /// The shape as floating coverage over its own box: x, y, width, height, values.
    pub fn take(&mut self) -> Patch {
        let mut data = vec![0.0f32; self.w * self.h];
        if self.open {
            for y in self.ymin..self.ymax.min(self.h) {
                if let Some((a, b)) = self.resolve(y) {
                    data[y * self.w + a..y * self.w + b].copy_from_slice(&self.cov[a..b]);
                }
            }
        }
        self.open = false;
        Patch { x: self.ox, y: self.oy, w: self.w, h: self.h, data }
    }
}

/// A piece of coverage cut out of a shape, to be combined with others before it is painted.
pub struct Patch {
    pub x: i32,
    pub y: i32,
    pub w: usize,
    pub h: usize,
    pub data: Vec<f32>,
}

impl Patch {
    /// Keep only what also lies inside `other`.
    pub fn clip(&mut self, other: &Patch) {
        for r in 0..self.h {
            let oy = self.y + r as i32 - other.y;
            for c in 0..self.w {
                let ox = self.x + c as i32 - other.x;
                let m = if oy >= 0 && (oy as usize) < other.h && ox >= 0 && (ox as usize) < other.w {
                    other.data[oy as usize * other.w + ox as usize]
                } else {
                    0.0
                };
                self.data[r * self.w + c] *= m;
            }
        }
    }

    pub fn paint<S: Surface>(&self, s: &mut S, mode: Blend, mut colour: impl FnMut(f32) -> (Rgb, f32)) {
        for r in 0..self.h {
            let row = &self.data[r * self.w..(r + 1) * self.w];
            let (col, a) = colour((self.y + r as i32) as f32 + 0.5);
            if a > 0.002 {
                s.span(self.x as usize, self.y as usize + r, row, col, a, mode);
            }
        }
        s.touch(Rect { x0: self.x, y0: self.y, x1: self.x + self.w as i32, y1: self.y + self.h as i32 });
    }
}

/// A surface and the rasteriser that draws on it, with the handful of shapes everything is made of.
pub struct Draw<'a, S: Surface> {
    pub s: &'a mut S,
    pub sh: &'a mut Shaper,
    row: Vec<f32>,
}

impl<'a, S: Surface> Draw<'a, S> {
    pub fn new(s: &'a mut S, sh: &'a mut Shaper) -> Self {
        Draw { s, sh, row: Vec::new() }
    }

    fn begin(&mut self, x0: f32, y0: f32, x1: f32, y1: f32) -> bool {
        self.sh.begin(x0, y0, x1, y1, self.s.w(), self.s.h())
    }

    fn begin_pts(&mut self, pts: &[P], pad: f32) -> bool {
        let (mut x0, mut y0, mut x1, mut y1) = (f32::MAX, f32::MAX, f32::MIN, f32::MIN);
        for p in pts {
            x0 = x0.min(p.0);
            y0 = y0.min(p.1);
            x1 = x1.max(p.0);
            y1 = y1.max(p.1);
        }
        self.begin(x0 - pad, y0 - pad, x1 + pad, y1 + pad)
    }

    pub fn poly(&mut self, pts: &[P], col: Rgb, a: f32) {
        self.poly_mode(pts, col, a, Blend::Over)
    }

    pub fn poly_mode(&mut self, pts: &[P], col: Rgb, a: f32, mode: Blend) {
        if a > 0.002 && self.begin_pts(pts, 0.0) {
            self.sh.poly(pts);
            self.sh.fill(self.s, col, a, mode);
        }
    }

    pub fn ellipse(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32) {
        self.ellipse_mode(cx, cy, rx, ry, col, a, Blend::Over)
    }

    pub fn ellipse_mode(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32, mode: Blend) {
        if a <= 0.002 {
            return;
        }
        if rx.max(ry) < 3.0 {
            return self.dot(cx, cy, rx, ry, col, a, mode);
        }
        if self.begin(cx - rx, cy - ry, cx + rx, cy + ry) {
            self.sh.ellipse(cx, cy, rx, ry);
            self.sh.fill(self.s, col, a, mode);
        }
    }

    pub fn circle(&mut self, cx: f32, cy: f32, r: f32, col: Rgb, a: f32) {
        self.ellipse(cx, cy, r, r, col, a)
    }

    /// A small disc, antialiased by distance: quicker than the rasteriser for stars and bulbs.
    pub fn dot(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32, mode: Blend) {
        let (w, h) = (self.s.w() as i32, self.s.h() as i32);
        let (x0, x1) = (((cx - rx - 1.0).floor() as i32).max(0), ((cx + rx + 1.0).ceil() as i32).min(w));
        let (y0, y1) = (((cy - ry - 1.0).floor() as i32).max(0), ((cy + ry + 1.0).ceil() as i32).min(h));
        if x1 <= x0 || y1 <= y0 || a <= 0.002 {
            return;
        }
        let n = (x1 - x0) as usize;
        if self.row.len() < n {
            self.row.resize(n, 0.0);
        }
        // a disc smaller than a pixel is a fainter pixel, not a smaller one
        let (rx_, ry_) = (rx.max(0.5), ry.max(0.5));
        let faint = (rx / rx_) * (ry / ry_);
        let r = rx_.min(ry_);
        for y in y0..y1 {
            let dy = (y as f32 + 0.5 - cy) / ry_;
            for (i, c) in self.row[..n].iter_mut().enumerate() {
                let dx = ((x0 + i as i32) as f32 + 0.5 - cx) / rx_;
                let d = (dx * dx + dy * dy).sqrt();
                *c = ((1.0 - d) * r + 0.5).clamp(0.0, 1.0) * faint;
            }
            self.s.span(x0 as usize, y as usize, &self.row[..n], col, a, mode);
        }
        self.s.touch(Rect { x0, y0, x1, y1 });
    }

    /// An axis-aligned rectangle, with soft edges where it does not sit on whole pixels.
    pub fn rect(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, a: f32) {
        self.rect_mode(x0, y0, x1, y1, col, a, Blend::Over)
    }

    pub fn rect_mode(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, a: f32, mode: Blend) {
        let (x0, x1) = (x0.min(x1), x0.max(x1));
        let (y0, y1) = (y0.min(y1), y0.max(y1));
        let (w, h) = (self.s.w() as i32, self.s.h() as i32);
        let (ix0, ix1) = ((x0.floor() as i32).max(0), (x1.ceil() as i32).min(w));
        let (iy0, iy1) = ((y0.floor() as i32).max(0), (y1.ceil() as i32).min(h));
        if ix1 <= ix0 || iy1 <= iy0 || a <= 0.002 {
            return;
        }
        let n = (ix1 - ix0) as usize;
        if self.row.len() < n {
            self.row.resize(n, 0.0);
        }
        for (i, c) in self.row[..n].iter_mut().enumerate() {
            let px = (ix0 + i as i32) as f32;
            *c = ((px + 1.0).min(x1) - px.max(x0)).clamp(0.0, 1.0);
        }
        for y in iy0..iy1 {
            let py = y as f32;
            let cy = ((py + 1.0).min(y1) - py.max(y0)).clamp(0.0, 1.0);
            self.s.span(ix0 as usize, y as usize, &self.row[..n], col, a * cy, mode);
        }
        self.s.touch(Rect { x0: ix0, y0: iy0, x1: ix1, y1: iy1 });
    }

    /// A stroke with round ends.
    pub fn line(&mut self, a: P, b: P, width: f32, col: Rgb, alpha: f32) {
        self.path(&[a, b], width, col, alpha)
    }

    /// A stroke through these points, joined round, painted once so it does not darken where it bends.
    pub fn path(&mut self, pts: &[P], width: f32, col: Rgb, alpha: f32) {
        self.path_mode(pts, width, col, alpha, Blend::Over)
    }

    pub fn path_mode(&mut self, pts: &[P], width: f32, col: Rgb, alpha: f32, mode: Blend) {
        if pts.len() < 2 || alpha <= 0.002 {
            return;
        }
        // thinner than a pixel: keep it a pixel wide and fade it instead
        let (width, alpha) = if width < 1.0 { (1.0, alpha * width.max(0.05)) } else { (width, alpha) };
        if !self.begin_pts(pts, width * 0.5 + 1.0) {
            return;
        }
        for k in 0..pts.len() - 1 {
            self.sh.stroke(pts[k], pts[k + 1], width);
        }
        if width >= 2.2 {
            for p in pts {
                self.sh.ellipse(p.0, p.1, width * 0.5, width * 0.5);
            }
        }
        self.sh.fill(self.s, col, alpha, mode);
    }

    /// Light that is brightest in the middle and gone at the edge of an ellipse.
    /// `power` 1 is a wide soft wash; 2 and 3 gather it towards the centre.
    pub fn glow(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32, power: u8, mode: Blend) {
        let (w, h) = (self.s.w() as i32, self.s.h() as i32);
        let (x0, x1) = (((cx - rx).floor() as i32).max(0), ((cx + rx).ceil() as i32).min(w));
        let (y0, y1) = (((cy - ry).floor() as i32).max(0), ((cy + ry).ceil() as i32).min(h));
        if x1 <= x0 || y1 <= y0 || a <= 0.002 || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let n = (x1 - x0) as usize;
        if self.row.len() < n {
            self.row.resize(n, 0.0);
        }
        let (irx, iry) = (1.0 / rx, 1.0 / ry);
        for y in y0..y1 {
            let dy = (y as f32 + 0.5 - cy) * iry;
            let top = 1.0 - dy * dy;
            if top <= 0.0 {
                continue;
            }
            // only the stretch of this row that lies inside the ellipse
            let half = top.sqrt() * rx;
            let a0 = ((cx - half).floor() as i32).clamp(x0, x1);
            let a1 = ((cx + half).ceil() as i32).clamp(x0, x1);
            if a1 <= a0 {
                continue;
            }
            let (a0, a1) = ((a0 - x0) as usize, (a1 - x0) as usize);
            let start = (x0 + a0 as i32) as f32 + 0.5 - cx;
            for (i, c) in self.row[a0..a1].iter_mut().enumerate() {
                let dx = (start + i as f32) * irx;
                let q = (top - dx * dx).max(0.0);
                *c = match power {
                    1 => q * q * (3.0 - 2.0 * q),
                    2 => q * q,
                    3 => q * q * q,
                    _ => {
                        let q2 = q * q;
                        q2 * q2 * q
                    }
                };
            }
            self.s.span(x0 as usize + a0, y as usize, &self.row[a0..a1], col, a, mode);
        }
        self.s.touch(Rect { x0, y0, x1, y1 });
    }

    /// A soft-edged ellipse that is solid out to `core` (0 to 1) of its radius.
    pub fn puff(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, col: Rgb, a: f32, core: f32) {
        let (w, h) = (self.s.w() as i32, self.s.h() as i32);
        let (x0, x1) = (((cx - rx).floor() as i32).max(0), ((cx + rx).ceil() as i32).min(w));
        let (y0, y1) = (((cy - ry).floor() as i32).max(0), ((cy + ry).ceil() as i32).min(h));
        if x1 <= x0 || y1 <= y0 || a <= 0.002 || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let n = (x1 - x0) as usize;
        if self.row.len() < n {
            self.row.resize(n, 0.0);
        }
        let (irx, iry) = (1.0 / rx, 1.0 / ry);
        let scale = 1.0 / (1.0 - core * core).max(1e-3);
        for y in y0..y1 {
            let dy = (y as f32 + 0.5 - cy) * iry;
            let top = 1.0 - dy * dy;
            if top <= 0.0 {
                continue;
            }
            // only the stretch of this row that lies inside the ellipse
            let half = top.sqrt() * rx;
            let a0 = ((cx - half).floor() as i32).clamp(x0, x1);
            let a1 = ((cx + half).ceil() as i32).clamp(x0, x1);
            if a1 <= a0 {
                continue;
            }
            let (a0, a1) = ((a0 - x0) as usize, (a1 - x0) as usize);
            let start = (x0 + a0 as i32) as f32 + 0.5 - cx;
            for (i, c) in self.row[a0..a1].iter_mut().enumerate() {
                let dx = (start + i as f32) * irx;
                let q = ((top - dx * dx) * scale).clamp(0.0, 1.0);
                *c = q * q * (3.0 - 2.0 * q);
            }
            self.s.span(x0 as usize + a0, y as usize, &self.row[a0..a1], col, a, Blend::Over);
        }
        self.s.touch(Rect { x0, y0, x1, y1 });
    }
}

// ── light that spills: the glow around lit windows ───────────────────────────

/// One row of a coarse grid, blended between the two grid rows that picture row `y` falls between.
fn coarse_row(data: &[f32], bw: usize, bh: usize, ch: usize, div: usize, y: usize, out: &mut Vec<f32>) {
    let fy = ((y as f32 + 0.5) / div as f32 - 0.5).max(0.0);
    let iy = (fy as usize).min(bh - 2);
    let ty = (fy - iy as f32).min(1.0);
    let (r0, r1) = (&data[iy * bw * ch..(iy + 1) * bw * ch], &data[(iy + 1) * bw * ch..(iy + 2) * bw * ch]);
    out.clear();
    out.extend(r0.iter().zip(r1).map(|(a, b)| a + (b - a) * ty));
}

/// For each picture column: the grid cell to its left, and how far towards the next one it is.
fn coarse_cols(w: usize, bw: usize, div: usize) -> Vec<(u32, f32)> {
    (0..w)
        .map(|x| {
            let fx = ((x as f32 + 0.5) / div as f32 - 0.5).max(0.0);
            let ix = (fx as usize).min(bw - 2);
            (ix as u32, (fx - ix as f32).min(1.0))
        })
        .collect()
}

/// Light gathered on a coarse grid, blurred, and laid back over the picture.
pub struct Bloom {
    div: usize,
    w: usize,
    h: usize,
    data: Vec<f32>,
    y0: usize,
    y1: usize,
}

impl Bloom {
    /// A grid one `div`th the size of a `w` by `h` picture.
    pub fn new(w: usize, h: usize, div: usize) -> Bloom {
        let div = div.max(1);
        let (bw, bh) = (w / div + 3, h / div + 3);
        Bloom { div, w: bw, h: bh, data: vec![0.0; bw * bh * 3], y0: bh, y1: 0 }
    }

    /// A lit rectangle, in picture pixels.
    pub fn light(&mut self, x0: f32, y0: f32, x1: f32, y1: f32, col: Rgb, power: f32) {
        let d = self.div as f32;
        let (x0, y0, x1, y1) = (x0 / d, y0 / d, x1 / d, y1 / d);
        let (ix0, ix1) = ((x0.floor() as i32).max(0), (x1.ceil() as i32).min(self.w as i32));
        let (iy0, iy1) = ((y0.floor() as i32).max(0), (y1.ceil() as i32).min(self.h as i32));
        for y in iy0..iy1 {
            let cy = ((y as f32 + 1.0).min(y1) - (y as f32).max(y0)).clamp(0.0, 1.0);
            for x in ix0..ix1 {
                let cx = ((x as f32 + 1.0).min(x1) - (x as f32).max(x0)).clamp(0.0, 1.0);
                let k = cx * cy * power;
                let at = (y as usize * self.w + x as usize) * 3;
                self.data[at] += col[0] * k;
                self.data[at + 1] += col[1] * k;
                self.data[at + 2] += col[2] * k;
            }
        }
        if iy1 > iy0 && ix1 > ix0 {
            self.y0 = self.y0.min(iy0 as usize);
            self.y1 = self.y1.max(iy1 as usize);
        }
    }

    /// Something solid now stands in front of the light gathered so far: `mask` covers whole rows from `y0`.
    pub fn hide(&mut self, mask: &[u8], w: usize, y0: usize) {
        let rows = mask.len() / w.max(1);
        for cy in self.y0..self.y1.min(self.h) {
            let py = cy * self.div + self.div / 2;
            if py < y0 || py - y0 >= rows {
                continue;
            }
            let src = &mask[(py - y0) * w..(py - y0 + 1) * w];
            for cx in 0..self.w {
                let px = cx * self.div + self.div / 2;
                if px < w && src[px] > 0 {
                    let keep = 1.0 - src[px] as f32 * (1.0 / 255.0);
                    let at = (cy * self.w + cx) * 3;
                    self.data[at] *= keep;
                    self.data[at + 1] *= keep;
                    self.data[at + 2] *= keep;
                }
            }
        }
    }

    /// Dim the light gathered so far inside an ellipse (a tree, say, standing in front of it).
    pub fn shade(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, keep: f32) {
        let d = self.div as f32;
        let (x0, x1) = ((((cx - rx) / d).floor() as i32).max(0), (((cx + rx) / d).ceil() as i32).min(self.w as i32));
        let (y0, y1) = ((((cy - ry) / d).floor() as i32).max(self.y0 as i32), (((cy + ry) / d).ceil() as i32).min(self.y1.min(self.h) as i32));
        for y in y0..y1 {
            for x in x0..x1 {
                let (dx, dy) = (((x as f32 + 0.5) * d - cx) / rx, ((y as f32 + 0.5) * d - cy) / ry);
                if dx * dx + dy * dy < 1.0 {
                    let at = (y as usize * self.w + x as usize) * 3;
                    self.data[at] *= keep;
                    self.data[at + 1] *= keep;
                    self.data[at + 2] *= keep;
                }
            }
        }
    }

    fn blur(&mut self, radius: usize) {
        let (w, h) = (self.w, self.h);
        let pad = radius * 3 + 1;
        let (y0, y1) = (self.y0.saturating_sub(pad), (self.y1 + pad).min(h));
        self.y0 = y0;
        self.y1 = y1;
        let mut tmp = vec![0.0f32; self.data.len()];
        let norm = 1.0 / (2 * radius + 1) as f32;
        for _ in 0..3 {
            // along the rows
            for y in y0..y1 {
                let (src, dst) = (&self.data[y * w * 3..(y + 1) * w * 3], &mut tmp[y * w * 3..(y + 1) * w * 3]);
                let mut s = [0.0f32; 3];
                for x in 0..=radius.min(w - 1) {
                    for ch in 0..3 {
                        s[ch] += src[x * 3 + ch];
                    }
                }
                for x in 0..w {
                    for ch in 0..3 {
                        dst[x * 3 + ch] = s[ch] * norm;
                    }
                    if x + radius + 1 < w {
                        for ch in 0..3 {
                            s[ch] += src[(x + radius + 1) * 3 + ch];
                        }
                    }
                    if x >= radius {
                        for ch in 0..3 {
                            s[ch] -= src[(x - radius) * 3 + ch];
                        }
                    }
                }
            }
            // and down the columns, a whole row of running sums at a time
            let mut sum = vec![0.0f32; w * 3];
            for y in y0..(y0 + radius + 1).min(y1) {
                for (s, v) in sum.iter_mut().zip(&tmp[y * w * 3..(y + 1) * w * 3]) {
                    *s += v;
                }
            }
            for y in y0..y1 {
                for (d, s) in self.data[y * w * 3..(y + 1) * w * 3].iter_mut().zip(&sum) {
                    *d = s * norm;
                }
                if y + radius + 1 < y1 {
                    for (s, v) in sum.iter_mut().zip(&tmp[(y + radius + 1) * w * 3..(y + radius + 2) * w * 3]) {
                        *s += v;
                    }
                }
                if y >= y0 + radius {
                    for (s, v) in sum.iter_mut().zip(&tmp[(y - radius) * w * 3..(y - radius + 1) * w * 3]) {
                        *s -= v;
                    }
                }
            }
        }
    }

    /// Blur what was gathered and lay it over the canvas as light.
    pub fn apply(&mut self, c: &mut Canvas, radius: usize) {
        if self.y1 <= self.y0 {
            return;
        }
        if radius > 0 {
            self.blur(radius);
        }
        let (py0, py1) = ((self.y0.saturating_sub(1)) * self.div, ((self.y1 + 1) * self.div).min(c.h));
        let cols = coarse_cols(c.w, self.w, self.div);
        let mut row = Vec::new();
        for y in py0..py1 {
            coarse_row(&self.data, self.w, self.h, 3, self.div, y, &mut row);
            let lit = |cell: &[f32]| cell[0] + cell[1] + cell[2] > 0.4;
            let Some(first) = row.chunks_exact(3).position(lit) else { continue };
            let last = row.chunks_exact(3).rposition(lit).unwrap_or(first);
            let (x0, x1) = (first.saturating_sub(1) * self.div, ((last + 2) * self.div).min(c.w));
            let dst = &mut c.px[(y * c.w + x0) * 3..(y * c.w + x1) * 3];
            for (p, &(ix, tx)) in dst.chunks_exact_mut(3).zip(&cols[x0..x1]) {
                let at = ix as usize * 3;
                for ch in 0..3 {
                    let v = (row[at + ch] + (row[at + 3 + ch] - row[at + ch]) * tx) * (1.0 / 255.0);
                    p[ch] += (255.0 - p[ch]).max(0.0) * v.min(1.0);
                }
            }
        }
    }
}

/// A wash of one colour whose strength is worked out on a coarse grid: for glows so wide and soft
/// that every pixel would be wasted effort.
pub struct Veil {
    div: usize,
    w: usize,
    h: usize,
    a: Vec<f32>,
}

impl Veil {
    pub fn new(w: usize, h: usize, div: usize) -> Veil {
        let div = div.max(1);
        let (bw, bh) = (w / div + 3, h / div + 3);
        Veil { div, w: bw, h: bh, a: vec![0.0; bw * bh] }
    }

    /// Add a glow, brightest in the middle of an ellipse and gone at its edge. Glows lie over one another.
    pub fn glow(&mut self, cx: f32, cy: f32, rx: f32, ry: f32, a: f32, power: u8) {
        if a <= 0.002 || rx <= 0.0 || ry <= 0.0 {
            return;
        }
        let d = self.div as f32;
        for y in 0..self.h {
            let dy = ((y as f32 + 0.5) * d - cy) / ry;
            if dy.abs() >= 1.0 {
                continue;
            }
            for x in 0..self.w {
                let dx = ((x as f32 + 0.5) * d - cx) / rx;
                let q = (1.0 - dx * dx - dy * dy).max(0.0);
                if q > 0.0 {
                    let f = match power {
                        1 => q * q * (3.0 - 2.0 * q),
                        2 => q * q,
                        _ => q * q * q,
                    };
                    let v = &mut self.a[y * self.w + x];
                    *v = 1.0 - (1.0 - *v) * (1.0 - (a * f).min(1.0));
                }
            }
        }
    }

    pub fn apply(&self, c: &mut Canvas, col: Rgb, mode: Blend) {
        let cols = coarse_cols(c.w, self.w, self.div);
        let unit = [col[0] / 255.0, col[1] / 255.0, col[2] / 255.0];
        let mut row = Vec::new();
        for y in 0..c.h {
            coarse_row(&self.a, self.w, self.h, 1, self.div, y, &mut row);
            let Some(first) = row.iter().position(|v| *v > 0.002) else { continue };
            let last = row.iter().rposition(|v| *v > 0.002).unwrap_or(first);
            let (x0, x1) = (first.saturating_sub(1) * self.div, ((last + 2) * self.div).min(c.w));
            let dst = &mut c.px[(y * c.w + x0) * 3..(y * c.w + x1) * 3];
            for (p, &(ix, tx)) in dst.chunks_exact_mut(3).zip(&cols[x0..x1]) {
                let ix = ix as usize;
                let k = row[ix] + (row[ix + 1] - row[ix]) * tx;
                match mode {
                    Blend::Over => {
                        p[0] += (col[0] - p[0]) * k;
                        p[1] += (col[1] - p[1]) * k;
                        p[2] += (col[2] - p[2]) * k;
                    }
                    Blend::Screen => {
                        p[0] += (255.0 - p[0]).max(0.0) * unit[0] * k;
                        p[1] += (255.0 - p[1]).max(0.0) * unit[1] * k;
                        p[2] += (255.0 - p[2]).max(0.0) * unit[2] * k;
                    }
                }
            }
        }
    }
}

// ── files ────────────────────────────────────────────────────────────────────

pub fn save_png(path: &str, w: usize, h: usize, px: &[u32]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("cannot write {}: {}", path, e))?;
    let mut enc = png::Encoder::new(BufWriter::with_capacity(1 << 16, file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.set_compression(png::Compression::Fast);
    let mut writer = enc.write_header().map_err(|e| e.to_string())?;
    let mut rgb = Vec::with_capacity(w * h * 3);
    for p in px {
        rgb.extend_from_slice(&[(p >> 16) as u8, (p >> 8) as u8, *p as u8]);
    }
    writer.write_image_data(&rgb).map_err(|e| e.to_string())?;
    writer.finish().map_err(|e| e.to_string())
}
