//! dormer — a window in the roof: the same rooftops under the real sky.

mod color;
mod buffer;
mod dev;
mod gfx;
mod interaction;
mod live;
mod paint;
mod pointer;
mod props;
mod rng;
mod scene;
mod settings;
mod sky;
mod sources;
mod tm;
mod town;
mod views;
mod wl;
mod words;

use std::collections::HashMap;
use std::fs;
use std::os::fd::AsRawFd;

use serde_json::{json, Map, Value};

use paint::Cache;
use scene::{Inputs, Machine, Place, Rates, Scene};
use sky::{describe, preset, Weather, PRESETS};
use sources::{cache_file, config};
use views::View;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

const USAGE: &str = "dormer — a window in the roof: the same rooftops under the real sky.

    dormer                      draw the wallpaper for this moment
    dormer explain              say what the picture is showing, and why
    dormer tour                 play the whole of today in about a minute
    dormer pretend snow         borrow some weather for ten minutes
    dormer live                 run the living wallpaper (birds, rain, shooting stars)
    dormer view moon            look out somewhere else: rooftops | street | city | coast | moon | vale
    dormer dev                  open the slider panel and bend the scene by hand
    dormer install              start on login, keep the still wallpaper up to date
    dormer doctor               show what dormer found on this machine

Moments (draw, explain, sheet):
    --at 'YYYY-MM-DD HH:MM'     another moment ('HH:MM' means today)
    --weather KIND              clear fair partly overcast fog drizzle rain heavy snow storm
    --temp C  --wind KMH        override the reading
    --mode dark|light           --size 2560x1600      --seed N (a different old town)
    --view NAME                 draw another view without changing yours (`dormer view` lists them)
    --agents N  --containers N  pretend this much is running

More:
    dormer draw --out a.png     write the picture to a file instead of setting it
    dormer draw --explain       also describe it          --force: draw even when held
    dormer sheet out.png 'YYYY-MM-DD HH:MM[,weather][,light][,view]' ...   several moments on one sheet
    dormer tour --seconds 60    how long the day takes
    dormer pretend off          stop pretending early     --minutes N: how long to pretend
    dormer live --frame a.png   draw one frame of the living wallpaper to a file
                                --pointer X,Y --click X,Y: try touching it (1600x1000 design units)
    dormer hold on|off          freeze or release the living wallpaper (for a lock screen)
    dormer install --no-live    still frames only         dormer uninstall
    dormer dev --here           run the panel in this terminal
";

pub struct Args {
    flags: HashMap<String, String>,
    pos: Vec<String>,
}

impl Args {
    fn parse(argv: &[String]) -> Result<Args, String> {
        const SWITCHES: [&str; 6] = ["force", "explain", "here", "no-live", "help", "quiet"];
        let (mut flags, mut pos) = (HashMap::new(), Vec::new());
        let mut i = 0;
        while i < argv.len() {
            let a = &argv[i];
            if let Some(name) = a.strip_prefix("--") {
                if let Some((k, v)) = name.split_once('=') {
                    flags.insert(k.to_string(), v.to_string());
                } else if SWITCHES.contains(&name) {
                    flags.insert(name.to_string(), "1".to_string());
                } else {
                    i += 1;
                    let v = argv.get(i).ok_or(format!("--{} needs a value", name))?;
                    flags.insert(name.to_string(), v.clone());
                }
            } else if a == "-h" {
                flags.insert("help".into(), "1".into());
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        Ok(Args { flags, pos })
    }

    pub fn get(&self, key: &str) -> Option<&str> {
        self.flags.get(key).map(|s| s.as_str())
    }

    pub fn has(&self, key: &str) -> bool {
        self.flags.contains_key(key)
    }

    fn num<T: std::str::FromStr>(&self, key: &str) -> Result<Option<T>, String> {
        match self.get(key) {
            None => Ok(None),
            Some(v) => v.parse().map(Some).map_err(|_| format!("--{} does not take '{}'", key, v)),
        }
    }
}

/// Everything a scene is built from, after flags and the dev panel have had their say.
pub struct Moment {
    pub ts: f64,
    pub wx: Weather,
    pub place: Place,
    pub machine: Machine,
    pub mode: String,
    pub rates: Rates,
    /// time is being held somewhere other than now
    pub bent: bool,
    pub view: &'static dyn View,
    /// the view's own knobs, where the dev panel is holding them
    pub knobs: Vec<(String, f64)>,
}

impl Moment {
    /// The same moment again, for another clock.
    pub fn at(&self, ts: f64) -> Moment {
        Moment { ts, wx: self.wx.clone(), place: self.place.clone(), machine: self.machine, mode: self.mode.clone(), rates: self.rates, bent: false, view: self.view, knobs: Vec::new() }
    }
}

/// Lay the dev panel's sliders over reality.
pub fn with_overrides(dev: &Map<String, Value>, m: &mut Moment) {
    if dev.is_empty() {
        return;
    }
    let f = |k: &str| dev.get(k).and_then(|v| v.as_f64());
    if let Some(view) = dev.get("view").and_then(|v| v.as_str()).and_then(views::find) {
        m.view = view;
    }
    // a knob the panel holds replaces the same knob from the saved profile; the rest stay
    for k in m.view.knobs() {
        if let Some(v) = f(k.key) {
            m.knobs.retain(|(name, _)| name != k.key);
            m.knobs.push((k.key.to_string(), v));
        }
    }
    if let Some(mode) = dev.get("mode").and_then(|v| v.as_str()) {
        match mode {
            "dark" | "light" => m.mode = mode.to_string(),
            "auto" => m.mode = sources::desktop_mode(),
            _ => {}
        }
    }
    // days and clock times are calendar days and wall-clock times, whatever daylight saving did
    if let Some(day) = f("day") {
        let l = tm::local(m.ts);
        m.ts = tm::stamp(l.year, l.month, l.mday + day.round() as i32, l.hour, l.min, l.sec);
        m.bent = true;
    }
    if let Some(time) = f("time") {
        let l = tm::local(m.ts);
        let minutes = (time * 60.0).round() as i32;
        m.ts = tm::stamp(l.year, l.month, l.mday, minutes.div_euclid(60), minutes.rem_euclid(60), 0);
        m.bent = true;
    }
    if ["cover", "rain", "snow", "fog", "storm", "wind", "wdir", "temp", "humidity", "rain_in"].iter().any(|k| dev.contains_key(*k)) {
        let wx = &mut m.wx;
        wx.note = "dev";
        wx.known = true;
        // the slider's far end means no rain in sight
        wx.rain_in = f("rain_in").filter(|h| *h < 3.9);
        if let Some(v) = f("cover") {
            wx.cover = v / 100.0;
        }
        if let Some(v) = f("rain") {
            wx.rain = v / 100.0;
        }
        if let Some(v) = f("snow") {
            wx.snow = v / 100.0;
        }
        if let Some(v) = f("fog") {
            wx.fog = v / 100.0;
        }
        if let Some(v) = f("storm") {
            wx.storm = v > 0.5;
        }
        if wx.storm {
            wx.rain = wx.rain.max(0.6);
        }
        let wet = wx.rain.max(wx.snow);
        if !dev.contains_key("cover") && wet > 0.0 {
            wx.cover = wx.cover.max(0.75 + 0.25 * wet); // rain needs a cloud to fall from
        }
        if let Some(v) = f("wind") {
            wx.wind = v;
        }
        if let Some(v) = f("wdir") {
            wx.wdir = v;
        }
        if let Some(v) = f("temp") {
            wx.temp = v;
        }
        if let Some(v) = f("humidity") {
            wx.humidity = v;
        }
        wx.kind = describe(wx).to_string();
    }
    if let Some(v) = f("agents") {
        m.machine.agents = v as u32;
    }
    if let Some(v) = f("containers") {
        m.machine.containers = v as u32;
    }
    if let Some(v) = f("load") {
        m.machine.load = (v / 100.0) as f32;
    }
    if let Some(v) = f("music") {
        m.machine.audio = v > 0.5;
    }
    if let Some(v) = f("birds") {
        m.rates.birds = v as f32;
    }
    if let Some(v) = f("meteors") {
        m.rates.meteors = v as f32;
    }
    if let Some(v) = f("planes") {
        m.rates.planes = v as f32;
    }
}

/// This moment as the world reports it: no saved profile, no panel draft, no flags.
pub fn raw_moment(mode: Option<&str>) -> Moment {
    let (wx, place) = sources::read_weather(true);
    Moment {
        ts: tm::now(),
        wx,
        place,
        machine: sources::read_machine(),
        mode: mode.map(|s| s.to_string()).unwrap_or_else(sources::theme_mode),
        rates: Rates::default(),
        bent: false,
        view: sources::view(),
        knobs: Vec::new(),
    }
}

/// Lay the saved profile, then the dev panel's draft, over a raw moment.
pub fn settle(m: &mut Moment, overrides: bool) {
    let (set, auto) = if overrides { sources::dev_state() } else { Default::default() };
    // keys the panel has given back to reality are not taken from the saved profile either
    if let Err(e) = settings::runtime().apply_except(m, &auto) {
        eprintln!("dormer: saved settings not applied: {}", e);
    }
    with_overrides(&set, m);
}

/// This moment as the world has it, with the saved profile and any overrides laid on top.
/// The command line comes after all three.
pub fn real_moment(mode: Option<&str>, overrides: bool) -> Moment {
    let mut m = raw_moment(mode);
    settle(&mut m, overrides);
    m
}

fn moment(args: &Args) -> Result<Moment, String> {
    let mode = match args.get("mode") {
        Some(m) if m == "dark" || m == "light" => Some(m),
        Some(m) => return Err(format!("--mode is dark or light, not '{}'", m)),
        None => None,
    };
    let mut m = real_moment(mode, !args.has("at"));
    if let Some(at) = args.get("at") {
        m.ts = tm::parse(at).ok_or(format!("cannot read the time '{}' (use 'YYYY-MM-DD HH:MM' or 'HH:MM')", at))?;
        m.bent = true;
    }
    if let Some(id) = args.get("view") {
        m.view = views::find(id).ok_or(format!("--view is one of: {}", views::names()))?;
    }
    if let Some(kind) = args.get("weather") {
        m.wx = preset(kind, &m.wx).ok_or(format!("--weather is one of: {}", PRESETS.map(|p| p.0).join(" ")))?;
    }
    if let Some(v) = args.num::<f64>("temp")? {
        m.wx.temp = v;
    }
    if let Some(v) = args.num::<f64>("wind")? {
        m.wx.wind = v;
    }
    if let Some(v) = args.num::<f64>("wdir")? {
        m.wx.wdir = v;
    }
    if let Some(v) = args.num::<f64>("humidity")? {
        m.wx.humidity = v;
    }
    if let Some(v) = args.num::<u32>("agents")? {
        m.machine.agents = v;
    }
    if let Some(v) = args.num::<u32>("containers")? {
        m.machine.containers = v;
    }
    if let Some(v) = args.num::<f32>("load")? {
        m.machine.load = v;
    }
    Ok(m)
}

pub fn build(m: &Moment, drift: Option<f32>) -> Scene {
    scene::scene(Inputs {
        ts: m.ts,
        wx: &m.wx,
        mode: &m.mode,
        machine: m.machine,
        place: &m.place,
        drift,
        top: sources::top_colour(&m.mode),
        rates: m.rates,
        horizon: m.view.horizon(),
        knobs: &m.knobs,
        void: m.view.sky() == views::Sky::Void,
    })
}

fn size_of(args: &Args) -> Result<(usize, usize), String> {
    match args.get("size") {
        Some(s) => sources::parse_size(s).ok_or(format!("--size looks like 2560x1600, not '{}'", s)),
        None => Ok(sources::screen_size()),
    }
}

fn seed_of(args: &Args) -> Result<u64, String> {
    Ok(args.num::<u64>("seed")?.unwrap_or(config().seed))
}

/// What the last draw left behind, so the next one carries on from it.
pub struct State {
    pub ts: f64,
    pub drift: f64,
    pub mode: String,
    pub lit: f32,
    pub slot: String,
}

impl State {
    pub fn load() -> State {
        let v = sources::read_json(&cache_file("state.json")).unwrap_or(Value::Null);
        State {
            ts: v["ts"].as_f64().unwrap_or(0.0),
            drift: v["drift"].as_f64().unwrap_or(0.0),
            mode: v["mode"].as_str().unwrap_or("").to_string(),
            lit: v["lit"].as_f64().unwrap_or(0.0) as f32,
            slot: v["slot"].as_str().unwrap_or("a").to_string(),
        }
    }

    fn save(&self) {
        let v = json!({"ts": self.ts, "drift": self.drift, "mode": self.mode, "lit": self.lit, "slot": self.slot});
        sources::write_atomic(&cache_file("state.json"), &v.to_string());
    }

    /// Where the clouds have got to by `ts`: they keep their place and move on with the wind blowing now.
    pub fn drift_at(&self, ts: f64, m: &Moment) -> f64 {
        if self.ts == 0.0 {
            return (ts / 3600.0 * scene::wind_x(&m.wx, &m.place) as f64 * 0.19).rem_euclid(scene::DRIFT_WRAP);
        }
        let gap = (ts - self.ts).clamp(0.0, 1800.0);
        (self.drift + gap / 3600.0 * scene::wind_x(&m.wx, &m.place) as f64 * 0.19).rem_euclid(scene::DRIFT_WRAP)
    }
}

/// Draw this scene to a file.
fn render_to(path: &str, sc: &mut Scene, cache: &Cache) -> Result<(), String> {
    let (cv, _) = paint::render(sc, cache);
    gfx::save_png(path, cache.w, cache.h, &cv.finish(sc.p.grain))
}

fn cmd_draw(args: &Args) -> Result<i32, String> {
    let m = moment(args)?;
    let (size, seed) = (size_of(args)?, seed_of(args)?);
    let cache = Cache::new(m.view, seed, size.0, size.1);
    if let Some(out) = args.get("out") {
        let mut sc = build(&m, None);
        render_to(out, &mut sc, &cache)?;
        if args.has("explain") {
            println!("{}", words::explain(&sc, m.view, &cache.stars));
        }
        return Ok(0);
    }

    let _ = fs::create_dir_all(sources::cache());
    let lock = fs::File::create(cache_file("lock")).map_err(|e| e.to_string())?;
    let how = libc::LOCK_EX | if args.has("force") { 0 } else { libc::LOCK_NB };
    if unsafe { libc::flock(lock.as_raw_fd(), how) } != 0 {
        return Ok(0); // another draw is already under way
    }
    let mut state = State::load();
    if sources::held() && !args.has("force") && m.ts - state.ts < 14.0 * 60.0 && state.mode == m.mode {
        return Ok(0); // asked to hold still: the sky moves every quarter hour
    }
    let drift = state.drift_at(m.ts, &m);
    let mut sc = build(&m, Some(drift as f32));
    // never overwrite the frame that is on screen
    state.slot = if state.slot == "a" { "b".into() } else { "a".into() };
    let path = cache_file(&format!("frame-{}.png", state.slot)).to_string_lossy().into_owned();
    render_to(&path, &mut sc, &cache)?;
    let shown = sources::set_wallpaper(&path);
    state.ts = m.ts;
    state.drift = drift;
    state.mode = m.mode.clone();
    state.lit = sc.lit_share;
    if !m.bent {
        state.save();
    }
    if args.has("explain") {
        println!("{}", words::explain(&sc, m.view, &cache.stars));
    }
    match shown {
        Ok(()) => Ok(0),
        // with the living wallpaper running the picture is on screen anyway
        Err(_) if sources::live_pid() != 0 => Ok(0),
        Err(e) => Err(e),
    }
}

fn cmd_explain(args: &Args) -> Result<i32, String> {
    let m = moment(args)?;
    let cache = Cache::new(m.view, seed_of(args)?, 480, 300);
    let state = State::load();
    let drift = if m.bent { None } else { Some(state.drift_at(m.ts, &m) as f32) };
    let mut sc = build(&m, drift);
    sc.lit_share = cache.stage.lights(&sc).share;
    println!("{}", words::explain(&sc, m.view, &cache.stars));
    Ok(0)
}

fn cmd_sheet(args: &Args) -> Result<i32, String> {
    let (out, moments) = match args.pos.split_first() {
        Some((out, rest)) if !rest.is_empty() => (out, rest),
        _ => return Err("usage: dormer sheet out.png 'YYYY-MM-DD HH:MM[,weather][,light]' ...".into()),
    };
    let (w, h) = match args.get("size") {
        Some(s) => sources::parse_size(s).ok_or("--size looks like 640x400")?,
        None => (640, 400),
    };
    let seed = seed_of(args)?;
    let mut caches: Vec<Cache> = Vec::new();
    let cols = args.num::<usize>("cols")?.unwrap_or(if moments.len() > 4 { 3 } else { 2 }).max(1);
    let rows = (moments.len() + cols - 1) / cols;
    let mut sheet = vec![0xFF00_0000u32; cols * w * rows * h];
    for (k, spec) in moments.iter().enumerate() {
        let mut parts = spec.split(',').map(|p| p.trim());
        let when = parts.next().unwrap_or("");
        let mut m = real_moment(Some("dark"), false);
        (m.wx, _) = sources::read_weather(false);
        if let Some(id) = args.get("view") {
            m.view = views::find(id).ok_or(format!("--view is one of: {}", views::names()))?;
        }
        m.ts = tm::parse(when).ok_or(format!("cannot read the time '{}'", when))?;
        if let Some(v) = args.num::<u32>("agents")? {
            m.machine.agents = v;
        }
        if let Some(v) = args.num::<u32>("containers")? {
            m.machine.containers = v;
        }
        for p in parts {
            if let Some(w) = preset(p, &m.wx) {
                m.wx = w;
            } else if p == "dark" || p == "light" {
                m.mode = p.to_string();
            } else if let Some(v) = views::find(p) {
                m.view = v;
            } else if let Some((key, v)) = p.split_once('=').filter(|kv| m.view.knobs().iter().any(|k| k.key == kv.0)) {
                m.knobs.push((key.to_string(), v.parse().map_err(|_| format!("bad {} in '{}'", key, spec))?));
            } else if let Some(v) = p.strip_prefix("temp=") {
                m.wx.temp = v.parse().map_err(|_| format!("bad temperature in '{}'", spec))?;
            } else if let Some(v) = p.strip_prefix("wind=") {
                m.wx.wind = v.parse().map_err(|_| format!("bad wind in '{}'", spec))?;
            } else if let Some(v) = p.strip_prefix("wdir=") {
                m.wx.wdir = v.parse().map_err(|_| format!("bad wind direction in '{}'", spec))?;
            } else if let Some(v) = p.strip_prefix("humidity=") {
                m.wx.humidity = v.parse().map_err(|_| format!("bad humidity in '{}'", spec))?;
            } else if let Some(v) = p.strip_prefix("rain_in=") {
                m.wx.rain_in = Some(v.parse().map_err(|_| format!("bad rain_in in '{}'", spec))?);
            } else {
                return Err(format!("do not know '{}' in '{}'", p, spec));
            }
        }
        if !caches.iter().any(|c| c.view.id() == m.view.id()) {
            caches.push(Cache::new(m.view, seed, w, h));
        }
        let cache = caches.iter().find(|c| c.view.id() == m.view.id()).unwrap();
        let mut sc = build(&m, None);
        let (cv, _) = paint::render(&mut sc, cache);
        let px = cv.finish(sc.p.grain);
        let (ox, oy) = ((k % cols) * w, (k / cols) * h);
        for y in 0..h {
            sheet[(oy + y) * cols * w + ox..(oy + y) * cols * w + ox + w].copy_from_slice(&px[y * w..(y + 1) * w]);
        }
    }
    gfx::save_png(out, cols * w, rows * h, &sheet)?;
    println!("{}", out);
    Ok(0)
}

fn cmd_pretend(args: &Args) -> Result<i32, String> {
    let kinds: Vec<&str> = PRESETS.iter().map(|p| p.0).collect();
    let kind = args.pos.first().map(|s| s.as_str()).unwrap_or("");
    let path = cache_file("pretend.json");
    if kind == "off" {
        let _ = fs::remove_file(&path);
    } else if kinds.contains(&kind) {
        let minutes = args.num::<f64>("minutes")?.unwrap_or(10.0);
        sources::write_atomic(&path, &json!({"weather": kind, "until": tm::now() + minutes * 60.0}).to_string());
        println!("dormer: {} for {} minutes; `dormer pretend off` to stop early", kind, minutes);
    } else {
        return Err(format!("usage: dormer pretend {} | off", kinds.join(" | ")));
    }
    let forced = Args { flags: HashMap::from([("force".to_string(), "1".to_string())]), pos: Vec::new() };
    cmd_draw(&forced)
}

static STOP: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

extern "C" fn on_stop(_: libc::c_int) {
    STOP.store(true, std::sync::atomic::Ordering::SeqCst);
}

/// Play today from midnight to midnight on the wallpaper.
fn cmd_tour(args: &Args) -> Result<i32, String> {
    use std::sync::atomic::Ordering;
    if dev::taken() {
        return Err("the dev panel is open; close it first".into());
    }
    let seconds = args.num::<f64>("seconds")?.unwrap_or(70.0).clamp(5.0, 3600.0);
    unsafe {
        libc::signal(libc::SIGINT, on_stop as extern "C" fn(libc::c_int) as libc::sighandler_t);
        libc::signal(libc::SIGTERM, on_stop as extern "C" fn(libc::c_int) as libc::sighandler_t);
    }
    let path = cache_file("dev.json");
    let began = std::time::Instant::now();
    if sources::live_pid() != 0 {
        // the living wallpaper redraws the moment the time it is held to changes
        while !STOP.load(Ordering::SeqCst) {
            let f = began.elapsed().as_secs_f64() / seconds;
            if f >= 1.0 {
                break;
            }
            sources::write_atomic(&path, &json!({"pid": std::process::id(), "set": {"time": 24.0 * f}}).to_string());
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
        let _ = fs::remove_file(&path);
        return Ok(0);
    }
    // without it, as a run of still frames
    let (size, seed) = (size_of(args)?, seed_of(args)?);
    let mut m = moment(args)?;
    let cache = Cache::new(m.view, seed, size.0, size.1);
    let midnight = tm::midnight(m.ts);
    let frames = (seconds / 0.9).ceil() as usize;
    for k in 0..frames {
        if STOP.load(Ordering::SeqCst) {
            break;
        }
        let tick = std::time::Instant::now();
        m.ts = midnight + 86400.0 * k as f64 / frames as f64;
        let mut sc = build(&m, None);
        let path = cache_file(&format!("frame-{}.png", if k % 2 == 0 { "a" } else { "b" })).to_string_lossy().into_owned();
        render_to(&path, &mut sc, &cache)?;
        sources::set_wallpaper(&path)?;
        std::thread::sleep(std::time::Duration::from_secs_f64((0.9 - tick.elapsed().as_secs_f64()).max(0.0)));
    }
    let forced = Args { flags: HashMap::from([("force".to_string(), "1".to_string())]), pos: Vec::new() };
    cmd_draw(&forced)
}

fn cmd_hold(args: &Args) -> Result<i32, String> {
    let sig = match args.pos.first().map(|s| s.as_str()) {
        Some("on") => libc::SIGUSR1,
        Some("off") => libc::SIGUSR2,
        _ => return Err("usage: dormer hold on | off".into()),
    };
    let pid = sources::live_pid();
    if pid == 0 {
        println!("dormer: the living wallpaper is not running");
        return Ok(1);
    }
    unsafe { libc::kill(pid as libc::pid_t, sig) };
    Ok(0)
}

/// Say which views there are, or choose the one to look out on from now on.
fn cmd_view(args: &Args) -> Result<i32, String> {
    let Some(id) = args.pos.first() else {
        let now = sources::view().id();
        for v in views::ALL {
            println!("{} {:<9} {}", if v.id() == now { "▸" } else { " " }, v.id(), v.about());
        }
        return Ok(0);
    };
    let path = sources::state().join("view");
    if id == "default" {
        let _ = fs::remove_file(&path);
    } else if views::find(id).is_some() {
        sources::write_atomic(&path, id);
    } else {
        return Err(format!("usage: dormer view {} | default", views::names()));
    }
    if sources::live_pid() == 0 {
        let forced = Args { flags: HashMap::from([("force".to_string(), "1".to_string())]), pos: Vec::new() };
        return cmd_draw(&forced);
    }
    // the living wallpaper sees the choice change and fades over; the still copy follows
    let exe = launcher();
    let _ = std::process::Command::new(exe).args(["draw", "--force"]).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null()).spawn();
    Ok(0)
}

/// The command that runs this dormer: what units and the dev panel should call.
pub fn launcher() -> String {
    launcher_from(std::env::current_exe().ok(), sources::which("dormer"))
}

fn launcher_from(current: Option<std::path::PathBuf>, installed: Option<std::path::PathBuf>) -> String {
    current.or(installed).map(|p| p.to_string_lossy().into_owned()).unwrap_or_else(|| "dormer".into())
}

const UNITS: [(&str, &str); 3] = [
    (
        "dormer.service",
        "[Unit]
Description=dormer: redraw the wallpaper from the real sky, weather and day
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=oneshot
ExecStart={exe} draw
Nice=19
IOSchedulingClass=idle
",
    ),
    (
        "dormer.timer",
        "[Unit]
Description=dormer: move the sky on every five minutes
PartOf=graphical-session.target

[Timer]
OnCalendar=*:0/5
AccuracySec=15s

[Install]
WantedBy=graphical-session.target
",
    ),
    (
        "dormer-live.service",
        "[Unit]
Description=dormer: the living wallpaper
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart={exe} live
Restart=on-failure
RestartSec=2
LimitCORE=0
Nice=10

[Install]
WantedBy=graphical-session.target
",
    ),
];

fn unit_dir() -> std::path::PathBuf {
    match std::env::var("XDG_CONFIG_HOME") {
        Ok(v) if !v.is_empty() => std::path::PathBuf::from(v),
        _ => sources::home().join(".config"),
    }
    .join("systemd")
    .join("user")
}

fn systemctl(args: &[&str]) {
    let mut cmd = vec!["systemctl", "--user"];
    cmd.extend_from_slice(args);
    let _ = std::process::Command::new(cmd[0]).args(&cmd[1..]).status();
}

fn cmd_install(args: &Args) -> Result<i32, String> {
    let exe = launcher();
    let dir = unit_dir();
    fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    for (name, text) in UNITS {
        let path = dir.join(name);
        fs::write(&path, text.replace("{exe}", &exe)).map_err(|e| format!("cannot write {}: {}", path.display(), e))?;
        println!("wrote {}", path.display());
    }
    systemctl(&["daemon-reload"]);
    systemctl(&["enable", "--now", "dormer.timer"]);
    if args.has("no-live") {
        systemctl(&["disable", "--now", "dormer-live.service"]);
    } else {
        systemctl(&["enable", "dormer-live.service"]);
        systemctl(&["restart", "dormer-live.service"]);
    }
    let forced = Args { flags: HashMap::from([("force".to_string(), "1".to_string())]), pos: Vec::new() };
    cmd_draw(&forced)
}

fn cmd_uninstall() -> Result<i32, String> {
    systemctl(&["disable", "--now", "dormer.timer", "dormer-live.service"]);
    for (name, _) in UNITS {
        let _ = fs::remove_file(unit_dir().join(name));
    }
    systemctl(&["daemon-reload"]);
    println!("dormer: stopped. The last frame stays as your wallpaper until you set another.");
    Ok(0)
}

fn cmd_doctor() -> Result<i32, String> {
    let (wx, place) = sources::read_weather(true);
    let m = sources::read_machine();
    let (w, h) = sources::screen_size();
    let cfg = sources::config_path();
    let plural = |n: u32| if n == 1 { "" } else { "s" };
    let live = sources::live_pid();
    let mut rows = vec![
        ("version", VERSION.to_string()),
        ("config", format!("{}{}", cfg.display(), if config().found { "" } else { "  (not there: defaults in use)" })),
        ("place", format!("{}  {:.2}, {:.2}", if place.name.is_empty() { "unnamed" } else { &place.name }, place.lat, place.lon)),
        (
            "weather",
            if wx.known {
                format!("{}, {}°, wind {} km/h, humidity {}%  (source: {})", wx.kind, wx.temp.round(), wx.wind.round(), wx.humidity.round(), wx.source)
            } else {
                "no reading: a fair sky is assumed".to_string()
            },
        ),
        ("forecast", match wx.rain_in {
            Some(h) => format!("rain in about {:.1} h", h),
            None => "no rain in the next few hours".to_string(),
        }),
        ("mode", sources::theme_mode()),
        ("view", format!("{}  ({})", sources::view().id(), sources::view().about())),
        ("size", format!("{}x{}", w, h)),
        ("wallpaper", sources::backend()),
        ("living layer", if live != 0 { format!("running (pid {})", live) } else { "not running".to_string() }),
        ("wayland", if std::env::var("WAYLAND_DISPLAY").is_ok() { "found".to_string() } else { "no WAYLAND_DISPLAY: `dormer live` cannot run here".to_string() }),
        ("holding still", if sources::held() { "yes" } else { "no" }.to_string()),
        ("dev panel", if sources::overrides().is_empty() { "closed" } else { "open" }.to_string()),
        ("house", format!("{} agent session{}, {} container{}", m.agents, plural(m.agents), m.containers, plural(m.containers))),
        ("processor", format!("{}% busy", (m.load * 100.0).round())),
        ("sound", if m.audio { "playing" } else { "quiet" }.to_string()),
    ];
    if let Some(b) = m.battery {
        rows.push(("battery", format!("{}%, discharging", (b * 100.0).round())));
    }
    for (key, value) in rows {
        println!("{:<14} {}", key, value);
    }
    Ok(0)
}

/// Time how long the still picture takes to draw.
fn cmd_bench(args: &Args) -> Result<i32, String> {
    let m = moment(args)?;
    let (size, seed) = (size_of(args)?, seed_of(args)?);
    let t1 = std::time::Instant::now();
    let cache = Cache::new(m.view, seed, size.0, size.1);
    let t2 = std::time::Instant::now();
    let n = args.num::<usize>("n")?.unwrap_or(20);
    let (mut draw, mut finish) = (0.0, 0.0);
    for _ in 0..n {
        let mut sc = build(&m, None);
        let a = std::time::Instant::now();
        let (cv, _) = paint::render(&mut sc, &cache);
        let b = std::time::Instant::now();
        let px = cv.finish(sc.p.grain);
        std::hint::black_box(&px);
        draw += (b - a).as_secs_f64();
        finish += b.elapsed().as_secs_f64();
    }
    println!("stage   {:6.2} ms", (t2 - t1).as_secs_f64() * 1e3);
    println!("paint   {:6.2} ms", draw / n as f64 * 1e3);
    println!("finish  {:6.2} ms", finish / n as f64 * 1e3);
    Ok(0)
}

/// Draw thousands of random moments, still and moving, at several sizes: anything that can
/// go wrong in the painter should go wrong here rather than on someone's desktop.
fn cmd_soak(args: &Args) -> Result<i32, String> {
    let n = args.num::<usize>("n")?.unwrap_or(2000);
    let seed = seed_of(args)?;
    let sizes = [(1920, 1200), (640, 400), (1280, 720), (2560, 1080), (540, 960), (1000, 1000)];
    // every view at every size
    let caches: Vec<Cache> = views::ALL.iter().flat_map(|v| sizes.iter().map(move |s| Cache::new(*v, seed, s.0, s.1))).collect();
    let mut r = rng::Fast::new(args.num::<u64>("from")?.unwrap_or(1));
    let (_, place) = sources::read_weather(false);
    let year = tm::stamp(2026, 1, 1, 0, 0, 0);
    let mut shaper = gfx::Shaper::new();
    for k in 0..n {
        let mut wx = sky::from_wmo(0);
        let wild = |r: &mut rng::Fast| if r.chance(0.35) { 0.0 } else if r.chance(0.3) { 1.0 } else { r.f() as f64 };
        wx.cover = wild(&mut r);
        wx.rain = wild(&mut r);
        wx.snow = wild(&mut r);
        wx.fog = wild(&mut r);
        wx.storm = r.chance(0.2);
        wx.wind = r.range(0.0, 90.0) as f64;
        wx.wdir = r.range(0.0, 360.0) as f64;
        wx.temp = r.range(-25.0, 48.0) as f64;
        wx.humidity = r.range(0.0, 100.0) as f64;
        wx.rain_in = if r.chance(0.3) { Some(r.range(0.0, 4.0) as f64) } else { None };
        let cache = &caches[(k * 7) % caches.len()];
        let m = Moment {
            view: cache.view,
            knobs: cache.view.knobs().iter().filter_map(|k| if r.chance(0.5) { Some((k.key.to_string(), k.lo + (k.hi - k.lo) * r.f() as f64)) } else { None }).collect(),
            ts: year + r.f() as f64 * 366.0 * 86400.0,
            wx,
            place: Place { lat: if r.chance(0.7) { place.lat } else { r.range(-70.0, 70.0) as f64 }, lon: r.range(-180.0, 180.0) as f64, name: String::new() },
            machine: Machine { agents: r.below(5) as u32, containers: r.below(12) as u32, load: r.f(), battery: if r.chance(0.3) { Some(r.f()) } else { None }, audio: r.chance(0.5) },
            mode: if r.chance(0.5) { "dark".into() } else { "light".into() },
            rates: Rates { birds: r.range(0.0, 5.0), meteors: r.range(0.0, 12.0), planes: r.range(0.0, 6.0) },
            bent: true,
        };
        let mut sc = build(&m, Some(r.range(0.0, 500.0)));
        sc.live = r.chance(0.6);
        let (cv, anchors) = paint::render(&mut sc, cache);
        let mut px = cv.finish(sc.p.grain);
        let mut world = live::World::new(k as u64);
        world.retune(&sc, anchors.flicker.len());
        let src = live::Sources::from(&anchors, &cache.lay);
        world.spawn_flock(Some(r.range(-100.0, 1700.0)));
        if let Some(b) = src.perch {
            world.scatter(b);
        }
        for step in 0..60 {
            world.step(r.range(0.01, 0.12), &sc, &src, m.ts + step as f64 * 61.0);
            if step % 20 == 19 {
                let mut dirty = Vec::new();
                let mut frame = gfx::Frame { w: cache.w, h: cache.h, px: &mut px, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None, full_damage: false };
                let mut d = gfx::Draw::new(&mut frame, &mut shaper);
                world.draw(&mut d, &sc, cache, &anchors, m.ts + step as f64);
                cache.stage.lively(&sc, m.ts + step as f64 * 61.0);
                live::coalesce(dirty, cache.w as i32, cache.h as i32);
            }
        }
        words::explain(&sc, cache.view, &cache.stars);
        if (k + 1) % 250 == 0 {
            println!("{} moments drawn", k + 1);
        }
    }
    Ok(0)
}

fn main() {
    let mut argv: Vec<String> = std::env::args().skip(1).collect();
    if argv.first().map_or(false, |a| a == "--version" || a == "-V") {
        println!("dormer {}", VERSION);
        return;
    }
    if argv.is_empty() || (argv[0].starts_with('-') && argv[0] != "-h" && argv[0] != "--help") {
        argv.insert(0, "draw".into());
    }
    let cmd = argv.remove(0);
    let args = match Args::parse(&argv) {
        Ok(a) => a,
        Err(e) => {
            eprintln!("dormer: {}", e);
            std::process::exit(2);
        }
    };
    if args.has("help") || cmd == "-h" || cmd == "--help" || cmd == "help" {
        print!("{}", USAGE);
        return;
    }
    let done = match cmd.as_str() {
        "draw" => cmd_draw(&args),
        "explain" => cmd_explain(&args),
        "sheet" => cmd_sheet(&args),
        "tour" => cmd_tour(&args),
        "pretend" => cmd_pretend(&args),
        "hold" => cmd_hold(&args),
        "install" => cmd_install(&args),
        "uninstall" => cmd_uninstall(),
        "doctor" => cmd_doctor(),
        "bench" => cmd_bench(&args),
        "soak" => cmd_soak(&args),
        "view" => cmd_view(&args),
        "globe" => {
            // the earth's coastlines, flattened out, for looking them over
            let (w, h, px) = views::globe::chart();
            gfx::save_png(args.get("out").unwrap_or("globe.png"), w, h, &px).map(|_| 0)
        }
        "town" => {
            print!("{}", town::Town::build(seed_of(&args).unwrap_or(12)).fingerprint());
            Ok(0)
        }
        "live" => match args.get("frame") {
            Some(path) => moment(&args).and_then(|m| {
                let seconds = args.num::<f64>("seconds")?.unwrap_or(20.0);
                let point = |key: &str| -> Result<Option<gfx::P>, String> {
                    args.get(key).map(|s| interaction::parse_point(s).ok_or_else(|| format!("--{} needs two finite coordinates: X,Y", key))).transpose()
                };
                wl::one_frame(path, seconds, &m, size_of(&args)?, seed_of(&args)?, point("pointer")?, point("click")?).map(|_| 0)
            }),
            None => Ok(wl::run()),
        },
        "dev" => Ok(dev::run(args.has("here"), args.get("back").map(|s| s.to_string()))),
        other => Err(format!("no such command '{}'. Try `dormer --help`.", other)),
    };
    match done {
        Ok(code) => std::process::exit(code),
        Err(e) => {
            eprintln!("dormer: {}", e);
            std::process::exit(1);
        }
    }
}

#[cfg(test)]
mod cli_tests {
    #[test]
    fn panel_launches_the_running_build_before_an_older_installed_copy() {
        assert_eq!(super::launcher_from(Some("/tmp/dormer-new".into()), Some("/tmp/dormer-old".into())), "/tmp/dormer-new");
        assert_eq!(super::launcher_from(None, Some("/tmp/dormer-old".into())), "/tmp/dormer-old");
    }
}
