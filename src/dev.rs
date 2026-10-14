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
use std::sync::mpsc::{sync_channel, SyncSender};
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
    changed: SyncSender<()>,
    status: Arc<Mutex<String>>,
}

impl Painter {
    fn start() -> Painter {
        let (changed, pending) = sync_channel(1);
        let status = Arc::new(Mutex::new(String::new()));
        let s = status.clone();
        std::thread::spawn(move || {
            let exe = launcher();
            while pending.recv().is_ok() {
                while pending.try_recv().is_ok() {}
                if sources::live_pid() != 0 {
                    *s.lock().unwrap() = "live".to_string();
                    continue;
                }
                *s.lock().unwrap() = "drawing".to_string();
                let began = std::time::Instant::now();
                let _ = Command::new(&exe).args(["draw", "--force"]).stdout(Stdio::null()).stderr(Stdio::null()).status();
                *s.lock().unwrap() = format!("drawn in {:.2}s", began.elapsed().as_secs_f64());
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
    group: usize,
    groups: Vec<(usize, usize, usize)>,
    drag: Option<(&'static str, (usize, usize))>,
    peek_until: f64,
    preview: bool,
    fade: Fade,
    pending: bool,
    published_at: f64,
}

impl Panel {
    fn new() -> Panel {
        let view = sources::view();
        let mut p = Panel {
            set: Map::new(),
            at: "time",
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
            group: 0,
            groups: Vec::new(),
            drag: None,
            peek_until: 0.0,
            preview: false,
            fade: Fade::new(),
            pending: false,
            published_at: 0.0,
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
                self.select_group(self.group);
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
        if key != "view" && (v - self.value(key)).abs() < 1e-8 { self.adjusting(); return; }
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
            self.rows = rows_for(self.view());
            return self.commit();
        }
        self.adjusting();
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
        self.pending = true;
    }

    fn publish(&mut self) -> bool {
        if !self.pending || tm::mono() - self.published_at < 1.0 / 30.0 { return false; }
        self.pending = false;
        self.published_at = tm::mono();
        sources::write_atomic(&cache_file("dev.json"), &json!({"pid": std::process::id(), "set": self.set}).to_string());
        self.describe();
        let _ = self.painter.changed.try_send(());
        true
    }

    fn adjusting(&mut self) {
        self.peek_until = tm::mono() + 0.9;
        self.fade.set(true);
    }

    fn select_group(&mut self, group: usize) {
        self.group = group.min(4);
        let label = ["when", "weather", "yours", "how often", "this view"][self.group];
        let mut inside = false;
        if let Some(r) = self.rows.iter().find(|r| {
            if r.key.is_empty() { inside = r.label == label; false } else { inside }
        }) { self.at = r.key; }
        self.peek_until = 0.0;
        self.preview = false;
        self.drag = None;
        self.fade.set(false);
    }

    fn paint(&mut self, out: &mut impl Write) {
        let (rows_n, cols) = term_size();
        let peek = self.preview || tm::mono() < self.peek_until || self.drag.is_some();
        self.fade.set(peek);
        let mut s = String::from("\x1b[H\x1b[2J");
        let (accent, dim, bold, off) = ("\x1b[33m", "\x1b[38;5;250m", "\x1b[1m", "\x1b[0m");
        let mut put = |y: usize, x: usize, text: &str, style: &str| {
            if y + 1 < rows_n.max(1) && x + 1 < cols {
                let cut: String = text.chars().take(cols - 1 - x).collect();
                s.push_str(&format!("\x1b[{};{}H{}{}{}", y + 1, x + 1, style, cut, off));
            }
        };
        put(0, 2, "dormer", &format!("{}{}", accent, bold));
        put(0, 10, if peek { "preview" } else { "scene controls" }, dim);
        let overrides = self.set.len();
        let drawing = self.painter.status.lock().unwrap().as_str() == "drawing";
        let status = if drawing { "drawing".to_string() } else if overrides == 0 { "following reality".to_string() } else { format!("{} held", overrides) };
        put(0, cols.saturating_sub(status.len() + 2).max(27), &status, dim);
        let (label_w, value_w) = (17usize, 15usize);
        let bar_x = 2 + label_w;
        let bar_w = cols.saturating_sub(bar_x + value_w + 4).max(2);
        self.bar = (bar_x, bar_w);
        self.tabs.clear();
        self.groups.clear();
        self.row_of.clear();
        let view = self.view();
        if !peek {
            let mut x = 2;
            for (k, v) in views::ALL.iter().enumerate() {
                let text = if v.id() == view.id() { format!("[{}]", v.id()) } else { v.id().to_string() };
                put(2, x, &text, if v.id() == view.id() { accent } else { dim });
                self.tabs.push((x, x + text.len(), k));
                x += text.len() + 2;
            }
            put(3, 2, view.about(), dim);
            let mut x = 2;
            for (k, label) in ["Time", "Weather", "Machine", "Motion", "Scene"].iter().enumerate() {
                let text = format!("{} {}", k + 1, label);
                put(5, x, &text, if k == self.group { accent } else { dim });
                self.groups.push((x, x + text.len(), k));
                x += text.len() + 2;
            }
        }
        let label = ["when", "weather", "yours", "how often", "this view"][self.group];
        let mut inside = false;
        let indices: Vec<usize> = self.rows.iter().enumerate().filter_map(|(i, r)| {
            if r.key.is_empty() { inside = r.label == label; None }
            else if inside && r.key != "view" { Some(i) } else { None }
        }).collect();
        let selected = indices.iter().position(|i| self.rows[*i].key == self.at).unwrap_or(0);
        let visible = rows_n.saturating_sub(11).max(1);
        let (first, last) = visible_range(selected, visible, indices.len().saturating_sub(1));
        let drawing: Vec<usize> = if peek { self.rows.iter().position(|r| r.key == self.at && r.key != "view").into_iter().collect() }
            else { indices.iter().skip(first).take(last - first).copied().collect() };
        if drawing.is_empty() { put(7, 2, "This view has no extra controls. Try another view above.", dim); }
        for (n, i) in drawing.iter().copied().enumerate() {
            let r = self.rows[i];
            let y = if peek { 3 } else { 7 + n };
            self.row_of.push((y, i));
            let chosen = r.key == self.at;
            let (v, held) = (self.value(r.key), self.set.contains_key(r.key));
            put(y, 0, if chosen { ">" } else { " " }, accent);
            put(y, 2, r.label, if chosen { bold } else { "" });
            let knob = (((v - r.lo) / (r.hi - r.lo)) * (bar_w - 1) as f64).round().clamp(0.0, (bar_w - 1) as f64) as usize;
            put(y, bar_x, &"━".repeat(knob), if held { accent } else { dim });
            put(y, bar_x + knob + 1, &"─".repeat(bar_w - knob - 1), dim);
            if held {
                let real = (((self.value_real(r.key) - r.lo) / (r.hi - r.lo)) * (bar_w - 1) as f64).round().clamp(0.0, (bar_w - 1) as f64) as usize;
                if real != knob { put(y, bar_x + real, "┆", dim); }
            }
            put(y, bar_x + knob, "●", if held { accent } else { bold });
            let text = r.show.map(|f| f(v)).unwrap_or_else(|| show(r.key, v));
            put(y, bar_x + bar_w + 2, &text, if held { accent } else { bold });
        }
        if peek {
            put(5, 2, "Release to return · p keeps the view open", dim);
        } else {
            if let Some(line) = self.info.first() { put(rows_n.saturating_sub(4), 2, line, dim); }
            put(rows_n.saturating_sub(3), 2, "Tab groups · ↑↓ select · ←→ / wheel adjust · drag slider", dim);
            put(rows_n.saturating_sub(2), 2, "r reset one · a reset all · n now · p preview · q restore & quit", dim);
        }
        let _ = out.write_all(s.as_bytes());
        let _ = out.flush();
    }

    /// False when it is time to quit.
    fn key(&mut self, k: Key) -> bool {
        let label = ["when", "weather", "yours", "how often", "this view"][self.group];
        let mut inside = false;
        let list: Vec<Row> = self.rows.iter().filter(|r| {
            if r.key.is_empty() { inside = r.label == label; false } else { inside && r.key != "view" }
        }).copied().collect();
        if let Key::Group(g) = k { self.select_group(g); return true; }
        if k == Key::NextGroup { self.select_group((self.group + 1) % 5); return true; }
        if k == Key::PrevGroup { self.select_group((self.group + 4) % 5); return true; }
        if k == Key::Preview { self.preview = !self.preview; self.peek_until = 0.0; return true; }
        if k == Key::Release { self.drag = None; self.peek_until = tm::mono() + 0.25; return true; }
        if k == Key::Quit { return false; }
        if k == Key::AllReal { self.set.clear(); self.commit(); return true; }
        if k == Key::Now { self.set.remove("time"); self.set.remove("day"); self.commit(); return true; }
        if k == Key::Real { self.set.remove(self.at); self.commit(); return true; }
        if let Key::Drag(x, _) = k {
            if let Some((key, bar)) = self.drag {
                if let Some(row) = self.rows.iter().find(|r| r.key == key).copied() { self.change(key, slider_value(x, bar, row.lo, row.hi, row.step)); }
            }
            return true;
        }
        if let Key::Mouse(x, y) = k {
            if y == 2 {
                if let Some(&(_, _, index)) = self.tabs.iter().find(|t| t.0 <= x && x < t.1) {
                    self.change("view", index as f64);
                    self.select_group(self.group);
                }
                return true;
            }
            if y == 5 {
                if let Some(&(_, _, group)) = self.groups.iter().find(|t| t.0 <= x && x < t.1) { self.select_group(group); }
                return true;
            }
        }
        if list.is_empty() { return true; }
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
                    if x >= bar_x && x < bar_x + bar_w {
                        self.drag = Some((r.key, self.bar));
                        self.change(r.key, slider_value(x, self.bar, r.lo, r.hi, r.step));
                    }
                }
            }
            Key::Wheel(_, y, direction) => {
                let row = self.row_of.iter().find(|r| r.0 == y).map(|r| self.rows[r.1]).unwrap_or(row);
                self.at = row.key;
                self.change(row.key, self.value(row.key) + direction as f64 * row.step);
            }
            Key::Quit => return false,
            Key::None => {}
            _ => {}
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
    Drag(usize, usize),
    Release,
    Wheel(usize, usize, i32),
    Group(usize),
    NextGroup,
    PrevGroup,
    Preview,
    Quit,
    None,
}

/// Turn what the terminal sent into keys. Escape sequences are read whole; a lone escape is ignored.
#[cfg(test)]
fn parse(bytes: &[u8]) -> Vec<Key> {
    decode(bytes).0
}

fn decode(bytes: &[u8]) -> (Vec<Key>, usize) {
    let mut keys = Vec::new();
    let mut i = 0;
    while i < bytes.len() {
        let b = bytes[i];
        if b == 0x1b && i + 1 == bytes.len() { break; }
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
                    if p.len() != 3 { Key::None }
                    else {
                        let (x, y) = (p[1].saturating_sub(1), p[2].saturating_sub(1));
                        if end == 'm' && p[0] & 3 == 0 { Key::Release }
                        else if p[0] & 64 != 0 { Key::Wheel(x, y, if p[0] & 1 == 0 { 1 } else { -1 }) }
                        else if p[0] & 3 == 0 { if p[0] & 32 != 0 { Key::Drag(x, y) } else { Key::Mouse(x, y) } }
                        else { Key::None }
                    }
                }
                ("", 'Z') => Key::PrevGroup,
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
            b'j' => Key::Down,
            b'\t' => Key::NextGroup,
            b'1'..=b'5' => Key::Group((b - b'1') as usize),
            b'p' => Key::Preview,
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
    (keys, i)
}

#[derive(Default)]
struct Decoder { pending: Vec<u8> }
impl Decoder {
    fn feed(&mut self, bytes: &[u8]) -> Vec<Key> {
        self.pending.extend_from_slice(bytes);
        let (keys, used) = decode(&self.pending);
        self.pending.drain(..used);
        if self.pending.len() > 256 { self.pending.clear(); }
        keys
    }
}

fn slider_value(x: usize, bar: (usize, usize), lo: f64, hi: f64, step: f64) -> f64 {
    let f = ((x as f64 - bar.0 as f64) / bar.1.saturating_sub(1).max(1) as f64).clamp(0.0, 1.0);
    (((lo + f * (hi - lo)) / step).round() * step).clamp(lo, hi)
}

fn visible_range(selected: usize, count: usize, last: usize) -> (usize, usize) {
    let count = count.max(1);
    let first = selected.saturating_sub(count - 1).min(last.saturating_add(1).saturating_sub(count));
    (first, (first + count).min(last + 1))
}

/// Only alter the dedicated panel terminal, never an unrelated terminal used with --here.
struct Fade { address: Option<String>, faded: Option<bool>, size: (usize, usize) }
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
        let mut panel_size = (620, 420);
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
            panel_size = (w, h);
            sources::run(&["hyprctl", "dispatch", "resizewindowpixel", &format!("exact {} {},{}", w, h, window)], 0.5);
            sources::run(&["hyprctl", "dispatch", "movewindowpixel", &format!("exact 24 28,{}", window)], 0.5);
        }
        Self { address, faded: None, size: panel_size }
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
            let height = if faded { self.size.1.min(140) } else { self.size.1 };
            sources::run(&["hyprctl", "dispatch", "resizewindowpixel", &format!("exact {} {},{}", self.size.0, height, window)], 0.5);
        }
    }
}

fn term_size() -> (usize, usize) {
    let mut ws: libc::winsize = unsafe { std::mem::zeroed() };
    if unsafe { libc::ioctl(1, libc::TIOCGWINSZ, &mut ws) } == 0 && ws.ws_row > 0 && ws.ws_col > 0 {
        (ws.ws_row as usize, ws.ws_col as usize)
    } else {
        (24, 80)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mouse_parser_distinguishes_drag_release_and_wheel() {
        assert_eq!(parse(b"\x1b[<0;20;8M"), vec![Key::Mouse(19, 7)]);
        assert_eq!(parse(b"\x1b[<32;24;8M"), vec![Key::Drag(23, 7)]);
        assert_eq!(parse(b"\x1b[<0;24;8m"), vec![Key::Release]);
        assert_eq!(parse(b"\x1b[<64;24;8M"), vec![Key::Wheel(23, 7, 1)]);
        assert_eq!(parse(b"\x1b[<65;24;8M"), vec![Key::Wheel(23, 7, -1)]);
        assert_eq!(parse(b"2\t\x1b[Zp"), vec![Key::Group(1), Key::NextGroup, Key::PrevGroup, Key::Preview]);
    }

    #[test]
    fn partial_escape_sequences_survive_terminal_reads() {
        let mut decoder = Decoder::default();
        assert!(decoder.feed(b"\x1b[<0;2").is_empty());
        assert_eq!(decoder.feed(b"0;8M\x1b["), vec![Key::Mouse(19, 7)]);
        assert_eq!(decoder.feed(b"C"), vec![Key::Right]);
    }

    #[test]
    fn controls_fit_small_terminals_and_drag_values_are_clamped() {
        assert_eq!(slider_value(0, (19, 20), 0.0, 100.0, 5.0), 0.0);
        assert_eq!(slider_value(100, (19, 20), 0.0, 100.0, 5.0), 100.0);
        assert_eq!(slider_value(29, (19, 21), 0.0, 100.0, 5.0), 50.0);
        assert_eq!(visible_range(14, 3, 19), (12, 15));
        assert_eq!(visible_range(0, 3, 2), (0, 3));
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
    let mut decoder = Decoder::default();
    let mut dirty = true;
    let mut was_peeking = false;
    let mut last_size = term_size();
    'outer: loop {
        if tm::now() - panel.real_at > 20.0 {
            panel.refresh_real(); // the real clock and weather move on underneath
            dirty = true;
        }
        dirty |= panel.publish();
        let size = term_size();
        if size != last_size { last_size = size; dirty = true; }
        let peeking = panel.preview || panel.drag.is_some() || tm::mono() < panel.peek_until;
        if peeking != was_peeking { dirty = true; was_peeking = peeking; }
        if dirty { panel.paint(&mut out); dirty = false; }
        let mut fds = [libc::pollfd { fd: 0, events: libc::POLLIN, revents: 0 }];
        let wait = if panel.pending { 34 } else if peeking && !panel.preview && panel.drag.is_none() { 100 } else { 1000 };
        if unsafe { libc::poll(fds.as_mut_ptr(), 1, wait) } <= 0 {
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
            if !panel.key(k) {
                break 'outer;
            }
            if k != Key::None { dirty = true; }
        }
    }
    panel.fade.set(false);
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
