"""The part of the wallpaper that moves.

A transparent layer between the wallpaper and the desktop. `dormer draw`
paints the still frame every few minutes and leaves a note (scene.json) about
what kind of moment it is; this layer adds what a still cannot show:

  by day     birds crossing, planes drawing their trails, leaves on the wind
  by night   stars that twinkle, shooting stars, a blinking crane light,
             televisions flickering behind a few windows, fireflies in summer
  weather    rain and snow that fall, lightning, smoke that leans with the wind
  yours      the lamp in your roof room breathes while an agent is working

It animates only while the wallpaper can be seen. With a tiled or fullscreen
window on the workspace (Hyprland), after `dormer hold on`, or while the
configured hold file exists, it stands still and costs nothing.
"""
import ctypes

ctypes.CDLL("libgtk4-layer-shell.so")       # must be loaded before GTK touches Wayland

import json
import math
import os
import random
import signal
import socket

import cairo
import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
gi.require_version("Gsk", "4.0")
gi.require_version("Graphene", "1.0")
gi.require_version("Gtk4LayerShell", "1.0")
gi.require_foreign("cairo")
from gi.repository import Gdk, Gio, GLib, Graphene, Gsk, Gtk
from gi.repository import Gtk4LayerShell as Shell

from .sources import CACHE, RUNTIME, SCENE, config, held

VW, VH = 1600.0, 1000.0          # everything is placed on this canvas and scaled to the screen
FPS = max(5, min(60, int(os.environ.get("DORMER_FPS") or config()["live"]["fps"])))
WALLPAPERS = ("noctalia-wallpaper", "hyprpaper", "swww-daemon", "wallpaper")   # layers this one must stay above

AMBER, BEACON, GLOWWORM = (224, 169, 81), (229, 115, 107), (216, 226, 122)
LEAVES = ((160, 86, 44), (176, 138, 51), (201, 122, 60))
PETALS = ((232, 178, 192), (240, 204, 212), (222, 160, 180))


def on_signal(sig, handler, *args):
    try:
        gi.require_version("GLibUnix", "2.0")
        from gi.repository import GLibUnix
        GLibUnix.signal_add(GLib.PRIORITY_DEFAULT, sig, handler, *args)
    except (ValueError, ImportError):
        GLib.unix_signal_add(GLib.PRIORITY_DEFAULT, sig, handler, *args)


def rect(x, y, w, h):
    return Graphene.Rect().init(x, y, w, h)


def rgba(col, a):
    c = Gdk.RGBA()
    c.red, c.green, c.blue, c.alpha = col[0] / 255.0, col[1] / 255.0, col[2] / 255.0, a
    return c


def texture(w, h, paint):
    surf = cairo.ImageSurface(cairo.FORMAT_ARGB32, w, h)
    paint(cairo.Context(surf), w, h)
    surf.flush()
    return Gdk.MemoryTexture.new(w, h, Gdk.MemoryFormat.B8G8R8A8_PREMULTIPLIED,
                                 GLib.Bytes.new(bytes(surf.get_data())), surf.get_stride())


def unit(col):
    return col[0] / 255.0, col[1] / 255.0, col[2] / 255.0


def soft_dot(col, size=64, core=0.0):
    def paint(cr, w, h):
        g = cairo.RadialGradient(w / 2, h / 2, 0, w / 2, h / 2, w / 2)
        g.add_color_stop_rgba(0, *unit(col), 1)
        if core:
            g.add_color_stop_rgba(core, *unit(col), 1)
        g.add_color_stop_rgba(max(0.5, core + 0.2), *unit(col), 0.30)
        g.add_color_stop_rgba(1, *unit(col), 0)
        cr.set_source(g)
        cr.paint()
    return texture(size, size, paint)


def bird_frames(col, n=9):
    frames = []
    for k in range(n):
        lift = -0.55 + 1.5 * k / (n - 1)           # wings from below the body to well above it

        def paint(cr, w, h, lift=lift):
            cr.set_source_rgba(*unit(col), 0.92)
            cr.set_line_width(h * 0.085)
            cr.set_line_cap(cairo.LINE_CAP_ROUND)
            cx, cy, span = w / 2, h * 0.56, w * 0.46
            for side in (-1, 1):
                tx, ty = cx + side * span * math.cos(lift * 0.6), cy - span * math.sin(lift) * 0.72
                cr.move_to(cx, cy)
                cr.curve_to(cx + side * span * 0.22, cy - h * 0.12, cx + side * span * 0.5,
                            cy - span * math.sin(lift) * 0.5 - h * 0.07, tx, ty)
            cr.stroke()
        frames.append(texture(66, 42, paint))
    return frames


def rain_tile(col, lean, count, seed, size=512):
    def paint(cr, w, h):
        rnd = random.Random(seed)
        cr.set_line_cap(cairo.LINE_CAP_ROUND)
        for _ in range(count):
            x, y, ln = rnd.uniform(0, w), rnd.uniform(0, h), rnd.uniform(16, 44)
            cr.set_source_rgba(*unit(col), rnd.uniform(0.10, 0.36))
            cr.set_line_width(rnd.choice((1.0, 1.2, 1.5)))
            for ox in (-w, 0, w):                  # drawn nine times over so the tile has no seam
                for oy in (-h, 0, h):
                    cr.move_to(x + ox, y + oy)
                    cr.line_to(x + ox + lean * ln, y + oy + ln)
            cr.stroke()
    return texture(size, size, paint)


def snow_tile(col, count, seed, big, size=512):
    def paint(cr, w, h):
        rnd = random.Random(seed)
        for _ in range(count):
            x, y, r = rnd.uniform(0, w), rnd.uniform(0, h), rnd.uniform(0.8, 1.6) * big
            cr.set_source_rgba(*unit(col), rnd.uniform(0.45, 0.92))
            for ox in (-w, 0, w):
                for oy in (-h, 0, h):
                    cr.arc(x + ox, y + oy, r, 0, 2 * math.pi)
                    cr.fill()
    return texture(size, size, paint)


def streak(col, w=256, h=12):
    """A shooting star: bright head on the right, tail fading to the left."""
    def paint(cr, w_, h_):
        g = cairo.LinearGradient(0, 0, w_, 0)
        g.add_color_stop_rgba(0.0, *unit(col), 0)
        g.add_color_stop_rgba(0.75, *unit(col), 0.55)
        g.add_color_stop_rgba(1.0, 1, 1, 1, 1)
        cr.set_source(g)
        cr.move_to(0, h_ / 2)
        cr.line_to(w_ - h_ * 0.5, h_ * 0.22)
        cr.arc(w_ - h_ * 0.5, h_ / 2, h_ * 0.28, -math.pi / 2, math.pi / 2)
        cr.close_path()
        cr.fill()
    return texture(w, h, paint)


def contrail(col, w=512, h=24):
    def paint(cr, w_, h_):
        g = cairo.LinearGradient(0, 0, w_, 0)
        g.add_color_stop_rgba(0.0, *unit(col), 0)
        g.add_color_stop_rgba(0.55, *unit(col), 0.22)
        g.add_color_stop_rgba(1.0, *unit(col), 0.85)
        cr.set_source(g)
        cr.move_to(0, h_ * 0.08)
        cr.line_to(w_, h_ * 0.44)
        cr.line_to(w_, h_ * 0.56)
        cr.line_to(0, h_ * 0.92)
        cr.close_path()
        cr.fill()
    return texture(w, h, paint)


def leaf(col, petal):
    def paint(cr, w, h):
        cr.set_source_rgba(*unit(col), 0.95)
        if petal:
            cr.save()
            cr.translate(w / 2, h / 2)
            cr.scale(w * 0.36, h * 0.42)
            cr.arc(0, 0, 1, 0, 2 * math.pi)
            cr.restore()
        else:
            cr.move_to(w * 0.04, h / 2)
            cr.curve_to(w * 0.3, h * 0.02, w * 0.7, h * 0.02, w * 0.96, h / 2)
            cr.curve_to(w * 0.7, h * 0.98, w * 0.3, h * 0.98, w * 0.04, h / 2)
        cr.fill()
    return texture(32, 18, paint)


class Scene:
    """The note `dormer draw` left, with safe defaults for anything it did not say."""
    DEFAULTS = dict(el=-30.0, day=0.0, night=0.0, dark=1.0, cover=0.0, rain=0.0, snow=0.0, fog=0.0, storm=False,
                    wind=0.0, wind_x=0.0, season=2, busy=0.5, am=False, stars=[], smoke=[], tv=[], lamp=None,
                    beacon=None, rates={}, bird=[20, 26, 32], rain_col=[201, 214, 222], snow_col=[233, 238, 240],
                    star=[235, 232, 220], trail=[242, 238, 227], mode="dark", ts=0)

    def __init__(self, path=SCENE):
        data = {}
        try:
            with open(path) as f:
                data = json.load(f)
        except (OSError, ValueError):
            pass
        self.known = bool(data)
        for key, val in self.DEFAULTS.items():
            setattr(self, key, data.get(key, val))


class World:
    def __init__(self, path=SCENE):
        self.path = path
        self.rnd = random.Random()
        self.t, self.clock = 0.0, None
        self.flocks, self.meteors, self.planes, self.puffs, self.leaves = [], [], [], [], []
        self.flash = None
        self.due = dict(flock=2.0, meteor=5.0, plane=12.0, gust=6.0, flash=3.0)
        self.rain_at, self.snow_at = [[0.0, 0.0], [0.0, 0.0]], [[0.0, 0.0], [0.0, 0.0], [0.0, 0.0]]
        self.tex, self.keys = {}, {}
        self.tv, self.acc = [], []
        self.flies = [dict(x=self.rnd.choice((self.rnd.uniform(30, 300), self.rnd.uniform(1300, 1570))),
                           y=self.rnd.uniform(800, 975), a=self.rnd.uniform(0, 6.28), b=self.rnd.uniform(0.2, 0.6),
                           c=self.rnd.uniform(0.6, 1.6)) for _ in range(12)]
        self.sc = Scene(path)
        self.load()

    # Textures are rebuilt only when the thing they depend on has changed.
    def want(self, name, key, make):
        if self.keys.get(name) != key:
            self.keys[name], self.tex[name] = key, make()
        return self.tex[name]

    def load(self):
        sc = self.sc = Scene(self.path)
        self.want("star", tuple(sc.star), lambda: soft_dot(sc.star, 48, 0.12))
        self.want("birds", tuple(sc.bird), lambda: bird_frames(sc.bird))
        self.want("streak", tuple(sc.star), lambda: streak(sc.star))
        self.want("trail", tuple(sc.trail), lambda: contrail(sc.trail))
        self.want("beacon", 0, lambda: soft_dot(BEACON, 64, 0.10))
        self.want("lamp", 0, lambda: soft_dot(AMBER, 96))
        self.want("fly", 0, lambda: soft_dot(GLOWWORM, 48, 0.10))
        self.want("light", 0, lambda: soft_dot((255, 255, 255), 32, 0.25))
        tone = sc.smoke[0][3] if sc.smoke else [120, 130, 140]
        self.want("smoke", tuple(tone), lambda: soft_dot(tone, 96))
        petals = sc.season == 1
        self.want("leaves", petals, lambda: [leaf(c, petals) for c in (PETALS if petals else LEAVES)])
        if sc.rain > 0:
            lean = max(-0.7, min(0.7, sc.wind_x * 0.45))
            n = int(50 + 230 * sc.rain)
            self.want("rain", (tuple(sc.rain_col), round(lean, 2), n),
                      lambda: [rain_tile(sc.rain_col, lean, n, 1), rain_tile(sc.rain_col, lean, int(n * 0.8), 2)])
            self.lean = lean
        if sc.snow > 0:
            n = int(30 + 90 * sc.snow)
            self.want("snow", (tuple(sc.snow_col), n),
                      lambda: [snow_tile(sc.snow_col, n, 3, 2.2), snow_tile(sc.snow_col, n, 4, 1.5),
                               snow_tile(sc.snow_col, int(n * 1.3), 5, 1.0)])
        if getattr(self, "last_rates", None) != sc.rates:
            self.last_rates = dict(sc.rates)
            for what in ("flock", "meteor", "plane"):
                self.due[what] = min(self.due[what], self.t + 1.5)
        self.tv = [dict(box=b, a=0.0, until=0.0) for b in sc.tv]
        self.acc = [0.0] * len(sc.smoke)

    # ── time passing ─────────────────────────────────────────────────────────

    def advance(self, now):
        dt = 1.0 / FPS if self.clock is None else now - self.clock
        if dt < 0.004:
            return                                  # another screen already moved the world this frame
        self.clock = now
        self.step(min(dt, 0.12))

    def step(self, dt):
        sc, r = self.sc, self.rnd
        t = self.t = self.t + dt

        often = lambda what: max(0.0, float(sc.rates.get(what, 1.0)))       # the dev panel can turn these up
        if sc.el > -3 and not sc.storm and sc.snow < 0.6 and sc.fog < 0.6 and often("birds") > 0:
            if t >= self.due["flock"] and len(self.flocks) < 3 + int(often("birds")):
                self.spawn_flock()
                self.due["flock"] = t + r.uniform(5, 17) * (1.6 if sc.el > 20 else 1.0) * (3.0 if sc.rain > 0.3 else 1.0) / often("birds")
        for f in self.flocks:
            f["x"] += f["way"] * f["speed"] * dt
            f["y"] += f["climb"] * dt
            f["age"] += dt
        self.flocks = [f for f in self.flocks if -260 < f["x"] < VW + 260]

        if sc.night > 0.4 and sc.cover < 0.7 and often("meteors") > 0 and t >= self.due["meteor"]:
            self.spawn_meteor()
            self.due["meteor"] = t + r.uniform(14, 55) / often("meteors")
        for m in self.meteors:
            m["age"] += dt
        self.meteors = [m for m in self.meteors if m["age"] < m["dur"]]

        if sc.cover < 0.8 and sc.fog < 0.5 and often("planes") > 0 and t >= self.due["plane"] and len(self.planes) < 2 + int(often("planes")):
            way = r.choice((-1, 1))
            self.planes.append(dict(way=way, x=-40.0 if way > 0 else VW + 40.0, y=r.uniform(80, 330),
                                    slope=r.uniform(-0.05, 0.05), speed=r.uniform(15, 23)))
            self.due["plane"] = t + r.uniform(55, 150) / max(0.2, sc.busy) / often("planes")
        for p in self.planes:
            p["x"] += p["way"] * p["speed"] * dt
            p["y"] += p["slope"] * p["speed"] * dt
        self.planes = [p for p in self.planes if -460 < p["x"] < VW + 460]

        for k, (fx, fy, scale, _tone) in enumerate(sc.smoke):
            self.acc[k] += dt * 2.1
            while self.acc[k] >= 1 and len(self.puffs) < 150:
                self.acc[k] -= 1
                self.puffs.append(dict(x=fx * VW + r.uniform(-1.5, 1.5), y=fy * VH, vx=sc.wind_x * r.uniform(3, 8),
                                       vy=-r.uniform(11, 17) * scale, age=0.0, life=r.uniform(4.5, 7.0),
                                       size=5 * scale, grow=r.uniform(2.6, 3.8) * scale))
        for p in self.puffs:
            p["vx"] += sc.wind_x * 8 * dt
            p["vy"] *= 1 - 0.10 * dt
            p["x"] += p["vx"] * dt
            p["y"] += p["vy"] * dt
            p["age"] += dt
        self.puffs = [p for p in self.puffs if p["age"] < p["life"]]

        if sc.season in (1, 3) and sc.wind >= 5 and sc.rain < 0.5 and sc.snow == 0 and t >= self.due["gust"]:
            side = 1 if sc.wind_x >= 0 else -1
            for k in range(r.randint(2, 7)):
                self.leaves.append(dict(x=(-30.0 if side > 0 else VW + 30.0) - side * k * r.uniform(30, 90),
                                        y=r.uniform(380, 930), vx=side * r.uniform(70, 150) * (0.6 + min(1.5, abs(sc.wind_x))),
                                        vy=r.uniform(8, 34), spin=r.uniform(-260, 260), turn=r.uniform(0, 360),
                                        ph=r.uniform(0, 6.28), kind=r.randrange(3), size=r.uniform(9, 15), age=0.0))
            self.due["gust"] = t + r.uniform(8, 28)
        for l in self.leaves:
            l["age"] += dt
            l["x"] += l["vx"] * dt
            l["y"] += (l["vy"] + math.sin(l["age"] * 3 + l["ph"]) * 24) * dt
            l["turn"] += l["spin"] * dt
        self.leaves = [l for l in self.leaves if -400 < l["x"] < VW + 400 and l["y"] < VH + 30]

        if sc.rain > 0:
            for k, speed in enumerate((1050.0, 720.0)):
                self.rain_at[k][0] = (self.rain_at[k][0] + self.lean * speed * dt) % 512
                self.rain_at[k][1] = (self.rain_at[k][1] + speed * dt) % 512
        if sc.snow > 0:
            for k, speed in enumerate((72.0, 48.0, 30.0)):
                self.snow_at[k][0] = (self.snow_at[k][0] + sc.wind_x * 26 * dt) % 512
                self.snow_at[k][1] = (self.snow_at[k][1] + speed * dt) % 512

        if sc.storm:
            if self.flash:
                self.flash["age"] += dt
                if self.flash["age"] > 0.55:
                    self.flash = None
            elif t >= self.due["flash"]:
                x, y, pts = r.uniform(240, 1360), 170.0, []
                while y < 790:
                    pts.append((x, y))
                    x, y = x + r.uniform(-30, 30), y + r.uniform(20, 55)
                self.flash = dict(age=0.0, pts=pts)
                self.due["flash"] = t + r.uniform(4, 15)
        else:
            self.flash = None

        for tv in self.tv:
            if t >= tv["until"]:
                tv["a"], tv["until"] = r.choice((0.0, 0.0, 0.10, 0.20, 0.32, 0.46)), t + r.uniform(0.07, 0.6)

    def spawn_flock(self, x=None):
        r = self.rnd
        way, depth = r.choice((-1, 1)), r.uniform(0.7, 1.3)
        birds = [dict(dx=0.0, dy=0.0, ph=r.uniform(0, 6.28), rate=r.uniform(9, 13))]
        for k in range(1, r.choice((1, 1, 2, 3, 3, 5, 7, 9))):
            rank, side = (k + 1) // 2, 1 if k % 2 else -1
            birds.append(dict(dx=-rank * r.uniform(17, 23) * depth, dy=side * rank * r.uniform(7, 11) * depth,
                              ph=r.uniform(0, 6.28), rate=r.uniform(9, 13)))
        self.flocks.append(dict(way=way, x=x if x is not None else (-60.0 if way > 0 else VW + 60.0),
                                y=r.uniform(410, 690), speed=r.uniform(52, 88) * depth, depth=depth, age=0.0,
                                bob=r.uniform(0, 6.28), climb=r.uniform(-5, 5), birds=birds))

    def spawn_meteor(self):
        r = self.rnd
        big = 1.5 if r.random() < 0.18 else 1.0
        ang, way = math.radians(r.uniform(16, 42)), r.choice((-1, 1))
        self.meteors.append(dict(x=r.uniform(220, 1380), y=r.uniform(60, 380), dx=math.cos(ang) * way, dy=math.sin(ang),
                                 dist=r.uniform(260, 500) * big, dur=r.uniform(0.6, 1.0) * big, age=0.0,
                                 len=r.uniform(110, 190) * big, thick=4.5 * big))

    # ── drawing ──────────────────────────────────────────────────────────────

    def blit(self, snap, tex, x, y, w, h, alpha=1.0):
        if alpha <= 0.004:
            return
        if alpha < 0.996:
            snap.push_opacity(alpha)
            snap.append_texture(tex, rect(x, y, w, h))
            snap.pop()
        else:
            snap.append_texture(tex, rect(x, y, w, h))

    def turned(self, snap, tex, x, y, angle, bx, by, w, h, alpha, flip=1):
        snap.save()
        snap.translate(Graphene.Point().init(x, y))
        snap.rotate(angle)
        if flip < 0:
            snap.scale(-1, 1)
        self.blit(snap, tex, bx, by, w, h, alpha)
        snap.restore()

    def tiled(self, snap, tex, ox, oy, size, alpha):
        snap.push_opacity(alpha)
        snap.push_repeat(rect(0, 0, VW, VH), rect(ox - size, oy - size, size, size))
        snap.append_texture(tex, rect(ox - size, oy - size, size, size))
        snap.pop()
        snap.pop()

    def draw(self, snap, w, h):
        sc, t = self.sc, self.t
        if not sc.known:
            return
        fixed = abs(w / h - VW / VH) < 0.03          # roof-top things only line up on a 16:10 screen
        snap.save()
        snap.scale(w / VW, h / VH)

        if sc.night > 0.05:
            star = self.tex["star"]
            for k, (fx, fy, size, a) in enumerate(sc.stars):
                pulse = (0.5 + 0.5 * math.sin(t * (0.7 + (k * 0.37) % 1.9) + k * 2.4)) ** 3
                s = size * 6.0
                self.blit(snap, star, fx * VW - s / 2, fy * VH - s / 2, s, s, a * 0.95 * pulse)
        for m in self.meteors:
            p = m["age"] / m["dur"]
            ease = 1 - (1 - p) ** 2
            tail = m["len"] * min(1.0, p * 3.5)
            self.turned(snap, self.tex["streak"], m["x"] + m["dx"] * m["dist"] * ease, m["y"] + m["dy"] * m["dist"] * ease,
                        math.degrees(math.atan2(m["dy"], m["dx"])), -tail, -m["thick"] / 2, tail, m["thick"],
                        math.sin(math.pi * p) ** 0.6 * sc.night)

        for p in self.planes:
            angle = math.degrees(math.atan2(p["slope"], 1.0)) * p["way"]
            if sc.day > 0.25:
                self.turned(snap, self.tex["trail"], p["x"], p["y"], angle, -400, -5, 400, 10, 0.62 * sc.day, p["way"])
                self.blit(snap, self.tex["light"], p["x"] - 3, p["y"] - 3, 6, 6, 0.9)
            else:
                strobe = 1.0 if (t * 0.9) % 1.0 < 0.10 else 0.35
                self.blit(snap, self.tex["light"], p["x"] - 3, p["y"] - 3, 6, 6, strobe)
                self.blit(snap, self.tex["beacon"], p["x"] - p["way"] * 7 - 4, p["y"] - 4, 8, 8, 0.9)

        frames = self.tex["birds"]
        for f in self.flocks:
            sw, sh = 24 * f["depth"], 15 * f["depth"]
            drift = math.sin(f["age"] * 0.13 + f["bob"]) * 18
            for b in f["birds"]:
                beat = math.sin(f["age"] * b["rate"] + b["ph"])
                if math.sin(f["age"] * 0.7 + b["ph"] * 1.7) > 0.55:
                    beat = 0.3                        # gliding for a moment
                k = int(round((beat * 0.5 + 0.5) * (len(frames) - 1)))
                self.blit(snap, frames[k], f["x"] + f["way"] * b["dx"] - sw / 2,
                          f["y"] + b["dy"] + drift - beat * 1.4 * f["depth"] - sh / 2, sw, sh)

        if fixed:
            for p in self.puffs:
                age = p["age"] / p["life"]
                s = p["size"] + p["grow"] * p["age"]
                self.blit(snap, self.tex["smoke"], p["x"] - s, p["y"] - s * 0.85, s * 2, s * 1.7,
                          0.36 * min(1.0, p["age"] / 0.6) * (1 - age) ** 1.4)
            if sc.beacon:
                pulse = max(0.0, math.sin(t * 2.2)) ** 6
                x, y = sc.beacon[0] * VW, sc.beacon[1] * VH
                self.blit(snap, self.tex["beacon"], x - 13, y - 13, 26, 26, 0.9 * pulse * sc.dark)
            if sc.lamp:
                x, y, lw, lh = sc.lamp[0] * VW, sc.lamp[1] * VH, sc.lamp[2] * VW, sc.lamp[3] * VH
                self.blit(snap, self.tex["lamp"], x - lw * 1.8, y - lh * 2.2, lw * 3.6, lh * 4.4,
                          (0.16 + 0.13 * math.sin(t * 1.15)) * (0.45 + 0.55 * sc.dark))
            if sc.dark > 0.5:
                for tv in self.tv:
                    if tv["a"] > 0:
                        x, y, bw, bh = tv["box"]
                        snap.append_color(rgba((150, 196, 236), tv["a"]), rect(x * VW, y * VH, bw * VW, bh * VH))

        for l in self.leaves:
            self.turned(snap, self.tex["leaves"][l["kind"]], l["x"], l["y"], l["turn"], -l["size"] / 2, -l["size"] * 0.28,
                        l["size"], l["size"] * 0.56, 0.22 + 0.73 * sc.day)
        if sc.season == 2 and sc.dark > 0.6 and sc.rain == 0:
            for f in self.flies:
                x = f["x"] + math.sin(t * f["b"] + f["a"]) * 34
                y = f["y"] + math.cos(t * f["b"] * 0.8 + f["a"] * 2) * 16
                self.blit(snap, self.tex["fly"], x - 5, y - 5, 10, 10, max(0.0, math.sin(t * f["c"] + f["a"])) ** 2 * 0.9)

        if sc.snow > 0 and "snow" in self.tex:
            for k, tile in enumerate(self.tex["snow"]):
                sway = math.sin(t * (0.55 - 0.1 * k) + k * 2.1) * 16
                self.tiled(snap, tile, (self.snow_at[k][0] + sway) % 512, self.snow_at[k][1], 512, (0.95, 0.8, 0.65)[k])
        if sc.rain > 0 and "rain" in self.tex:
            vis = 0.6 + 0.4 * sc.day
            for k, tile in enumerate(self.tex["rain"]):
                self.tiled(snap, tile, self.rain_at[k][0], self.rain_at[k][1], 512, (0.95, 0.6)[k] * vis)
        if self.flash:
            age = self.flash["age"]
            lit = max(0.0, 1 - age / 0.5) * (1.0 if age < 0.07 or 0.13 < age < 0.21 else 0.35)
            snap.append_color(rgba((214, 224, 242), 0.22 * lit), rect(0, 0, VW, VH * 0.86))
            if age < 0.21:
                cr = snap.append_cairo(rect(0, 0, VW, VH))
                cr.set_source_rgba(0.93, 0.95, 0.99, 0.95)
                cr.set_line_width(2.0)
                cr.set_line_join(cairo.LINE_JOIN_ROUND)
                cr.move_to(*self.flash["pts"][0])
                for px, py in self.flash["pts"][1:]:
                    cr.line_to(px, py)
                cr.stroke()
        snap.restore()


class Pane(Gtk.Widget):
    def __init__(self, world):
        super().__init__()
        self.world = world
        self.set_hexpand(True)
        self.set_vexpand(True)

    def do_snapshot(self, snap):
        w, h = self.get_width(), self.get_height()
        if w > 0 and h > 0:
            self.world.draw(snap, w, h)


class Live(Gtk.Application):
    def __init__(self):
        super().__init__(application_id="dev.dormer.live", flags=Gio.ApplicationFlags.NON_UNIQUE)
        self.world = None
        self.panes = {}            # connector name -> (window, pane, tick id, last frame time)
        self.held = False          # lock screen, or told to hold still
        self.saver = False
        self.open = {}             # connector name -> can the wallpaper be seen there
        self.check_due = 0
        sig = os.environ.get("HYPRLAND_INSTANCE_SIGNATURE", "")
        self.hypr_dir = os.path.join(RUNTIME, "hypr", sig) if sig else ""

    def do_activate(self):
        if self.world:
            return
        self.hold()
        self.world = World()
        css = Gtk.CssProvider()
        css.load_from_string("window.sky { background: none; }")
        Gtk.StyleContext.add_provider_for_display(Gdk.Display.get_default(), css, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION)
        monitors = Gdk.Display.get_default().get_monitors()
        monitors.connect("items-changed", lambda *_: self.build())
        self.build()
        os.makedirs(CACHE, exist_ok=True)
        with open(os.path.join(CACHE, "live.pid"), "w") as f:
            f.write(str(os.getpid()))
        self.monitor = Gio.File.new_for_path(CACHE).monitor_directory(Gio.FileMonitorFlags.WATCH_MOVES, None)
        self.monitor.connect("changed", self.on_file)
        on_signal(signal.SIGUSR1, self.on_hold, True)
        on_signal(signal.SIGUSR2, self.on_hold, False)
        for s in (signal.SIGTERM, signal.SIGINT):
            on_signal(s, self.on_quit)
        GLib.timeout_add_seconds(5, self.on_slow)
        self.listen()
        self.look()

    def build(self):
        monitors = Gdk.Display.get_default().get_monitors()
        seen = set()
        for i in range(monitors.get_n_items()):
            mon = monitors.get_item(i)
            name = mon.get_connector() or "screen-%d" % i
            seen.add(name)
            if name in self.panes:
                continue
            win = Gtk.Window(application=self)
            win.add_css_class("sky")
            win.set_decorated(False)
            Shell.init_for_window(win)
            Shell.set_namespace(win, "dormer")
            Shell.set_layer(win, Shell.Layer.BACKGROUND)
            Shell.set_monitor(win, mon)
            for edge in (Shell.Edge.TOP, Shell.Edge.BOTTOM, Shell.Edge.LEFT, Shell.Edge.RIGHT):
                Shell.set_anchor(win, edge, True)
            Shell.set_exclusive_zone(win, -1)
            Shell.set_keyboard_mode(win, Shell.KeyboardMode.NONE)
            pane = Pane(self.world)
            win.set_child(pane)
            win.connect("map", lambda w: w.get_surface().set_input_region(cairo.Region()))   # clicks pass through
            win.present()
            self.panes[name] = dict(win=win, pane=pane, tick=0, last=0.0)
        for name in list(self.panes):
            if name not in seen:
                self.panes.pop(name)["win"].destroy()
        self.apply()

    # ── when to move ─────────────────────────────────────────────────────────

    def apply(self):
        for name, p in self.panes.items():
            run = not self.held and not self.saver and self.open.get(name, True)
            if run and not p["tick"]:
                self.world.clock = None
                p["tick"] = p["pane"].add_tick_callback(self.on_tick, p)
            elif not run and p["tick"]:
                p["pane"].remove_tick_callback(p["tick"])
                p["tick"] = 0

    def on_tick(self, pane, clock, p):
        now = clock.get_frame_time() / 1e6
        if now - p["last"] < 1.0 / FPS - 0.004:
            return True
        p["last"] = now
        self.world.advance(now)
        pane.queue_draw()
        return True

    def on_hold(self, held):
        self.held = held
        self.apply()
        return True

    def on_quit(self):
        try:
            os.remove(os.path.join(CACHE, "live.pid"))
        except OSError:
            pass
        self.quit()
        return False

    def on_slow(self):
        saver = held()
        if saver != self.saver:
            self.saver = saver
            self.apply()
        return True

    def on_file(self, _mon, file, other, event):
        names = {f.get_basename() for f in (file, other) if f}
        if "scene.json" in names and event in (Gio.FileMonitorEvent.RENAMED, Gio.FileMonitorEvent.MOVED_IN,
                                               Gio.FileMonitorEvent.CHANGES_DONE_HINT, Gio.FileMonitorEvent.CREATED):
            self.world.load()
            for p in self.panes.values():
                p["pane"].queue_draw()

    # ── asking Hyprland whether anyone can see the wallpaper ─────────────────

    def ask(self, what):
        with socket.socket(socket.AF_UNIX, socket.SOCK_STREAM) as s:
            s.settimeout(1.0)
            s.connect(os.path.join(self.hypr_dir, ".socket.sock"))
            s.sendall(what.encode())
            chunks = []
            while True:
                data = s.recv(65536)
                if not data:
                    break
                chunks.append(data)
        return json.loads(b"".join(chunks).decode())

    def listen(self):
        if not self.hypr_dir:
            return
        try:
            self.events = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
            self.events.connect(os.path.join(self.hypr_dir, ".socket2.sock"))
            self.events.setblocking(False)
        except OSError:
            return
        GLib.io_add_watch(self.events.fileno(), GLib.IO_IN | GLib.IO_HUP, self.on_event)

    def on_event(self, _fd, cond):
        if cond & GLib.IO_HUP:
            return False
        try:
            text = self.events.recv(65536).decode(errors="replace")
        except OSError:
            return True
        if not text:
            return False
        if any("openlayer>>" + name in text for name in WALLPAPERS):
            GLib.timeout_add(400, self.restack)       # a fresh wallpaper surface would sit on top of this one
        if any(key in text for key in ("workspace", "window", "fullscreen", "floating", "special", "monitor")):
            if not self.check_due:
                self.check_due = GLib.timeout_add(140, self.look)
        return True

    def restack(self):
        for p in self.panes.values():
            p["win"].set_visible(False)
            p["win"].set_visible(True)
        return False

    def look(self):
        self.check_due = 0
        if not self.hypr_dir:
            return False
        try:
            monitors, clients = self.ask("j/monitors"), self.ask("j/clients")
        except (OSError, ValueError):
            return False
        for mon in monitors:
            shown = {mon.get("activeWorkspace", {}).get("id")}
            special = mon.get("specialWorkspace", {}).get("id")
            if special:
                shown.add(special)
            covered = any(c.get("workspace", {}).get("id") in shown and c.get("mapped", True) and not c.get("hidden")
                          and (not c.get("floating") or c.get("fullscreen")) for c in clients)
            self.open[mon.get("name")] = not covered
        self.apply()
        return False


def one_frame(path, seconds, scene):
    """Run the world for a while without a window and save what it would be showing."""
    Gtk.init()
    world = World(scene or SCENE)
    world.sc.known = True
    if world.sc.el > -3:
        world.spawn_flock(520.0)
        world.spawn_flock(1050.0)
    for _ in range(int(seconds * FPS)):
        world.step(1.0 / FPS)
    if world.sc.night > 0.4 and not world.meteors:
        world.spawn_meteor()
        for _ in range(int(0.45 * FPS)):
            world.step(1.0 / FPS)
    snap = Gtk.Snapshot.new()
    world.draw(snap, VW, VH)
    node = snap.to_node()
    if node is None:
        raise SystemExit("dormer: nothing moves in this scene")
    renderer = Gsk.CairoRenderer()
    renderer.realize_for_display(Gdk.Display.get_default())
    renderer.render_texture(node, rect(0, 0, VW, VH)).save_to_png(path)
    renderer.unrealize()
    print(path, "flocks=%d meteors=%d puffs=%d leaves=%d planes=%d" % (
        len(world.flocks), len(world.meteors), len(world.puffs), len(world.leaves), len(world.planes)))


def main(args):
    if args.frame:
        return one_frame(args.frame, args.seconds, args.scene)
    return Live().run([])
