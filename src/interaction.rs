//! Where a pointer is, in the picture's own units, and how fast it is going.
//!
//! The pointer itself belongs to Wayland (`pointer.rs`); what it means belongs
//! to the scene (`story/`). This is only the coordinates between them:
//! logical surface points to design units, parsing a point given on the
//! command line, and the velocity a thrown thing leaves the hand with, taken
//! from a few recent samples with their timestamps.

use crate::gfx::P;
use crate::props::Layout;

pub fn parse_point(text: &str) -> Option<P> {
    let (x, y) = text.split_once(',')?;
    let p: P = (x.trim().parse().ok()?, y.trim().parse().ok()?);
    (p.0.is_finite() && p.1.is_finite()).then_some(p)
}

/// Wayland coordinates are logical, not buffer pixels (including rotated outputs).
pub fn pointer_position(p: (f64, f64), logical: (u32, u32), lay: &Layout) -> Option<P> {
    if logical.0 == 0 || logical.1 == 0 || !p.0.is_finite() || !p.1.is_finite()
        || p.0 < 0.0 || p.1 < 0.0 || p.0 >= logical.0 as f64 || p.1 >= logical.1 as f64 {
        return None;
    }
    Some(lay.back(p.0 as f32 * lay.w / logical.0 as f32, p.1 as f32 * lay.h / logical.1 as f32))
}

/// The last few pointer positions with the time they were seen, for a release velocity.
pub struct Track {
    samples: [(f64, P); 8],
    count: usize,
    next: usize,
}

/// Only the last stretch of the movement says how it was let go.
const WINDOW: f64 = 0.10;

impl Track {
    pub fn new() -> Track {
        Track { samples: [(0.0, (0.0, 0.0)); 8], count: 0, next: 0 }
    }

    pub fn reset(&mut self) {
        self.count = 0;
        self.next = 0;
    }

    /// A position at a time (seconds, any clock that does not go backwards).
    pub fn push(&mut self, t: f64, p: P) {
        if !t.is_finite() || !p.0.is_finite() || !p.1.is_finite() {
            return;
        }
        // a clock that went backwards starts the track again
        if self.count > 0 {
            let last = self.samples[(self.next + 7) % 8].0;
            if t < last {
                self.reset();
            }
        }
        self.samples[self.next] = (t, p);
        self.next = (self.next + 1) % 8;
        self.count = (self.count + 1).min(8);
    }

    /// Design units a second, over the recent window; zero when the pointer had stopped or
    /// there is too little to tell. Bounded to what a hand could do.
    pub fn velocity(&self, now: f64, limit: f32) -> P {
        if self.count < 2 || !now.is_finite() {
            return (0.0, 0.0);
        }
        let newest = self.samples[(self.next + 7) % 8];
        // the pointer rested before the release: nothing is thrown
        if now - newest.0 > WINDOW {
            return (0.0, 0.0);
        }
        let mut oldest = newest;
        for k in 1..self.count {
            let s = self.samples[(self.next + 8 - 1 - k) % 8];
            if newest.0 - s.0 > WINDOW {
                break;
            }
            oldest = s;
        }
        let dt = newest.0 - oldest.0;
        if dt < 0.012 {
            return (0.0, 0.0);
        }
        let v = ((newest.1 .0 - oldest.1 .0) / dt as f32, (newest.1 .1 - oldest.1 .1) / dt as f32);
        let speed = (v.0 * v.0 + v.1 * v.1).sqrt();
        if !speed.is_finite() {
            return (0.0, 0.0);
        }
        if speed > limit {
            (v.0 * limit / speed, v.1 * limit / speed)
        } else {
            v
        }
    }
}

impl Default for Track {
    fn default() -> Self {
        Track::new()
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use crate::props::Layout;
    use crate::scene::Scene;

    #[test]
    fn pointer_matches_fractional_and_portrait_objects() {
        let lay = Layout::new(2000, 1250, 0.7);
        assert_eq!(pointer_position((800.0, 800.0), (1600, 1000), &lay), Some((800.0, 800.0)));
        let portrait = Layout::new(540, 960, 0.7);
        let p = pointer_position((270.0, 900.0), (540, 960), &portrait).unwrap();
        assert!((p.0 - 800.0).abs() < 0.01);
        assert!((p.1 - 822.2222).abs() < 0.01);
        assert!(pointer_position((f64::NAN, 0.0), (540, 960), &portrait).is_none());
        assert!(pointer_position((-1.0, 0.0), (540, 960), &portrait).is_none());
        assert!(pointer_position((0.0, 0.0), (0, 0), &portrait).is_none());
    }

    #[test]
    fn preview_coordinates_require_two_finite_numbers() {
        assert_eq!(parse_point("850,920"), Some((850.0, 920.0)));
        assert_eq!(parse_point(" 1.5, -12 "), Some((1.5, -12.0)));
        for input in ["1", "1,2,3", "NaN,2", "1,inf", "x,4"] {
            assert!(parse_point(input).is_none());
        }
    }

    #[test]
    fn release_velocity_comes_from_recent_timestamps_and_is_bounded() {
        let mut t = Track::new();
        assert_eq!(t.velocity(0.0, 900.0), (0.0, 0.0));
        // moving right at 500 units a second for a tenth of a second
        for k in 0..6 {
            t.push(1.0 + k as f64 * 0.02, (100.0 + k as f32 * 10.0, 50.0));
        }
        let v = t.velocity(1.10, 900.0);
        assert!((v.0 - 500.0).abs() < 1.0 && v.1.abs() < 1e-3, "{:?}", v);
        // the same motion, let go much later: it had stopped, nothing is thrown
        assert_eq!(t.velocity(2.0, 900.0), (0.0, 0.0));
        // faster than a hand: clamped, direction kept
        let mut fast = Track::new();
        fast.push(0.0, (0.0, 0.0));
        fast.push(0.02, (100.0, 100.0));
        let v = fast.velocity(0.03, 900.0);
        assert!(((v.0 * v.0 + v.1 * v.1).sqrt() - 900.0).abs() < 0.5);
        assert!((v.0 - v.1).abs() < 1e-3);
        // nonsense is ignored, and a clock that goes backwards starts again
        let mut odd = Track::new();
        odd.push(f64::NAN, (1.0, 1.0));
        odd.push(1.0, (f32::NAN, 1.0));
        assert_eq!(odd.velocity(1.0, 900.0), (0.0, 0.0));
        odd.push(5.0, (0.0, 0.0));
        odd.push(5.05, (10.0, 0.0));
        odd.push(1.0, (500.0, 0.0));
        assert_eq!(odd.velocity(1.0, 900.0), (0.0, 0.0));
    }

    /// A scene for tests that need one: Tangier on an autumn evening, dry or wet.
    pub fn scene(view: &dyn crate::views::View, rain: bool) -> Scene {
        let mut wx = crate::sky::from_wmo(if rain { 61 } else { 0 });
        wx.temp = 18.0;
        crate::scene::scene(crate::scene::Inputs {
            ts: 1791925200.0, wx: &wx, mode: "dark", machine: Default::default(),
            place: &crate::scene::Place { lat: 35.7758, lon: -5.7909, name: String::new() },
            drift: None, top: None, rates: Default::default(), horizon: view.horizon(),
            knobs: &[], void: view.sky() == crate::views::Sky::Void,
        })
    }
}
