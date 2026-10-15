//! What the user has chosen, as a typed, validated, immutable snapshot.
//!
//! `config.toml` is read into a [`RuntimeSettings`] that the live layer, the
//! still painter and the dev panel all share. Every adjustable input of the
//! scene (the time, the date, each part of the weather, what the machine
//! shows) is a [`Source`]: `Auto` follows reality, `Fixed` holds the value the
//! user saved. Nothing in here touches the file system except `load`; writing
//! is `file.rs`'s job.
//!
//! Precedence, lowest first: real inputs, the saved profile, the dev panel's
//! live draft, explicit command-line arguments.

use std::collections::BTreeMap;
use std::fmt;
use std::path::Path;
use std::sync::{Arc, RwLock};

pub mod file;
pub mod profile;

/// Where one input of the scene comes from.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Source<T> {
    /// Follow the real world (or the real machine).
    Auto,
    /// Hold this value, whatever the world does.
    Fixed(T),
}

impl<T> Default for Source<T> {
    fn default() -> Self {
        Source::Auto
    }
}

impl<T> Source<T> {
    pub fn is_fixed(&self) -> bool {
        matches!(self, Source::Fixed(_))
    }
}

/// A wall-clock time of day, kept as minutes after midnight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Clock(pub u16);

impl Clock {
    pub fn new(hour: u32, min: u32) -> Option<Clock> {
        (hour < 24 && min < 60).then(|| Clock((hour * 60 + min) as u16))
    }

    pub fn hour(self) -> i32 {
        (self.0 / 60) as i32
    }

    pub fn min(self) -> i32 {
        (self.0 % 60) as i32
    }

    /// "HH:MM"
    pub fn text(self) -> String {
        format!("{:02}:{:02}", self.hour(), self.min())
    }

    pub fn parse(text: &str) -> Option<Clock> {
        let parts: Vec<&str> = text.trim().split(':').collect();
        if !(2..=3).contains(&parts.len()) || parts.iter().any(|p| p.is_empty() || p.len() > 2 || !p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        Clock::new(parts[0].parse().ok()?, parts[1].parse().ok()?)
    }
}

/// A calendar date. Not an offset from today: a saved date does not age.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Date {
    pub year: i32,
    pub month: u32,
    pub day: u32,
}

impl Date {
    pub fn new(year: i32, month: u32, day: u32) -> Option<Date> {
        let leap = (year % 4 == 0 && year % 100 != 0) || year % 400 == 0;
        let days = match month {
            1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
            4 | 6 | 9 | 11 => 30,
            2 if leap => 29,
            2 => 28,
            _ => return None,
        };
        ((1970..=2200).contains(&year) && (1..=days).contains(&day)).then_some(Date { year, month, day })
    }

    /// "YYYY-MM-DD"
    pub fn text(self) -> String {
        format!("{:04}-{:02}-{:02}", self.year, self.month, self.day)
    }

    pub fn parse(text: &str) -> Option<Date> {
        let parts: Vec<&str> = text.trim().split('-').collect();
        if parts.len() != 3 || parts[0].len() != 4 || parts[1].len() != 2 || parts[2].len() != 2 {
            return None;
        }
        if parts.iter().any(|p| !p.bytes().all(|b| b.is_ascii_digit())) {
            return None;
        }
        Date::new(parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?)
    }
}

/// How a channel's number is written and checked.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Kind {
    Float,
    Whole,
    Switch,
}

/// One adjustable live input that is not the clock: a part of the weather or of the machine.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Channel {
    Cloud,
    Rain,
    Snow,
    Fog,
    Storm,
    RainIn,
    Wind,
    WindFrom,
    Temperature,
    Humidity,
    Agents,
    Containers,
    Load,
    Music,
}

impl Channel {
    pub const ALL: [Channel; 14] = [
        Channel::Cloud,
        Channel::Rain,
        Channel::Snow,
        Channel::Fog,
        Channel::Storm,
        Channel::RainIn,
        Channel::Wind,
        Channel::WindFrom,
        Channel::Temperature,
        Channel::Humidity,
        Channel::Agents,
        Channel::Containers,
        Channel::Load,
        Channel::Music,
    ];

    /// The name the dev panel and `dev.json` use for it.
    pub fn key(self) -> &'static str {
        match self {
            Channel::Cloud => "cover",
            Channel::Rain => "rain",
            Channel::Snow => "snow",
            Channel::Fog => "fog",
            Channel::Storm => "storm",
            Channel::RainIn => "rain_in",
            Channel::Wind => "wind",
            Channel::WindFrom => "wdir",
            Channel::Temperature => "temp",
            Channel::Humidity => "humidity",
            Channel::Agents => "agents",
            Channel::Containers => "containers",
            Channel::Load => "load",
            Channel::Music => "music",
        }
    }

    /// The key in `config.toml`.
    pub fn toml_key(self) -> &'static str {
        match self {
            Channel::Cloud => "cloud",
            Channel::WindFrom => "wind_from",
            Channel::Temperature => "temperature",
            other => other.key(),
        }
    }

    /// The table in `config.toml` it lives in. Machine values are named as simulation.
    pub fn section(self) -> &'static [&'static str] {
        if self.is_machine() {
            &["scene", "simulated_machine"]
        } else {
            &["scene"]
        }
    }

    pub fn is_machine(self) -> bool {
        matches!(self, Channel::Agents | Channel::Containers | Channel::Load | Channel::Music)
    }

    pub fn range(self) -> (f64, f64) {
        match self {
            Channel::Cloud | Channel::Rain | Channel::Snow | Channel::Fog | Channel::Humidity | Channel::Load => (0.0, 100.0),
            Channel::Storm | Channel::Music => (0.0, 1.0),
            Channel::RainIn => (0.0, 4.0),
            Channel::Wind => (0.0, 200.0),
            Channel::WindFrom => (0.0, 360.0),
            Channel::Temperature => (-60.0, 60.0),
            Channel::Agents => (0.0, 16.0),
            Channel::Containers => (0.0, 32.0),
        }
    }

    pub fn kind(self) -> Kind {
        match self {
            Channel::Storm | Channel::Music => Kind::Switch,
            Channel::Agents | Channel::Containers => Kind::Whole,
            _ => Kind::Float,
        }
    }

    fn index(self) -> usize {
        Channel::ALL.iter().position(|c| *c == self).unwrap_or(0)
    }
}

/// The saved profile of live inputs. Everything is `Auto` unless the user fixed it.
#[derive(Clone, Debug, PartialEq)]
pub struct SceneSources {
    pub time: Source<Clock>,
    pub date: Source<Date>,
    values: [Source<f64>; 14],
}

impl Default for SceneSources {
    fn default() -> Self {
        SceneSources { time: Source::Auto, date: Source::Auto, values: [Source::Auto; 14] }
    }
}

impl SceneSources {
    pub fn get(&self, channel: Channel) -> Source<f64> {
        self.values[channel.index()]
    }

    /// Fix a channel at `value`, clamped to its range. Not finite is refused.
    pub fn fix(&mut self, channel: Channel, value: f64) -> bool {
        if !value.is_finite() {
            return false;
        }
        let (lo, hi) = channel.range();
        let mut v = value.clamp(lo, hi);
        match channel.kind() {
            Kind::Whole => v = v.round(),
            Kind::Switch => v = if v > 0.5 { 1.0 } else { 0.0 },
            Kind::Float => v = (v * 10000.0).round() / 10000.0,
        }
        self.values[channel.index()] = Source::Fixed(v);
        true
    }

    pub fn release(&mut self, channel: Channel) {
        self.values[channel.index()] = Source::Auto;
    }
}

/// How often the live layer's visitors turn up, as multiples of the usual.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ActivityRates {
    pub birds: f32,
    pub meteors: f32,
    pub planes: f32,
}

impl Default for ActivityRates {
    fn default() -> Self {
        ActivityRates { birds: 1.0, meteors: 1.0, planes: 1.0 }
    }
}

/// Settings that can change while dormer runs. Replaced whole, never edited in place.
#[derive(Clone, Debug, PartialEq)]
pub struct RuntimeSettings {
    /// `[look] view`; the persisted `dormer view` choice outranks it until Save reconciles them
    pub view: String,
    /// `auto`, `dark` or `light`
    pub mode: String,
    pub fps: u32,
    pub interactive: bool,
    pub rates: ActivityRates,
    /// `[activity] stories`: do incidents and daily life happen
    pub stories: bool,
    /// `[views.<id>]`: the knob values chosen for each view
    pub views: BTreeMap<String, BTreeMap<String, f64>>,
    pub scene: SceneSources,
}

impl Default for RuntimeSettings {
    fn default() -> Self {
        RuntimeSettings {
            view: "rooftops".into(),
            mode: "auto".into(),
            fps: 30,
            interactive: true,
            rates: ActivityRates::default(),
            stories: true,
            views: BTreeMap::new(),
            scene: SceneSources::default(),
        }
    }
}

impl RuntimeSettings {
    /// The knob values saved for a view.
    pub fn knobs(&self, view: &str) -> impl Iterator<Item = (&str, f64)> {
        self.views.get(view).into_iter().flat_map(|m| m.iter().map(|(k, v)| (k.as_str(), *v)))
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum SettingsError {
    Io(String),
    Parse(String),
    Invalid { key: String, why: String },
}

impl fmt::Display for SettingsError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SettingsError::Io(e) => write!(f, "cannot read the settings: {}", e),
            SettingsError::Parse(e) => write!(f, "the settings file is not valid TOML: {}", e),
            SettingsError::Invalid { key, why } => write!(f, "{}: {}", key, why),
        }
    }
}

impl std::error::Error for SettingsError {}

fn invalid(key: &str, why: impl Into<String>) -> SettingsError {
    SettingsError::Invalid { key: key.to_string(), why: why.into() }
}

fn table<'a>(root: &'a toml::Table, path: &[&str]) -> Result<Option<&'a toml::Table>, SettingsError> {
    let mut at = root;
    for (n, name) in path.iter().enumerate() {
        match at.get(*name) {
            None => return Ok(None),
            Some(toml::Value::Table(t)) => at = t,
            Some(_) => return Err(invalid(&path[..=n].join("."), "must be a table")),
        }
    }
    Ok(Some(at))
}

fn number(v: &toml::Value, key: &str) -> Result<f64, SettingsError> {
    let n = match v {
        toml::Value::Float(f) => *f,
        toml::Value::Integer(i) => *i as f64,
        _ => return Err(invalid(key, "must be a number")),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(invalid(key, "must be a finite number"))
    }
}

fn ranged(v: &toml::Value, key: &str, lo: f64, hi: f64) -> Result<f64, SettingsError> {
    let n = number(v, key)?;
    if (lo..=hi).contains(&n) {
        Ok(n)
    } else {
        Err(invalid(key, format!("must be between {} and {}", lo, hi)))
    }
}

fn text<'a>(v: &'a toml::Value, key: &str) -> Result<std::borrow::Cow<'a, str>, SettingsError> {
    match v {
        toml::Value::String(s) => Ok(s.as_str().into()),
        // an unquoted 21:30:00 or 2026-10-15 is a TOML datetime: take it as written
        toml::Value::Datetime(d) => Ok(d.to_string().into()),
        _ => Err(invalid(key, "must be text")),
    }
}

impl RuntimeSettings {
    /// Read settings out of TOML text. Anything unreadable is an error, never a silent default.
    pub fn parse(source: &str) -> Result<RuntimeSettings, SettingsError> {
        let root: toml::Table = source.parse().map_err(|e: toml::de::Error| SettingsError::Parse(e.to_string().replace('\n', " ")))?;
        Self::from_table(&root)
    }

    pub fn from_table(root: &toml::Table) -> Result<RuntimeSettings, SettingsError> {
        let mut out = RuntimeSettings::default();
        if let Some(look) = table(root, &["look"])? {
            if let Some(v) = look.get("view") {
                out.view = text(v, "look.view")?.trim().to_string();
            }
            if let Some(v) = look.get("mode") {
                let mode = text(v, "look.mode")?.to_string();
                if !["auto", "dark", "light"].contains(&mode.as_str()) {
                    return Err(invalid("look.mode", "must be auto, dark or light"));
                }
                out.mode = mode;
            }
        }
        if let Some(live) = table(root, &["live"])? {
            if let Some(v) = live.get("fps") {
                out.fps = ranged(v, "live.fps", 5.0, 60.0)?.round() as u32;
            }
            if let Some(v) = live.get("interactive") {
                out.interactive = v.as_bool().ok_or_else(|| invalid("live.interactive", "must be true or false"))?;
            }
        }
        if let Some(activity) = table(root, &["activity"])? {
            for (key, slot) in [("birds", &mut out.rates.birds), ("meteors", &mut out.rates.meteors), ("planes", &mut out.rates.planes)] {
                if let Some(v) = activity.get(key) {
                    *slot = ranged(v, &format!("activity.{}", key), 0.0, 20.0)? as f32;
                }
            }
            if let Some(v) = activity.get("stories") {
                out.stories = v.as_bool().ok_or_else(|| invalid("activity.stories", "must be true or false"))?;
            }
        }
        if let Some(views) = table(root, &["views"])? {
            for (id, knobs) in views {
                let toml::Value::Table(knobs) = knobs else { return Err(invalid(&format!("views.{}", id), "must be a table")) };
                let mut saved = BTreeMap::new();
                for (key, v) in knobs {
                    saved.insert(key.clone(), number(v, &format!("views.{}.{}", id, key))?);
                }
                out.views.insert(id.clone(), saved);
            }
        }
        if let Some(scene) = table(root, &["scene"])? {
            if let Some(v) = scene.get("time") {
                let t = text(v, "scene.time")?;
                out.scene.time = Source::Fixed(Clock::parse(&t).ok_or_else(|| invalid("scene.time", "must look like 21:30"))?);
            }
            if let Some(v) = scene.get("date") {
                let t = text(v, "scene.date")?;
                out.scene.date = Source::Fixed(Date::parse(&t).ok_or_else(|| invalid("scene.date", "must be a real date like 2026-10-15"))?);
            }
        }
        for channel in Channel::ALL {
            let Some(section) = table(root, channel.section())? else { continue };
            let Some(v) = section.get(channel.toml_key()) else { continue };
            let key = format!("{}.{}", channel.section().join("."), channel.toml_key());
            let (lo, hi) = channel.range();
            let n = match (channel.kind(), v) {
                (Kind::Switch, toml::Value::Boolean(b)) => *b as u8 as f64,
                (Kind::Switch, _) => return Err(invalid(&key, "must be true or false")),
                (_, v) => ranged(v, &key, lo, hi)?,
            };
            out.scene.fix(channel, n);
        }
        Ok(out)
    }
}

/// The settings as loaded from one file.
#[derive(Clone, Debug, PartialEq)]
pub struct Settings {
    pub runtime: RuntimeSettings,
    /// was there a file to read
    pub found: bool,
}

impl Settings {
    /// A missing file is the defaults; an unreadable or invalid one is an error.
    pub fn load(path: &Path) -> Result<Settings, SettingsError> {
        match std::fs::read_to_string(path) {
            Ok(text) => Ok(Settings { runtime: RuntimeSettings::parse(&text)?, found: true }),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Settings { runtime: RuntimeSettings::default(), found: false }),
            Err(e) => Err(SettingsError::Io(format!("{}: {}", path.display(), e))),
        }
    }
}

/// The snapshot in force. Replaced as a whole, only by a snapshot that validated.
pub struct Holder {
    current: RwLock<Option<Arc<RuntimeSettings>>>,
}

impl Holder {
    pub const fn new() -> Holder {
        Holder { current: RwLock::new(None) }
    }

    pub fn get(&self) -> Option<Arc<RuntimeSettings>> {
        self.current.read().ok().and_then(|g| g.clone())
    }

    pub fn set(&self, settings: RuntimeSettings) -> Arc<RuntimeSettings> {
        let arc = Arc::new(settings);
        if let Ok(mut g) = self.current.write() {
            *g = Some(arc.clone());
        }
        arc
    }

    /// Adopt what `path` now says. On any error the previous snapshot stays in force.
    /// Ok(true) when the snapshot changed.
    pub fn reload(&self, path: &Path) -> Result<bool, SettingsError> {
        let next = Settings::load(path)?.runtime;
        if self.get().as_deref() == Some(&next) {
            return Ok(false);
        }
        self.set(next);
        Ok(true)
    }
}

static GLOBAL: Holder = Holder::new();

/// The settings in force for this process: read on first use, replaced by `reload`.
pub fn runtime() -> Arc<RuntimeSettings> {
    if let Some(s) = GLOBAL.get() {
        return s;
    }
    let path = crate::sources::config_path();
    match Settings::load(&path) {
        Ok(s) => GLOBAL.set(s.runtime),
        Err(e) => {
            // never silently: the defaults stand in, and the file is left alone
            eprintln!("dormer: {} ({}); using built-in defaults until it is fixed", e, path.display());
            GLOBAL.set(RuntimeSettings::default())
        }
    }
}

/// Re-read the config file; a snapshot that does not validate is refused and the old one stays.
pub fn reload() -> Result<bool, SettingsError> {
    let _ = runtime();
    GLOBAL.reload(&crate::sources::config_path())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn defaults_are_all_auto() {
        let s = RuntimeSettings::parse("").unwrap();
        assert_eq!(s, RuntimeSettings::default());
        assert!(Channel::ALL.iter().all(|c| s.scene.get(*c) == Source::Auto));
        assert!(s.interactive && s.stories && s.fps == 30);
    }

    #[test]
    fn auto_and_fixed_have_explicit_precedence() {
        let text = "[scene]\ntime = \"21:30\"\ndate = \"2026-10-15\"\nrain = 40\nstorm = true\n\
                    [scene.simulated_machine]\nagents = 3\nload = 80.5\n";
        let s = RuntimeSettings::parse(text).unwrap();
        assert_eq!(s.scene.time, Source::Fixed(Clock::new(21, 30).unwrap()));
        assert_eq!(s.scene.date, Source::Fixed(Date::new(2026, 10, 15).unwrap()));
        assert_eq!(s.scene.get(Channel::Rain), Source::Fixed(40.0));
        assert_eq!(s.scene.get(Channel::Storm), Source::Fixed(1.0));
        assert_eq!(s.scene.get(Channel::Agents), Source::Fixed(3.0));
        assert_eq!(s.scene.get(Channel::Load), Source::Fixed(80.5));
        // what was not written stays automatic, so real readings keep flowing
        assert_eq!(s.scene.get(Channel::Snow), Source::Auto);
        assert_eq!(s.scene.get(Channel::Containers), Source::Auto);
        assert_eq!(Channel::ALL.iter().filter(|c| s.scene.get(**c).is_fixed()).count(), 4);
    }

    #[test]
    fn invalid_values_are_rejected() {
        for bad in [
            "[scene]\ntime = \"25:00\"",
            "[scene]\ntime = \"9\"",
            "[scene]\ndate = \"2026-02-30\"",
            "[scene]\ndate = \"tomorrow\"",
            "[scene]\nrain = 140",
            "[scene]\nrain = \"heavy\"",
            "[scene]\nstorm = 1",
            "[scene]\nwind = nan",
            "[scene.simulated_machine]\nload = inf",
            "[look]\nmode = \"purple\"",
            "[live]\nfps = 500",
            "[live]\ninteractive = \"invalid\"",
            "[activity]\nbirds = -1",
            "[views.street]\nrain_barrel = \"full\"",
            "scene = 4",
        ] {
            assert!(RuntimeSettings::parse(bad).is_err(), "{:?} should be refused", bad);
        }
        assert!(matches!(RuntimeSettings::parse("[scene"), Err(SettingsError::Parse(_))));
    }

    #[test]
    fn invalid_values_keep_last_snapshot() {
        let dir = std::env::temp_dir().join(format!("dormer-settings-{}-keep", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("config.toml");
        std::fs::write(&path, "[live]\nfps = 24\n").unwrap();
        let holder = Holder::new();
        assert_eq!(holder.reload(&path), Ok(true));
        assert_eq!(holder.get().unwrap().fps, 24);
        assert_eq!(holder.reload(&path), Ok(false));
        std::fs::write(&path, "[live]\nfps = \"fast\"\n").unwrap();
        assert!(holder.reload(&path).is_err());
        assert_eq!(holder.get().unwrap().fps, 24);
        std::fs::write(&path, "[live\n").unwrap();
        assert!(matches!(holder.reload(&path), Err(SettingsError::Parse(_))));
        assert_eq!(holder.get().unwrap().fps, 24);
        std::fs::write(&path, "[live]\nfps = 12\n").unwrap();
        assert_eq!(holder.reload(&path), Ok(true));
        assert_eq!(holder.get().unwrap().fps, 12);
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_missing_file_is_the_defaults_but_an_unreadable_one_is_an_error() {
        let dir = std::env::temp_dir().join(format!("dormer-settings-{}-missing", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        let none = Settings::load(&dir.join("config.toml")).unwrap();
        assert!(!none.found && none.runtime == RuntimeSettings::default());
        std::fs::create_dir_all(dir.join("config.toml")).unwrap(); // a directory where a file should be
        assert!(matches!(Settings::load(&dir.join("config.toml")), Err(SettingsError::Io(_))));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn unquoted_toml_dates_and_times_are_read() {
        let s = RuntimeSettings::parse("[scene]\ndate = 2026-10-15\ntime = 07:05:00\n").unwrap();
        assert_eq!(s.scene.date, Source::Fixed(Date::new(2026, 10, 15).unwrap()));
        assert_eq!(s.scene.time, Source::Fixed(Clock::new(7, 5).unwrap()));
    }

    #[test]
    fn per_view_knobs_and_activity_are_read() {
        let s = RuntimeSettings::parse("[activity]\nbirds = 2.5\nstories = false\n[views.coast]\ntide = 0.5\n[views.moon]\nzoom = 2\n").unwrap();
        assert_eq!(s.rates.birds, 2.5);
        assert!(!s.stories);
        assert_eq!(s.knobs("coast").collect::<Vec<_>>(), vec![("tide", 0.5)]);
        assert_eq!(s.knobs("moon").collect::<Vec<_>>(), vec![("zoom", 2.0)]);
        assert_eq!(s.knobs("city").count(), 0);
    }
}
