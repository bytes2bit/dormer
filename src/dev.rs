//! Development only: a panel of sliders that bends the scene by hand.
//!
//! `dormer dev` opens a small floating terminal on an empty workspace, so the
//! wallpaper is in view, and redraws it as the sliders move: which view to
//! look out on, time of day, date (and with it the weekday, the season, the
//! moon and the stars), every part of the weather, what the house shows, how
//! often birds, planes and shooting stars turn up, and whatever knobs the
//! view itself has.
//!
//! Nothing here is permanent. The panel writes its overrides to dev.json in
//! the cache, and they count only while the panel is alive. Quit, and the
//! real sky comes back.

use std::io::{Read, Write};
use std::process::{Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde_json::{json, Map, Value};

use crate::paint::Cache;
use crate::sources::{self, cache_file, config};
use crate::tm::{self, DAYS, MONTHS};
use crate::views::{self, View};
use crate::{build, launcher, real_moment, with_overrides, Moment};

const WINDS: [&str; 8] = ["N", "NE", "E", "SE", "S", "SW", "W", "NW"];

/// One line of the panel: a heading (no key), the choice of view, or a slider.
#[derive(Clone, Copy)]
struct Row {
    key: &'static str,
    label: &'static str,
    lo: f64,
    hi: f64,
    step: f64,
    big: f64,
    /// how a knob of the view writes its value; the panel's own sliders are written by `show`
    show: Option<fn(f64) -> String>,
}

const fn row(key: &'static str, label: &'static str, lo: f64, hi: f64, step: f64, big: f64) -> Row {
    Row { key, label, lo, hi, step, big, show: None }
}

const ROWS: &[Row] = &[
    row("", "when", 0.0, 0.0, 0.0, 0.0),
    row("time", "time of day", 0.0, 24.0, 1.0 / 12.0, 1.0),
    row("day", "date", -183.0, 183.0, 1.0, 7.0),
    row("", "weather", 0.0, 0.0, 0.0, 0.0),
    row("cover", "cloud", 0.0, 100.0, 5.0, 20.0),
    row("rain", "rain", 0.0, 100.0, 5.0, 20.0),
    row("snow", "snow", 0.0, 100.0, 5.0, 20.0),
    row("fog", "fog", 0.0, 100.0, 5.0, 20.0),
    row("storm", "thunderstorm", 0.0, 1.0, 1.0, 1.0),
    row("rain_in", "rain due in", 0.0, 4.0, 0.5, 1.0),
    row("wind", "wind", 0.0, 70.0, 2.0, 10.0),
    row("wdir", "wind from", 0.0, 345.0, 15.0, 45.0),
    row("temp", "temperature", -12.0, 42.0, 1.0, 5.0),
    row("humidity", "humidity", 20.0, 100.0, 5.0, 20.0),
    row("", "yours", 0.0, 0.0, 0.0, 0.0),
    row("agents", "lamp: agents", 0.0, 4.0, 1.0, 1.0),
    row("containers", "blue lights", 0.0, 8.0, 1.0, 1.0),
    row("load", "smoke: cpu", 0.0, 100.0, 5.0, 25.0),
    row("music", "music", 0.0, 1.0, 1.0, 1.0),
    row("", "how often", 0.0, 0.0, 0.0, 0.0),
    row("birds", "birds", 0.0, 5.0, 0.25, 1.0),
    row("meteors", "shooting stars", 0.0, 12.0, 0.5, 2.0),
    row("planes", "planes", 0.0, 6.0, 0.25, 1.0),
];

/// Every line of the panel for this view: the choice of view first, the view's own knobs last.
fn rows_for(view: &'static dyn View) -> Vec<Row> {
    let mut out = vec![row("", "looking out on", 0.0, 0.0, 0.0, 0.0), row("view", "", 0.0, (views::ALL.len() - 1) as f64, 1.0, 1.0)];
    out.extend_from_slice(ROWS);
    if !view.knobs().is_empty() {
        out.push(row("", "this view", 0.0, 0.0, 0.0, 0.0));
        out.extend(view.knobs().iter().map(|k| Row { key: k.key, label: k.label, lo: k.lo, hi: k.hi, step: k.step, big: k.big, show: Some(k.show) }));
    }
    out
}

fn show(key: &str, v: f64) -> String {
    match key {
        "time" => {
            let v = v.min(24.0 - 1.0 / 60.0);
            format!("{:02}:{:02}", v as i64, ((v.fract() * 60.0).round() as i64) % 60)
        }
        "day" => {
            let l = tm::local(tm::now() + v * 86400.0);
            let rel = if v == 0.0 { "  today".to_string() } else { format!("  {:+}d", v as i64) };
            format!("{} {:02} {}{}", &DAYS[l.dow][..3], l.mday, MONTHS[(l.month - 1) as usize], rel)
        }
        "cover" | "rain" | "snow" | "fog" | "load" | "humidity" => format!("{}%", v as i64),
        "storm" | "music" => if v > 0.5 { "on" } else { "off" }.to_string(),
        "rain_in" => {
            if v >= 3.9 {
                "not soon".to_string()
            } else if v < 0.25 {
                "any minute".to_string()
            } else {
                format!("{} h", v)
            }
        }
        "wind" => format!("{} km/h", v as i64),
        "wdir" => format!("{}  {}°", WINDS[(((v + 22.5).rem_euclid(360.0)) / 45.0) as usize % 8], v as i64),
        "temp" => format!("{}°", v as i64),
        "agents" | "containers" => format!("{}", v as i64),
        _ => {
            if v == 0.0 {
                "off".to_string()
            } else {
                format!("×{}", v)
            }
        }
    }
}

/// Without the living wallpaper running, each change is drawn to the still wallpaper in the background.
struct Painter {
    changed: Arc<AtomicBool>,
    status: Arc<Mutex<String>>,
}

impl Painter {
    fn start() -> Painter {
        let (changed, status) = (Arc::new(AtomicBool::new(false)), Arc::new(Mutex::new(String::new())));
        let (c, s) = (changed.clone(), status.clone());
        std::thread::spawn(move || {
            let exe = launcher();
            loop {
                if !c.swap(false, Ordering::SeqCst) {
                    std::thread::sleep(std::time::Duration::from_millis(40));
                    continue;
                }
                if sources::live_pid() != 0 {
                    *s.lock().unwrap() = "live".to_string();
                    continue;
                }
                *s.lock().unwrap() = "drawing".to_string();
                let began = std::time::Instant::now();
                let _ = Command::new(&exe).args(["draw", "--force"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                if !c.load(Ordering::SeqCst) {
                    *s.lock().unwrap() = format!("drawn in {:.2}s", began.elapsed().as_secs_f64());
                }
            }
        });
        Painter { changed, status }
    }
}

struct Panel {
    set: Map<String, Value>,
    /// the key of the row the cursor is on
    at: &'static str,
    real: Map<String, Value>,
    real_at: f64,
    base: Option<Moment>,
    info: Vec<String>,
    painter: Painter,
    rows: Vec<Row>,
    /// column where the bars start, and their width
    bar: (usize, usize),
    /// screen row -> row of the panel
    row_of: Vec<(usize, usize)>,
    /// where each view's name is written: first column, one past the last, which view
    tabs: Vec<(usize, usize, usize)>,
    cache: Cache,
}

impl Panel {
    fn new() -> Panel {
        let view = sources::view();
        let mut p = Panel {
            set: Map::new(),
            at: "view",
            real: Map::new(),
            real_at: 0.0,
            base: None,
            info: Vec::new(),
            painter: Painter::start(),
            rows: rows_for(view),
            bar: (0, 0),
            row_of: Vec::new(),
            tabs: Vec::new(),
            cache: Cache::new(view, config().seed, 320, 200),
        };
        p.refresh_real();
        p
    }

    /// The view the panel is showing: the one it chose, or the one in use.
    fn view(&self) -> &'static dyn View {
        self.set.get("view").and_then(|v| v.as_str()).and_then(views::find).unwrap_or_else(sources::view)
    }

    /// What the sliders rest on when left alone: the real time, weather and machine.
    fn refresh_real(&mut self) {
        let mut m = real_moment(None, false);
        (m.wx, _) = sources::read_weather(false);
        let l = tm::local(m.ts);
        let round = |v: f64, step: f64| (v / step).round() * step;
        self.real = json!({
            "time": l.hour as f64 + l.min as f64 / 60.0, "day": 0.0,
            "cover": round(m.wx.cover * 100.0, 5.0), "rain": round(m.wx.rain * 100.0, 5.0),
            "snow": round(m.wx.snow * 100.0, 5.0), "fog": round(m.wx.fog * 100.0, 5.0),
            "storm": if m.wx.storm { 1.0 } else { 0.0 }, "wind": m.wx.wind.round(),
            "rain_in": round(m.wx.rain_in.unwrap_or(4.0).min(4.0), 0.5),
            "wdir": round(m.wx.wdir, 15.0).rem_euclid(360.0), "temp": m.wx.temp.round(),
            "humidity": round(m.wx.humidity, 5.0),
            "agents": (m.machine.agents.min(4)) as f64, "containers": (m.machine.containers.min(8)) as f64,
            "load": round(m.machine.load as f64 * 100.0, 5.0), "music": if m.machine.audio { 1.0 } else { 0.0 },
            "birds": 1.0, "meteors": 1.0, "planes": 1.0,
            "view": views::ALL.iter().position(|v| v.id() == m.view.id()).unwrap_or(0) as f64,
        })
        .as_object()
        .cloned()
        .unwrap_or_default();
        self.real_at = tm::now();
        self.base = Some(m);
        self.describe();
    }

    fn value(&self, key: &str) -> f64 {
        if key == "view" {
            let id = self.view().id();
            return views::ALL.iter().position(|v| v.id() == id).unwrap_or(0) as f64;
        }
        self.set.get(key).or(self.real.get(key)).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }

    /// The first lines of `dormer explain` for the scene as the sliders have it, and where the
    /// view's own knobs would rest if left alone.
    fn describe(&mut self) {
        let Some(base) = &self.base else { return };
        let mut m = base.at(tm::now());
        with_overrides(&self.set, &mut m);
        if m.view.id() != self.cache.view.id() {
            self.cache = Cache::new(m.view, config().seed, 320, 200);
            self.rows = rows_for(m.view);
            if !self.rows.iter().any(|r| r.key == self.at) {
                self.at = "view";
            }
        }
        let held = std::mem::take(&mut m.knobs);
        let free = build(&m, None);
        for k in m.view.knobs() {
            self.real.insert(k.key.to_string(), json!(((m.view.real(k.key, &free) / k.step).round() * k.step).clamp(k.lo, k.hi)));
        }
        m.knobs = held;
        let mut sc = build(&m, None);
        sc.lit_share = self.cache.stage.lights(&sc).share;
        self.info = crate::words::explain(&sc, m.view, &self.cache.stars).lines().take_while(|l| !l.starts_with("week")).map(|l| l.to_string()).collect();
    }

    fn change(&mut self, key: &str, v: f64) {
        let Some(row) = self.rows.iter().find(|r| r.key == key).copied() else { return };
        let (lo, hi, step) = (row.lo, row.hi, row.step);
        let v = ((v / step).round() * step).clamp(lo, hi);
        if key == "view" {
            let id = views::ALL[v as usize].id();
            // a knob of the view being left means nothing in the next one
            for k in self.view().knobs() {
                self.set.remove(k.key);
            }
            if id == sources::view().id() {
                self.set.remove("view");
            } else {
                self.set.insert("view".to_string(), json!(id));
            }
            return self.commit();
        }
        if (v - self.value_real(key)).abs() < step / 2.0 && key != "time" {
            self.set.remove(key);
        } else {
            self.set.insert(key.to_string(), json!((v * 10000.0).round() / 10000.0));
        }
        self.commit();
    }

    fn value_real(&self, key: &str) -> f64 {
        self.real.get(key).and_then(|v| v.as_f64()).unwrap_or(0.0)
    }

    fn commit(&mut self) {
        sources::write_atomic(&cache_file("dev.json"), &json!({"pid": std::process::id(), "set": self.set}).to_string());
        self.describe();
        self.painter.changed.store(true, Ordering::SeqCst);
    }

    fn paint(&mut self, out: &mut impl Write) {
        let (rows_n, cols) = term_size();
        let mut s = String::from("\x1b[H\x1b[2J");
        let (accent, dim, bold, off) = ("\x1b[33m", "\x1b[90m", "\x1b[1m", "\x1b[0m");
        let mut put = |y: usize, x: usize, text: &str, style: &str| {
            if y + 1 < rows_n.max(1) && x + 1 < cols {
                let room = cols - 1 - x;
                let cut: String = text.chars().take(room).collect();
                s.push_str(&format!("\x1b[{};{}H{}{}{}", y + 1, x + 1, style, cut, off));
            }
        };
        put(0, 2, "dormer", &format!("{}{}", accent, bold));
        put(0, 9, "dev panel", dim);
        let state = if sources::live_pid() != 0 { "live".to_string() } else { self.painter.status.lock().unwrap().clone() };
        put(0, cols.saturating_sub(2 + state.chars().count()).max(20), &state, if state == "drawing" { accent } else { dim });
        for (i, line) in self.info.iter().take(5).enumerate() {
            put(1 + i, 2, line, if i == 0 { bold } else { dim });
        }
        let (label_w, value_w) = (17usize, 18usize);
        let (bar_x, bar_w) = (2 + label_w, cols.saturating_sub(2 + label_w + value_w + 3).max(8));
        self.bar = (bar_x, bar_w);
        self.row_of.clear();
        self.tabs.clear();
        let lines = self.rows.len() + 1;
        let groups = self.rows.iter().filter(|r| r.key.is_empty()).count();
        let roomy = rows_n >= 7 + lines + groups + 3; // space for a blank line between groups?
        let view = self.view();
        let mut y = 7;
        for (i, r) in self.rows.clone().into_iter().enumerate() {
            let chosen = r.key == self.at;
            if r.key.is_empty() {
                if i > 0 && roomy {
                    y += 1;
                }
                put(y, 2, r.label, dim);
                put(y, 3 + r.label.len(), &"─".repeat(cols.saturating_sub(6 + r.label.len())), dim);
                y += 1;
                continue;
            }
            self.row_of.push((y, i));
            put(y, 0, if chosen { "▸" } else { " " }, &format!("{}{}", accent, bold));
            if r.key == "view" {
                // the views side by side, the one in the picture marked
                let bent = self.set.contains_key("view");
                let mut x = 2;
                for (k, v) in views::ALL.iter().enumerate() {
                    let on = v.id() == view.id();
                    let text = format!("{} {}", if on { "●" } else { "○" }, v.id());
                    let style = if on { format!("{}{}", if bent || chosen { accent } else { "" }, bold) } else { dim.to_string() };
                    put(y, x, &text, &style);
                    self.tabs.push((x, x + text.chars().count(), k));
                    x += text.chars().count() + 4;
                }
                put(y + 1, 4, view.about(), dim);
                y += 2;
                continue;
            }
            let (v, bent) = (self.value(r.key), self.set.contains_key(r.key));
            put(y, 2, r.label, if chosen { bold } else if bent { "" } else { dim });
            let place = |v: f64| (((v - r.lo) / (r.hi - r.lo)) * (bar_w - 1) as f64).round().clamp(0.0, (bar_w - 1) as f64) as usize;
            let knob = place(v);
            put(y, bar_x, &"━".repeat(knob), if bent { accent } else { dim });
            put(y, bar_x + knob + 1, &"─".repeat(bar_w - knob - 1), dim);
            if bent {
                // a tick where reality is
                let real = place(self.value_real(r.key));
                if real != knob {
                    put(y, bar_x + real, "┆", dim);
                }
            }
            put(y, bar_x + knob, "●", &format!("{}{}", if bent { accent } else { "" }, bold));
            let text = match r.show {
                Some(f) => f(v),
                None => show(r.key, v),
            };
            put(y, bar_x + bar_w + 2, &text, &format!("{}{}", if bent { accent } else { dim }, if chosen { bold } else { "" }));
            y += 1;
        }
        put(rows_n.saturating_sub(3), 2, "↑↓ choose   ←→ adjust   PgUp PgDn big steps   click or drag", dim);
        put(rows_n.saturating_sub(2), 2, "r this one real   a all real   n now   q quit and restore", dim);
        let _ = out.write_all(s.as_bytes());
        let _ = out.flush();
    }

    /// False when it is time to quit.
    fn key(&mut self, k: Key) -> bool {
        let list: Vec<Row> = self.rows.iter().filter(|r| !r.key.is_empty()).copied().collect();
        let pos = list.iter().position(|r| r.key == self.at).unwrap_or(0);
        let row = list[pos];
        let (key, lo, hi, step, big) = (row.key, row.lo, row.hi, row.step, row.big);
        let v = self.value(key);
        match k {
            Key::Up => self.at = list[(pos + list.len() - 1) % list.len()].key,
            Key::Down => self.at = list[(pos + 1) % list.len()].key,
            // the views go round and round
            Key::Left | Key::BigLeft if key == "view" => self.change(key, (v - 1.0).rem_euclid(hi + 1.0)),
            Key::Right | Key::BigRight if key == "view" => self.change(key, (v + 1.0).rem_euclid(hi + 1.0)),
            Key::Left => self.change(key, v - step),
            Key::Right => self.change(key, v + step),
            Key::BigLeft => self.change(key, v - big),
            Key::BigRight => self.change(key, v + big),
            Key::Home => self.change(key, lo),
            Key::End => self.change(key, hi),
            Key::Real => {
                self.set.remove(key);
                self.commit();
            }
            Key::Now => {
                self.set.remove("time");
                self.set.remove("day");
                self.commit();
            }
            Key::AllReal => {
                self.set.clear();
                self.commit();
            }
            Key::Mouse(x, y) => {
                if let Some(&(_, i)) = self.row_of.iter().find(|r| r.0 == y) {
                    let r = self.rows[i];
                    self.at = r.key;
                    let (bar_x, bar_w) = self.bar;
                    if r.key == "view" {
                        if let Some(&(_, _, k)) = self.tabs.iter().find(|t| t.0 <= x + 1 && x < t.1 + 1) {
                            self.change("view", k as f64);
                        }
                    } else if x + 1 >= bar_x && x <= bar_x + bar_w {
                        let f = ((x as f64 - bar_x as f64) / (bar_w - 1) as f64).clamp(0.0, 1.0);
                        self.change(r.key, r.lo + f * (r.hi - r.lo));
                    }
                }
            }
            Key::Quit => return false,
            Key::None => {}
        }
        true
    }
}

#[derive(Clone, Copy, PartialEq, Debug)]
enum Key {
    Up,
    Down,
    Left,
    Right,
    BigLeft,
    BigRight,
    Home,
    End,
    Real,
    Now,
    AllReal,
    Mouse(usize, usize),
    Quit,
    None,
}

/// Turn what the terminal sent into keys. Escape sequences are read whole; a lone escape is ignored.
fn parse(bytes: &[u8]) -> Vec<Key> {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 < bytes.len() && (bytes[i + 1] == b'[' || bytes[i + 1] == b'O') {
            let mut j = i + 2;
            while j < bytes.len() && !(0x40..=0x7e).contains(&bytes[j]) {
                j += 1;
            }
            if j >= bytes.len() {
                break;
            }
            let body = std::str::from_utf8(&bytes[i + 2..j]).unwrap_or("");
            let end = bytes[j] as char;
            keys.push(match (body, end) {
                (b, 'M') | (b, 'm') if b.starts_with('<') => {
                    let p: Vec<usize> = b[1..].split(';').filter_map(|v| v.parse().ok()).collect();
                    // a press or a drag with the left button; releases and the wheel are ignored
                    if p.len() == 3 && end == 'M' && (p[0] & 0b11) == 0 && p[0] & 64 == 0 {
                        Key::Mouse(p[1].saturating_sub(1), p[2].saturating_sub(1))
                    } else {
                        Key::None
                    }
                }
                ("", 'A') => Key::Up,
                ("", 'B') => Key::Down,
                ("", 'C') => Key::Right,
                ("", 'D') => Key::Left,
                ("1;2C", _) | ("1;5C", 'C') => Key::BigRight,
                ("1;2D", _) | ("1;5D", 'D') => Key::BigLeft,
                (b, 'C') if b.ends_with(";2") || b.ends_with(";5") => Key::BigRight,
                (b, 'D') if b.ends_with(";2") || b.ends_with(";5") => Key::BigLeft,
                ("5", '~') => Key::BigRight,
                ("6", '~') => Key::BigLeft,
                ("", 'H') | ("1", '~') | ("7", '~') => Key::Home,
                ("", 'F') | ("4", '~') | ("8", '~') => Key::End,
                ("3", '~') => Key::Real,
                _ => Key::None,
            });
            i = j + 1;
            continue;
        }
        keys.push(match b {
            b'k' => Key::Up,
            b'j' | b'\t' => Key::Down,
            b'h' => Key::Left,
            b'l' => Key::Right,
            b'[' | b'H' => Key::BigLeft,
            b']' | b'L' => Key::BigRight,
            b'r' | 127 | 8 => Key::Real,
            b'n' => Key::Now,
            b'a' | b'0' => Key::AllReal,
            b'q' | 3 => Key::Quit,
            _ => Key::None,
        });
        i += 1;
    }
    keys
}

fn term_size() -> (usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
        (ws.ws_row as usize, ws.ws_col as usize)
    } else {
        (24, 80)
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
        ("ghostty", vec![format!("--class={}", name), format!("--title={}", title), "--window-width=66".into(), "--window-height=43".into(), "--font-size=10".into(), "--confirm-close-surface=false".into(), "-e".into()]),
        ("kitty", vec!["--class".into(), name.into(), "--title".into(), title.into(), "-o".into(), "initial_window_width=66c".into(), "-o".into(), "initial_window_height=43c".into()]),
        ("foot", vec!["--app-id".into(), name.into(), "--title".into(), title.into(), "--window-size-chars=66x43".into()]),
        ("alacritty", vec!["--class".into(), name.into(), "--title".into(), title.into(), "-o".into(), "window.dimensions.columns=66".into(), "-o".into(), "window.dimensions.lines=43".into(), "-e".into()]),
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

    // the terminal: no echo, a key at a time, the mouse reported while a button is held
    let mut saved: libc::termios = unsafe { std::mem::zeroed() };
    unsafe {
        libc::tcgetattr(0, &mut saved);
        let mut raw = saved;
        libc::cfmakeraw(&mut raw);
        libc::tcsetattr(0, libc::TCSANOW, &raw);
        libc::signal(libc::SIGINT, libc::SIG_IGN);
    }
    let mut out = std::io::stdout();
    let _ = out.write_all(b"\x1b[?1049h\x1b[?25l\x1b[?1002h\x1b[?1006h");
    let mut panel = Panel::new();
    panel.commit();
    let mut stdin = std::io::stdin();
    'outer: loop {
        if tm::now() - panel.real_at > 20.0 {
            panel.refresh_real(); // the real clock and weather move on underneath
        }
        panel.paint(&mut out);
        let mut fds = [libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 }];
        if unsafe { libc::poll(fds.as_mut_ptr(), 1, 250) } <= 0 {
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
        for k in parse(&raw[..n]) {
            if !panel.key(k) {
                break 'outer;
            }
        }
    }
    let _ = out.write_all(b"\x1b[?1006l\x1b[?1002l\x1b[?25h\x1b[?1049l");
    let _ = out.flush();
    unsafe { libc::tcsetattr(0, libc::TCSANOW, &saved) };
    let _ = std::fs::remove_file(cache_file("dev.json"));
    if sources::live_pid() == 0 {
        println!("putting the real sky back…");
        let _ = Command::new(&exe).args(["draw", "--force"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
    }
    if let Some(b) = back {
        hyprland(&format!("hl.dsp.focus({{ workspace = \"{}\" }})", b), &["workspace", &b]);
    }
    0
}
