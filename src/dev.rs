//! The settings panel: choose a view, bend the scene by hand while watching
//! it, then save what should stay.
//!
//! `dormer dev` opens a small floating terminal on an empty workspace, so the
//! wallpaper is in view. What the panel shows is a *draft* of the settings.
//! While it is open the draft is previewed on the wallpaper through
//! `dev.json`, which counts only while the panel is alive; Save writes the
//! draft to `config.toml` (see `settings::file`), and from then on the saved
//! settings are what the wallpaper follows, panel or no panel.
//!
//! The parts: `model` (state and actions, no terminal), `input` (keys and
//! mouse out of bytes), `layout` (one grid for drawing and hit-testing),
//! `render` (the picture of it). This file is the shell around them: the
//! terminal, the event loop, saving, and previewing on the wallpaper.

mod input;
mod layout;
mod model;
mod render;

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{sync_channel, SyncSender};
use std::sync::Arc;

use serde_json::{json, Value};

use crate::paint::Cache;
use crate::settings::file::{ConfigFile, SaveError, SavePolicy};
use crate::settings::{Channel, Date, Settings};
use crate::sources::{self, cache_file, config};
use crate::tm;
use crate::{build, launcher, raw_moment, with_overrides, Moment};

use input::{Decoder, Key};
use layout::{Button, ControlId, Layout};
use model::{Action, Choice, Dialog, Effect, Field, Model, Reality, SceneCommand, Shape};

/// Without the living wallpaper running, each change is drawn to the still wallpaper in the background.
struct Painter {
    changed: SyncSender<()>,
}

impl Painter {
    fn start() -> Painter {
        let (changed, pending) = sync_channel(1);
        std::thread::spawn(move || {
            let exe = launcher();
            while pending.recv().is_ok() {
                while pending.try_recv().is_ok() {}
                if sources::live_pid() != 0 {
                    continue;
                }
                let _ = Command::new(&exe).args(["draw", "--force"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
            }
        });
        Painter { changed }
    }
}

struct Panel {
    model: Model,
    layout: Layout,
    painter: Painter,
    fade: Fade,
    file: Option<ConfigFile>,
    /// the raw world, read at most every twenty seconds
    base: Option<Moment>,
    real_at: f64,
    info: Vec<String>,
    cache: Cache,
    pending: bool,
    published_at: f64,
    /// a button the mouse went down on, which fires if it is released there
    armed: Option<ControlId>,
    size: (u16, u16),
    /// what the scene tools have asked of the running scenes: a serial the wallpaper uses to tell
    /// a new command from one it has seen, the playback state, and one-shot commands
    scene_serial: u64,
    scene_paused: bool,
    scene_once: (Option<String>, bool, bool),
}

impl Panel {
    fn new() -> Panel {
        let runtime = crate::settings::runtime();
        // the view in force (a persisted `dormer view` choice outranks the config) is the baseline
        let mut saved = (*runtime).clone();
        let view = sources::view();
        saved.view = view.id().to_string();
        let today = {
            let l = tm::local(tm::now());
            Date { year: l.year, month: l.month as u32, day: l.mday as u32 }
        };
        let path = sources::config_path();
        let model = Model::new(Arc::new(saved), Reality::default(), today, path.clone());
        let size = term_size();
        let mut p = Panel {
            layout: Layout::build(size, &model),
            model,
            painter: Painter::start(),
            fade: Fade::new(),
            file: None,
            base: None,
            real_at: 0.0,
            info: Vec::new(),
            cache: Cache::new(view, config().seed, 320, 200),
            pending: false,
            published_at: 0.0,
            armed: None,
            size,
            scene_serial: 0,
            scene_paused: false,
            scene_once: (None, false, false),
        };
        match ConfigFile::open(&path) {
            Ok(f) => p.file = Some(f),
            Err(e) => p.model.fail(format!("{} Fix the file, then Save again.", e)),
        }
        p.refresh_real();
        p
    }

    /// What the inputs rest on when the draft leaves them alone: the real time, weather and machine.
    fn refresh_real(&mut self) {
        let m = raw_moment(None);
        let l = tm::local(m.ts);
        let round = |v: f64, step: f64| (v / step).round() * step;
        let r = &mut self.model.reality;
        r.time = l.hour as f64 + l.min as f64 / 60.0;
        let mut put = |c: Channel, v: f64| {
            r.channels.insert(c, v);
        };
        put(Channel::Cloud, round(m.wx.cover * 100.0, 5.0));
        put(Channel::Rain, round(m.wx.rain * 100.0, 5.0));
        put(Channel::Snow, round(m.wx.snow * 100.0, 5.0));
        put(Channel::Fog, round(m.wx.fog * 100.0, 5.0));
        put(Channel::Storm, if m.wx.storm { 1.0 } else { 0.0 });
        put(Channel::RainIn, round(m.wx.rain_in.unwrap_or(4.0).min(4.0), 0.5));
        put(Channel::Wind, m.wx.wind.round());
        put(Channel::WindFrom, round(m.wx.wdir, 15.0).rem_euclid(360.0));
        put(Channel::Temperature, m.wx.temp.round());
        put(Channel::Humidity, round(m.wx.humidity, 5.0));
        put(Channel::Agents, m.machine.agents.min(4) as f64);
        put(Channel::Containers, m.machine.containers.min(8) as f64);
        put(Channel::Load, round(m.machine.load as f64 * 100.0, 5.0));
        put(Channel::Music, if m.machine.audio { 1.0 } else { 0.0 });
        self.base = Some(m);
        self.real_at = tm::mono();
        self.describe();
    }

    /// The first lines of `dormer explain` for the scene as the draft has it, and where the
    /// view's own knobs would rest if left alone.
    fn describe(&mut self) {
        let Some(base) = &self.base else { return };
        let (set, auto) = self.model.overrides();
        let mut m = base.at(tm::now());
        let _ = crate::settings::runtime().apply_except(&mut m, &auto);
        with_overrides(&set, &mut m);
        if m.view.id() != self.cache.view.id() || self.model.stories.is_empty() && self.cache.stage.story().is_some() {
            self.cache = Cache::new(m.view, config().seed, 320, 200);
            let names = self.cache.stage.story().map(|d| d.stories.iter().map(|s| s.name.to_string()).collect()).unwrap_or_default();
            self.model.set_stories(names);
        }
        let held = std::mem::take(&mut m.knobs);
        let free = build(&m, None);
        for k in m.view.knobs() {
            self.model.reality.knobs.insert(k.key.to_string(), ((m.view.real(k.key, &free) / k.step).round() * k.step).clamp(k.lo, k.hi));
        }
        m.knobs = held;
        let mut sc = build(&m, None);
        sc.lit_share = self.cache.stage.lights(&sc).share;
        self.info = crate::words::explain(&sc, m.view, &self.cache.stars).lines().take_while(|l| !l.starts_with("week")).map(|l| l.to_string()).collect();
    }

    fn publish(&mut self) -> bool {
        if !self.pending || tm::mono() - self.published_at < 1.0 / 30.0 {
            return false;
        }
        self.pending = false;
        self.published_at = tm::mono();
        let (set, auto) = self.model.overrides();
        if set.is_empty() && auto.is_empty() && self.scene_serial == 0 {
            // nothing held that the saved settings do not already say: the preview is the saved scene
            let _ = std::fs::remove_file(cache_file("dev.json"));
        } else {
            let mut doc = json!({"pid": std::process::id(), "set": set, "auto": auto});
            if self.scene_serial > 0 {
                let (trigger, step, reset) = std::mem::take(&mut self.scene_once);
                doc["scene"] = json!({"serial": self.scene_serial, "pause": self.scene_paused, "trigger": trigger, "step": step, "reset": reset});
            }
            sources::write_atomic(&cache_file("dev.json"), &doc.to_string());
        }
        self.describe();
        let _ = self.painter.changed.try_send(());
        true
    }

    /// Carry out what the model asked for. True when the panel should close.
    fn run(&mut self, effects: Vec<Effect>) -> bool {
        let mut queue: std::collections::VecDeque<Effect> = effects.into();
        while let Some(effect) = queue.pop_front() {
            match effect {
                Effect::Publish => self.pending = true,
                Effect::Exit => return true,
                Effect::Save { policy, .. } => {
                    let result = self.save(policy);
                    queue.extend(self.model.save_finished(result));
                }
                Effect::Reload => self.reload(),
                Effect::Scene(cmd) => {
                    // a command for the running scenes: sent through dev.json, never saved
                    self.scene_serial += 1;
                    match cmd {
                        SceneCommand::Trigger(name) => self.scene_once.0 = Some(name),
                        SceneCommand::Pause(p) => self.scene_paused = p,
                        SceneCommand::Step => self.scene_once.1 = true,
                        SceneCommand::Reset => {
                            self.scene_once.2 = true;
                            self.scene_paused = false;
                        }
                    }
                    self.pending = true;
                }
            }
        }
        false
    }

    fn save(&mut self, policy: SavePolicy) -> Result<crate::settings::file::SaveReceipt, SaveError> {
        let mut file = match self.file.take() {
            Some(f) => f,
            None => ConfigFile::open(&sources::config_path())?,
        };
        let result = file.save(&self.model.draft.edited, policy);
        self.file = Some(file);
        if result.is_ok() {
            // this process now follows what it just wrote
            let _ = crate::settings::reload();
            self.pending = true;
        }
        result
    }

    fn reload(&mut self) {
        let path = sources::config_path();
        let outcome = ConfigFile::open(&path).and_then(|f| {
            Settings::load(&path).map(|s| (f, s.runtime)).map_err(|e| SaveError::Parse(e.to_string()))
        });
        match outcome {
            Ok((file, runtime)) => {
                self.file = Some(file);
                let _ = crate::settings::reload();
                self.model.reloaded(runtime);
                self.pending = true;
            }
            Err(e) => self.model.fail(format!("Could not reload: {}", e)),
        }
    }

    /// One key or mouse event. True when the panel should close.
    fn key(&mut self, key: Key) -> bool {
        let actions = translate(&self.model, &self.layout, key, &mut self.armed);
        let mut exit = false;
        for a in actions {
            let effects = self.model.dispatch(a);
            exit |= self.run(effects);
        }
        exit
    }
}

/// What a key or mouse event means right now. Pure: the same model and layout give the same answer.
fn translate(model: &Model, layout: &Layout, key: Key, armed: &mut Option<ControlId>) -> Vec<Action> {
    let focus = model.focus;
    // a question is open: its own keys, and its own buttons
    if let Some(dialog) = &model.dialog {
        let pick = |c: Choice| vec![Action::DialogChoice(c)];
        return match (dialog, key) {
            (Dialog::Unsaved, Key::Char('s' | 'S') | Key::Enter | Key::Ctrl('s')) => pick(Choice::Save),
            (Dialog::Unsaved, Key::Char('d' | 'D')) => pick(Choice::Discard),
            (Dialog::Conflict, Key::Char('r' | 'R') | Key::Enter) => pick(Choice::Reload),
            (Dialog::Conflict, Key::Char('o' | 'O')) => pick(Choice::Overwrite),
            (_, Key::Char('c' | 'C' | 'q') | Key::Escape | Key::Ctrl('c')) => pick(Choice::Cancel),
            (_, Key::Press(x, y)) => {
                *armed = layout.hit(x as u16, y as u16);
                Vec::new()
            }
            (_, Key::Release(x, y)) => {
                let hit = layout.hit(x as u16, y as u16);
                let was = armed.take();
                match (hit, was) {
                    (Some(ControlId::Choice(c)), Some(ControlId::Choice(d))) if c == d => pick(c),
                    _ => Vec::new(),
                }
            }
            _ => Vec::new(),
        };
    }
    // a value is being typed
    if model.entry.is_some() {
        return match key {
            Key::Char(c) => vec![Action::EditChar(c)],
            Key::Backspace => vec![Action::EditBackspace],
            Key::Enter => vec![Action::EditCommit],
            Key::Escape | Key::Ctrl('c') => vec![Action::EditCancel],
            _ => Vec::new(),
        };
    }
    let info = model.info(focus);
    match key {
        Key::Up | Key::Char('k') => vec![Action::FocusStep(-1)],
        Key::Down | Key::Char('j') | Key::Tab => vec![Action::FocusStep(1)],
        Key::BackTab => vec![Action::FocusStep(-1)],
        Key::Left | Key::Char('h') => vec![Action::Adjust(focus, -1.0)],
        Key::Right | Key::Char('l') => vec![Action::Adjust(focus, 1.0)],
        Key::BigLeft | Key::Char('[' | 'H') => vec![Action::Big(focus, -1.0)],
        Key::BigRight | Key::Char(']' | 'L') => vec![Action::Big(focus, 1.0)],
        Key::Home => vec![Action::SetValue(focus, info.lo)],
        Key::End => vec![Action::SetValue(focus, info.hi)],
        Key::Enter if matches!(info.shape, Shape::Toggle | Shape::Button) => vec![Action::Toggle(focus)],
        Key::Enter => vec![Action::EditStart],
        Key::Char(' ') => match (info.shape, model.fixed(focus)) {
            (Shape::Toggle, _) | (Shape::Choice, _) | (Shape::Button, _) => vec![Action::Toggle(focus)],
            (_, Some(true)) => vec![Action::SetAuto(focus)],
            _ => vec![Action::SetValue(focus, model.value(focus))],
        },
        Key::Char('a') | Key::Delete | Key::Backspace => vec![Action::SetAuto(focus)],
        Key::Char('n') => vec![Action::Now],
        Key::Char('r') => vec![Action::ResetField(focus)],
        Key::Char('d') => vec![Action::Discard],
        Key::Char('s') | Key::Ctrl('s') => vec![Action::Save],
        Key::Char('p') => vec![Action::Preview],
        Key::Char('?') => vec![Action::Help],
        Key::Char('q') | Key::Ctrl('c') => vec![Action::Quit],
        Key::Escape => vec![Action::Escape],
        Key::Char(c @ '1'..='9') => {
            let cats = model.categories();
            cats.get(c as usize - '1' as usize).map(|c| vec![Action::Category(*c)]).unwrap_or_default()
        }
        Key::Wheel(_, _, dir) => vec![Action::Scroll(-dir)],
        Key::FocusOut => vec![Action::CancelDrag],
        Key::Drag(x, _) => {
            if model.capture.is_some() {
                vec![Action::Drag(x as u16)]
            } else {
                Vec::new()
            }
        }
        Key::Press(x, y) => {
            if model.capture.is_some() {
                return Vec::new();
            }
            let hit = layout.hit(x as u16, y as u16);
            *armed = None;
            match hit {
                Some(ControlId::Slider(f)) => match layout.span(f) {
                    Some(span) => vec![Action::BeginDrag { field: f, span, x: x as u16 }],
                    None => vec![Action::Focus(f)],
                },
                Some(ControlId::Row(f)) => vec![Action::Focus(f)],
                Some(ControlId::Toggle(f)) => vec![Action::Focus(f), Action::Toggle(f)],
                Some(ControlId::Step(f, d)) => vec![Action::Focus(f), Action::Adjust(f, d as f64)],
                Some(ControlId::Source(f)) => {
                    if model.fixed(f) == Some(true) {
                        vec![Action::Focus(f), Action::SetAuto(f)]
                    } else {
                        vec![Action::Focus(f), Action::SetValue(f, model.value(f))]
                    }
                }
                Some(ControlId::ViewTab(i)) => vec![Action::SetValue(Field::View, i as f64)],
                Some(ControlId::Category(c)) => vec![Action::Category(c)],
                Some(ControlId::ScrollUp) => vec![Action::Scroll(-1)],
                Some(ControlId::ScrollDown) => vec![Action::Scroll(1)],
                Some(b @ ControlId::Button(_)) => {
                    *armed = Some(b);
                    Vec::new()
                }
                _ => Vec::new(),
            }
        }
        Key::Release(x, y) => {
            let mut out = Vec::new();
            if model.capture.is_some() {
                // the gesture ends where it is let go, whatever row that is
                out.push(Action::EndDrag);
            }
            let was = armed.take();
            if let (Some(ControlId::Button(b)), Some(ControlId::Button(a))) = (layout.hit(x as u16, y as u16), was) {
                if a == b {
                    out.push(match b {
                        Button::Save => Action::Save,
                        Button::Discard => Action::Discard,
                        Button::Reset => Action::ResetField(focus),
                        Button::Preview => Action::Preview,
                        Button::Help => Action::Help,
                        Button::Quit => Action::Quit,
                    });
                }
            }
            out
        }
        _ => Vec::new(),
    }
}

static RESIZED: AtomicBool = AtomicBool::new(false);

extern "C" fn on_resize(_: libc::c_int) {
    RESIZED.store(true, Ordering::SeqCst);
}

/// Only alter the dedicated panel terminal, never an unrelated terminal used with --here.
struct Fade { address: Option<String>, faded: Option<bool> }
impl Fade {
    fn new() -> Self {
        let mut ancestors = Vec::new();
        let mut pid = std::process::id() as i64;
        for _ in 0..12 {
            ancestors.push(pid);
            let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) else { break };
            let Some(end) = stat.rfind(')') else { break };
            pid = stat[end + 1..].split_whitespace().nth(1).and_then(|p| p.parse().ok()).unwrap_or(0);
            if pid <= 1 { break; }
        }
        let mut address = None;
        for attempt in 0..3 {
            address = sources::hyprland("j/clients").and_then(|v| {
                v.as_array()?.iter().find(|c| c["class"].as_str() == Some("dev.dormer.panel") && ancestors.contains(&c["pid"].as_i64().unwrap_or(0)))?
                    .get("address")?.as_str().map(str::to_string)
            });
            if address.is_some() { break; }
            if attempt < 2 { std::thread::sleep(std::time::Duration::from_millis(30)); }
        }
        if let Some(a) = &address {
            let window = format!("address:{}", a);
            sources::run(&["hyprctl", "dispatch", "setfloating", &window], 0.5);
            let size = sources::hyprland("j/monitors").and_then(|v| {
                let m = v.as_array()?.iter().find(|m| m["focused"].as_bool() == Some(true))?;
                let scale = m["scale"].as_f64().unwrap_or(1.0).max(0.1);
                let (mut w, mut h) = (m["width"].as_f64()? / scale, m["height"].as_f64()? / scale);
                if m["transform"].as_u64().unwrap_or(0) % 2 == 1 { std::mem::swap(&mut w, &mut h); }
                Some((w as usize, h as usize))
            }).unwrap_or((960, 600));
            let (w, h) = (size.0.saturating_sub(48).min(620).max(1), size.1.saturating_sub(56).min(420).max(1));
            sources::run(&["hyprctl", "dispatch", "resizewindowpixel", &format!("exact {} {},{}", w, h, window)], 0.5);
            sources::run(&["hyprctl", "dispatch", "movewindowpixel", &format!("exact 24 28,{}", window)], 0.5);
        }
        Self { address, faded: None }
    }
    fn set(&mut self, faded: bool) {
        if self.faded == Some(faded) { return; }
        self.faded = Some(faded);
        if let Some(address) = &self.address {
            let window = format!("address:{}", address);
            let alpha = if faded { "0.46" } else { "1.0" };
            let current = sources::run(&["hyprctl", "dispatch", "setprop", &window, "opacity", alpha], 0.5);
            if !current.is_some_and(|r| r.0 && r.1.trim() == "ok") {
                sources::run(&["hyprctl", "dispatch", "setprop", &window, "alpha", &format!("{} override", alpha)], 0.5);
            }
            sources::run(&["hyprctl", "dispatch", "setprop", &window, "no_blur", if faded { "1" } else { "unset" }], 0.5);
        }
    }
}

/// (columns, rows)
fn term_size() -> (u16, u16) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
        (ws.ws_col, ws.ws_row)
    } else {
        (80, 24)
    }
}


#[cfg(test)]
mod tests {
    use super::model::test_model;
    use super::*;

    fn press_release(m: &mut Model, l: &Layout, x: u16, y: u16) -> Vec<Action> {
        let mut armed = None;
        let mut out = translate(m, l, Key::Press(x as usize, y as usize), &mut armed);
        out.extend(translate(m, l, Key::Release(x as usize, y as usize), &mut armed));
        out
    }

    #[test]
    fn keys_and_mouse_end_up_as_the_same_actions() {
        let m = test_model();
        let l = Layout::build((70, 24), &m);
        let mut armed = None;
        assert_eq!(translate(&m, &l, Key::Right, &mut armed), vec![Action::Adjust(Field::Time, 1.0)]);
        assert_eq!(translate(&m, &l, Key::Char('l'), &mut armed), translate(&m, &l, Key::Right, &mut armed));
        assert_eq!(translate(&m, &l, Key::Ctrl('s'), &mut armed), vec![Action::Save]);
        assert_eq!(translate(&m, &l, Key::Enter, &mut armed), vec![Action::EditStart]);
        // the Save button is a button: a press alone does nothing, press and release on it saves
        let (r, _) = l.buttons.iter().find(|(_, b)| *b == Button::Save).unwrap();
        let mut m2 = test_model();
        assert!(translate(&m, &l, Key::Press(r.x as usize, r.y as usize), &mut armed).is_empty());
        assert_eq!(translate(&m, &l, Key::Release(r.x as usize, r.y as usize), &mut armed), vec![Action::Save]);
        assert_eq!(press_release(&mut m2, &l, r.x, r.y), vec![Action::Save]);
        // pressed on Save, released somewhere else: nothing is saved
        let mut armed = None;
        translate(&m, &l, Key::Press(r.x as usize, r.y as usize), &mut armed);
        assert!(translate(&m, &l, Key::Release(0, 5), &mut armed).is_empty());
    }

    #[test]
    fn a_drag_released_outside_its_row_ends_without_touching_anything_else() {
        let mut m = test_model();
        let l = Layout::build((70, 24), &m);
        let row = l.row(Field::Channel(Channel::Rain)).unwrap();
        let bar = row.bar.unwrap();
        let mut armed = None;
        let press = translate(&m, &l, Key::Press(bar.x as usize + 2, row.y as usize), &mut armed);
        assert!(matches!(press[0], Action::BeginDrag { field: Field::Channel(Channel::Rain), .. }));
        for a in press {
            m.dispatch(a);
        }
        let l = Layout::build((70, 24), &m);
        // moving onto the Save button's row and releasing there neither saves nor fires it
        let (save, _) = l.buttons.iter().find(|(_, b)| *b == Button::Save).unwrap();
        let drag = translate(&m, &l, Key::Drag(bar.x as usize + 9, save.y as usize), &mut armed);
        assert_eq!(drag, vec![Action::Drag(bar.x + 9)]);
        let release = translate(&m, &l, Key::Release(save.x as usize, save.y as usize), &mut armed);
        assert_eq!(release, vec![Action::EndDrag]);
    }

    #[test]
    fn dialogs_have_their_own_keys_and_swallow_the_rest() {
        let mut m = test_model();
        m.dispatch(Action::SetValue(Field::Fps, 12.0));
        m.dispatch(Action::Quit);
        let l = Layout::build((70, 24), &m);
        let mut armed = None;
        assert_eq!(translate(&m, &l, Key::Char('s'), &mut armed), vec![Action::DialogChoice(Choice::Save)]);
        assert_eq!(translate(&m, &l, Key::Char('d'), &mut armed), vec![Action::DialogChoice(Choice::Discard)]);
        assert_eq!(translate(&m, &l, Key::Escape, &mut armed), vec![Action::DialogChoice(Choice::Cancel)]);
        assert!(translate(&m, &l, Key::Right, &mut armed).is_empty());
        assert!(translate(&m, &l, Key::Press(2, 2), &mut armed).is_empty());
    }

    #[test]
    fn typing_goes_to_the_entry_not_to_commands() {
        let mut m = test_model();
        m.dispatch(Action::EditStart);
        let l = Layout::build((70, 24), &m);
        let mut armed = None;
        // q would quit, s would save: while typing they are letters
        assert_eq!(translate(&m, &l, Key::Char('q'), &mut armed), vec![Action::EditChar('q')]);
        assert_eq!(translate(&m, &l, Key::Escape, &mut armed), vec![Action::EditCancel]);
    }
}

/// Ask Hyprland for something: the Lua form first (0.56+), then the old dispatcher.
fn hyprland(lua: &str, classic: &[&str]) {
    if sources::which("hyprctl").is_none() {
        return;
    }
    let done = sources::run(&["hyprctl", "dispatch", lua], 3.0);
    if !done.map_or(false, |d| d.1.contains("ok")) && !classic.is_empty() {
        let mut cmd = vec!["hyprctl", "dispatch"];
        cmd.extend_from_slice(classic);
        sources::run(&cmd, 3.0);
    }
}

/// The command that opens the panel in a small terminal of its own.
fn terminal(exe: &str, back: Option<&str>) -> Option<Vec<String>> {
    let (name, title) = ("dev.dormer.panel", "dormer dev");
    let known: [(&str, Vec<String>); 5] = [
        ("ghostty", vec![format!("--class={}", name), format!("--title={}", title), "--window-width=70".into(), "--window-height=24".into(), "--font-size=10".into(), "--background-opacity=0.72".into(), "--resize-overlay=never".into(), "--gtk-single-instance=false".into(), "--confirm-close-surface=false".into(), "-e".into()]),
        ("kitty", vec!["--class".into(), name.into(), "--title".into(), title.into(), "-o".into(), "initial_window_width=70c".into(), "-o".into(), "initial_window_height=24c".into()]),
        ("foot", vec!["--app-id".into(), name.into(), "--title".into(), title.into(), "--window-size-chars=70x24".into()]),
        ("alacritty", vec!["--class".into(), name.into(), "--title".into(), title.into(), "-o".into(), "window.dimensions.columns=70".into(), "-o".into(), "window.dimensions.lines=24".into(), "-e".into()]),
        ("wezterm", vec!["start".into(), "--class".into(), name.into(), "--".into()]),
    ];
    let env = std::env::var("TERMINAL").unwrap_or_default();
    let wanted = if config().terminal.is_empty() { env.rsplit('/').next().unwrap_or("").to_string() } else { config().terminal.clone() };
    let order = std::iter::once(wanted.as_str()).chain(known.iter().map(|k| k.0));
    for term in order {
        if let Some((_, flags)) = known.iter().find(|k| k.0 == term) {
            if sources::which(term).is_some() {
                let mut cmd = vec![term.to_string()];
                cmd.extend(flags.iter().cloned());
                cmd.extend([exe.to_string(), "dev".to_string(), "--here".to_string()]);
                if let Some(b) = back {
                    cmd.extend(["--back".to_string(), b.to_string()]);
                }
                return Some(cmd);
            }
        }
    }
    None
}

/// Is another panel (or a tour) already holding the scene?
pub fn taken() -> bool {
    !sources::overrides().is_empty() || sources::read_json(&cache_file("dev.json")).map_or(false, |d| {
        let pid = d["pid"].as_i64().unwrap_or(0);
        pid != std::process::id() as i64 && std::path::Path::new(&format!("/proc/{}", pid)).exists()
    })
}

pub fn run(here: bool, back: Option<String>) -> i32 {
    let exe = launcher();
    if taken() {
        eprintln!("dormer: a dev panel or a tour is already running");
        return 1;
    }
    if !here {
        let back = sources::run(&["hyprctl", "-j", "activeworkspace"], 3.0)
            .and_then(|d| serde_json::from_str::<Value>(&d.1).ok())
            .and_then(|v| v["id"].as_i64())
            .map(|id| id.to_string());
        let Some(cmd) = terminal(&exe, back.as_deref()) else {
            eprintln!("dormer: no terminal I know how to open; run `dormer dev --here` in one");
            return 1;
        };
        if back.is_some() {
            // go where the wallpaper can be seen
            hyprland("hl.dsp.focus({ workspace = \"emptym\" })", &["workspace", "emptym"]);
        }
        unsafe { libc::signal(libc::SIGHUP, libc::SIG_IGN) };
        let spawned = Command::new(&cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stdout(Stdio::null()).stderr(Stdio::null()).spawn();
        return if spawned.is_ok() { 0 } else { 1 };
    }

    // the terminal: no echo, a key at a time, the mouse reported while a button is held, and
    // told when focus leaves it
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    unsafe {
        libc::tcgetattr(0, &mut saved);
        let mut raw = saved;
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
        libc::signal(libc::SIGINT, libc::SIG_IGN);
        libc::signal(libc::SIGWINCH, on_resize as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h\x1b[?1004h");
    let mut panel = Panel::new();
    panel.pending = true;
    let mut stdin = std::io::stdin();
    let mut decoder = Decoder::default();
    let mut dirty = true;
    let mut was_peeking = false;
    let mut real_due = tm::mono() + 20.0;
    'outer: loop {
        let now = tm::mono();
        panel.model.now = now;
        if now >= real_due {
            panel.refresh_real(); // the real clock and weather move on underneath
            real_due = now + 20.0;
            dirty = true;
        }
        dirty |= panel.publish();
        let size = term_size();
        if size != panel.size || RESIZED.swap(false, Ordering::SeqCst) {
            // a held slider's cells are no longer where they were
            let effects = panel.model.dispatch(Action::CancelDrag);
            panel.run(effects);
            panel.size = size;
            dirty = true;
        }
        let peeking = panel.model.peeking();
        if peeking != was_peeking {
            dirty = true;
            was_peeking = peeking;
        }
        if dirty {
            panel.fade.set(peeking);
            panel.layout = Layout::build(panel.size, &panel.model);
            let first = panel.layout.first;
            panel.model.viewed(first);
            let _ = out.write_all(render::render(&panel.model, &panel.layout).as_bytes());
            let _ = out.flush();
            dirty = false;
        }
        // sleep until something can change: input, a publish due, a preview ending, a lone
        // escape deciding what it is, or the real sky being read again
        let mut wake = real_due;
        if panel.pending {
            wake = wake.min(panel.published_at + 1.0 / 30.0);
        }
        if peeking && panel.model.capture.is_none() && !panel.model.pinned {
            wake = wake.min(panel.model.peek_until);
        }
        if decoder.holding_escape() {
            wake = wake.min(now + 0.05);
        }
        let wait = ((wake - tm::mono()).max(0.0) * 1000.0).ceil() as i32 + 1;
        let mut fds = [libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 }];
        let ready = unsafe { libc::poll(fds.as_mut_ptr(), 1, wait) };
        if ready <= 0 {
            // timed out (or a resize signal): a held escape that nothing followed is the Escape key
            for k in decoder.flush() {
                dirty = true;
                if panel.key(k) {
                    break 'outer;
                }
            }
            continue;
        }
        if fds[0].revents & (libc::POLLHUP | libc::POLLERR) != 0 {
            break;
        }
        let mut raw = [0u8; 512];
        let n = stdin.read(&mut raw).unwrap_or(0);
        if n == 0 {
            break;
        }
        for k in decoder.feed(&raw[..n]) {
            if k != Key::None {
                dirty = true;
            }
            if panel.key(k) {
                break 'outer;
            }
        }
    }
    panel.fade.set(false);
    let _ = out.write_all(b"\x1b[?1004l\x1b[?1006l\x1b[?1002l\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    // only what was transient goes: what was saved stays in force
    let _ = std::fs::remove_file(cache_file("dev.json"));
    if sources::live_pid() == 0 {
        println!("putting the sky back…");
        let _ = Command::new(&exe).args(["draw", "--force"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    if let Some(b) = back {
        hyprland(&format!("hl.dsp.focus({{ workspace = \"{}\" }})", b), &["workspace", &b]);
    }
    0
}
