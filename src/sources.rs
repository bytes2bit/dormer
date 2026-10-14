//! Where dormer learns things, and where it puts the picture.
//!
//! Its own config, the weather, the theme mode, what this machine is doing,
//! and the wallpaper daemon that shows the result. Everything is optional:
//! with no config at all dormer works out as much as it can and says what it
//! assumed (`dormer doctor`).

use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::color::{parse_hex, Rgb};
use crate::scene::{Machine, Place};
use crate::sky::{from_wmo, is_wet, preset, Weather};
use crate::tm;

pub fn home() -> PathBuf {
    PathBuf::from(std::env::var("HOME").unwrap_or_else(|_| "/".into()))
}

fn xdg(var: &str, fallback: &str) -> PathBuf {
    match std::env::var(var) {
        Ok(v) if !v.is_empty() => PathBuf::from(v),
        _ => home().join(fallback),
    }
}

pub fn cache() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("dormer")
}

pub fn cache_file(name: &str) -> PathBuf {
    cache().join(name)
}

pub fn config_path() -> PathBuf {
    xdg("XDG_CONFIG_HOME", ".config").join("dormer").join("config.toml")
}

/// Where choices made on the command line are kept.
pub fn state() -> PathBuf {
    xdg("XDG_STATE_HOME", ".local/state").join("dormer")
}

/// The view to look out on: what `dormer view` last chose, else the config's, else the rooftops.
pub fn view() -> &'static dyn crate::views::View {
    fs::read_to_string(state().join("view"))
        .ok()
        .and_then(|id| crate::views::find(id.trim()))
        .or_else(|| crate::views::find(&config().view))
        .unwrap_or(crate::views::ALL[0])
}

pub fn runtime() -> PathBuf {
    PathBuf::from(std::env::var("XDG_RUNTIME_DIR").unwrap_or_else(|_| "/tmp".into()))
}

fn noctalia_cache() -> PathBuf {
    xdg("XDG_CACHE_HOME", ".cache").join("noctalia")
}

pub struct Config {
    pub latitude: Option<f64>,
    pub longitude: Option<f64>,
    pub place_name: String,
    pub seed: u64,
    pub view: String,
    pub size: String,
    pub mode: String,
    pub top_dark: String,
    pub top_light: String,
    pub backend: String,
    pub command: String,
    pub weather: String,
    pub lamp: Vec<String>,
    pub containers: bool,
    pub fps: u32,
    pub interactive: bool,
    pub hold_file: String,
    pub terminal: String,
    /// was there a config file to read
    pub found: bool,
}

/// The config file laid over the defaults.
pub fn config() -> &'static Config {
    static CONFIG: OnceLock<Config> = OnceLock::new();
    CONFIG.get_or_init(|| {
        let text = fs::read_to_string(config_path()).ok();
        let found = text.is_some();
        let table: toml::Table = text.and_then(|t| t.parse().ok()).unwrap_or_default();
        let get = |section: &str, key: &str| table.get(section).and_then(|s| s.as_table()).and_then(|s| s.get(key)).cloned();
        let text = |section: &str, key: &str, or: &str| {
            get(section, key).and_then(|v| v.as_str().map(|s| s.to_string())).unwrap_or_else(|| or.to_string())
        };
        let number = |section: &str, key: &str| get(section, key).and_then(|v| v.as_float().or(v.as_integer().map(|i| i as f64)));
        Config {
            latitude: number("place", "latitude"),
            longitude: number("place", "longitude"),
            place_name: text("place", "name", ""),
            seed: number("look", "seed").map(|v| v.max(0.0) as u64).unwrap_or(12),
            view: text("look", "view", "rooftops"),
            size: text("look", "size", "auto"),
            mode: text("look", "mode", "auto"),
            top_dark: text("look", "top_dark", "#1A2027"),
            top_light: text("look", "top_light", "#ECEFEE"),
            backend: text("wallpaper", "backend", "auto"),
            command: text("wallpaper", "command", ""),
            weather: text("weather", "source", "auto"),
            lamp: get("house", "lamp")
                .and_then(|v| v.as_array().map(|a| a.iter().filter_map(|s| s.as_str().map(|s| s.to_string())).collect()))
                .unwrap_or_else(|| vec!["claude".into(), "codex".into()]),
            containers: get("house", "containers").and_then(|v| v.as_bool()).unwrap_or(true),
            fps: number("live", "fps").map(|v| v.clamp(5.0, 60.0) as u32).unwrap_or(30),
            interactive: interactive_setting(&table),
            hold_file: text("power", "hold_file", ""),
            terminal: text("dev", "terminal", ""),
            found,
        }
    })
}

fn interactive_setting(table: &toml::Table) -> bool {
    table.get("live").and_then(|t| t.get("interactive")).and_then(|v| v.as_bool()).unwrap_or(true)
}

/// Run a command and wait a little for it. None when it could not be run or took too long.
pub fn run(cmd: &[&str], timeout: f64) -> Option<(bool, String, String)> {
    let mut child = Command::new(cmd[0]).args(&cmd[1..]).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().ok()?;
    let began = Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let (mut out, mut err) = (String::new(), String::new());
                if let Some(mut s) = child.stdout.take() {
                    let _ = s.read_to_string(&mut out);
                }
                if let Some(mut s) = child.stderr.take() {
                    let _ = s.read_to_string(&mut err);
                }
                return Some((status.success(), out, err));
            }
            Ok(None) if began.elapsed().as_secs_f64() > timeout => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
            Ok(None) => std::thread::sleep(Duration::from_millis(8)),
            Err(_) => return None,
        }
    }
}

pub fn which(name: &str) -> Option<PathBuf> {
    let path = std::env::var("PATH").unwrap_or_default();
    path.split(':').map(|dir| Path::new(dir).join(name)).find(|p| p.is_file())
}

pub fn read_json(path: &Path) -> Option<Value> {
    serde_json::from_str(&fs::read_to_string(path).ok()?).ok()
}

/// Write a file so that anyone watching sees either the old one or the whole new one.
pub fn write_atomic(path: &Path, text: &str) -> bool {
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let tmp = path.with_extension("tmp");
    fs::write(&tmp, text).is_ok() && fs::rename(&tmp, path).is_ok()
}

// ── weather and place ────────────────────────────────────────────────────────

/// Seconds since 1970 of a UTC calendar date and time.
fn utc_stamp(y: i64, m: i64, d: i64, hh: i64, mm: i64) -> f64 {
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let doy = (153 * (m + if m > 2 { -3 } else { 9 }) + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    ((era * 146097 + doe - 719468) * 86400 + hh * 3600 + mm * 60) as f64
}

fn iso_stamp(text: &str, offset: f64) -> Option<f64> {
    let (date, time) = text.split_once('T')?;
    let d: Vec<i64> = date.split('-').filter_map(|p| p.parse().ok()).collect();
    let t: Vec<i64> = time.split(':').filter_map(|p| p.parse().ok()).collect();
    if d.len() != 3 || t.len() < 2 {
        return None;
    }
    Some(utc_stamp(d[0], d[1], d[2], t[0], t[1]) - offset)
}

/// (weather or None, place or None) from the Noctalia shell's own cache.
fn noctalia_weather() -> (Option<Weather>, Option<Place>) {
    let dir = noctalia_cache();
    let Some(snap) = read_json(&dir.join("weather.json")).and_then(|v| v.get("snapshot").cloned()) else {
        let place = read_json(&dir.join("location.json")).and_then(|loc| {
            Some(Place {
                lat: loc.get("latitude")?.as_f64()?,
                lon: loc.get("longitude")?.as_f64()?,
                name: loc.get("name").and_then(|n| n.as_str()).unwrap_or("").to_string(),
            })
        });
        return (None, place);
    };
    let f = |v: &Value, key: &str| v.get(key).and_then(|x| x.as_f64());
    let place = match (f(&snap, "latitude"), f(&snap, "longitude")) {
        (Some(lat), Some(lon)) => {
            Some(Place { lat, lon, name: snap.get("location_name").and_then(|n| n.as_str()).unwrap_or("").to_string() })
        }
        _ => None,
    };
    let now = tm::now();
    let fresh = snap.get("valid").and_then(|v| v.as_bool()).unwrap_or(true) && now - f(&snap, "fetched_at").unwrap_or(0.0) < 4.0 * 3600.0;
    let mut wx = None;
    if let (true, Some(cur)) = (fresh, snap.get("current")) {
        if let (Some(code), Some(temp), Some(wind), Some(wdir)) =
            (f(cur, "weather_code"), f(cur, "temperature_c"), f(cur, "wind_speed_kmh"), f(cur, "wind_direction_deg"))
        {
            let mut w = from_wmo(code as i64);
            w.temp = temp;
            w.wind = wind;
            w.wdir = wdir;
            w.humidity = f(cur, "relative_humidity_percent").unwrap_or(60.0);
            w.source = "noctalia".into();
            // is rain forecast for the next few hours?
            let offset = f(&snap, "utc_offset_seconds").unwrap_or(0.0);
            if let Some(hours) = snap.get("forecast_hours").and_then(|h| h.as_array()) {
                for h in hours {
                    let Some(at) = h.get("time_iso").and_then(|t| t.as_str()).and_then(|t| iso_stamp(t, offset)) else { continue };
                    let ahead = (at - now) / 3600.0;
                    if !(0.0..=4.0).contains(&ahead) {
                        continue;
                    }
                    let wet = is_wet(f(h, "weather_code").unwrap_or(0.0) as i64);
                    if wet && f(h, "precipitation_probability_percent").unwrap_or(100.0) >= 45.0 {
                        w.rain_in = Some(ahead);
                        break;
                    }
                }
            }
            wx = Some(w);
        }
    }
    (wx, place)
}

/// Ask Open-Meteo directly, at most every twenty minutes. No key needed.
fn open_meteo(place: &Place) -> Option<Weather> {
    let path = cache_file("weather.json");
    let mut kept = read_json(&path).unwrap_or(Value::Null);
    let f = |v: &Value, key: &str| v.get(key).and_then(|x| x.as_f64());
    let now = tm::now();
    let same = (f(&kept, "lat").unwrap_or(99.0) - place.lat).abs() < 0.05 && (f(&kept, "lon").unwrap_or(999.0) - place.lon).abs() < 0.05;
    if !same || now - f(&kept, "fetched").unwrap_or(0.0) > 20.0 * 60.0 {
        let url = format!(
            "https://api.open-meteo.com/v1/forecast?latitude={:.3}&longitude={:.3}&current=temperature_2m,weather_code,wind_speed_10m,wind_direction_10m,relative_humidity_2m",
            place.lat, place.lon
        );
        if let Some((true, body, _)) = run(&["curl", "-fsS", "--max-time", "6", &url], 8.0) {
            if let Some(cur) = serde_json::from_str::<Value>(&body).ok().and_then(|v| v.get("current").cloned()) {
                if let (Some(code), Some(temp), Some(wind), Some(wdir)) =
                    (f(&cur, "weather_code"), f(&cur, "temperature_2m"), f(&cur, "wind_speed_10m"), f(&cur, "wind_direction_10m"))
                {
                    kept = serde_json::json!({"lat": place.lat, "lon": place.lon, "fetched": now, "code": code, "temp": temp,
                                              "wind": wind, "wdir": wdir, "humidity": f(&cur, "relative_humidity_2m").unwrap_or(60.0)});
                    write_atomic(&path, &kept.to_string());
                }
            }
        }
    }
    let same = (f(&kept, "lat").unwrap_or(99.0) - place.lat).abs() < 0.05 && (f(&kept, "lon").unwrap_or(999.0) - place.lon).abs() < 0.05;
    if !same || now - f(&kept, "fetched").unwrap_or(0.0) > 4.0 * 3600.0 {
        return None;
    }
    let mut w = from_wmo(f(&kept, "code")? as i64);
    w.temp = f(&kept, "temp")?;
    w.wind = f(&kept, "wind")?;
    w.wdir = f(&kept, "wdir")?;
    w.humidity = f(&kept, "humidity").unwrap_or(60.0);
    w.source = "open-meteo".into();
    Some(w)
}

/// (weather, place). Weather is always usable; `known` says whether it was really measured.
pub fn read_weather(pretend: bool) -> (Weather, Place) {
    let cfg = config();
    let (mut wx, mut place) = (None, None);
    if cfg.weather == "auto" || cfg.weather == "noctalia" {
        (wx, place) = noctalia_weather();
    }
    if let (Some(lat), Some(lon)) = (cfg.latitude, cfg.longitude) {
        place = Some(Place { lat, lon, name: cfg.place_name.clone() });
    }
    if wx.is_none() && (cfg.weather == "auto" || cfg.weather == "open-meteo") {
        if let Some(p) = &place {
            wx = open_meteo(p);
        }
    }
    let mut wx = wx.unwrap_or_else(|| {
        let mut w = from_wmo(1);
        w.known = false;
        w
    });
    let place = place.unwrap_or(Place { lat: 51.48, lon: 0.0, name: "Greenwich (no place set)".into() });
    if pretend {
        // `dormer pretend snow` holds for a few minutes
        if let Some(act) = read_json(&cache_file("pretend.json")) {
            let until = act.get("until").and_then(|v| v.as_f64()).unwrap_or(0.0);
            if let (true, Some(name)) = (until > tm::now(), act.get("weather").and_then(|v| v.as_str())) {
                if let Some(mut w) = preset(name, &wx) {
                    w.note = "pretend";
                    if name == "snow" {
                        w.temp = w.temp.min(-1.0);
                    }
                    wx = w;
                }
            }
        }
    }
    (wx, place)
}

// ── this machine ─────────────────────────────────────────────────────────────

/// What the lamp, the blue windows and the stovepipe show.
pub fn read_machine() -> Machine {
    let cfg = config();
    let mut sessions: Vec<(String, String)> = Vec::new();
    if !cfg.lamp.is_empty() {
        if let Ok(dir) = fs::read_dir("/proc") {
            for entry in dir.flatten() {
                let name = entry.file_name();
                let Some(pid) = name.to_str().filter(|n| n.bytes().all(|b| b.is_ascii_digit())) else { continue };
                let Ok(stat) = fs::read_to_string(format!("/proc/{}/stat", pid)) else { continue };
                let (Some(a), Some(b)) = (stat.find('('), stat.rfind(')')) else { continue };
                let comm = &stat[a + 1..b];
                if cfg.lamp.iter().any(|n| n == comm) {
                    if let Some(session) = stat[b + 1..].split_whitespace().nth(3) {
                        let key = (comm.to_string(), session.to_string());
                        if !sessions.contains(&key) {
                            sessions.push(key);
                        }
                    }
                }
            }
        }
    }
    let mut boxes = 0;
    if cfg.containers {
        if let Ok(dir) = fs::read_dir("/sys/fs/cgroup/system.slice") {
            boxes = dir.flatten().filter(|e| e.file_name().to_str().map_or(false, |n| n.starts_with("docker-") && n.ends_with(".scope"))).count();
        }
    }
    let cores = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(1) as f32;
    let load = fs::read_to_string("/proc/loadavg")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|v| v.parse::<f32>().ok()))
        .map(|l| (l / cores).clamp(0.0, 1.0))
        .unwrap_or(0.0);
    let mut battery = None;
    if let Ok(dir) = fs::read_dir("/sys/class/power_supply") {
        for e in dir.flatten() {
            let p = e.path();
            if fs::read_to_string(p.join("type")).map_or(false, |t| t.trim() == "Battery")
                && fs::read_to_string(p.join("status")).map_or(false, |s| s.trim() == "Discharging")
            {
                battery = fs::read_to_string(p.join("capacity")).ok().and_then(|c| c.trim().parse::<f32>().ok()).map(|c| c / 100.0);
            }
        }
    }
    Machine { agents: sessions.len() as u32, containers: boxes as u32, load, battery, audio: audio_playing() }
}

/// Is any sound card playing right now?
fn audio_playing() -> bool {
    let Ok(cards) = fs::read_dir("/proc/asound") else { return false };
    for card in cards.flatten().filter(|c| c.file_name().to_str().map_or(false, |n| n.starts_with("card"))) {
        let Ok(pcms) = fs::read_dir(card.path()) else { continue };
        for pcm in pcms.flatten().filter(|p| p.file_name().to_str().map_or(false, |n| n.starts_with("pcm") && n.ends_with('p'))) {
            let Ok(subs) = fs::read_dir(pcm.path()) else { continue };
            for sub in subs.flatten() {
                if fs::read_to_string(sub.path().join("status")).map_or(false, |s| s.contains("RUNNING")) {
                    return true;
                }
            }
        }
    }
    false
}

/// dark or light: the config, else the shell's theme, else the desktop's colour scheme.
pub fn theme_mode() -> String {
    let ok = |m: &str| m == "dark" || m == "light";
    let cfg = config();
    if ok(&cfg.mode) {
        return cfg.mode.clone();
    }
    let mut mode = std::env::var("NOCTALIA_THEME_MODE").unwrap_or_default();
    if !ok(&mode) && which("noctalia").is_some() {
        if let Some((_, out, _)) = run(&["noctalia", "msg", "theme-mode-get"], 4.0) {
            mode = out.trim().to_string();
        }
    }
    if !ok(&mode) && which("gsettings").is_some() {
        if let Some((_, out, _)) = run(&["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"], 3.0) {
            if !out.trim().is_empty() {
                mode = if out.contains("light") || out.contains("default") { "light".into() } else { "dark".into() };
            }
        }
    }
    if ok(&mode) {
        mode
    } else {
        "dark".into()
    }
}

/// The colour the sky fades into at the top: set it to your bar's colour.
pub fn top_colour(mode: &str) -> Option<Rgb> {
    let cfg = config();
    parse_hex(if mode == "light" { &cfg.top_light } else { &cfg.top_dark })
}

pub fn parse_size(text: &str) -> Option<(usize, usize)> {
    let (w, h) = text.trim().to_lowercase().split_once('x').map(|(a, b)| (a.trim().parse().ok(), b.trim().parse().ok()))?;
    match (w, h) {
        (Some(w), Some(h)) if (64..=16384).contains(&w) && (64..=16384).contains(&h) => Some((w, h)),
        _ => None,
    }
}

/// Pixels to draw: the config, else the largest connected screen, else 1920x1200.
pub fn screen_size() -> (usize, usize) {
    if let Some(size) = parse_size(&config().size) {
        return size;
    }
    let mut best = (1920, 1200);
    {
        if let Some(mons) = hyprland("j/monitors") {
            let mut area = 0;
            for m in mons.as_array().into_iter().flatten() {
                let (w, h) = (m["width"].as_u64().unwrap_or(0) as usize, m["height"].as_u64().unwrap_or(0) as usize);
                let (w, h) = if m["transform"].as_u64().unwrap_or(0) % 2 == 1 { (h, w) } else { (w, h) };
                if w * h > area {
                    area = w * h;
                    best = (w, h);
                }
            }
        }
    }
    best
}

fn expand(path: &str) -> String {
    let mut out = path.to_string();
    if let Some(rest) = out.strip_prefix("~/") {
        out = home().join(rest).to_string_lossy().into_owned();
    }
    for var in ["XDG_RUNTIME_DIR", "XDG_CACHE_HOME", "XDG_CONFIG_HOME", "HOME"] {
        if let Ok(v) = std::env::var(var) {
            out = out.replace(&format!("${}", var), &v).replace(&format!("${{{}}}", var), &v);
        }
    }
    out
}

/// True when something else has asked for stillness (a battery saver, say).
pub fn held() -> bool {
    let path = expand(&config().hold_file);
    !path.is_empty() && Path::new(&path).exists()
}

// ── the live layer and the dev panel ─────────────────────────────────────────

fn alive(pid: i64, needles: &[&str]) -> bool {
    fs::read(format!("/proc/{}/cmdline", pid)).map_or(false, |line| {
        let line = String::from_utf8_lossy(&line);
        needles.iter().all(|n| line.contains(n))
    })
}

/// The pid of the running live layer, or 0.
pub fn live_pid() -> i64 {
    let pid = fs::read_to_string(cache_file("live.pid")).ok().and_then(|s| s.trim().parse::<i64>().ok()).unwrap_or(0);
    if pid > 0 && alive(pid, &["dormer", "live"]) {
        pid
    } else {
        0
    }
}

/// What the dev panel (or the tour) is holding the scene to, while it is running.
pub fn overrides() -> serde_json::Map<String, Value> {
    let path = cache_file("dev.json");
    let Some(data) = read_json(&path) else { return Default::default() };
    if !alive(data.get("pid").and_then(|p| p.as_i64()).unwrap_or(0), &["dormer"]) {
        let _ = fs::remove_file(&path); // it died without tidying up
        return Default::default();
    }
    data.get("set").and_then(|s| s.as_object()).cloned().unwrap_or_default()
}

// ── putting the picture on the screen ────────────────────────────────────────

/// Is a program of this name running? Read from /proc: cheaper than starting pgrep.
fn running(name: &str) -> bool {
    let Ok(dir) = fs::read_dir("/proc") else { return false };
    dir.flatten().any(|e| {
        e.file_name().to_str().map_or(false, |n| n.bytes().all(|b| b.is_ascii_digit()))
            && fs::read_to_string(e.path().join("comm")).map_or(false, |c| c.trim() == name)
    })
}

/// Ask Hyprland something over its socket. None when it is not the compositor.
pub fn hyprland(what: &str) -> Option<Value> {
    use std::io::Write;
    let sig = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty())?;
    let mut s = std::os::unix::net::UnixStream::connect(runtime().join("hypr").join(sig).join(".socket.sock")).ok()?;
    let _ = s.set_read_timeout(Some(Duration::from_secs(1)));
    s.write_all(what.as_bytes()).ok()?;
    let mut text = String::new();
    s.read_to_string(&mut text).ok()?;
    serde_json::from_str(&text).ok()
}

/// Which wallpaper daemon to hand the picture to.
pub fn backend() -> String {
    let cfg = config();
    if cfg.backend != "auto" {
        return cfg.backend.clone();
    }
    if !cfg.command.is_empty() {
        return "command".into();
    }
    if which("noctalia").is_some() && running("noctalia") {
        "noctalia".into()
    } else if which("swww").is_some() && running("swww-daemon") {
        "swww".into()
    } else if which("hyprctl").is_some() && running("hyprpaper") {
        "hyprpaper".into()
    } else {
        "none".into()
    }
}

/// Show this file as the wallpaper.
pub fn set_wallpaper(path: &str) -> Result<(), String> {
    let kind = backend();
    let quoted = format!("'{}'", path);
    let shell = config().command.replace("{path}", &quoted);
    let reload = format!(",{}", path);
    let cmd: Vec<&str> = match kind.as_str() {
        "noctalia" => vec!["noctalia", "msg", "wallpaper-set", path],
        "swww" => vec!["swww", "img", path, "--transition-type", "fade", "--transition-duration", "1"],
        "hyprpaper" => vec!["hyprctl", "hyprpaper", "reload", &reload],
        "command" => vec!["sh", "-c", &shell],
        _ => return Err(format!("no wallpaper daemon found (set [wallpaper] backend or command in {})", config_path().display())),
    };
    match run(&cmd, 15.0) {
        None => Err(format!("{} did not answer", kind)),
        Some((false, out, err)) => Err(format!("{} refused the picture: {}", kind, if err.trim().is_empty() { out.trim() } else { err.trim() })),
        Some(_) => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn interactive_defaults_on_and_can_restore_click_through() {
        assert!(super::interactive_setting(&toml::Table::new()));
        assert!(!super::interactive_setting(&"[live]\ninteractive = false".parse().unwrap()));
        assert!(super::interactive_setting(&"[live]\ninteractive = true".parse().unwrap()));
        assert!(super::interactive_setting(&"[live]\ninteractive = 'invalid'".parse().unwrap()));
    }
}
