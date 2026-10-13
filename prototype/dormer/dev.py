"""Development only: a panel of sliders that bends the scene by hand.

`dormer dev` opens a small floating terminal on an empty workspace, so the
wallpaper is in view, and redraws it as the sliders move: time of day, date
(and with it the weekday, the season and the moon), every part of the weather,
what the house shows, and how often the live layer's birds, planes and
shooting stars turn up.

Nothing here is permanent. The panel writes its overrides to dev.json in the
cache; `dormer draw` obeys them only while the panel is alive. Quit, and the
real sky comes back.
"""
import curses
import json
import os
import shutil
import subprocess
import threading
import time

from .sources import CACHE, config, read_machine, read_weather, run, theme_mode
from .words import explain

DEV = os.path.join(CACHE, "dev.json")
WINDS = ("N", "NE", "E", "SE", "S", "SW", "W", "NW")

# key, label, low, high, step, big step
ROWS = [
    ("", "when"),
    ("time", "time of day", 0.0, 24.0, 1 / 12, 1.0),
    ("day", "date", -183.0, 183.0, 1.0, 7.0),
    ("", "weather"),
    ("cover", "cloud", 0.0, 100.0, 5.0, 20.0),
    ("rain", "rain", 0.0, 100.0, 5.0, 20.0),
    ("snow", "snow", 0.0, 100.0, 5.0, 20.0),
    ("fog", "fog", 0.0, 100.0, 5.0, 20.0),
    ("storm", "thunderstorm", 0.0, 1.0, 1.0, 1.0),
    ("wind", "wind", 0.0, 70.0, 2.0, 10.0),
    ("wdir", "wind from", 0.0, 345.0, 15.0, 45.0),
    ("temp", "temperature", -12.0, 42.0, 1.0, 5.0),
    ("", "your house"),
    ("agents", "lamp: agents", 0.0, 4.0, 1.0, 1.0),
    ("containers", "blue windows", 0.0, 8.0, 1.0, 1.0),
    ("", "how often (live layer)"),
    ("birds", "birds", 0.0, 5.0, 0.25, 1.0),
    ("meteors", "shooting stars", 0.0, 12.0, 0.5, 2.0),
    ("planes", "planes", 0.0, 6.0, 0.25, 1.0),
]
SLIDERS = [i for i, r in enumerate(ROWS) if r[0]]


def show(key, v):
    if key == "time":
        v = min(v, 24 - 1 / 60)
        return "%02d:%02d" % (int(v), int(round((v % 1) * 60)) % 60)
    if key == "day":
        when = time.localtime(time.time() + v * 86400)
        return time.strftime("%a %d %b", when) + ("  today" if v == 0 else "  %+dd" % v)
    if key in ("cover", "rain", "snow", "fog"):
        return "%d%%" % v
    if key == "storm":
        return "on" if v else "off"
    if key == "wind":
        return "%d km/h" % v
    if key == "wdir":
        return "%s  %d°" % (WINDS[int((v + 22.5) % 360 // 45)], v)
    if key == "temp":
        return "%d°" % v
    if key in ("agents", "containers"):
        return "%d" % v
    return "off" if v == 0 else "×%g" % v


class Painter(threading.Thread):
    """Redraws in the background. While a slider is moving, only the newest position is drawn."""

    def __init__(self, exe):
        super().__init__(daemon=True)
        self.exe, self.changed, self.status = exe, threading.Event(), ""

    def run(self):
        while True:
            self.changed.wait()
            self.changed.clear()
            self.status = "drawing"
            began = time.time()
            subprocess.run([self.exe, "draw", "--force"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
            if not self.changed.is_set():
                self.status = "drawn in %.1fs" % (time.time() - began)


class Panel:
    def __init__(self, scr, exe):
        self.scr, self.set, self.at = scr, {}, SLIDERS[0]
        self.real, self.real_at, self.info = {}, 0.0, []
        self.painter = Painter(exe)
        self.painter.start()
        self.bar = (0, 0)                        # column where the slider bars start, and their width
        self.refresh_real()

    # ── values ───────────────────────────────────────────────────────────────

    def refresh_real(self):
        """What the sliders rest on when left alone: the real time, weather and machine."""
        now = time.localtime()
        wx, _place = read_weather(pretend=False)
        agents, boxes = read_machine()
        self.real = dict(time=now.tm_hour + now.tm_min / 60.0, day=0.0, cover=round(wx["cover"] * 20) * 5.0,
                         rain=round(wx["rain"] * 20) * 5.0, snow=round(wx["snow"] * 20) * 5.0,
                         fog=round(wx["fog"] * 20) * 5.0, storm=1.0 if wx["storm"] else 0.0,
                         wind=float(round(wx["wind"])), wdir=float(round(wx["wdir"] / 15) * 15 % 360),
                         temp=float(round(wx["temp"])), agents=float(min(4, agents)), containers=float(min(8, boxes)),
                         birds=1.0, meteors=1.0, planes=1.0)
        self.real_at = time.time()
        self.describe()

    def value(self, key):
        return self.set.get(key, self.real[key])

    def describe(self):
        """The first lines of `dormer explain` for the scene as the sliders have it."""
        from .cli import build, with_dev
        try:
            wx, place = read_weather(pretend=False)
            ts, wx, machine, rates = with_dev(self.set, time.time(), wx, list(read_machine()))
            self.info = explain(build(ts, wx, theme_mode(), machine, place, rates=rates)).split("\n")[:4]
        except Exception as err:                 # a dev tool should show the fault, not die of it
            self.info = ["could not work out the scene: %s" % err]

    def change(self, key, v):
        row = next(r for r in ROWS if r[0] == key)
        lo, hi, step = row[2], row[3], row[4]
        v = max(lo, min(hi, round(v / step) * step))
        if abs(v - self.real[key]) < step / 2 and key not in ("time",):
            self.set.pop(key, None)
        else:
            self.set[key] = round(v, 4)
        self.commit()

    def commit(self):
        os.makedirs(CACHE, exist_ok=True)
        with open(DEV + ".tmp", "w") as f:
            json.dump({"pid": os.getpid(), "set": self.set}, f)
        os.replace(DEV + ".tmp", DEV)
        self.describe()
        self.painter.changed.set()

    # ── drawing ──────────────────────────────────────────────────────────────

    def paint(self):
        scr = self.scr
        scr.erase()
        rows, cols = scr.getmaxyx()
        accent, dim, bold = curses.color_pair(1), curses.color_pair(2), curses.A_BOLD

        def put(y, x, text, attr=0):
            if 0 <= y < rows - 1 and x < cols - 1:
                try:
                    scr.addstr(y, x, text[:max(0, cols - 1 - x)], attr)
                except curses.error:
                    pass
        put(0, 2, "dormer", accent | bold)
        put(0, 9, "dev panel", dim)
        state = self.painter.status
        put(0, max(20, cols - 2 - len(state)), state, accent if state == "drawing" else dim)
        for i, line in enumerate(self.info[:4]):
            put(1 + i, 2, line, bold if i == 0 else dim)
        label_w, value_w = 17, 18
        bar_x, bar_w = 2 + label_w, max(8, cols - 2 - label_w - value_w - 3)
        self.bar = (bar_x, bar_w)
        y = 6
        self.row_y = {}
        roomy = rows >= 6 + len(ROWS) + 3 + 3     # space for a blank line between groups?
        for i, r in enumerate(ROWS):
            if not r[0]:
                y += 1 if i and roomy else 0
                put(y, 2, r[1], dim)
                put(y, 3 + len(r[1]), "─" * max(0, cols - 6 - len(r[1])), dim)
                y += 1
                continue
            key, label, lo, hi = r[0], r[1], r[2], r[3]
            v, bent, chosen = self.value(key), key in self.set, i == self.at
            self.row_y[y] = i
            put(y, 0, "▸" if chosen else " ", accent | bold)
            put(y, 2, label, (bold if chosen else 0) | (0 if bent or chosen else dim))
            knob = int(round((v - lo) / (hi - lo) * (bar_w - 1)))
            put(y, bar_x, "━" * knob, accent if bent else dim)
            put(y, bar_x + knob + 1, "─" * (bar_w - knob - 1), dim)
            if bent:                             # a tick where reality is
                real = int(round((self.real[key] - lo) / (hi - lo) * (bar_w - 1)))
                if real != knob:
                    put(y, bar_x + real, "┆", dim)
            put(y, bar_x + knob, "●", (accent if bent else 0) | bold)
            put(y, bar_x + bar_w + 2, show(key, v), (accent if bent else dim) | (bold if chosen else 0))
            y += 1
        put(rows - 3, 2, "↑↓ choose   ←→ adjust   PgUp PgDn big steps   click or drag", dim)
        put(rows - 2, 2, "r this one real   a all real   n now   q quit and restore", dim)
        scr.refresh()

    # ── input ────────────────────────────────────────────────────────────────

    def key(self, ch):
        key, lo, hi, step, big = ROWS[self.at][0], *ROWS[self.at][2:6]
        v = self.value(key)
        if ch in (curses.KEY_UP, ord("k")):
            self.at = SLIDERS[(SLIDERS.index(self.at) - 1) % len(SLIDERS)]
        elif ch in (curses.KEY_DOWN, ord("j"), ord("\t")):
            self.at = SLIDERS[(SLIDERS.index(self.at) + 1) % len(SLIDERS)]
        elif ch in (curses.KEY_LEFT, ord("h")):
            self.change(key, v - step)
        elif ch in (curses.KEY_RIGHT, ord("l")):
            self.change(key, v + step)
        elif ch in (curses.KEY_SLEFT, curses.KEY_NPAGE, ord("["), ord("H")):
            self.change(key, v - big)
        elif ch in (curses.KEY_SRIGHT, curses.KEY_PPAGE, ord("]"), ord("L")):
            self.change(key, v + big)
        elif ch == curses.KEY_HOME:
            self.change(key, lo)
        elif ch == curses.KEY_END:
            self.change(key, hi)
        elif ch in (ord("r"), curses.KEY_BACKSPACE, 127):
            self.set.pop(key, None)
            self.commit()
        elif ch == ord("n"):
            self.set.pop("time", None)
            self.set.pop("day", None)
            self.commit()
        elif ch in (ord("a"), ord("0")):
            self.set = {}
            self.commit()
        elif ch == curses.KEY_MOUSE:
            try:
                _id, x, y, _z, _state = curses.getmouse()
            except curses.error:
                return True
            if y in self.row_y:
                self.at = self.row_y[y]
                bar_x, bar_w = self.bar
                if bar_x - 1 <= x <= bar_x + bar_w:
                    r = ROWS[self.at]
                    f = max(0.0, min(1.0, (x - bar_x) / float(bar_w - 1)))
                    self.change(r[0], r[2] + f * (r[3] - r[2]))
        elif ch == ord("q"):
            return False
        return True

    def loop(self):
        self.scr.timeout(200)
        while True:
            if time.time() - self.real_at > 20:
                self.refresh_real()              # the real clock and weather move on underneath
            self.paint()
            ch = self.scr.getch()
            if ch != -1 and not self.key(ch):
                return


def panel(scr, exe):
    curses.curs_set(0)
    curses.use_default_colors()
    curses.init_pair(1, curses.COLOR_YELLOW, -1)
    curses.init_pair(2, 8 if curses.COLORS >= 16 else curses.COLOR_WHITE, -1)
    curses.mousemask(curses.ALL_MOUSE_EVENTS | curses.REPORT_MOUSE_POSITION)
    curses.mouseinterval(0)
    print("\033[?1002h", end="", flush=True)     # report the mouse while a button is held, so bars can be dragged
    try:
        Panel(scr, exe).loop()
    finally:
        print("\033[?1002l", end="", flush=True)


def hyprland(lua, *classic):
    """Ask Hyprland for something: the Lua form first (0.56+), then the old dispatcher."""
    if not shutil.which("hyprctl"):
        return
    done = run(["hyprctl", "dispatch", lua], 3)
    if (not done or "ok" not in done.stdout) and classic:
        run(["hyprctl", "dispatch", *classic], 3)


def terminal(exe, back):
    """The command that opens the panel in a small terminal of its own."""
    inner = [exe, "dev", "--here"] + (["--back", str(back)] if back else [])
    name, title = "dev.dormer.panel", "dormer dev"
    known = {
        "ghostty": ["ghostty", "--class=" + name, "--title=" + title, "--window-width=66", "--window-height=33",
                    "--font-size=10", "--confirm-close-surface=false", "-e"],
        "kitty": ["kitty", "--class", name, "--title", title, "-o", "initial_window_width=66c",
                  "-o", "initial_window_height=33c"],
        "foot": ["foot", "--app-id", name, "--title", title, "--window-size-chars=66x33"],
        "alacritty": ["alacritty", "--class", name, "--title", title, "-o", "window.dimensions.columns=66",
                      "-o", "window.dimensions.lines=33", "-e"],
        "wezterm": ["wezterm", "start", "--class", name, "--"],
    }
    wanted = config()["dev"]["terminal"] or os.path.basename(os.environ.get("TERMINAL", ""))
    for term in [wanted] + list(known):
        if term in known and shutil.which(term):
            return known[term] + inner
    return None


def main(args):
    from .cli import launcher
    exe = launcher()
    if not args.here:
        back = None
        done = run(["hyprctl", "-j", "activeworkspace"], 3) if shutil.which("hyprctl") else None
        try:
            back = json.loads(done.stdout)["id"]
        except (AttributeError, ValueError, KeyError):
            pass
        cmd = terminal(exe, back)
        if not cmd:
            print("dormer: no terminal I know how to open; run `dormer dev --here` in one")
            return 1
        if back is not None:                     # go where the wallpaper can be seen
            hyprland('hl.dsp.focus({ workspace = "emptym" })', "workspace", "emptym")
        subprocess.Popen(cmd, start_new_session=True, stdin=subprocess.DEVNULL, stdout=subprocess.DEVNULL,
                         stderr=subprocess.DEVNULL)
        return 0
    try:
        curses.wrapper(panel, exe)
    except KeyboardInterrupt:
        pass
    finally:
        try:
            os.remove(DEV)
        except OSError:
            pass
        print("putting the real sky back…")
        subprocess.run([exe, "draw", "--force"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if args.back:
            hyprland('hl.dsp.focus({ workspace = "%s" })' % args.back, "workspace", str(args.back))
    return 0
