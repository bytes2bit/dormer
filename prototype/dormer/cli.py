"""dormer — a window in the roof: the same rooftops under the real sky.

    dormer                      draw the wallpaper for this moment
    dormer explain              say what the picture is showing, and why
    dormer tour                 play the whole of today in two minutes
    dormer pretend snow         borrow some weather for ten minutes
    dormer live                 run the moving layer (birds, rain, shooting stars)
    dormer dev                  open the slider panel and bend the scene by hand
    dormer install              start on login, redraw every five minutes
    dormer doctor               show what dormer found on this machine

`dormer <command> --help` lists the options of each.
"""
import argparse
import fcntl
import json
import os
import shutil
import signal
import subprocess
import sys
import time

from . import __version__
from .almanac import PRESETS, describe, preset
from .sources import (CACHE, CONFIG, backend, config, dev_overrides, held, live_pid, read_machine, read_weather,
                      screen_size, set_wallpaper, tell_live, theme_mode, top_colour)

UNITS = {
    "dormer.service": """[Unit]
Description=dormer: redraw the wallpaper from the real sky, weather and day
PartOf=graphical-session.target
After=graphical-session.target

[Service]
Type=oneshot
ExecStart={exe} draw
Nice=19
IOSchedulingClass=idle
""",
    "dormer.timer": """[Unit]
Description=dormer: move the sky on every five minutes
PartOf=graphical-session.target

[Timer]
OnCalendar=*:0/5
AccuracySec=15s

[Install]
WantedBy=graphical-session.target
""",
    "dormer-live.service": """[Unit]
Description=dormer: the moving layer of the wallpaper
PartOf=graphical-session.target
After=graphical-session.target

[Service]
ExecStart={exe} live
Restart=on-failure
RestartSec=5
Nice=10

[Install]
WantedBy=graphical-session.target
""",
}


def parse_when(text):
    for fmt in ("%Y-%m-%d %H:%M", "%Y-%m-%dT%H:%M", "%H:%M"):
        try:
            tm = time.strptime(text.strip(), fmt)
        except ValueError:
            continue
        if fmt == "%H:%M":
            now = time.localtime()
            tm = time.struct_time((now.tm_year, now.tm_mon, now.tm_mday, tm.tm_hour, tm.tm_min, 0, 0, 0, -1))
        return time.mktime(tm[:8] + (-1,))
    raise SystemExit("dormer: cannot read the time %r (use 'YYYY-MM-DD HH:MM' or 'HH:MM')" % text)


def with_dev(dev, ts, wx, machine):
    """Lay the dev panel's sliders over reality. Returns (ts, weather, machine, live rates)."""
    if not dev:
        return ts, wx, machine, {}
    if "day" in dev:
        ts += float(dev["day"]) * 86400
    if "time" in dev:
        lt = time.localtime(ts)
        ts = time.mktime((lt.tm_year, lt.tm_mon, lt.tm_mday, 0, 0, 0, 0, 0, -1)) + float(dev["time"]) * 3600
    if any(k in dev for k in ("cover", "rain", "snow", "fog", "storm", "wind", "wdir", "temp")):
        wx = dict(wx, dev=True, known=True)
        for k in ("cover", "rain", "snow", "fog"):
            if k in dev:
                wx[k] = float(dev[k]) / 100.0
        if "storm" in dev:
            wx["storm"] = bool(dev["storm"])
        if wx["storm"]:
            wx["rain"] = max(wx["rain"], 0.6)
        wet = max(wx["rain"], wx["snow"])
        if "cover" not in dev and wet > 0:
            wx["cover"] = max(wx["cover"], 0.75 + 0.25 * wet)       # rain needs a cloud to fall from
        for k in ("wind", "wdir", "temp"):
            if k in dev:
                wx[k] = float(dev[k])
        wx["kind"] = describe(wx)
    machine = [int(dev.get("agents", machine[0])), int(dev.get("containers", machine[1]))]
    return ts, wx, machine, {k: float(dev[k]) for k in ("birds", "meteors", "planes") if k in dev}


def moment(args, dev=True):
    """Everything a scene is built from, after flags and the dev panel have had their say."""
    wx, place = read_weather()
    machine = list(read_machine())
    ts, rates = time.time(), {}
    if getattr(args, "at", None):
        ts = parse_when(args.at)
    elif dev:
        ts, wx, machine, rates = with_dev(dev_overrides(), ts, wx, machine)
    if getattr(args, "weather", None):
        wx = preset(args.weather, wx)
    if getattr(args, "temp", None) is not None:
        wx["temp"] = args.temp
    if getattr(args, "wind", None) is not None:
        wx["wind"] = args.wind
    if getattr(args, "agents", None) is not None:
        machine[0] = args.agents
    if getattr(args, "containers", None) is not None:
        machine[1] = args.containers
    mode = getattr(args, "mode", None) or theme_mode()
    return ts, wx, place, machine, mode, rates


def size_of(args):
    if getattr(args, "size", None):
        w, h = (int(v) for v in args.size.lower().split("x"))
        return w, h
    return screen_size()


def seed_of(args):
    return args.seed if getattr(args, "seed", None) is not None else int(config()["look"]["seed"])


def build(ts, wx, mode, machine, place, drift=None, rates=None):
    from .scene import scene
    return scene(ts, wx, mode, machine, place, drift, top_colour(mode), rates)


def load_state():
    try:
        with open(os.path.join(CACHE, "state.json")) as f:
            return json.load(f)
    except (OSError, ValueError):
        return {}


def publish(sc, size, seed, city, state):
    """Draw this scene, hand it to the wallpaper daemon, and tell the live layer what it is looking at."""
    from .paint import render
    state["slot"] = slot = "b" if state.get("slot") == "a" else "a"   # never overwrite the frame on screen
    path = os.path.join(CACHE, "frame-%s.png" % slot)
    sc.live = bool(live_pid())
    render(sc, size[0], size[1], seed, city).save(path, compress_level=1)
    problem = set_wallpaper(path)
    if problem:
        sys.stderr.write("dormer: %s\n" % problem)
        return False
    tell_live(sc)
    return True


# ── commands ─────────────────────────────────────────────────────────────────

def cmd_draw(args):
    from .city import build_city
    from .words import explain
    ts, wx, place, machine, mode, rates = moment(args)
    size, seed = size_of(args), seed_of(args)
    if args.out:
        from .paint import render
        sc = build(ts, wx, mode, machine, place, rates=rates)
        render(sc, size[0], size[1], seed).save(args.out)
        if args.explain:
            print(explain(sc))
        return 0

    os.makedirs(CACHE, exist_ok=True)
    lock = open(os.path.join(CACHE, "lock"), "w")
    try:
        fcntl.flock(lock, fcntl.LOCK_EX | (0 if args.force or args.tour else fcntl.LOCK_NB))
    except OSError:
        return 0                                 # another draw is already under way
    state = load_state()
    if held() and not (args.force or args.tour) and ts - state.get("ts", 0) < 14 * 60 and state.get("mode") == mode:
        return 0                                 # asked to hold still: the sky moves every quarter hour
    city = build_city(seed)

    if args.tour:                                # a whole day in a couple of minutes, then back to now
        lt = time.localtime(ts)
        midnight = time.mktime((lt.tm_year, lt.tm_mon, lt.tm_mday, 0, 0, 0, 0, 0, -1))
        try:
            for minute in range(0, 24 * 60, args.step):
                began = time.time()
                publish(build(midnight + minute * 60, wx, mode, machine, place, rates=rates), size, seed, city, state)
                time.sleep(max(0.0, 0.9 - (time.time() - began)))
        except KeyboardInterrupt:
            pass
        ts = time.time()
    # Clouds keep their place between frames and move on with the wind that is blowing now.
    gap = min(1800.0, max(0.0, ts - state.get("ts", ts)))
    probe = build(ts, wx, mode, machine, place, 0.0)
    drift = (state.get("drift", 0.0) + gap / 3600.0 * probe.wind_x * 0.19) % 1.7
    sc = build(ts, wx, mode, machine, place, drift, rates)
    if not publish(sc, size, seed, city, state):
        return 1
    state.update(ts=ts, drift=drift, mode=mode, lit=sc.lit_share)
    with open(os.path.join(CACHE, "state.json.tmp"), "w") as f:
        json.dump(state, f)
    os.replace(os.path.join(CACHE, "state.json.tmp"), os.path.join(CACHE, "state.json"))
    if args.explain:
        print(explain(sc))
    return 0


def cmd_explain(args):
    from .words import explain
    ts, wx, place, machine, mode, rates = moment(args)
    state = load_state()
    sc = build(ts, wx, mode, machine, place, None if args.at else state.get("drift"), rates)
    if args.at or args.weather:
        from .paint import render                # the share of lit windows is only known once drawn
        render(sc, 480, 300, seed_of(args))
    else:
        sc.lit_share = state.get("lit", 0.0)
    print(explain(sc))
    return 0


def cmd_tour(args):
    args.force, args.tour, args.out, args.explain = True, True, None, False
    return cmd_draw(args)


def cmd_pretend(args):
    path = os.path.join(CACHE, "pretend.json")
    os.makedirs(CACHE, exist_ok=True)
    if args.kind == "off":
        if os.path.exists(path):
            os.remove(path)
    else:
        with open(path, "w") as f:
            json.dump({"weather": args.kind, "until": time.time() + args.minutes * 60}, f)
        print("dormer: %s for %g minutes; `dormer pretend off` to stop early" % (args.kind, args.minutes))
    return subprocess.call([launcher(), "draw", "--force"])


def cmd_sheet(args):
    from PIL import Image, ImageDraw, ImageFont
    from .city import build_city
    from .paint import render
    from .words import DAYS, clock
    wx, place = read_weather(pretend=False)
    machine, seed = list(read_machine()), seed_of(args)
    if args.agents is not None:
        machine[0] = args.agents
    if args.containers is not None:
        machine[1] = args.containers
    w, h = (int(v) for v in (args.size or "640x400").lower().split("x"))
    cols = 3 if len(args.moments) > 4 else 2
    rows = (len(args.moments) + cols - 1) // cols
    out = Image.new("RGB", (cols * w, rows * h), (0, 0, 0))
    try:
        font = ImageFont.load_default(size=max(12, h // 18))
    except (OSError, TypeError):
        font = ImageFont.load_default()
    city = build_city(seed)
    for k, spec in enumerate(args.moments):
        parts = [p.strip() for p in spec.split(",")]
        ts, this, mode = parse_when(parts[0]), dict(wx), "dark"
        for p in parts[1:]:
            if p in PRESETS:
                this = preset(p, this)
            elif p in ("dark", "light"):
                mode = p
            elif p.startswith("temp="):
                this["temp"] = float(p[5:])
            elif p.startswith("wind="):
                this["wind"] = float(p[5:])
        sc = build(ts, this, mode, machine, place)
        out.paste(render(sc, w, h, seed, city), ((k % cols) * w, (k // cols) * h))
        label = "%s %s  %s" % (DAYS[sc.dow][:3], clock(ts), this["kind"])
        ImageDraw.Draw(out).text(((k % cols) * w + 10, (k // cols) * h + 6), label,
                                 fill=(230, 223, 207) if mode == "dark" else (40, 50, 60), font=font)
    out.save(args.out)
    print(args.out)
    return 0


def cmd_live(args):
    from . import live
    return live.main(args)


def cmd_dev(args):
    from . import dev
    return dev.main(args)


def cmd_hold(args):
    pid = live_pid()
    if not pid:
        print("dormer: the live layer is not running")
        return 1
    os.kill(pid, signal.SIGUSR1 if args.state == "on" else signal.SIGUSR2)
    return 0


def launcher():
    """The command that runs this dormer: what units and the dev panel should call."""
    here = os.path.abspath(sys.argv[0])
    if os.path.basename(here) == "dormer" and os.access(here, os.X_OK):
        return shutil.which("dormer") or here
    return shutil.which("dormer") or here


def cmd_install(args):
    exe = launcher()
    home = os.path.join(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")), "systemd", "user")
    os.makedirs(home, exist_ok=True)
    for name, text in UNITS.items():
        with open(os.path.join(home, name), "w") as f:
            f.write(text.format(exe=exe))
        print("wrote", os.path.join(home, name))
    subprocess.call(["systemctl", "--user", "daemon-reload"])
    subprocess.call(["systemctl", "--user", "enable", "--now", "dormer.timer"])
    if not args.no_live:
        subprocess.call(["systemctl", "--user", "enable", "--now", "dormer-live.service"])
    return subprocess.call([exe, "draw", "--force"])


def cmd_uninstall(args):
    home = os.path.join(os.environ.get("XDG_CONFIG_HOME", os.path.expanduser("~/.config")), "systemd", "user")
    subprocess.call(["systemctl", "--user", "disable", "--now", "dormer.timer", "dormer-live.service"])
    for name in UNITS:
        try:
            os.remove(os.path.join(home, name))
        except OSError:
            pass
    subprocess.call(["systemctl", "--user", "daemon-reload"])
    print("dormer: stopped. The last frame stays as your wallpaper until you set another.")
    return 0


def cmd_doctor(args):
    wx, place = read_weather()
    agents, boxes = read_machine()
    w, h = screen_size()
    rows = [("version", __version__),
            ("config", CONFIG + ("" if os.path.exists(CONFIG) else "  (not there: defaults in use)")),
            ("place", "%s  %.2f, %.2f" % (place.get("name") or "unnamed", place["lat"], place["lon"])),
            ("weather", "%s, %d°, wind %d km/h  (source: %s)" % (wx["kind"], round(wx["temp"]), round(wx["wind"]), wx.get("source", "none"))
             if wx.get("known") else "no reading: a fair sky is assumed"),
            ("mode", theme_mode()),
            ("size", "%dx%d" % (w, h)),
            ("wallpaper", backend()),
            ("live layer", "running (pid %d)" % live_pid() if live_pid() else "not running"),
            ("holding still", "yes" if held() else "no"),
            ("dev panel", "open" if dev_overrides() else "closed"),
            ("house", "%d agent session%s, %d container%s" % (agents, "" if agents == 1 else "s", boxes, "" if boxes == 1 else "s"))]
    try:
        import PIL
        rows.append(("Pillow", PIL.__version__))
    except ImportError:
        rows.append(("Pillow", "MISSING: nothing can be drawn"))
    try:
        import gi
        gi.require_version("Gtk", "4.0")
        gi.require_version("Gtk4LayerShell", "1.0")
        rows.append(("GTK layer shell", "found"))
    except (ImportError, ValueError):
        rows.append(("GTK layer shell", "missing: `dormer live` will not run (the still wallpaper still works)"))
    for key, value in rows:
        print("%-14s %s" % (key, value))
    return 0


def main(argv=None):
    argv = list(sys.argv[1:] if argv is None else argv)
    if not argv or (argv[0].startswith("-") and argv[0] not in ("-h", "--help", "--version")):
        argv.insert(0, "draw")
    ap = argparse.ArgumentParser(prog="dormer", description=__doc__, formatter_class=argparse.RawDescriptionHelpFormatter)
    ap.add_argument("--version", action="version", version="dormer " + __version__)
    sub = ap.add_subparsers(dest="cmd", metavar="command")

    def moment_flags(p):
        p.add_argument("--at", help="another moment: 'YYYY-MM-DD HH:MM' or 'HH:MM' today")
        p.add_argument("--weather", choices=sorted(PRESETS), help="pretend the weather for this one picture")
        p.add_argument("--temp", type=float, help="°C")
        p.add_argument("--wind", type=float, help="km/h")
        p.add_argument("--mode", choices=("dark", "light"))
        p.add_argument("--agents", type=int, help="pretend this many agent sessions")
        p.add_argument("--containers", type=int, help="pretend this many running containers")
        p.add_argument("--size", help="pixels, e.g. 2560x1600 (default: your largest screen)")
        p.add_argument("--seed", type=int, help="a different number is a different city")

    p = sub.add_parser("draw", help="draw the wallpaper for this moment (the default)")
    moment_flags(p)
    p.add_argument("--out", help="write the picture here instead of setting the wallpaper")
    p.add_argument("--force", action="store_true", help="draw even when asked to hold still")
    p.add_argument("--explain", action="store_true", help="also describe the scene in words")
    p.set_defaults(run=cmd_draw, tour=False, step=12)

    p = sub.add_parser("explain", help="say what the picture is showing, and why")
    moment_flags(p)
    p.set_defaults(run=cmd_explain)

    p = sub.add_parser("tour", help="play today from midnight to midnight on the wallpaper")
    moment_flags(p)
    p.add_argument("--step", type=int, default=12, help="minutes between frames (default 12)")
    p.set_defaults(run=cmd_tour)

    p = sub.add_parser("pretend", help="show some other weather on the real wallpaper for a while")
    p.add_argument("kind", choices=sorted(PRESETS) + ["off"])
    p.add_argument("--minutes", type=float, default=10)
    p.set_defaults(run=cmd_pretend)

    p = sub.add_parser("sheet", help="a contact sheet of several moments, for looking at changes")
    p.add_argument("out")
    p.add_argument("moments", nargs="+", help="'YYYY-MM-DD HH:MM[,weather][,light]'")
    p.add_argument("--size", help="size of each tile (default 640x400)")
    p.add_argument("--seed", type=int)
    p.add_argument("--agents", type=int, help="pretend this many agent sessions")
    p.add_argument("--containers", type=int, help="pretend this many running containers")
    p.set_defaults(run=cmd_sheet)

    p = sub.add_parser("live", help="run the moving layer: birds, rain, shooting stars")
    p.add_argument("--frame", help="draw one frame to this PNG and exit")
    p.add_argument("--seconds", type=float, default=20.0, help="how long the world runs before that frame")
    p.add_argument("--scene", help="read this scene note instead of the current one")
    p.set_defaults(run=cmd_live)

    p = sub.add_parser("dev", help="development only: a slider panel that bends the scene by hand")
    p.add_argument("--here", action="store_true", help="run the panel in this terminal instead of opening one")
    p.add_argument("--back", help=argparse.SUPPRESS)
    p.set_defaults(run=cmd_dev)

    p = sub.add_parser("hold", help="freeze or release the moving layer (a lock screen would call this)")
    p.add_argument("state", choices=("on", "off"))
    p.set_defaults(run=cmd_hold)

    p = sub.add_parser("install", help="start on login and redraw every five minutes (systemd user units)")
    p.add_argument("--no-live", action="store_true", help="still frames only, no moving layer")
    p.set_defaults(run=cmd_install)
    sub.add_parser("uninstall", help="stop and remove those units").set_defaults(run=cmd_uninstall)
    sub.add_parser("doctor", help="show what dormer found on this machine").set_defaults(run=cmd_doctor)

    args = ap.parse_args(argv)
    if not getattr(args, "run", None):
        ap.print_help()
        return 0
    return args.run(args) or 0
