"""Where dormer learns things, and where it puts the picture.

Its own config, the weather, the theme mode, what this machine is doing, and
the wallpaper daemon that shows the result. Everything is optional: with no
config at all dormer works out as much as it can and says what it assumed
(`dormer doctor`).
"""
import json
import os
import shutil
import subprocess
import time
import tomllib
import urllib.request

from .almanac import PRESETS, from_wmo, preset
from .palette import rgb

HOME = os.path.expanduser("~")
CACHE = os.path.join(os.environ.get("XDG_CACHE_HOME", HOME + "/.cache"), "dormer")
CONFIG = os.path.join(os.environ.get("XDG_CONFIG_HOME", HOME + "/.config"), "dormer", "config.toml")
RUNTIME = os.environ.get("XDG_RUNTIME_DIR", "/tmp")
NOCTALIA_CACHE = os.path.join(os.environ.get("XDG_CACHE_HOME", HOME + "/.cache"), "noctalia")
SCENE = os.path.join(CACHE, "scene.json")

DEFAULTS = {
    "place": {"latitude": None, "longitude": None, "name": ""},
    "look": {"seed": 12, "size": "auto", "mode": "auto", "top_dark": "#1A2027", "top_light": "#ECEFEE"},
    "wallpaper": {"backend": "auto", "command": ""},
    "weather": {"source": "auto"},
    "house": {"lamp": ["claude", "codex"], "containers": True},
    "live": {"fps": 30},
    "power": {"hold_file": ""},
    "dev": {"terminal": ""},
}
GREENWICH = dict(lat=51.48, lon=0.0, name="Greenwich (no place set)")
_config = None


def config():
    """The config file laid over the defaults."""
    global _config
    if _config is None:
        _config = {section: dict(values) for section, values in DEFAULTS.items()}
        try:
            with open(CONFIG, "rb") as f:
                given = tomllib.load(f)
        except (OSError, tomllib.TOMLDecodeError):
            given = {}
        for section, values in given.items():
            if section in _config and isinstance(values, dict):
                _config[section].update(values)
    return _config


def run(cmd, timeout=6):
    try:
        return subprocess.run(cmd, capture_output=True, text=True, timeout=timeout)
    except (OSError, subprocess.TimeoutExpired):
        return None


# ── weather and place ────────────────────────────────────────────────────────

def _noctalia_weather():
    """(weather or None, place or None) from the Noctalia shell's own cache."""
    wx = place = None
    try:
        with open(os.path.join(NOCTALIA_CACHE, "weather.json")) as f:
            snap = json.load(f)["snapshot"]
        place = dict(lat=float(snap["latitude"]), lon=float(snap["longitude"]), name=snap.get("location_name", ""))
        cur = snap["current"]
        if snap.get("valid", True) and time.time() - float(snap.get("fetched_at", 0)) < 4 * 3600:
            wx = from_wmo(int(cur["weather_code"]))
            wx.update(temp=float(cur["temperature_c"]), wind=float(cur["wind_speed_kmh"]),
                      wdir=float(cur["wind_direction_deg"]), known=True, source="noctalia")
    except (OSError, ValueError, KeyError, TypeError):
        try:
            with open(os.path.join(NOCTALIA_CACHE, "location.json")) as f:
                loc = json.load(f)
            place = dict(lat=float(loc["latitude"]), lon=float(loc["longitude"]), name=loc.get("name", ""))
        except (OSError, ValueError, KeyError, TypeError):
            pass
    return wx, place


def _open_meteo(place):
    """Ask Open-Meteo directly, at most every twenty minutes. No key needed."""
    path = os.path.join(CACHE, "weather.json")
    try:
        with open(path) as f:
            kept = json.load(f)
    except (OSError, ValueError):
        kept = {}
    same_place = abs(kept.get("lat", 99) - place["lat"]) < 0.05 and abs(kept.get("lon", 999) - place["lon"]) < 0.05
    if not same_place or time.time() - kept.get("fetched", 0) > 20 * 60:
        url = ("https://api.open-meteo.com/v1/forecast?latitude=%.3f&longitude=%.3f"
               "&current=temperature_2m,weather_code,wind_speed_10m,wind_direction_10m" % (place["lat"], place["lon"]))
        try:
            with urllib.request.urlopen(url, timeout=6) as r:
                cur = json.load(r)["current"]
            kept = dict(lat=place["lat"], lon=place["lon"], fetched=time.time(), code=int(cur["weather_code"]),
                        temp=float(cur["temperature_2m"]), wind=float(cur["wind_speed_10m"]),
                        wdir=float(cur["wind_direction_10m"]))
            os.makedirs(CACHE, exist_ok=True)
            with open(path, "w") as f:
                json.dump(kept, f)
        except (OSError, ValueError, KeyError, TypeError):
            pass
    if not same_place and "code" not in kept or time.time() - kept.get("fetched", 0) > 4 * 3600 or "code" not in kept:
        return None
    wx = from_wmo(kept["code"])
    wx.update(temp=kept["temp"], wind=kept["wind"], wdir=kept["wdir"], known=True, source="open-meteo")
    return wx


def read_weather(pretend=True):
    """(weather, place). Weather is always usable; `known` says whether it was really measured."""
    cfg = config()
    source = cfg["weather"]["source"]
    wx = place = None
    if source in ("auto", "noctalia"):
        wx, place = _noctalia_weather()
    if cfg["place"]["latitude"] is not None and cfg["place"]["longitude"] is not None:
        place = dict(lat=float(cfg["place"]["latitude"]), lon=float(cfg["place"]["longitude"]), name=cfg["place"]["name"])
    if wx is None and place and source in ("auto", "open-meteo"):
        wx = _open_meteo(place)
    if wx is None:
        wx = from_wmo(1)
        wx.update(temp=18.0, wind=8.0, wdir=270.0, known=False, source="none")
    place = place or dict(GREENWICH)
    if pretend:
        try:                                     # `dormer pretend snow` holds for a few minutes
            with open(os.path.join(CACHE, "pretend.json")) as f:
                act = json.load(f)
            if act["until"] > time.time() and act["weather"] in PRESETS:
                wx = preset(act["weather"], wx)
                wx["pretend"] = True
                if act["weather"] == "snow":
                    wx["temp"] = min(wx["temp"], -1.0)
        except (OSError, ValueError, KeyError, TypeError):
            pass
    return wx, place


# ── this machine ─────────────────────────────────────────────────────────────

def read_machine():
    """(agent sessions, running containers): what the lamp and the blue windows show."""
    cfg = config()["house"]
    names, sessions = set(cfg["lamp"] or ()), set()
    if names:
        for pid in os.listdir("/proc"):
            if not pid.isdigit():
                continue
            try:
                with open("/proc/%s/stat" % pid) as f:
                    stat = f.read()
                comm = stat[stat.index("(") + 1:stat.rindex(")")]
                if comm in names:
                    sessions.add((comm, stat[stat.rindex(")") + 2:].split()[3]))
            except (OSError, ValueError, IndexError):
                continue
    boxes = 0
    if cfg["containers"]:
        try:
            boxes = sum(1 for d in os.listdir("/sys/fs/cgroup/system.slice")
                        if d.startswith("docker-") and d.endswith(".scope"))
        except OSError:
            pass
    return len(sessions), boxes


def theme_mode():
    """dark or light: the config, else the shell's theme, else the desktop's colour scheme."""
    mode = config()["look"]["mode"]
    if mode in ("dark", "light"):
        return mode
    mode = os.environ.get("NOCTALIA_THEME_MODE", "")
    if mode not in ("dark", "light") and shutil.which("noctalia"):
        done = run(["noctalia", "msg", "theme-mode-get"], 4)
        mode = done.stdout.strip() if done else ""
    if mode not in ("dark", "light") and shutil.which("gsettings"):
        done = run(["gsettings", "get", "org.gnome.desktop.interface", "color-scheme"], 3)
        if done and done.stdout.strip():
            mode = "light" if "light" in done.stdout or "default" in done.stdout else "dark"
    return mode if mode in ("dark", "light") else "dark"


def top_colour(mode):
    """The colour the sky fades into at the top: set it to your bar's colour."""
    try:
        return rgb(config()["look"]["top_" + mode])
    except (ValueError, KeyError, IndexError):
        return None


def screen_size():
    """Pixels to draw: the config, else the largest connected screen, else 1920x1200."""
    size = str(config()["look"]["size"]).lower()
    if "x" in size:
        try:
            w, h = (int(v) for v in size.split("x"))
            return w, h
        except ValueError:
            pass
    done = run(["hyprctl", "-j", "monitors"], 3) if shutil.which("hyprctl") else None
    try:
        best = max(json.loads(done.stdout), key=lambda m: m["width"] * m["height"])
        w, h = (best["height"], best["width"]) if best.get("transform", 0) % 2 else (best["width"], best["height"])
        return int(w), int(h)
    except (AttributeError, ValueError, KeyError, TypeError):
        return 1920, 1200


def held():
    """True when something else has asked for stillness (a battery saver, say)."""
    path = os.path.expandvars(os.path.expanduser(config()["power"]["hold_file"] or ""))
    return bool(path) and os.path.exists(path)


# ── the live layer and the dev panel ─────────────────────────────────────────

def _alive(pid, *needles):
    try:
        with open("/proc/%d/cmdline" % int(pid), "rb") as f:
            line = f.read()
        return all(n in line for n in needles)
    except (OSError, ValueError, TypeError):
        return False


def live_pid():
    """The pid of the running live layer, or 0."""
    try:
        with open(os.path.join(CACHE, "live.pid")) as f:
            pid = int(f.read().strip())
    except (OSError, ValueError):
        return 0
    return pid if _alive(pid, b"dormer", b"live") else 0


def dev_overrides():
    """What the dev panel is holding the scene to, while that panel is open."""
    path = os.path.join(CACHE, "dev.json")
    try:
        with open(path) as f:
            data = json.load(f)
    except (OSError, ValueError):
        return {}
    if not _alive(data.get("pid"), b"dormer"):
        try:
            os.remove(path)                      # the panel died without tidying up
        except OSError:
            pass
        return {}
    return dict(data.get("set") or {})


def tell_live(sc):
    """Leave the note the live layer reads: what kind of moment this is."""
    from .palette import mix
    note = dict(sc.out, ts=sc.ts, mode=sc.mode, el=round(sc.sun.el, 2), day=round(sc.day, 3), night=round(sc.night, 3),
                dark=round(sc.dark, 3), cover=sc.cover, rain=sc.rain, snow=sc.snow, fog=sc.fog, storm=sc.storm,
                wind=sc.wind, wind_x=round(sc.wind_x, 3), season=sc.season, busy=sc.busy, am=sc.am, rates=sc.rates,
                bird=list(mix(sc.P["bird"], sc.sky[3], 0.25)), rain_col=list(sc.P["rain"]), snow_col=list(sc.P["snow"]),
                star=list(sc.P["star"]), trail=list(mix(sc.P["trail"], sc.glow, 0.6 * sc.twilight)))
    os.makedirs(CACHE, exist_ok=True)
    with open(SCENE + ".tmp", "w") as f:
        json.dump(note, f)
    os.replace(SCENE + ".tmp", SCENE)


# ── putting the picture on the screen ────────────────────────────────────────

def backend():
    """Which wallpaper daemon to hand the picture to."""
    choice = config()["wallpaper"]["backend"]
    if choice != "auto":
        return choice
    if config()["wallpaper"]["command"]:
        return "command"
    running = lambda name: bool((run(["pgrep", "-x", name], 3) or subprocess.CompletedProcess((), 1)).returncode == 0)
    if shutil.which("noctalia") and running("noctalia"):
        return "noctalia"
    if shutil.which("swww") and running("swww-daemon"):
        return "swww"
    if shutil.which("hyprctl") and running("hyprpaper"):
        return "hyprpaper"
    return "none"


def set_wallpaper(path):
    """Show this file as the wallpaper. Returns an error string, or None when it worked."""
    kind = backend()
    if kind == "noctalia":
        cmd = [shutil.which("noctalia") or "noctalia", "msg", "wallpaper-set", path]
    elif kind == "swww":
        cmd = ["swww", "img", path, "--transition-type", "fade", "--transition-duration", "1"]
    elif kind == "hyprpaper":
        cmd = ["hyprctl", "hyprpaper", "reload", "," + path]
    elif kind == "command":
        cmd = ["sh", "-c", config()["wallpaper"]["command"].replace("{path}", "'%s'" % path)]
    else:
        return "no wallpaper daemon found (set [wallpaper] backend or command in %s)" % CONFIG
    done = run(cmd, 15)
    if done is None:
        return "%s did not answer" % kind
    if done.returncode != 0:
        return "%s refused the picture: %s" % (kind, (done.stderr or done.stdout).strip())
    return None
