"""One moment, worked out: everything the painter and the live layer need to know."""
import math
import random
import time

from .almanac import ephemeris
from .palette import PALETTES, Bag, gauss, keyed, mix, smooth


def scene(ts, wx, mode, machine, place, drift=None, top=None, rates=None):
    """Work out one moment. `top` is the bar colour; `rates` scale how often live things happen."""
    sc = Bag()
    sc.ts, sc.mode, sc.P, sc.wx, sc.place = ts, mode, PALETTES[mode], wx, place
    lt = time.localtime(ts)
    sc.t = lt.tm_hour + lt.tm_min / 60.0 + lt.tm_sec / 3600.0
    sc.dow, sc.month, sc.mday, sc.day_id = lt.tm_wday, lt.tm_mon, lt.tm_mday, lt.tm_year * 1000 + lt.tm_yday
    sc.slot = int(ts // 300)
    sc.sun, sc.moon = ephemeris(ts, place["lat"], place["lon"])
    sc.agents, sc.containers = machine
    sc.live, sc.out = False, {"stars": [], "smoke": [], "tv": [], "lamp": None, "beacon": None}
    sc.rates = dict(birds=1.0, meteors=1.0, planes=1.0)
    sc.rates.update(rates or {})
    P, el = sc.P, sc.sun.el
    sc.am = sc.sun.ha < 0

    sc.cover, sc.rain, sc.snow, sc.fog, sc.storm = wx["cover"], wx["rain"], wx["snow"], wx["fog"], wx["storm"]
    sc.temp, sc.wind = wx["temp"], wx["wind"]
    # Looking south, west is on the right: a wind from the west carries things leftwards.
    side = 1 if place["lat"] >= 0 else -1
    sc.wind_x = side * math.sin(math.radians(wx["wdir"])) * min(1.6, sc.wind / 16.0)
    sc.drift = drift if drift is not None else ts / 3600.0 * sc.wind_x * 0.19
    overcast = smooth(0.45, 1.0, sc.cover)
    sc.gloom = max(overcast * 0.30, sc.rain * 0.75, sc.fog * 0.5, sc.snow * 0.4, 0.92 if sc.storm else 0.0)
    sc.el_eff = el * (1 - 0.85 * sc.gloom) - 9.0 * sc.gloom      # how bright it feels down in the street
    sc.day = smooth(-6, 14, el)
    sc.dark = 1 - smooth(-7, 3, sc.el_eff)
    sc.night = smooth(-5, -15, el) * (1 - overcast) * (1 - sc.fog)
    sc.twilight = max(0.0, 1 - abs(el + 1) / 11.0)

    sky = [tuple(c) for c in keyed(P["sky"], el)]
    if top:
        sky[0] = tuple(top)
    if sc.am:                                                    # dawn is rosier than dusk
        for i, wgt in ((3, 0.10), (4, 0.30), (5, 0.38), (6, 0.30)):
            sky[i] = mix(sky[i], P["rose"], wgt * sc.twilight)
    grey = mix(P["grey"][0], P["grey"][1], sc.day)
    if sc.storm:
        grey = mix(grey, P["storm"], 0.7)
    veil = max(overcast * 0.78, sc.rain * 0.85, sc.fog * 0.7, sc.snow * 0.6, 0.9 if sc.storm else 0.0)
    for i in range(1, len(sky)):
        sky[i] = mix(sky[i], mix(grey, sky[i], 0.30), veil)
    sc.sky, sc.veil = sky, veil
    sc.layers = [mix(c, mix(grey, c, 0.55), veil * 0.5) for c in keyed(P["layers"], el)]
    sc.haze = mix(keyed(P["haze"], el), grey, veil * 0.7)
    gain = keyed(P["haze_gain"], el)
    sc.haze_alpha = [a * gain for a in P["haze_alpha"]]
    sc.cloud = mix(keyed(P["cloud"], el), mix(grey, P["storm"], 0.5 if mode == "dark" else 0.2), sc.gloom * 0.8)
    sc.cloud_alpha = keyed(P["cloud_alpha"], el) * (0.45 + 0.55 * sc.cover)
    sc.glow = keyed(P["glow"], el)
    sc.glow_alpha = P["glow_alpha"] * keyed(P["glow_gain"], el) * (1 - 0.8 * veil)
    sc.sun_col = mix(P["sun"][0], P["sun"][1], smooth(0, 24, el))
    sc.sun_vis = (1 - smooth(0.55, 0.92, sc.cover)) * (1 - sc.fog * 0.8)

    # the working week, as a number: how much the city has on
    sc.busy = (1.0, 0.8, 0.8, 0.8, 0.9, 0.35, 0.2)[sc.dow]
    sc.traffic = sc.busy * (gauss(sc.t, 8.1, 0.8) + gauss(sc.t, 18.2, 1.0)) + 0.22 * smooth(18.5, 20, sc.t) * smooth(24, 22.5, sc.t)
    dry = sc.rain < 0.05 and sc.snow < 0.05 and not sc.storm
    t, dow = sc.t, sc.dow
    sc.sleepers = 4 if dow == 6 and t < 10.75 else 1 if dow == 5 and t < 9.0 else 0
    sc.laundry = (2 if dow == 6 else 1) if dow >= 5 and 9.5 <= t <= 18.5 and dry and el > 2 and sc.fog < 0.5 else 0
    sc.kite = dow >= 5 and 14 <= t < 18.6 and el > 3 and 7 <= sc.wind <= 42 and dry and sc.fog < 0.5
    sc.balloon = dow >= 5 and 7.5 <= t < 11.5 and el > 2 and sc.wind < 22 and sc.cover < 0.7 and dry and sc.fog < 0.5
    sc.festoon = ((dow in (4, 5) and t >= 17) or (dow in (5, 6) and t < 1.5)) and sc.el_eff < 1 and dry
    sc.crane_works = dow < 5 and (8 <= t < 12 or 13 <= t < 17.75) and not sc.storm
    sc.fireworks = (sc.month == 12 and sc.mday == 31 and t >= 21.5) or (sc.month == 1 and sc.mday == 1 and t < 1.0)
    sc.cat_asleep = t >= 22.5 or t < 6.5 or (dow == 6 and t < 10.5)
    sc.cat_out = sc.rain < 0.2 and sc.snow < 0.2 and not sc.storm
    meal = max(gauss(t, 7.6, 0.7) * (0.3 if dow == 6 else 1), gauss(t, 13.4, 0.8), gauss(t, 20.3, 0.9))
    sc.smoke = min(0.75, max(0.0, (15.0 - sc.temp) / 16.0) + 0.10 * (meal > 0.45))
    season = ((sc.month % 12) // 3 + (0 if place["lat"] >= 0 else 2)) % 4      # 0 winter … 3 autumn
    sc.season = season

    # planes: each day has its own timetable, heavier on working days
    sc.planes = []
    r = random.Random(sc.day_id * 13 + 5)
    for _ in range((46, 30, 30, 30, 36, 18, 10)[dow]):
        start, dur = r.uniform(5.5, 23.5), r.uniform(9, 15) / 60.0
        way, y0, slope = r.choice((-1, 1)), r.uniform(0.08, 0.30), r.uniform(-0.05, 0.05)
        if start <= t < start + dur and sc.cover < 0.8 and sc.fog < 0.5:
            p = (t - start) / dur
            x = -0.12 + 1.24 * p if way > 0 else 1.12 - 1.24 * p
            sc.planes.append((x, y0 + slope * p, way, slope))
    # birds cross at each end of the day
    sc.birds = []
    if -3 < el < 13 and dry and sc.fog < 0.5:
        way = 1 if sc.am else -1
        p = (ts / 300.0 * 0.061) % 1.3 - 0.15
        sc.birds = [(p if way > 0 else 1 - p, 0.50 + 0.07 * math.sin(ts / 2100.0), way)]
    return sc
