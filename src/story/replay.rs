//! Running a scene without a screen, and recording what it did.
//!
//! A replay is a small JSON file: a view, a seed, a duration, timed pointer
//! gestures and named triggers, and the times at which to save a frame. It is
//! a test input, not a language. `run` plays it through exactly the way the
//! live wallpaper plays a scene (the same story world, the same base repaint
//! of only what changed) and writes PNG frames and a trace of what happened.
//! The same seed and inputs give the same frames.

use std::path::Path;

use serde_json::{json, Value};

use super::physics::MAX_THROW;
use super::{Facts, Gesture, StoryWorld};
use crate::gfx::{Draw, Frame, Rect, Shaper};
use crate::live::{Sources, World};
use crate::paint::{self, Cache};
use crate::scene::Scene;
use crate::settings::RuntimeSettings;
use crate::Moment;

/// What a replay may ask for.
pub const MAX_INPUTS: usize = 4096;
pub const MAX_SECONDS: f64 = 600.0;
pub const MAX_FRAMES: usize = 64;

#[derive(Clone, Debug, PartialEq)]
pub enum What {
    Gesture(Gesture),
    Trigger(String),
    /// the panel's controls
    Pause(bool),
    Reset,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Input {
    pub t: f64,
    pub what: What,
}

#[derive(Clone, Debug)]
pub struct Replay {
    pub view: String,
    pub seed: u64,
    pub seconds: f64,
    pub inputs: Vec<Input>,
    pub frames: Vec<f64>,
    /// optional: the moment and the weather, in the words the command line uses
    pub at: Option<String>,
    pub weather: Option<String>,
    pub mode: Option<String>,
    pub size: Option<(usize, usize)>,
}

#[derive(Clone, Debug, PartialEq)]
pub enum ReplayError {
    Read(String),
    Invalid(String),
}

impl std::fmt::Display for ReplayError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ReplayError::Read(e) => write!(f, "cannot read the replay: {}", e),
            ReplayError::Invalid(e) => write!(f, "the replay is not usable: {}", e),
        }
    }
}

impl std::error::Error for ReplayError {}

fn bad<T>(why: impl Into<String>) -> Result<T, ReplayError> {
    Err(ReplayError::Invalid(why.into()))
}

fn num(v: &Value, key: &str, what: &str) -> Result<f64, ReplayError> {
    match v.get(key).and_then(|x| x.as_f64()) {
        Some(n) if n.is_finite() => Ok(n),
        Some(_) => bad(format!("{}: {} is not a finite number", what, key)),
        None => bad(format!("{}: needs a number for {}", what, key)),
    }
}

impl Replay {
    pub fn read(path: &Path) -> Result<Replay, ReplayError> {
        let text = std::fs::read_to_string(path).map_err(|e| ReplayError::Read(format!("{}: {}", path.display(), e)))?;
        Replay::parse(&text)
    }

    pub fn parse(text: &str) -> Result<Replay, ReplayError> {
        let v: Value = serde_json::from_str(text).map_err(|e| ReplayError::Invalid(e.to_string()))?;
        let view = v.get("view").and_then(|x| x.as_str()).map(str::to_string);
        let Some(view) = view else { return bad("it needs a \"view\"") };
        let seed = v.get("seed").and_then(|x| x.as_u64()).unwrap_or(12);
        let seconds = num(&v, "seconds", "the replay")?;
        if !(0.0..=MAX_SECONDS).contains(&seconds) {
            return bad(format!("{} seconds is outside 0 to {}", seconds, MAX_SECONDS));
        }
        let mut inputs = Vec::new();
        if let Some(list) = v.get("inputs").and_then(|x| x.as_array()) {
            if list.len() > MAX_INPUTS {
                return bad(format!("{} inputs is more than the {} allowed", list.len(), MAX_INPUTS));
            }
            let mut last = 0.0;
            for (n, item) in list.iter().enumerate() {
                let at = format!("input {}", n + 1);
                let t = num(item, "t", &at)?;
                if t < 0.0 || t > seconds {
                    return bad(format!("{}: time {} is outside the replay", at, t));
                }
                if t < last {
                    return bad(format!("{}: time {} goes backwards from {}", at, t, last));
                }
                last = t;
                let point = || -> Result<(f32, f32), ReplayError> { Ok((num(item, "x", &at)? as f32, num(item, "y", &at)? as f32)) };
                let what = if let Some(name) = item.get("trigger").and_then(|x| x.as_str()) {
                    What::Trigger(name.to_string())
                } else {
                    match item.get("gesture").and_then(|x| x.as_str()) {
                        Some("hover") => What::Gesture(Gesture::Hover(point()?)),
                        Some("press") => What::Gesture(Gesture::Press(point()?)),
                        Some("drag") => What::Gesture(Gesture::Drag(point()?)),
                        Some("release") => {
                            let (vx, vy) = (item.get("vx").map_or(Ok(0.0), |_| num(item, "vx", &at))?, item.get("vy").map_or(Ok(0.0), |_| num(item, "vy", &at))?);
                            if vx.abs() > MAX_THROW as f64 * 4.0 || vy.abs() > MAX_THROW as f64 * 4.0 {
                                return bad(format!("{}: a release velocity of {},{} is not a hand's", at, vx, vy));
                            }
                            What::Gesture(Gesture::Release { at: point()?, velocity: (vx as f32, vy as f32) })
                        }
                        Some("cancel") => What::Gesture(Gesture::Cancel),
                        Some("pause") => What::Pause(true),
                        Some("resume") => What::Pause(false),
                        Some("reset") => What::Reset,
                        Some(other) => return bad(format!("{}: there is no gesture '{}'", at, other)),
                        None => return bad(format!("{}: needs a \"gesture\" or a \"trigger\"", at)),
                    }
                };
                inputs.push(Input { t, what });
            }
        }
        let mut frames = Vec::new();
        if let Some(list) = v.get("frames").and_then(|x| x.as_array()) {
            if list.len() > MAX_FRAMES {
                return bad(format!("{} frames is more than the {} allowed", list.len(), MAX_FRAMES));
            }
            for f in list {
                match f.as_f64() {
                    Some(t) if t.is_finite() && (0.0..=seconds).contains(&t) => frames.push(t),
                    _ => return bad(format!("frame time {} is not inside the replay", f)),
                }
            }
        }
        let size = match v.get("size").and_then(|x| x.as_str()) {
            Some(s) => Some(crate::sources::parse_size(s).ok_or_else(|| ReplayError::Invalid(format!("size '{}' is not like 640x400", s)))?),
            None => None,
        };
        let text = |k: &str| v.get(k).and_then(|x| x.as_str()).map(str::to_string);
        Ok(Replay { view, seed, seconds, inputs, frames, at: text("at"), weather: text("weather"), mode: text("mode"), size })
    }
}

#[derive(Clone, Debug, Default)]
pub struct ReplayReport {
    pub frames: Vec<String>,
    pub trace: String,
    pub ticks: u64,
    pub base_repaints: u32,
    /// the most pixels any one repaint wrote, as a share of the screen
    pub widest_repaint: f32,
}

/// One scene being played: the base picture, the world and the live layer over it.
pub struct Session {
    pub cache: Cache,
    pub sc: Scene,
    pub story: StoryWorld,
    pub shown: Vec<u32>,
    anchors: crate::views::Anchors,
    live: World,
    sources: Sources,
    depth: Option<crate::gfx::Depth>,
    facts: Facts,
    pub base_repaints: u32,
    pub widest_repaint: f32,
    since_paint: f64,
    now: f64,
}

impl Session {
    pub fn new(m: &Moment, size: (usize, usize), seed: u64) -> Result<Session, String> {
        Session::with_live_seed(m, size, seed, seed)
    }

    /// `live_seed` is the seed of the ambient world (birds, rain): kept apart from the scene's so
    /// that exports that predate the living scene keep their pictures.
    pub fn with_live_seed(m: &Moment, size: (usize, usize), seed: u64, live_seed: u64) -> Result<Session, String> {
        let cache = Cache::new(m.view, seed, size.0, size.1);
        // a view with nothing to change (the moon) has a scene with nothing in it
        let def = cache.stage.story().unwrap_or_else(|| super::Definition { view: m.view.id(), ..Default::default() });
        let story = StoryWorld::new(def, seed).map_err(|e| e.to_string())?;
        let mut sc = crate::build(m, None);
        sc.live = true;
        let (cv, anchors) = paint::render_state(&mut sc, &cache, &story.snapshot());
        let shown = cv.finish(sc.p.grain);
        let mut live = World::new(live_seed);
        live.retune(&sc, anchors.flicker.len());
        let sources = Sources::from(&anchors, &cache.lay);
        let facts = super::facts_of(&sc);
        Ok(Session { cache, sc, story, shown, anchors, live, sources, depth: None, facts, base_repaints: 0, widest_repaint: 0.0, since_paint: 1.0, now: 0.0 })
    }

    pub fn time(&self) -> f64 {
        self.now
    }

    /// Hold the wind at this strength (-1.6 to 1.6) whatever the sky says: for a test.
    #[cfg(test)]
    pub fn set_wind(&mut self, wind_x: f32) {
        self.facts.wind_x = wind_x;
        self.facts.wind = wind_x.abs() * 16.0;
    }

    pub fn live_everywhere(&self) -> bool {
        self.live.everywhere()
    }

    pub fn live_mut(&mut self) -> &mut World {
        &mut self.live
    }

    pub fn sources(&self) -> &Sources {
        &self.sources
    }

    pub fn anchors(&self) -> &crate::views::Anchors {
        &self.anchors
    }

    pub fn gesture(&mut self, g: Gesture) {
        self.story.gesture(g);
    }

    /// Run `seconds` of the scene in presentation frames of `frame`.
    pub fn advance(&mut self, seconds: f64, frame: f64) {
        let mut left = seconds;
        while left > 1e-9 {
            let dt = left.min(frame);
            left -= dt;
            self.now += dt;
            self.story.step_facts(dt, &self.facts);
            self.live.step(dt as f32, &self.sc, &self.sources, self.sc.ts + self.now);
            self.since_paint += dt;
            self.repaint_if_due();
        }
    }

    /// The base picture is repainted only where the scene has changed it, and not more often
    /// than a few times a second.
    fn repaint_if_due(&mut self) {
        if !self.story.has_base_changes() || self.since_paint < 0.1 {
            return;
        }
        let regions = self.story.take_base_changes();
        let lay = self.cache.lay;
        let (cv, _) = paint::render_state(&mut self.sc, &self.cache, &self.story.snapshot());
        let fresh = cv.finish(self.sc.p.grain);
        let rects: Vec<Rect> = regions.iter().map(|r| super::pixel_rect(r, &lay, super::LIGHT_MARGIN)).collect();
        let written = crate::buffer::install_regions(&mut self.shown, &fresh, self.cache.w, &rects);
        self.widest_repaint = self.widest_repaint.max(written as f32 / (self.cache.w * self.cache.h) as f32);
        self.base_repaints += 1;
        self.since_paint = 0.0;
        self.depth = self.cache.stage.story_depth(&self.story.snapshot());
    }

    /// The picture as it is now: the base, and the moving things over it.
    pub fn frame(&mut self) -> Vec<u32> {
        // anything not yet repainted is repainted so the frame is true to the state
        self.since_paint = 1.0;
        self.repaint_if_due();
        let mut px = self.shown.clone();
        let mut dirty = Vec::new();
        let mut shaper = Shaper::new();
        self.draw_over(&mut px, &mut dirty, &mut shaper, false);
        px
    }

    /// Draw the moving things over `px` (which must hold the base), noting what was touched.
    pub fn draw_over(&self, px: &mut [u32], dirty: &mut Vec<Rect>, shaper: &mut Shaper, full_damage: bool) {
        let depth = self.depth.as_ref().unwrap_or_else(|| self.cache.stage.depth());
        let mut f = Frame { w: self.cache.w, h: self.cache.h, px, dirty, depth: Some(depth), behind: None, full_damage };
        let mut d = Draw::new(&mut f, shaper);
        self.live.draw(&mut d, &self.sc, &self.cache, &self.anchors, self.sc.ts + self.now, Some(&self.story));
    }

    /// The 20-second refresh: the sky is worked out afresh and the whole base painted again,
    /// from the scene's own state, so nothing that has been broken is mended by it.
    pub fn refresh(&mut self) {
        let (cv, anchors) = paint::render_state(&mut self.sc, &self.cache, &self.story.snapshot());
        self.shown = cv.finish(self.sc.p.grain);
        self.anchors = anchors;
        self.depth = self.cache.stage.story_depth(&self.story.snapshot());
        self.story.take_base_changes();
        self.since_paint = 0.0;
    }

    /// Bring the base up to date with the scene before drawing over it.
    pub fn settle(&mut self) {
        self.since_paint = 1.0;
        self.repaint_if_due();
    }

    /// A line of what the scene is doing, for the trace.
    pub fn trace_line(&self) -> Value {
        let defs = self.story.definition();
        let actors: Vec<Value> = self
            .story
            .actors()
            .iter()
            .map(|a| json!({"kind": format!("{:?}", a.kind), "role": format!("{:?}", a.role), "x": a.pos.0.round(), "y": a.pos.1.round(), "pose": format!("{:?}", a.pose), "state": format!("{:?}", a.state)}))
            .collect();
        let objects: Vec<Value> = self
            .story
            .objects()
            .iter()
            .filter(|(_, s)| s.damage != super::Damage::Intact || s.pose != super::Pose::default() || !s.at_rest() || !s.visible)
            .map(|(id, s)| {
                let name = defs.object(id).map_or("?", |d| d.name);
                json!({"object": name, "damage": format!("{:?}", s.damage), "attachment": format!("{:?}", s.attachment), "dx": s.pose.dx.round(), "dy": s.pose.dy.round(), "visible": s.visible})
            })
            .collect();
        let stories: Vec<Value> = self.story.director().status(defs).iter().map(|(n, stage, esc, prot)| json!({"story": n, "stage": stage, "escalations": esc, "protected": prot})).collect();
        json!({
            "t": (self.now * 100.0).round() / 100.0,
            "tick": self.story.tick(),
            "stories": stories,
            "actors": actors,
            "objects": objects,
            "bodies": self.story.bodies().len(),
            "puddles": self.story.physics().patches.len(),
        })
    }
}

/// Play a replay and write its frames and trace into `out`.
pub fn run(replay: &Replay, out: &Path, settings: &RuntimeSettings) -> Result<ReplayReport, ReplayError> {
    let view = crate::views::find(&replay.view).ok_or_else(|| ReplayError::Invalid(crate::views::refusal(&replay.view)))?;
    std::fs::create_dir_all(out).map_err(|e| ReplayError::Read(format!("{}: {}", out.display(), e)))?;
    // a moment that is the same every time: a fixed evening unless the replay says otherwise
    let mut m = crate::replay_moment(view, replay.at.as_deref(), replay.weather.as_deref(), replay.mode.as_deref(), settings).map_err(ReplayError::Invalid)?;
    m.view = view;
    let size = replay.size.unwrap_or((640, 400));
    let mut session = Session::new(&m, size, replay.seed).map_err(ReplayError::Invalid)?;
    let mut report = ReplayReport::default();
    // events: inputs and frame requests, in time order, inputs first at the same instant
    let mut inputs = replay.inputs.iter().peekable();
    let mut frames = replay.frames.iter().copied().collect::<Vec<f64>>();
    frames.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut frames = frames.into_iter().enumerate().peekable();
    let mut trace: Vec<Value> = Vec::new();
    let step: f64 = 1.0 / 30.0;
    let mut next_trace = 0.0;
    let mut t = 0.0;
    while t < replay.seconds + 1e-9 {
        while let Some(i) = inputs.peek() {
            if i.t > t + 1e-9 {
                break;
            }
            let i = inputs.next().unwrap();
            match &i.what {
                What::Gesture(g) => session.gesture(*g),
                What::Trigger(name) => {
                    let id = session.story.definition().stories.iter().find(|s| s.name == name).map(|s| s.id);
                    match id {
                        Some(id) => {
                            session.story.trigger(id);
                        }
                        None => return bad(format!("there is no story '{}' in the view '{}'", name, replay.view)),
                    }
                }
                What::Pause(p) => session.story.set_paused(*p),
                What::Reset => session.story.reset(),
            }
        }
        while let Some((k, ft)) = frames.peek().copied() {
            if ft > t + 1e-9 {
                break;
            }
            frames.next();
            let path = out.join(format!("frame-{:02}-{:06.2}s.png", k + 1, ft));
            let px = session.frame();
            crate::gfx::save_png(&path.to_string_lossy(), session.cache.w, session.cache.h, &px).map_err(ReplayError::Read)?;
            report.frames.push(path.to_string_lossy().into_owned());
        }
        if t + 1e-9 >= next_trace {
            trace.push(session.trace_line());
            next_trace += 0.25;
        }
        if t >= replay.seconds {
            break;
        }
        let dt = step.min(replay.seconds - t);
        session.advance(dt, step);
        t += dt;
    }
    let trace_path = out.join("trace.json");
    let text = serde_json::to_string_pretty(&json!({"view": replay.view, "seed": replay.seed, "seconds": replay.seconds, "trace": trace})).unwrap_or_default();
    std::fs::write(&trace_path, &text).map_err(|e| ReplayError::Read(format!("{}: {}", trace_path.display(), e)))?;
    report.trace = trace_path.to_string_lossy().into_owned();
    report.ticks = session.story.ticks_run;
    report.base_repaints = session.base_repaints;
    report.widest_repaint = session.widest_repaint;
    Ok(report)
}

/// A session on a named view at a named moment, for the tests of each view's stories.
#[cfg(test)]
pub fn test_session(view: &str, at: &str, weather: &str, size: (usize, usize)) -> Session {
    let view = crate::views::find(view).unwrap();
    let m = crate::replay_moment(view, Some(at), Some(weather), None, &RuntimeSettings::default()).unwrap();
    Session::new(&m, size, 12).unwrap()
}

/// Helpers shared by the tests of each view's stories.
#[cfg(test)]
pub mod scripted {
    use super::*;
    use crate::story::model::ObjectId;

    pub fn at_of(s: &Session, id: ObjectId) -> crate::gfx::P {
        s.story.definition().object(id).unwrap().at
    }

    pub fn run(s: &mut Session, seconds: f64) {
        s.advance(seconds, 1.0 / 30.0);
    }

    /// Touch an object where it is: a press and a release on it.
    pub fn poke(s: &mut Session, id: ObjectId) {
        let p = at_of(s, id);
        s.gesture(Gesture::Press(p));
        s.gesture(Gesture::Release { at: p, velocity: (0.0, 0.0) });
    }

    /// No principal incident is left, and nobody holds anything.
    pub fn quiet(s: &Session) {
        let defs = s.story.definition();
        assert_eq!(s.story.director().incidents_running(defs), 0, "{:?}", s.story.director().status(defs));
        assert_eq!(s.story.actors().reservation_count(), 0);
        assert!(s.story.physics().bodies.len() < 40);
    }

    /// Run in steps of a tenth of a second until `done`, or `limit` seconds; returns when.
    pub fn until(s: &mut Session, limit: f64, mut done: impl FnMut(&Session) -> bool) -> Option<f64> {
        let mut t = 0.0;
        while t < limit {
            if done(s) {
                return Some(t);
            }
            s.advance(0.1, 1.0 / 30.0);
            t += 0.1;
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn replays_reject_backwards_nonfinite_and_oversized_inputs() {
        let ok = r#"{"view":"street","seed":3,"seconds":10,"inputs":[{"t":1,"gesture":"press","x":10,"y":20},{"t":2,"gesture":"release","x":10,"y":20,"vx":50,"vy":-10},{"t":2.5,"trigger":"hydrant"}],"frames":[0,5,10]}"#;
        let r = Replay::parse(ok).unwrap();
        assert_eq!((r.view.as_str(), r.seed, r.inputs.len(), r.frames.len()), ("street", 3, 3, 3));
        assert_eq!(r.inputs[1].what, What::Gesture(Gesture::Release { at: (10.0, 20.0), velocity: (50.0, -10.0) }));
        for (bad_text, needle) in [
            (r#"{"view":"street","seconds":10,"inputs":[{"t":2,"gesture":"cancel"},{"t":1,"gesture":"cancel"}]}"#, "backwards"),
            (r#"{"view":"street","seconds":10,"inputs":[{"t":1,"gesture":"press","x":"a","y":1}]}"#, "number"),
            (r#"{"view":"street","seconds":10,"inputs":[{"t":99,"gesture":"cancel"}]}"#, "outside"),
            (r#"{"view":"street","seconds":10,"inputs":[{"t":1,"gesture":"fling"}]}"#, "no gesture"),
            (r#"{"view":"street","seconds":10,"inputs":[{"t":1,"gesture":"release","x":0,"y":0,"vx":99999,"vy":0}]}"#, "hand"),
            (r#"{"view":"street","seconds":1e9}"#, "outside"),
            (r#"{"view":"street","seconds":10,"frames":[11]}"#, "frame"),
            (r#"{"seconds":10}"#, "view"),
            (r#"not json"#, "expected"),
        ] {
            let e = Replay::parse(bad_text).unwrap_err().to_string();
            assert!(e.contains(needle), "{:?} gave {}", bad_text, e);
        }
        // a flood of inputs is refused whole
        let many: Vec<String> = (0..MAX_INPUTS + 1).map(|_| r#"{"t":0,"gesture":"cancel"}"#.to_string()).collect();
        let flood = format!(r#"{{"view":"street","seconds":1,"inputs":[{}]}}"#, many.join(","));
        assert!(Replay::parse(&flood).unwrap_err().to_string().contains("more than"));
    }
}
