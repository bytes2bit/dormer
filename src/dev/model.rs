//! The panel's state: a draft of the settings against the saved ones, what is
//! focused, and what the user is in the middle of. No terminal escape codes
//! and no file access here: `dispatch` takes an action and returns the effects
//! that the caller (`dev.rs`) should carry out, so every behaviour can be
//! tested without a terminal.

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use crate::settings::file::{Recovery, SaveError, SavePolicy, SaveReceipt};
use crate::settings::{Channel, Clock, Date, Kind, RuntimeSettings, Source};
use crate::tm::{self, DAYS, MONTHS};
use crate::views::{self, Knob, View};

const WINDS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Category {
    Appearance,
    Sky,
    Machine,
    Activity,
    View,
    /// the incidents of this view, to start by hand, and the controls of the playback
    Scenes,
}

impl Category {
    pub const ALL: [Category; 6] = [Category::Appearance, Category::Sky, Category::Machine, Category::Activity, Category::View, Category::Scenes];

    pub fn label(self) -> &'static str {
        match self {
            Category::Appearance => "Appearance",
            Category::Sky => "Sky & weather",
            Category::Machine => "Machine preview",
            Category::Activity => "Activity",
            Category::View => "This view",
            Category::Scenes => "Scenes",
        }
    }

    /// Fits a tab in a narrow terminal.
    pub fn short(self) -> &'static str {
        match self {
            Category::Appearance => "Look",
            Category::Sky => "Sky",
            Category::Machine => "Machine",
            Category::Activity => "Activity",
            Category::View => "View",
            Category::Scenes => "Scenes",
        }
    }
}

/// One adjustable thing. The same value of a `Field` is the same control for as long as
/// the panel runs, whatever view or category is showing.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash)]
pub enum Field {
    View,
    Mode,
    Fps,
    Interactive,
    Time,
    Date,
    Channel(Channel),
    Birds,
    Meteors,
    Planes,
    Stories,
    Knob(&'static str),
    /// start the nth incident of this view
    Story(u8),
    /// hold every scene where it is, or let it go on
    ScenePause,
    /// run one tick
    SceneStep,
    /// everything back to an ordinary scene
    SceneReset,
}

impl Field {
    pub fn category(self) -> Category {
        match self {
            Field::View | Field::Mode | Field::Fps | Field::Interactive => Category::Appearance,
            Field::Time | Field::Date => Category::Sky,
            Field::Channel(c) if c.is_machine() => Category::Machine,
            Field::Channel(_) => Category::Sky,
            Field::Birds | Field::Meteors | Field::Planes | Field::Stories => Category::Activity,
            Field::Knob(_) => Category::View,
            Field::Story(_) | Field::ScenePause | Field::SceneStep | Field::SceneReset => Category::Scenes,
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Shape {
    Slider,
    Toggle,
    Choice,
    /// does something when pressed; holds no value
    Button,
}

/// How one field is presented and bounded.
#[derive(Clone, Debug)]
pub struct Info {
    pub label: String,
    pub shape: Shape,
    pub lo: f64,
    pub hi: f64,
    pub step: f64,
    pub big: f64,
    /// does it have an Auto / Fixed source
    pub sourced: bool,
    /// is the change seen only after saving
    pub on_save: bool,
    pub help: String,
}

fn slider(label: &str, lo: f64, hi: f64, step: f64, big: f64, help: &str) -> Info {
    Info { label: label.into(), shape: Shape::Slider, lo, hi, step, big, sourced: false, on_save: false, help: help.into() }
}

fn sourced(mut i: Info) -> Info {
    i.sourced = true;
    i
}

fn toggle(label: &str, help: &str) -> Info {
    Info { label: label.into(), shape: Shape::Toggle, lo: 0.0, hi: 1.0, step: 1.0, big: 1.0, sourced: false, on_save: false, help: help.into() }
}

/// What a live input rests on when the draft leaves it alone: the real world right now.
#[derive(Clone, Debug, Default)]
pub struct Reality {
    /// hours, local
    pub time: f64,
    pub channels: HashMap<Channel, f64>,
    pub knobs: HashMap<String, f64>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Choice {
    Save,
    Discard,
    Cancel,
    Reload,
    Overwrite,
}

#[derive(Clone, PartialEq, Debug)]
pub enum Dialog {
    /// quitting with unsaved changes
    Unsaved,
    /// the file changed on disk while the panel was open
    Conflict,
}

impl Dialog {
    pub fn choices(&self) -> &'static [Choice] {
        match self {
            Dialog::Unsaved => &[Choice::Save, Choice::Discard, Choice::Cancel],
            Dialog::Conflict => &[Choice::Reload, Choice::Overwrite, Choice::Cancel],
        }
    }

    pub fn question(&self) -> &'static str {
        match self {
            Dialog::Unsaved => "You have unsaved changes.",
            Dialog::Conflict => "config.toml was changed by something else.",
        }
    }
}

/// The panel's own verbs. Keyboard and mouse both end up as one of these.
#[derive(Clone, Debug, PartialEq)]
pub enum Action {
    Focus(Field),
    FocusStep(i32),
    Category(Category),
    /// move by this many steps of the field's size (negative goes down)
    Adjust(Field, f64),
    Big(Field, f64),
    SetValue(Field, f64),
    Toggle(Field),
    /// the first press of a drag, with the slider's cells, and the cell it went down on
    BeginDrag { field: Field, span: (u16, u16), x: u16 },
    Drag(u16),
    EndDrag,
    CancelDrag,
    EditStart,
    EditChar(char),
    EditBackspace,
    EditCommit,
    EditCancel,
    /// give the field back to reality
    SetAuto(Field),
    ResetField(Field),
    /// put the time and date back to now
    Now,
    Save,
    Discard,
    Preview,
    Help,
    Quit,
    Escape,
    DialogChoice(Choice),
    Scroll(i32),
}

/// What the panel asks of the running scenes. None of it is a setting, so none of it is saved.
#[derive(Clone, Debug, PartialEq)]
pub enum SceneCommand {
    Trigger(String),
    Pause(bool),
    Step,
    Reset,
}

/// What the caller must do about it.
#[derive(Clone, Debug, PartialEq)]
pub enum Effect {
    /// a scene tool was used
    Scene(SceneCommand),
    /// the draft changed: show it on the wallpaper
    Publish,
    Save { policy: SavePolicy, exit: bool },
    /// read the config file again and merge the draft over it
    Reload,
    Exit,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    Idle,
    Saving,
    Saved(String),
    Failed(String),
    Notice(String),
}

#[derive(Clone, Debug)]
pub struct Entry {
    pub field: Field,
    pub text: String,
    pub error: Option<String>,
}

#[derive(Clone, Copy, Debug)]
pub struct Capture {
    pub field: Field,
    pub span: (u16, u16),
}

/// The settings being edited against the settings in force.
#[derive(Clone)]
pub struct Draft {
    pub saved: Arc<RuntimeSettings>,
    pub edited: RuntimeSettings,
}

impl Draft {
    pub fn new(saved: Arc<RuntimeSettings>) -> Draft {
        Draft { edited: (*saved).clone(), saved }
    }
}

pub struct Model {
    pub draft: Draft,
    pub reality: Reality,
    pub today: Date,
    pub category: Category,
    pub focus: Field,
    remembered: HashMap<Category, Field>,
    /// first visible row of the list, as last drawn
    pub scroll: usize,
    /// the next layout must keep the focused row in view (a key or a click moved it)
    pub follow: bool,
    pub entry: Option<Entry>,
    pub dialog: Option<Dialog>,
    pub pinned: bool,
    pub help: bool,
    pub capture: Option<Capture>,
    pub status: Status,
    pub config_path: PathBuf,
    /// the incidents and daily activities of the view being edited, by name
    pub stories: Vec<String>,
    pub scenes_paused: bool,
    /// the clock the caller keeps for the panel, in monotonic seconds
    pub now: f64,
    pub peek_until: f64,
    exit_after_save: bool,
}

impl Model {
    pub fn new(saved: Arc<RuntimeSettings>, reality: Reality, today: Date, config_path: PathBuf) -> Model {
        let mut m = Model {
            draft: Draft::new(saved),
            reality,
            today,
            category: Category::Sky,
            focus: Field::Time,
            remembered: HashMap::new(),
            scroll: 0,
            follow: true,
            entry: None,
            dialog: None,
            pinned: false,
            help: false,
            capture: None,
            status: Status::Idle,
            config_path,
            stories: Vec::new(),
            scenes_paused: false,
            now: 0.0,
            peek_until: 0.0,
            exit_after_save: false,
        };
        m.category = Category::Sky;
        m
    }

    /// The view has changed or is new: which stories it has, for the scene tools.
    pub fn set_stories(&mut self, names: Vec<String>) {
        self.stories = names;
        self.normalise_focus();
    }

    /// The layout has settled on where the list starts.
    pub fn viewed(&mut self, first: usize) {
        self.scroll = first;
        self.follow = false;
    }

    // ── what is there ────────────────────────────────────────────────────────

    pub fn view(&self) -> &'static dyn View {
        views::find(&self.draft.edited.view).unwrap_or(views::ALL[0])
    }

    pub fn view_index(&self) -> usize {
        views::ALL.iter().position(|v| v.id() == self.view().id()).unwrap_or(0)
    }

    pub fn fields(&self, category: Category) -> Vec<Field> {
        match category {
            Category::Appearance => vec![Field::View, Field::Mode, Field::Fps, Field::Interactive],
            Category::Sky => {
                let mut v = vec![Field::Time, Field::Date];
                v.extend(Channel::ALL.iter().filter(|c| !c.is_machine()).map(|c| Field::Channel(*c)));
                v
            }
            Category::Machine => Channel::ALL.iter().filter(|c| c.is_machine()).map(|c| Field::Channel(*c)).collect(),
            Category::Activity => vec![Field::Birds, Field::Meteors, Field::Planes, Field::Stories],
            Category::View => self.view().knobs().iter().map(|k| Field::Knob(k.key)).collect(),
            Category::Scenes if self.stories.is_empty() => Vec::new(),
            Category::Scenes => (0..self.stories.len().min(24)).map(|i| Field::Story(i as u8)).chain([Field::ScenePause, Field::SceneStep, Field::SceneReset]).collect(),
        }
    }

    /// The categories that have something in them.
    pub fn categories(&self) -> Vec<Category> {
        Category::ALL.into_iter().filter(|c| !self.fields(*c).is_empty()).collect()
    }

    fn knob(&self, key: &str) -> Option<&'static Knob> {
        self.view().knobs().iter().find(|k| k.key == key)
    }

    pub fn info(&self, field: Field) -> Info {
        match field {
            Field::View => Info {
                label: "view".into(),
                shape: Shape::Choice,
                lo: 0.0,
                hi: (views::ALL.len() - 1) as f64,
                step: 1.0,
                big: 1.0,
                sourced: false,
                on_save: false,
                help: self.view().about().into(),
            },
            Field::Mode => Info {
                label: "light or dark".into(),
                shape: Shape::Choice,
                lo: 0.0,
                hi: 2.0,
                step: 1.0,
                big: 1.0,
                sourced: false,
                on_save: false,
                help: "auto follows the desktop's colour scheme".into(),
            },
            Field::Fps => Info { on_save: true, ..slider("frame rate", 5.0, 60.0, 1.0, 5.0, "the most frames a second the moving wallpaper draws") },
            Field::Interactive => Info {
                on_save: true,
                ..toggle("reacts to the mouse", "off lets clicks through to icons under the wallpaper")
            },
            Field::Time => sourced(slider("time of day", 0.0, 23.0 + 55.0 / 60.0, 1.0 / 12.0, 1.0, "the local time the scene is lit for")),
            Field::Date => sourced(slider("date", -183.0, 183.0, 1.0, 7.0, "sets the weekday, the season, the moon and the stars")),
            Field::Channel(c) => {
                let (label, lo, hi, step, big, help) = match c {
                    Channel::Cloud => ("cloud", 0.0, 100.0, 5.0, 20.0, "how much of the sky is covered"),
                    Channel::Rain => ("rain", 0.0, 100.0, 5.0, 20.0, "how hard it rains"),
                    Channel::Snow => ("snow", 0.0, 100.0, 5.0, 20.0, "how hard it snows"),
                    Channel::Fog => ("fog", 0.0, 100.0, 5.0, 20.0, "how thick the fog lies"),
                    Channel::Storm => ("thunderstorm", 0.0, 1.0, 1.0, 1.0, "lightning and a darker sky"),
                    Channel::RainIn => ("rain due in", 0.0, 4.0, 0.5, 1.0, "hours until rain arrives; the far end means none in sight"),
                    Channel::Wind => ("wind", 0.0, 70.0, 2.0, 10.0, "how hard the wind blows, in kilometres an hour"),
                    Channel::WindFrom => ("wind from", 0.0, 345.0, 15.0, 45.0, "the compass bearing the wind comes from"),
                    Channel::Temperature => ("temperature", -12.0, 42.0, 1.0, 5.0, "degrees Celsius; snow settles below freezing"),
                    Channel::Humidity => ("humidity", 20.0, 100.0, 5.0, 20.0, "damp air leaves mist at dawn"),
                    Channel::Agents => ("lamp: agents", 0.0, 4.0, 1.0, 1.0, "simulated agent sessions: the lamp is lit while any run"),
                    Channel::Containers => ("blue lights", 0.0, 8.0, 1.0, 1.0, "simulated containers: one blue window each"),
                    Channel::Load => ("smoke: cpu", 0.0, 100.0, 5.0, 25.0, "simulated processor load: more smoke"),
                    Channel::Music => ("music", 0.0, 1.0, 1.0, 1.0, "simulated sound playing: music drifts out"),
                };
                let shape = if c.kind() == Kind::Switch { Shape::Toggle } else { Shape::Slider };
                let mut help = help.to_string();
                if c.is_machine() {
                    help.push_str(" (a simulation value, not a reading)");
                }
                sourced(Info { label: label.into(), shape, lo, hi, step, big, sourced: true, on_save: false, help })
            }
            Field::Birds => slider("birds", 0.0, 5.0, 0.25, 1.0, "how often birds cross, as a multiple of the usual"),
            Field::Meteors => slider("shooting stars", 0.0, 12.0, 0.5, 2.0, "how often meteors streak across a clear night"),
            Field::Planes => slider("planes", 0.0, 6.0, 0.25, 1.0, "how often planes cross the sky"),
            Field::Stories => Info { on_save: true, ..toggle("incidents & daily life", "people and incidents in the developed views") },
            Field::Knob(key) => match self.knob(key) {
                Some(k) => sourced(slider(k.label, k.lo, k.hi, k.step, k.big, "a control of this view only")),
                None => slider(key, 0.0, 1.0, 0.1, 0.5, ""),
            },
            Field::Story(i) => {
                let name = self.stories.get(i as usize).cloned().unwrap_or_default();
                Info {
                    label: name.replace('_', " "),
                    shape: Shape::Button,
                    lo: 0.0,
                    hi: 0.0,
                    step: 0.0,
                    big: 0.0,
                    sourced: false,
                    on_save: false,
                    help: "start it now, on this view; it is never saved, and ends by itself".into(),
                }
            }
            Field::ScenePause => Info {
                label: "pause scenes".into(),
                shape: Shape::Toggle,
                lo: 0.0,
                hi: 1.0,
                step: 1.0,
                big: 1.0,
                sourced: false,
                on_save: false,
                help: "hold everything where it is; resume with the same control".into(),
            },
            Field::SceneStep => Info {
                label: "step one tick".into(),
                shape: Shape::Button,
                lo: 0.0,
                hi: 0.0,
                step: 0.0,
                big: 0.0,
                sourced: false,
                on_save: false,
                help: "advance the scenes by 1/30 of a second (while paused or not)".into(),
            },
            Field::SceneReset => Info {
                label: "reset scenes".into(),
                shape: Shape::Button,
                lo: 0.0,
                hi: 0.0,
                step: 0.0,
                big: 0.0,
                sourced: false,
                on_save: false,
                help: "end every incident, send everyone home, mend everything".into(),
            },
        }
    }

    /// Is the field following reality (Some(false)), held (Some(true)), or neither (None)?
    pub fn fixed(&self, field: Field) -> Option<bool> {
        let s = &self.draft.edited;
        match field {
            Field::Time => Some(s.scene.time.is_fixed()),
            Field::Date => Some(s.scene.date.is_fixed()),
            Field::Channel(c) => Some(s.scene.get(c).is_fixed()),
            Field::Knob(k) => Some(s.views.get(self.view().id()).is_some_and(|m| m.contains_key(k))),
            _ => None,
        }
    }

    /// The number the control stands at.
    pub fn value(&self, field: Field) -> f64 {
        let s = &self.draft.edited;
        match field {
            Field::View => self.view_index() as f64,
            Field::Mode => ["auto", "dark", "light"].iter().position(|m| *m == s.mode).unwrap_or(0) as f64,
            Field::Fps => s.fps as f64,
            Field::Interactive => s.interactive as u8 as f64,
            Field::Stories => s.stories as u8 as f64,
            Field::Time => match s.scene.time {
                Source::Fixed(c) => c.0 as f64 / 60.0,
                Source::Auto => self.reality.time,
            },
            Field::Date => match s.scene.date {
                Source::Fixed(d) => self.day_offset(d) as f64,
                Source::Auto => 0.0,
            },
            Field::Channel(c) => match s.scene.get(c) {
                Source::Fixed(v) => v,
                Source::Auto => self.reality.channels.get(&c).copied().unwrap_or(0.0),
            },
            Field::Birds => s.rates.birds as f64,
            Field::Meteors => s.rates.meteors as f64,
            Field::Planes => s.rates.planes as f64,
            Field::Story(_) | Field::SceneStep | Field::SceneReset => 0.0,
            Field::ScenePause => self.scenes_paused as u8 as f64,
            Field::Knob(k) => s
                .views
                .get(self.view().id())
                .and_then(|m| m.get(k))
                .copied()
                .or_else(|| self.reality.knobs.get(k).copied())
                .unwrap_or_else(|| self.knob(k).map_or(0.0, |k| k.lo)),
        }
    }

    fn day_offset(&self, d: Date) -> i64 {
        let noon = |d: Date| tm::stamp(d.year, d.month as i32, d.day as i32, 12, 0, 0);
        ((noon(d) - noon(self.today)) / 86400.0).round() as i64
    }

    fn date_at(&self, offset: i64) -> Date {
        let t = self.today;
        let l = tm::local(tm::stamp(t.year, t.month as i32, t.day as i32 + offset as i32, 12, 0, 0));
        Date { year: l.year, month: l.month as u32, day: l.mday as u32 }
    }

    /// The value as the panel writes it.
    pub fn show(&self, field: Field) -> String {
        let v = self.value(field);
        match field {
            Field::View => self.view().id().to_string(),
            Field::Mode => self.draft.edited.mode.clone(),
            Field::Fps => format!("{} fps", v as i64),
            Field::Interactive | Field::Stories => if v > 0.5 { "on" } else { "off" }.into(),
            Field::Time => {
                let v = v.min(24.0 - 1.0 / 60.0);
                format!("{:02}:{:02}", v as i64, ((v.fract() * 60.0).round() as i64) % 60)
            }
            Field::Date => {
                let d = match self.draft.edited.scene.date {
                    Source::Fixed(d) => d,
                    Source::Auto => self.today,
                };
                let l = tm::local(tm::stamp(d.year, d.month as i32, d.day as i32, 12, 0, 0));
                let rel = match v as i64 {
                    0 => "today".to_string(),
                    n => format!("{:+}d", n),
                };
                let year = if l.year == self.today.year { String::new() } else { format!(" {}", l.year) };
                format!("{} {:02} {}{}  {}", &DAYS[l.dow][..3], l.mday, MONTHS[(l.month - 1) as usize], year, rel)
            }
            Field::Channel(c) => match c {
                Channel::Cloud | Channel::Rain | Channel::Snow | Channel::Fog | Channel::Load | Channel::Humidity => format!("{}%", v as i64),
                Channel::Storm | Channel::Music => if v > 0.5 { "on" } else { "off" }.into(),
                Channel::RainIn => {
                    if v >= 3.9 {
                        "not soon".into()
                    } else if v < 0.25 {
                        "any minute".into()
                    } else {
                        format!("{} h", v)
                    }
                }
                Channel::Wind => format!("{} km/h", v as i64),
                Channel::WindFrom => format!("{} {}°", WINDS[(((v + 22.5).rem_euclid(360.0)) / 45.0) as usize % 8], v as i64),
                Channel::Temperature => format!("{}°", v as i64),
                Channel::Agents | Channel::Containers => format!("{}", v as i64),
            },
            Field::Birds | Field::Meteors | Field::Planes => {
                if v == 0.0 {
                    "off".into()
                } else {
                    format!("×{}", v)
                }
            }
            Field::Knob(k) => self.knob(k).map(|k| (k.show)(v)).unwrap_or_default(),
            Field::Story(_) => "start".into(),
            Field::ScenePause => if self.scenes_paused { "paused" } else { "running" }.into(),
            Field::SceneStep => "step".into(),
            Field::SceneReset => "reset".into(),
        }
    }

    // ── what is different ────────────────────────────────────────────────────

    /// Number of settings that differ from what was saved.
    pub fn dirty_count(&self) -> usize {
        let (a, s) = (&self.draft.edited, &*self.draft.saved);
        let mut n = 0;
        n += (a.view != s.view) as usize + (a.mode != s.mode) as usize + (a.fps != s.fps) as usize + (a.interactive != s.interactive) as usize;
        n += (a.rates.birds != s.rates.birds) as usize + (a.rates.meteors != s.rates.meteors) as usize + (a.rates.planes != s.rates.planes) as usize;
        n += (a.stories != s.stories) as usize;
        n += (a.scene.time != s.scene.time) as usize + (a.scene.date != s.scene.date) as usize;
        n += Channel::ALL.iter().filter(|c| a.scene.get(**c) != s.scene.get(**c)).count();
        let mut ids: Vec<&String> = a.views.keys().chain(s.views.keys()).collect();
        ids.sort();
        ids.dedup();
        for id in ids {
            let (x, y) = (a.views.get(id), s.views.get(id));
            let mut keys: Vec<&String> = x.into_iter().chain(y).flat_map(|m| m.keys()).collect();
            keys.sort();
            keys.dedup();
            n += keys.into_iter().filter(|k| x.and_then(|m| m.get(*k)) != y.and_then(|m| m.get(*k))).count();
        }
        n
    }

    pub fn dirty(&self) -> bool {
        self.dirty_count() > 0
    }

    /// Everything held fixed in the draft: what a save would write as simulation or fixed values.
    pub fn fixed_summary(&self) -> Vec<String> {
        let mut out = Vec::new();
        let s = &self.draft.edited.scene;
        if let Source::Fixed(c) = s.time {
            out.push(format!("time {}", c.text()));
        }
        if let Source::Fixed(d) = s.date {
            out.push(format!("date {}", d.text()));
        }
        for c in Channel::ALL {
            if s.get(c).is_fixed() {
                out.push(format!("{} {}", self.info(Field::Channel(c)).label, self.show(Field::Channel(c))));
            }
        }
        out
    }

    /// What the wallpaper should be held to while the draft is open, as `dev.json` says it:
    /// the values that differ from what is saved, and the keys the draft has given back to
    /// reality although the saved profile fixes them.
    pub fn overrides(&self) -> (serde_json::Map<String, serde_json::Value>, Vec<String>) {
        use serde_json::json;
        let (e, s) = (&self.draft.edited, &*self.draft.saved);
        let mut set = serde_json::Map::new();
        let mut auto = Vec::new();
        if e.view != s.view {
            set.insert("view".into(), json!(e.view));
        }
        if e.mode != s.mode {
            set.insert("mode".into(), json!(e.mode));
        }
        for (key, a, b) in [("birds", e.rates.birds, s.rates.birds), ("meteors", e.rates.meteors, s.rates.meteors), ("planes", e.rates.planes, s.rates.planes)] {
            if a != b {
                set.insert(key.into(), json!(a));
            }
        }
        if e.scene.time != s.scene.time {
            match e.scene.time {
                Source::Fixed(c) => {
                    set.insert("time".into(), json!(c.0 as f64 / 60.0));
                }
                Source::Auto => auto.push("time".to_string()),
            }
        }
        if e.scene.date != s.scene.date {
            match e.scene.date {
                Source::Fixed(d) => {
                    set.insert("day".into(), json!(self.day_offset(d)));
                }
                Source::Auto => auto.push("day".to_string()),
            }
        }
        for c in Channel::ALL {
            if e.scene.get(c) != s.scene.get(c) {
                match e.scene.get(c) {
                    Source::Fixed(v) => {
                        set.insert(c.key().into(), json!(v));
                    }
                    Source::Auto => auto.push(c.key().to_string()),
                }
            }
        }
        for k in self.view().knobs() {
            let now = e.views.get(self.view().id()).and_then(|m| m.get(k.key));
            let was = s.views.get(self.view().id()).and_then(|m| m.get(k.key));
            if now != was {
                match now {
                    Some(v) => {
                        set.insert(k.key.into(), json!(v));
                    }
                    None => auto.push(k.key.to_string()),
                }
            }
        }
        (set, auto)
    }

    pub fn peeking(&self) -> bool {
        self.pinned || self.capture.is_some() || self.now < self.peek_until
    }

    fn touched(&mut self) {
        self.peek_until = self.now + 0.9;
    }

    // ── doing things ─────────────────────────────────────────────────────────

    pub fn dispatch(&mut self, action: Action) -> Vec<Effect> {
        if !matches!(action, Action::Scroll(_) | Action::Drag(_) | Action::EndDrag | Action::CancelDrag) {
            self.follow = true;
        }
        // while a question is open it is the only thing that answers
        if let Some(dialog) = self.dialog.clone() {
            return match action {
                Action::DialogChoice(c) if dialog.choices().contains(&c) => self.answer(c),
                Action::Escape | Action::Quit => {
                    self.dialog = None;
                    self.exit_after_save = false;
                    Vec::new()
                }
                _ => Vec::new(),
            };
        }
        if self.entry.is_some() {
            match action {
                Action::EditChar(_) | Action::EditBackspace | Action::EditCommit | Action::EditCancel => {}
                Action::Escape => return self.dispatch(Action::EditCancel),
                _ => return Vec::new(),
            }
        }
        match action {
            Action::Focus(f) => {
                if self.fields(f.category()).contains(&f) {
                    self.category = f.category();
                    self.focus = f;
                    self.remembered.insert(self.category, f);
                }
                Vec::new()
            }
            Action::FocusStep(d) => {
                // one list of every control, category after category
                let cats = self.categories();
                let mut all: Vec<Field> = Vec::new();
                for c in &cats {
                    all.extend(self.fields(*c));
                }
                if !all.is_empty() {
                    let at = all.iter().position(|f| *f == self.focus).unwrap_or(0) as i32;
                    let to = all[(at + d).rem_euclid(all.len() as i32) as usize];
                    self.category = to.category();
                    self.focus = to;
                    self.remembered.insert(self.category, to);
                }
                Vec::new()
            }
            Action::Category(c) => {
                self.set_category(c);
                Vec::new()
            }
            Action::Adjust(f, steps) => self.step(f, steps, false),
            Action::Big(f, steps) => self.step(f, steps, true),
            Action::SetValue(f, v) => {
                self.touched();
                self.set(f, v)
            }
            Action::Toggle(f) => {
                self.touched();
                self.toggle(f)
            }
            Action::BeginDrag { field, span, x } => {
                if self.info(field).shape != Shape::Slider {
                    return self.dispatch(Action::Toggle(field));
                }
                self.focus = field;
                self.remembered.insert(field.category(), field);
                self.capture = Some(Capture { field, span });
                self.drag_to(x)
            }
            Action::Drag(x) => self.drag_to(x),
            Action::EndDrag => {
                self.capture = None;
                self.peek_until = self.now + 0.25;
                Vec::new()
            }
            Action::CancelDrag => {
                self.capture = None;
                Vec::new()
            }
            Action::EditStart => {
                let field = self.focus;
                self.entry = Some(Entry { field, text: String::new(), error: None });
                Vec::new()
            }
            Action::EditChar(c) => {
                if let Some(e) = &mut self.entry {
                    if e.text.chars().count() < 24 && !c.is_control() {
                        e.text.push(c);
                        e.error = None;
                    }
                }
                Vec::new()
            }
            Action::EditBackspace => {
                if let Some(e) = &mut self.entry {
                    e.text.pop();
                    e.error = None;
                }
                Vec::new()
            }
            Action::EditCancel => {
                self.entry = None;
                Vec::new()
            }
            Action::EditCommit => {
                let Some(e) = self.entry.clone() else { return Vec::new() };
                match self.parse_entry(e.field, &e.text) {
                    Ok(Parsed::Auto) => {
                        self.entry = None;
                        self.dispatch(Action::SetAuto(e.field))
                    }
                    Ok(Parsed::Value(v)) => {
                        self.entry = None;
                        self.set_exact(e.field, v)
                    }
                    Ok(Parsed::Text(t)) => {
                        self.entry = None;
                        self.set_text(e.field, &t)
                    }
                    Err(why) => {
                        // stays open, with the reason, until it is fixed or cancelled
                        if let Some(entry) = &mut self.entry {
                            entry.error = Some(why);
                        }
                        Vec::new()
                    }
                }
            }
            Action::SetAuto(f) => self.auto(f),
            Action::ResetField(f) => self.reset(f),
            Action::Now => {
                let mut out = self.auto(Field::Time);
                out.extend(self.auto(Field::Date));
                out
            }
            Action::Save => self.begin_save(SavePolicy::IfUnchanged, false),
            Action::Discard => {
                self.draft.edited = (*self.draft.saved).clone();
                self.status = Status::Notice("Changes discarded; back to what is saved".into());
                self.normalise_focus();
                vec![Effect::Publish]
            }
            Action::Preview => {
                self.pinned = !self.pinned;
                Vec::new()
            }
            Action::Help => {
                self.help = !self.help;
                Vec::new()
            }
            Action::Escape => {
                // one layer at a time: help, the pinned preview, then nothing
                if self.help {
                    self.help = false;
                } else if self.pinned {
                    self.pinned = false;
                }
                Vec::new()
            }
            Action::Quit => {
                if self.dirty() {
                    self.dialog = Some(Dialog::Unsaved);
                    Vec::new()
                } else {
                    vec![Effect::Exit]
                }
            }
            Action::DialogChoice(_) => Vec::new(),
            Action::Scroll(d) => {
                self.scroll = (self.scroll as i64 + d as i64).max(0) as usize;
                self.follow = false;
                Vec::new()
            }
        }
    }

    fn answer(&mut self, choice: Choice) -> Vec<Effect> {
        let dialog = self.dialog.take();
        match (dialog, choice) {
            (_, Choice::Cancel) => {
                self.exit_after_save = false;
                Vec::new()
            }
            (Some(Dialog::Unsaved), Choice::Save) => self.begin_save(SavePolicy::IfUnchanged, true),
            (Some(Dialog::Unsaved), Choice::Discard) => {
                self.draft.edited = (*self.draft.saved).clone();
                vec![Effect::Exit]
            }
            (Some(Dialog::Conflict), Choice::Reload) => vec![Effect::Reload],
            (Some(Dialog::Conflict), Choice::Overwrite) => {
                let exit = std::mem::take(&mut self.exit_after_save);
                self.begin_save(SavePolicy::Overwrite, exit)
            }
            _ => Vec::new(),
        }
    }

    fn begin_save(&mut self, policy: SavePolicy, exit: bool) -> Vec<Effect> {
        self.status = Status::Saving;
        self.exit_after_save = exit;
        vec![Effect::Save { policy, exit }]
    }

    /// The caller saved the draft. Only now does the panel say Saved.
    pub fn save_finished(&mut self, result: Result<SaveReceipt, SaveError>) -> Vec<Effect> {
        match result {
            Ok(receipt) => {
                self.draft.saved = Arc::new(self.draft.edited.clone());
                let mut said = format!("Saved to {}", receipt.config.display());
                if let Some(b) = &receipt.backup {
                    said.push_str(&format!("; previous kept as {}", b.display()));
                }
                if receipt.view_file_cleared {
                    said.push_str("; the view chosen with `dormer view` now follows this setting");
                }
                self.status = Status::Saved(said);
                if std::mem::take(&mut self.exit_after_save) {
                    return vec![Effect::Exit];
                }
                vec![Effect::Publish]
            }
            Err(SaveError::Conflict) => {
                self.status = Status::Failed("config.toml changed since it was opened; nothing was written".into());
                self.dialog = Some(Dialog::Conflict);
                Vec::new()
            }
            Err(e) => {
                // the draft, the focus and any exit that was asked for are kept exactly as they were
                self.exit_after_save = false;
                let said = match &e {
                    SaveError::Io { why, recovery } => {
                        let tail = match recovery {
                            Recovery::Untouched => "Nothing was changed; your edits are still here.",
                            Recovery::RolledBack => "The earlier write was put back; your edits are still here.",
                            Recovery::Partial => "The files may be half updated: check config.toml and the saved view, then Save again.",
                        };
                        format!("{} {}", why, tail)
                    }
                    other => format!("{} Your edits are still here.", other),
                };
                // the operating system's error number says nothing useful here
                let said = said.replace(|c: char| c == '\n', " ");
                let said = match said.find(" (os error ") {
                    Some(i) => match said[i..].find(')') {
                        Some(j) => format!("{}{}", &said[..i], &said[i + j + 1..]),
                        None => said,
                    },
                    None => said,
                };
                self.status = Status::Failed(format!("Not saved: {}", said));
                Vec::new()
            }
        }
    }

    /// The caller re-read the config file because the user chose to reload: what is there now
    /// is the new saved baseline, and the fields the draft had changed stay changed.
    pub fn reloaded(&mut self, now_saved: RuntimeSettings) {
        let old = self.draft.saved.clone();
        let mut merged = now_saved.clone();
        let (e, o) = (&self.draft.edited, &*old);
        if e.view != o.view {
            merged.view = e.view.clone();
        }
        if e.mode != o.mode {
            merged.mode = e.mode.clone();
        }
        if e.fps != o.fps {
            merged.fps = e.fps;
        }
        if e.interactive != o.interactive {
            merged.interactive = e.interactive;
        }
        if e.stories != o.stories {
            merged.stories = e.stories;
        }
        if e.rates.birds != o.rates.birds {
            merged.rates.birds = e.rates.birds;
        }
        if e.rates.meteors != o.rates.meteors {
            merged.rates.meteors = e.rates.meteors;
        }
        if e.rates.planes != o.rates.planes {
            merged.rates.planes = e.rates.planes;
        }
        if e.scene.time != o.scene.time {
            merged.scene.time = e.scene.time;
        }
        if e.scene.date != o.scene.date {
            merged.scene.date = e.scene.date;
        }
        for c in Channel::ALL {
            if e.scene.get(c) != o.scene.get(c) {
                match e.scene.get(c) {
                    Source::Fixed(v) => {
                        merged.scene.fix(c, v);
                    }
                    Source::Auto => merged.scene.release(c),
                }
            }
        }
        for (id, knobs) in &e.views {
            for (k, v) in knobs {
                if o.views.get(id).and_then(|m| m.get(k)) != Some(v) {
                    merged.views.entry(id.clone()).or_default().insert(k.clone(), *v);
                }
            }
        }
        for (id, knobs) in &o.views {
            for k in knobs.keys() {
                if e.views.get(id).and_then(|m| m.get(k)).is_none() {
                    if let Some(m) = merged.views.get_mut(id) {
                        m.remove(k);
                    }
                }
            }
        }
        self.draft.saved = Arc::new(now_saved);
        self.draft.edited = merged;
        self.status = Status::Notice("Reloaded config.toml; your changes are kept on top of it".into());
        self.normalise_focus();
    }

    /// Show that nothing was written, with why.
    pub fn fail(&mut self, why: String) {
        self.status = Status::Failed(why);
        self.exit_after_save = false;
    }

    fn set_category(&mut self, c: Category) {
        if !self.categories().contains(&c) {
            return;
        }
        self.remembered.insert(self.category, self.focus);
        self.category = c;
        self.scroll = 0;
        let list = self.fields(c);
        self.focus = self.remembered.get(&c).copied().filter(|f| list.contains(f)).unwrap_or(list[0]);
    }

    /// After the view changes or the draft is replaced: keep the focus where it can still be.
    fn normalise_focus(&mut self) {
        if !self.categories().contains(&self.category) {
            self.category = Category::Appearance;
        }
        let list = self.fields(self.category);
        if !list.contains(&self.focus) {
            self.focus = self.remembered.get(&self.category).copied().filter(|f| list.contains(f)).unwrap_or(list[0]);
        }
    }

    fn drag_to(&mut self, x: u16) -> Vec<Effect> {
        let Some(c) = self.capture else { return Vec::new() };
        let info = self.info(c.field);
        let (x0, w) = c.span;
        let f = ((x as f64 - x0 as f64) / w.saturating_sub(1).max(1) as f64).clamp(0.0, 1.0);
        self.set(c.field, info.lo + f * (info.hi - info.lo))
    }

    fn step(&mut self, f: Field, steps: f64, big: bool) -> Vec<Effect> {
        let info = self.info(f);
        self.touched();
        match info.shape {
            Shape::Toggle | Shape::Button => self.toggle(f),
            Shape::Choice => {
                let n = (info.hi + 1.0) as i64;
                let at = self.value(f) as i64;
                let next = (at + steps.signum() as i64).rem_euclid(n.max(1));
                self.set(f, next as f64)
            }
            Shape::Slider => {
                let unit = if big { info.big } else { info.step };
                let from = self.value(f);
                self.set(f, from + steps * unit)
            }
        }
    }

    fn toggle(&mut self, f: Field) -> Vec<Effect> {
        // the scene tools do things; they change no setting
        match f {
            Field::Story(i) => {
                return self.stories.get(i as usize).cloned().map(|n| vec![Effect::Scene(SceneCommand::Trigger(n))]).unwrap_or_default();
            }
            Field::ScenePause => {
                self.scenes_paused = !self.scenes_paused;
                return vec![Effect::Scene(SceneCommand::Pause(self.scenes_paused))];
            }
            Field::SceneStep => return vec![Effect::Scene(SceneCommand::Step)],
            Field::SceneReset => {
                self.scenes_paused = false;
                return vec![Effect::Scene(SceneCommand::Reset)];
            }
            _ => {}
        }
        let on = self.value(f) > 0.5;
        match self.info(f).shape {
            Shape::Choice => {
                let n = (self.info(f).hi + 1.0) as i64;
                self.set(f, ((self.value(f) as i64 + 1).rem_euclid(n)) as f64)
            }
            _ => self.set(f, if on { 0.0 } else { 1.0 }),
        }
    }

    /// Set a value from a control: moved to the field's step, kept inside its limits.
    fn set(&mut self, f: Field, v: f64) -> Vec<Effect> {
        if !v.is_finite() {
            return Vec::new();
        }
        let info = self.info(f);
        let q = if info.step > 0.0 { (v / info.step).round() * info.step } else { v };
        self.set_exact(f, q.clamp(info.lo, info.hi))
    }

    /// Set a value as given (direct entry): checked against the field's real limits by `parse_entry`.
    fn set_exact(&mut self, f: Field, v: f64) -> Vec<Effect> {
        if !v.is_finite() {
            return Vec::new();
        }
        let before = self.draft.edited.clone();
        let edited = &mut self.draft.edited;
        match f {
            // tools, not settings
            Field::Story(_) | Field::ScenePause | Field::SceneStep | Field::SceneReset => return Vec::new(),
            Field::View => {
                if let Some(view) = views::ALL.get(v.round().max(0.0) as usize) {
                    edited.view = view.id().to_string();
                }
            }
            Field::Mode => edited.mode = ["auto", "dark", "light"][(v.round().clamp(0.0, 2.0)) as usize].to_string(),
            Field::Fps => edited.fps = v.round().clamp(5.0, 60.0) as u32,
            Field::Interactive => edited.interactive = v > 0.5,
            Field::Stories => edited.stories = v > 0.5,
            Field::Birds => edited.rates.birds = v.clamp(0.0, 20.0) as f32,
            Field::Meteors => edited.rates.meteors = v.clamp(0.0, 20.0) as f32,
            Field::Planes => edited.rates.planes = v.clamp(0.0, 20.0) as f32,
            Field::Time => {
                let minutes = (v * 60.0).round().clamp(0.0, 23.0 * 60.0 + 59.0) as u32;
                edited.scene.time = Source::Fixed(Clock(minutes as u16));
            }
            Field::Date => {
                let d = self.date_at(v.round() as i64);
                self.draft.edited.scene.date = Source::Fixed(d);
            }
            Field::Channel(c) => {
                edited.scene.fix(c, v);
            }
            Field::Knob(k) => {
                let id = self.view().id().to_string();
                if let Some(knob) = self.knob(k) {
                    let v = v.clamp(knob.lo.min(knob.hi), knob.hi.max(knob.lo));
                    self.draft.edited.views.entry(id).or_default().insert(k.to_string(), (v * 10000.0).round() / 10000.0);
                }
            }
        }
        let changed = self.draft.edited != before;
        if f == Field::View {
            self.normalise_focus();
        }
        if changed {
            self.status = Status::Idle;
            vec![Effect::Publish]
        } else {
            Vec::new()
        }
    }

    fn auto(&mut self, f: Field) -> Vec<Effect> {
        let before = self.draft.edited.clone();
        match f {
            Field::Time => self.draft.edited.scene.time = Source::Auto,
            Field::Date => self.draft.edited.scene.date = Source::Auto,
            Field::Channel(c) => self.draft.edited.scene.release(c),
            Field::Knob(k) => {
                let id = self.view().id().to_string();
                if let Some(m) = self.draft.edited.views.get_mut(&id) {
                    m.remove(k);
                    if m.is_empty() {
                        self.draft.edited.views.remove(&id);
                    }
                }
            }
            _ => return self.reset(f),
        }
        self.after_change(before)
    }

    /// The field as it was saved.
    fn reset(&mut self, f: Field) -> Vec<Effect> {
        let before = self.draft.edited.clone();
        let saved = self.draft.saved.clone();
        let e = &mut self.draft.edited;
        match f {
            Field::Story(_) | Field::ScenePause | Field::SceneStep | Field::SceneReset => return Vec::new(),
            Field::View => e.view = saved.view.clone(),
            Field::Mode => e.mode = saved.mode.clone(),
            Field::Fps => e.fps = saved.fps,
            Field::Interactive => e.interactive = saved.interactive,
            Field::Stories => e.stories = saved.stories,
            Field::Birds => e.rates.birds = saved.rates.birds,
            Field::Meteors => e.rates.meteors = saved.rates.meteors,
            Field::Planes => e.rates.planes = saved.rates.planes,
            Field::Time => e.scene.time = saved.scene.time,
            Field::Date => e.scene.date = saved.scene.date,
            Field::Channel(c) => match saved.scene.get(c) {
                Source::Fixed(v) => {
                    e.scene.fix(c, v);
                }
                Source::Auto => e.scene.release(c),
            },
            Field::Knob(k) => {
                let id = self.draft.edited.view.clone();
                let was = saved.views.get(&id).and_then(|m| m.get(k)).copied();
                match was {
                    Some(v) => {
                        self.draft.edited.views.entry(id).or_default().insert(k.to_string(), v);
                    }
                    None => {
                        if let Some(m) = self.draft.edited.views.get_mut(&id) {
                            m.remove(k);
                            if m.is_empty() {
                                self.draft.edited.views.remove(&id);
                            }
                        }
                    }
                }
            }
        }
        self.after_change(before)
    }

    fn after_change(&mut self, before: RuntimeSettings) -> Vec<Effect> {
        self.normalise_focus();
        if self.draft.edited != before {
            self.status = Status::Idle;
            vec![Effect::Publish]
        } else {
            Vec::new()
        }
    }

    // ── direct entry ─────────────────────────────────────────────────────────

    fn parse_entry(&self, f: Field, text: &str) -> Result<Parsed, String> {
        let t = text.trim();
        if t.is_empty() {
            return Err("type a value, or Escape to cancel".into());
        }
        let info = self.info(f);
        let lower = t.to_lowercase();
        if info.sourced && (lower == "auto" || lower == "real" || lower == "live") {
            return Ok(Parsed::Auto);
        }
        match f {
            Field::View => {
                return views::find(&lower).map(|v| Parsed::Text(v.id().to_string())).ok_or_else(|| format!("choose one of: {}", views::names()));
            }
            Field::Mode => {
                return if ["auto", "dark", "light"].contains(&lower.as_str()) { Ok(Parsed::Text(lower)) } else { Err("choose auto, dark or light".into()) };
            }
            Field::Time => {
                if let Some(c) = Clock::parse(t) {
                    return Ok(Parsed::Value(c.0 as f64 / 60.0));
                }
                let h: f64 = t.parse().map_err(|_| "a time looks like 21:30".to_string())?;
                return if (0.0..24.0).contains(&h) { Ok(Parsed::Value(h)) } else { Err("a time of day is 00:00 to 23:59".into()) };
            }
            Field::Date => {
                if lower == "today" || lower == "now" {
                    return Ok(Parsed::Value(0.0));
                }
                if let Some(d) = Date::parse(t) {
                    return Ok(Parsed::Value(self.day_offset(d) as f64));
                }
                if let Ok(n) = t.trim_start_matches('+').parse::<i64>() {
                    return if n.abs() <= 3650 { Ok(Parsed::Value(n as f64)) } else { Err("that many days is out of range".into()) };
                }
                return Err("a date looks like 2026-10-15, or +3 for three days on".into());
            }
            _ => {}
        }
        if info.shape == Shape::Toggle {
            return match lower.as_str() {
                "on" | "true" | "yes" | "1" => Ok(Parsed::Value(1.0)),
                "off" | "false" | "no" | "0" => Ok(Parsed::Value(0.0)),
                _ => Err("say on or off".into()),
            };
        }
        let number = lower.trim_end_matches(|c: char| c.is_alphabetic() || matches!(c, '%' | '°' | '/' | ' ' | '×' | 'x')).trim_start_matches(['×', 'x']);
        let v: f64 = number.trim().parse().map_err(|_| "that is not a number".to_string())?;
        if !v.is_finite() {
            return Err("that is not a finite number".into());
        }
        let (lo, hi) = match f {
            Field::Channel(c) => {
                let (lo, hi) = c.range();
                // the panel's sliders stop short of what the file accepts; typing may go as far
                (lo.min(info.lo), hi.max(info.hi))
            }
            Field::Birds | Field::Meteors | Field::Planes => (0.0, 20.0),
            _ => (info.lo, info.hi),
        };
        if v < lo || v > hi {
            return Err(format!("must be between {} and {}", lo, hi));
        }
        Ok(Parsed::Value(v))
    }

    fn set_text(&mut self, f: Field, text: &str) -> Vec<Effect> {
        match f {
            Field::View => {
                let at = views::ALL.iter().position(|v| v.id() == text).unwrap_or(0);
                self.set_exact(f, at as f64)
            }
            Field::Mode => {
                let at = ["auto", "dark", "light"].iter().position(|m| *m == text).unwrap_or(0);
                self.set_exact(f, at as f64)
            }
            _ => Vec::new(),
        }
    }
}

enum Parsed {
    Auto,
    Value(f64),
    Text(String),
}

#[cfg(test)]
pub fn test_model() -> Model {
    let saved = Arc::new(RuntimeSettings::default());
    let mut reality = Reality { time: 9.25, ..Default::default() };
    reality.channels.insert(Channel::Rain, 30.0);
    reality.channels.insert(Channel::Agents, 1.0);
    Model::new(saved, reality, Date::new(2026, 10, 15).unwrap(), PathBuf::from("/tmp/dormer-test/config.toml"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn receipt() -> SaveReceipt {
        SaveReceipt { config: "/tmp/c.toml".into(), backup: Some("/tmp/c.toml.bak".into()), revision: 1, view_file_cleared: false, changed_keys: vec![] }
    }

    #[test]
    fn unsaved_quit_supports_save_discard_cancel() {
        let mut m = test_model();
        // clean: quits at once
        assert_eq!(m.dispatch(Action::Quit), vec![Effect::Exit]);
        m.dispatch(Action::SetValue(Field::Channel(Channel::Rain), 60.0));
        assert!(m.dirty());
        assert_eq!(m.dispatch(Action::Quit), Vec::new());
        assert_eq!(m.dialog, Some(Dialog::Unsaved));
        // cancel: back to the panel with the edit
        assert!(m.dispatch(Action::DialogChoice(Choice::Cancel)).is_empty());
        assert!(m.dialog.is_none() && m.dirty());
        // other actions are ignored while the question is open
        m.dispatch(Action::Quit);
        assert!(m.dispatch(Action::Discard).is_empty());
        assert!(m.dirty(), "discard cannot be reached around the dialog");
        // save: exits only once the save has succeeded
        let fx = m.dispatch(Action::DialogChoice(Choice::Save));
        assert_eq!(fx, vec![Effect::Save { policy: SavePolicy::IfUnchanged, exit: true }]);
        assert_eq!(m.status, Status::Saving);
        assert_eq!(m.save_finished(Ok(receipt())), vec![Effect::Exit]);
        assert!(!m.dirty());
        // discard: leaves without writing, and the draft is the saved one again
        m.dispatch(Action::SetValue(Field::Fps, 12.0));
        m.dispatch(Action::Quit);
        assert_eq!(m.dispatch(Action::DialogChoice(Choice::Discard)), vec![Effect::Exit]);
        assert!(!m.dirty());
    }

    #[test]
    fn failed_save_keeps_edit_and_focus() {
        let mut m = test_model();
        m.dispatch(Action::Category(Category::Machine));
        m.dispatch(Action::Focus(Field::Channel(Channel::Load)));
        m.dispatch(Action::SetValue(Field::Channel(Channel::Load), 80.0));
        m.dispatch(Action::Quit);
        m.dispatch(Action::DialogChoice(Choice::Save));
        let err = SaveError::Io { why: "cannot write /x: permission denied".into(), recovery: Recovery::Untouched };
        assert!(m.save_finished(Err(err)).is_empty(), "a failed save must not exit");
        assert!(m.dirty());
        assert_eq!((m.category, m.focus), (Category::Machine, Field::Channel(Channel::Load)));
        match &m.status {
            Status::Failed(s) => assert!(s.contains("permission denied") && s.contains("still here") && s.starts_with("Not saved: "), "{}", s),
            other => panic!("{:?}", other),
        }
        // and a later successful save is what says Saved
        m.dispatch(Action::Save);
        assert_eq!(m.save_finished(Ok(receipt())), vec![Effect::Publish]);
        assert!(matches!(m.status, Status::Saved(_)) && !m.dirty());
    }

    #[test]
    fn a_conflict_offers_reload_or_overwrite_and_keeps_the_draft() {
        let mut m = test_model();
        m.dispatch(Action::SetValue(Field::Fps, 20.0));
        m.dispatch(Action::Save);
        m.save_finished(Err(SaveError::Conflict));
        assert_eq!(m.dialog, Some(Dialog::Conflict));
        assert!(m.dirty());
        assert_eq!(m.dispatch(Action::DialogChoice(Choice::Overwrite)), vec![Effect::Save { policy: SavePolicy::Overwrite, exit: false }]);
        m.save_finished(Err(SaveError::Conflict));
        assert_eq!(m.dispatch(Action::DialogChoice(Choice::Reload)), vec![Effect::Reload]);
        // the file now says something else about the weather; the fps edit stays on top of it
        let mut disk = RuntimeSettings::default();
        disk.scene.fix(Channel::Snow, 40.0);
        m.reloaded(disk);
        assert_eq!(m.draft.edited.fps, 20);
        assert_eq!(m.draft.edited.scene.get(Channel::Snow), Source::Fixed(40.0));
        assert_eq!(m.dirty_count(), 1, "only the user's own edit is unsaved");
    }

    #[test]
    fn source_mode_and_direct_entry_are_explicit() {
        let mut m = test_model();
        let rain = Field::Channel(Channel::Rain);
        // untouched: automatic, showing the real reading
        assert_eq!(m.fixed(rain), Some(false));
        assert_eq!(m.value(rain), 30.0);
        assert_eq!(m.show(rain), "30%");
        // any adjustment makes it Fixed, visibly
        m.dispatch(Action::Adjust(rain, 1.0));
        assert_eq!(m.fixed(rain), Some(true));
        assert_eq!(m.value(rain), 35.0);
        // and the user can give it back
        m.dispatch(Action::SetAuto(rain));
        assert_eq!(m.fixed(rain), Some(false));
        assert_eq!(m.value(rain), 30.0);
        assert!(!m.dirty());
        // direct entry: units, ranges, words
        m.dispatch(Action::Focus(rain));
        m.dispatch(Action::EditStart);
        for c in "45%".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        assert!(m.entry.is_none());
        assert_eq!(m.draft.edited.scene.get(Channel::Rain), Source::Fixed(45.0));
        m.dispatch(Action::EditStart);
        for c in "140".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        let e = m.entry.as_ref().expect("an invalid entry stays open");
        assert!(e.error.as_deref().unwrap().contains("between"), "{:?}", e.error);
        assert_eq!(m.value(rain), 45.0, "nothing applied");
        m.dispatch(Action::Escape);
        assert!(m.entry.is_none());
        // the time: HH:MM, and the word auto
        m.dispatch(Action::Focus(Field::Time));
        m.dispatch(Action::EditStart);
        for c in "21:30".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        assert_eq!(m.draft.edited.scene.time, Source::Fixed(Clock::new(21, 30).unwrap()));
        assert_eq!(m.show(Field::Time), "21:30");
        m.dispatch(Action::EditStart);
        for c in "auto".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        assert_eq!(m.draft.edited.scene.time, Source::Auto);
        // a date is a calendar date, whatever today is
        m.dispatch(Action::Focus(Field::Date));
        m.dispatch(Action::EditStart);
        for c in "2026-12-25".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        assert_eq!(m.draft.edited.scene.date, Source::Fixed(Date::new(2026, 12, 25).unwrap()));
        assert_eq!(m.value(Field::Date), 71.0);
        // and the summary says what saving would hold fixed
        assert!(m.fixed_summary().iter().any(|l| l.contains("2026-12-25")));
        assert!(m.fixed_summary().iter().any(|l| l.contains("rain 45%")));
    }

    #[test]
    fn focus_survives_category_and_view_changes() {
        let mut m = test_model();
        m.dispatch(Action::Focus(Field::Channel(Channel::Fog)));
        m.dispatch(Action::Category(Category::Machine));
        assert_eq!(m.focus, Field::Channel(Channel::Agents));
        m.dispatch(Action::Focus(Field::Channel(Channel::Music)));
        m.dispatch(Action::Category(Category::Sky));
        assert_eq!(m.focus, Field::Channel(Channel::Fog));
        m.dispatch(Action::Category(Category::Machine));
        assert_eq!(m.focus, Field::Channel(Channel::Music));
        // the view's own category exists only where the view has knobs, and its focus falls back
        m.dispatch(Action::Focus(Field::View));
        let street = views::ALL.iter().position(|v| v.id() == "street").unwrap() as f64;
        m.dispatch(Action::SetValue(Field::View, street));
        assert!(m.categories().contains(&Category::View));
        m.dispatch(Action::Category(Category::View));
        let first = m.fields(Category::View)[0];
        assert_eq!(m.focus, first);
        let rooftops = views::ALL.iter().position(|v| v.id() == "rooftops").unwrap() as f64;
        m.dispatch(Action::SetValue(Field::View, rooftops));
        assert!(!m.categories().contains(&Category::View));
        assert_eq!(m.category, Category::Appearance);
        assert!(m.fields(m.category).contains(&m.focus));
    }

    #[test]
    fn drags_are_captured_by_the_original_slider_and_can_be_cancelled() {
        let mut m = test_model();
        let cloud = Field::Channel(Channel::Cloud);
        m.dispatch(Action::BeginDrag { field: cloud, span: (20, 21), x: 30 });
        assert_eq!(m.value(cloud), 50.0);
        // far outside the row: still this slider, clamped
        m.dispatch(Action::Drag(200));
        assert_eq!(m.value(cloud), 100.0);
        m.dispatch(Action::Drag(0));
        assert_eq!(m.value(cloud), 0.0);
        m.dispatch(Action::CancelDrag);
        assert!(m.capture.is_none());
        // drag events after a cancel (a terminal resize) do nothing
        m.dispatch(Action::Drag(40));
        assert_eq!(m.value(cloud), 0.0);
        // a switch is toggled by a press, not dragged
        m.dispatch(Action::BeginDrag { field: Field::Channel(Channel::Storm), span: (20, 21), x: 30 });
        assert!(m.capture.is_none());
        assert_eq!(m.value(Field::Channel(Channel::Storm)), 1.0);
    }

    #[test]
    fn discard_and_reset_return_to_what_was_saved_not_to_defaults() {
        let mut saved = RuntimeSettings::default();
        saved.fps = 24;
        saved.scene.fix(Channel::Fog, 30.0);
        let mut m = test_model();
        m.draft = Draft::new(Arc::new(saved));
        m.dispatch(Action::SetValue(Field::Fps, 40.0));
        m.dispatch(Action::SetAuto(Field::Channel(Channel::Fog)));
        m.dispatch(Action::SetValue(Field::Channel(Channel::Rain), 10.0));
        assert_eq!(m.dirty_count(), 3);
        m.dispatch(Action::ResetField(Field::Fps));
        assert_eq!(m.draft.edited.fps, 24);
        m.dispatch(Action::ResetField(Field::Channel(Channel::Fog)));
        assert_eq!(m.draft.edited.scene.get(Channel::Fog), Source::Fixed(30.0));
        assert_eq!(m.dispatch(Action::Discard), vec![Effect::Publish]);
        assert!(!m.dirty());
        assert_eq!(m.draft.edited.scene.get(Channel::Rain), Source::Auto);
    }

    #[test]
    fn overrides_carry_only_what_differs_and_what_was_given_back() {
        let mut saved = RuntimeSettings::default();
        saved.scene.fix(Channel::Fog, 30.0);
        saved.scene.time = Source::Fixed(Clock::new(8, 0).unwrap());
        let mut m = test_model();
        m.draft = Draft::new(Arc::new(saved));
        let (set, auto) = m.overrides();
        assert!(set.is_empty() && auto.is_empty(), "an untouched draft holds nothing of its own");
        m.dispatch(Action::SetValue(Field::Channel(Channel::Rain), 55.0));
        m.dispatch(Action::SetAuto(Field::Channel(Channel::Fog)));
        m.dispatch(Action::SetValue(Field::Time, 22.5));
        m.dispatch(Action::SetValue(Field::Birds, 2.0));
        let (set, auto) = m.overrides();
        assert_eq!(set.get("rain").and_then(|v| v.as_f64()), Some(55.0));
        assert_eq!(set.get("time").and_then(|v| v.as_f64()), Some(22.5));
        assert_eq!(set.get("birds").and_then(|v| v.as_f64()), Some(2.0));
        assert_eq!(auto, vec!["fog".to_string()]);
        assert!(!set.contains_key("fog"));
    }

    #[test]
    fn invalid_and_nonfinite_values_change_nothing() {
        let mut m = test_model();
        assert!(m.dispatch(Action::SetValue(Field::Fps, f64::NAN)).is_empty());
        assert!(m.dispatch(Action::SetValue(Field::Channel(Channel::Wind), f64::INFINITY)).is_empty());
        assert!(!m.dirty());
        // beyond the slider is clamped, never stored out of range
        m.dispatch(Action::SetValue(Field::Fps, 5000.0));
        assert_eq!(m.draft.edited.fps, 60);
    }
}
