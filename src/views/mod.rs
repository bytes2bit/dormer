//! Views: the places the same sky can be seen from.
//!
//! A view owns everything in front of the sky. The sun, moon, stars, clouds
//! and weather are painted the same way for all of them; a view says where
//! its horizon lies, lays its ground out once for each screen size (a
//! `Stage`), paints it into the still frame, and draws the few things of its
//! own that move.
//!
//! Most of what moves is not drawn by a view at all. While painting, a view
//! leaves `Anchors` behind: here is a chimney, this window has a television
//! in it, this light blinks. The live layer knows how to animate each kind,
//! so a new view gets smoke, flicker and twinkle by pointing at where they
//! go.
//!
//! To add a view: write a module with a `View` and its `Stage`, and add it
//! to `ALL`. Everything else (the dev panel, `--view`, `dormer view`) finds
//! it there.

pub mod coast;
pub mod downtown;
pub mod kit;
pub mod rooftops;

use crate::color::Rgb;
use crate::gfx::{Bloom, Canvas, Depth, Draw, Frame};
use crate::props::Layout;
use crate::scene::Scene;

/// A point of light the live layer makes twinkle.
pub struct Spark {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub a: f32,
    pub col: Rgb,
    /// how near the horizon it is, 0 to 1: low stars twinkle hardest
    pub low: f32,
}

/// Somewhere smoke or steam rises from.
#[derive(Clone, Copy)]
pub struct Smoke {
    pub x: f32,
    pub y: f32,
    pub scale: f32,
    pub col: Rgb,
    /// the layer it is hidden behind (see `Depth`); past the last one, nothing hides it
    pub behind: usize,
}

/// A light that flashes: a beacon on a mast, a buoy.
pub struct Blink {
    pub x: f32,
    pub y: f32,
    pub r: f32,
    pub col: Rgb,
    /// seconds from one flash to the next
    pub every: f32,
    /// where in that cycle it starts, 0 to 1, so that neighbours do not flash together
    pub phase: f32,
    pub behind: Option<usize>,
}

/// A glow that swells and fades slowly: a lamp someone is working by.
pub struct Breath {
    pub x: f32,
    pub y: f32,
    pub rx: f32,
    pub ry: f32,
    pub col: Rgb,
    pub strength: f32,
    /// it gutters instead of breathing (the battery is nearly flat)
    pub low: bool,
    pub behind: Option<usize>,
}

/// Places in the picture that the live layer needs to find again.
#[derive(Default)]
pub struct Anchors {
    pub stars: Vec<Spark>,
    pub smoke: Vec<Smoke>,
    /// smokes only while the processor is busy
    pub stove: Option<Smoke>,
    /// windows with a television behind them: x, y, w, h, and the layer that hides them
    pub flicker: Vec<([f32; 4], usize)>,
    pub breath: Vec<Breath>,
    pub blink: Vec<Blink>,
    /// small coloured bulbs: x, y, colour, the layer that hides them
    pub twinkle: Vec<(f32, f32, Rgb, usize)>,
    /// where the birds sit until the hour strikes
    pub perch: Option<(f32, f32)>,
    /// where music drifts out from
    pub music: Option<(f32, f32)>,
    /// patches of greenery where fireflies come out on summer nights: x0, y0, x1, y1
    pub glades: Vec<[f32; 4]>,
    /// are there trees about for the wind to take leaves and petals from
    pub leaves: bool,
}

/// Which windows are lit at this moment.
pub struct Lights {
    /// the share of all windows that are lit: how awake the place is
    pub share: f32,
    /// one entry a window, in the stage's own order
    pub on: Vec<Option<Rgb>>,
}

/// Something about a view that the dev panel can turn by hand.
pub struct Knob {
    pub key: &'static str,
    pub label: &'static str,
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
    pub big: f64,
    pub show: fn(f64) -> String,
}

/// A view laid out for one size of screen.
pub trait Stage {
    fn lay(&self) -> &Layout;

    /// What hides what, for the things that move.
    fn depth(&self) -> &Depth;

    fn lights(&self, sc: &Scene) -> Lights;

    /// Paint everything that stands in front of the sky, far to near. When `sc.live` is set the
    /// things that move are left out: `alive` draws them.
    fn ground(&self, d: &mut Draw<Canvas>, sc: &Scene, lights: &Lights, bloom: &mut Bloom, out: &mut Anchors);

    /// Draw this view's own moving things over the still frame. `t` is seconds that run smoothly
    /// from when the wallpaper started; `now` is the time on the clock.
    fn alive(&self, d: &mut Draw<Frame>, sc: &Scene, a: &Anchors, t: f32, now: f64);

    /// Is something of this view's moving fast enough to want every frame?
    fn lively(&self, _sc: &Scene, _now: f64) -> bool {
        false
    }
}

pub trait View: Sync {
    /// What it is called on the command line and in the config.
    fn id(&self) -> &'static str;

    /// One line saying what is in the picture.
    fn about(&self) -> &'static str;

    /// Where the horizon lies, as a fraction of the design's height.
    fn horizon(&self) -> f64 {
        crate::sky::HORIZON
    }

    fn knobs(&self) -> &'static [Knob] {
        &[]
    }

    /// What a knob rests on when nobody is holding it.
    fn real(&self, _key: &str, _sc: &Scene) -> f64 {
        0.0
    }

    fn stage(&self, seed: u64, w: usize, h: usize) -> Box<dyn Stage>;

    /// The lines of `dormer explain` that are about this view: a short label, and what to say.
    fn words(&self, sc: &Scene) -> Vec<(&'static str, String)>;
}

pub static ALL: [&dyn View; 3] = [&rooftops::Rooftops, &downtown::Downtown, &coast::Coast];

pub fn find(id: &str) -> Option<&'static dyn View> {
    ALL.iter().copied().find(|v| v.id() == id)
}

pub fn names() -> String {
    ALL.iter().map(|v| v.id()).collect::<Vec<_>>().join(" | ")
}

/// What the house lines of `dormer explain` say in every view: the same signals, staged differently.
pub struct Yours {
    pub lamp: &'static str,
    /// what one running container shows as, and what several do
    pub containers: (&'static str, &'static str),
    pub smoke: &'static str,
}

pub fn yours(sc: &Scene, y: &Yours) -> String {
    let m = &sc.machine;
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    let mut out = vec![
        format!("{} {} ({} agent session{})", y.lamp, if m.agents > 0 { "on" } else { "off" }, m.agents, plural(m.agents)),
        format!("{} {}", m.containers.min(8), if m.containers == 1 { y.containers.0 } else { y.containers.1 }),
    ];
    if m.load > 0.35 {
        out.push(format!("{} (processor busy)", y.smoke));
    }
    if m.audio {
        out.push("music drifting out".to_string());
    }
    if let Some(b) = m.battery {
        if b < 0.2 {
            out.push(format!("the lamp burning low (battery {}%)", (b * 100.0).round()));
        }
    }
    out.join(", ")
}

/// The colours of the machine's lamp: what the window shows, and what it throws around it.
pub fn lamp_colours(sc: &Scene) -> (Rgb, Rgb, bool) {
    use crate::color::{mix, AMBER, ROSE};
    let low = matches!(sc.machine.battery, Some(b) if b < 0.2);
    let lamp = mix(AMBER, [255.0, 236.0, 190.0], 0.45);
    if low {
        (mix(lamp, ROSE, 0.55), ROSE, true) // nearly out of battery: the lamp burns low and red
    } else {
        (lamp, AMBER, false)
    }
}
