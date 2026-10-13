"""The city: its roofs, its windows, and the hours each window keeps.

Everything here is decided by one seed. The order of the random calls in
`skyline` and `build_city` is the city's fingerprint: change it and every
roof moves.
"""
import random

from .palette import COOL, SCREEN, WARM, Bag

REF_W, REF_H = 5120, 3200     # the canvas the city is laid out on; drawing scales from it
SPEC = [  # roof rows, far to near: base, min h, max h, min w, max w (fractions of H / W)
    (0.800, 0.020, 0.060, 0.018, 0.045),
    (0.835, 0.030, 0.085, 0.024, 0.060),
    (0.880, 0.040, 0.110, 0.034, 0.080),
    (0.940, 0.045, 0.115, 0.045, 0.100),
    (1.020, 0.050, 0.120, 0.070, 0.140),
]
LIT = (0.0, 0.08, 0.13, 0.15, 0.10)


def skyline(rng, w, base, lo, hi, wmin, wmax, depth):
    """One row of roofs. The order of the rng calls is the city's fingerprint: do not reorder."""
    floor = base + hi
    pts = [(0, floor)]
    chimneys, buildings = [], []
    x = -rng.randint(0, wmax)
    while x < w:
        bw = rng.randint(wmin, wmax)
        bh = rng.randint(lo, hi)
        top = base - bh
        kind = rng.random()
        left, right = x, x + bw
        pts.append((left, floor))
        b = Bag()
        b.rect, b.flat, b.height, b.chimney = (left, top, right, floor), None, (bh - lo) / max(1, hi - lo), None
        if kind < 0.06 and depth < 3:                      # spire on a slim tower
            tw = max(bw // 3, 30)
            cx = left + bw // 2
            tip = top - bh * rng.uniform(0.9, 1.4)
            pts += [(left, top), (cx - tw // 2, top), (cx - tw // 2, top - bh * 0.25), (cx, tip),
                    (cx + tw // 2, top - bh * 0.25), (cx + tw // 2, top), (right, top)]
            b.roof, b.apex = "spire", (cx, tip)
        elif kind < 0.45:                                  # gable
            rise = bw * rng.uniform(0.22, 0.40)
            pts += [(left, top), (left + bw / 2, top - rise), (right, top)]
            b.roof, b.apex = "gable", (left + bw / 2, top - rise)
        elif kind < 0.70:                                  # mansard
            inset = bw * 0.14
            rise = bw * rng.uniform(0.16, 0.26)
            pts += [(left, top), (left + inset, top - rise), (right - inset, top - rise), (right, top)]
            b.roof, b.apex = "mansard", (left + bw / 2, top - rise)
            b.flat = (left + inset, right - inset, top - rise)
        else:                                              # flat
            pts += [(left, top), (right, top)]
            b.roof, b.apex = "flat", (left + bw / 2, top)
            b.flat = (left, right, top)
        pts.append((right, floor))
        if rng.random() < 0.45:
            cw = max(bw // 10, 8)
            cxp = left + rng.uniform(0.15, 0.75) * bw
            ch = bh * rng.uniform(0.10, 0.18) + bw * 0.12
            b.chimney = (cxp, top - ch, cxp + cw, top + 2)
            chimneys.append(b.chimney)
        buildings.append(b)
        x = right + rng.randint(0, max(2, wmin // 5))
    pts += [(w, floor), (w, 10 ** 6), (0, 10 ** 6)]
    return pts, chimneys, buildings


def build_city(seed):
    """Roofs, windows and trees in reference pixels, plus who lives behind each window."""
    rng = random.Random(seed)
    people = random.Random(seed * 7919 + 17)    # its own stream, so the roofs stay exactly as first drawn
    W, H = REF_W, REF_H
    city = Bag()
    city.clouds = []
    for _ in range(5):
        cx, cy = rng.uniform(-0.1, 0.55) * W, rng.uniform(0.10, 0.42) * H
        cw, ch = rng.uniform(0.25, 0.5) * W, rng.uniform(0.03, 0.06) * H
        city.clouds.append((cx / W, cy / H, cw / W, ch / H))
    city.layers = []
    for i, (base, lo, hi, wmin, wmax) in enumerate(SPEC):
        L = Bag()
        L.base, L.hi = base, hi
        L.poly, L.chimneys, L.buildings = skyline(rng, W, int(base * H), int(lo * H), int(hi * H),
                                                  int(wmin * W), int(wmax * W), i)
        L.windows = []
        L.ww = max(3, int((0.0020 + 0.0007 * i) * W))
        L.wh = int(L.ww * 1.45)
        if LIT[i] > 0:
            for bi, b in enumerate(L.buildings):
                l, t, r, bot = b.rect
                office = b.roof == "flat" and i in (1, 2) and b.height > 0.5
                cols = max(1, int((r - l - L.ww) / (L.ww * 2.6)))
                pad = ((r - l) - (cols * L.ww * 2.6 - L.ww * 1.6)) / 2
                y = t + L.wh * 1.4
                while y + L.wh < min(bot, H) - L.wh:
                    for c in range(cols):
                        glass, tone = rng.random() < LIT[i], 0
                        if glass:
                            tone = WARM.index(rng.choice(WARM))
                        w = Bag()
                        w.x, w.y, w.b, w.office, w.glass = l + pad + c * L.ww * 2.6, y, bi, office, glass
                        w.who = (people.random(), people.random(), people.random(), people.random())
                        w.tone = tone if glass else people.randrange(4)
                        L.windows.append(w)
                    y += L.wh * 2.4
        city.layers.append(L)
    city.trees = []
    for side in (0, 1):
        for n in range(70):
            cx = (rng.uniform(-0.03, 0.17) if side == 0 else rng.uniform(0.83, 1.03)) * W
            edge = abs(cx / W - (0.0 if side == 0 else 1.0))
            cy = (0.915 + edge * 0.60 + rng.uniform(-0.012, 0.030)) * H
            city.trees.append((cx, cy, rng.uniform(0.010, 0.024) * W, n))
    cast(city)
    return city


def cast(city):
    """Give roofs their parts: the house that is yours, where the washing hangs, where the cat goes."""
    unit = REF_H / 1200.0
    mid = lambda f: (f[0] + f[1]) / 2 / REF_W
    wide = lambda f: f[1] - f[0]
    L3 = city.layers[3]
    flats3 = [(bi, b.flat) for bi, b in enumerate(L3.buildings) if b.flat]
    homes = [c for c in flats3 if 0.52 <= mid(c[1]) <= 0.80 and wide(c[1]) >= 86 * unit] or \
            sorted(flats3, key=lambda c: -wide(c[1]))[:1]
    city.house = min(homes, key=lambda c: c[1][2])           # the tallest of them
    rest = sorted((c for c in flats3 if c[0] != city.house[0] and 0.16 <= mid(c[1]) <= 0.86),
                  key=lambda c: -wide(c[1]))
    city.laundry = [c[1] for c in rest[:2]]
    city.festoon = [c[1] for c in rest[2:3]] or city.laundry[:1]
    spots = [(3, c[1]) for c in rest[3:]]
    spots += [(4, b.flat) for b in city.layers[4].buildings if b.flat and 0.22 <= mid(b.flat) <= 0.80]
    spots += [(2, b.flat) for b in city.layers[2].buildings
              if b.flat and 0.25 <= mid(b.flat) <= 0.75 and wide(b.flat) > 60 * unit]
    city.cat_spots = spots or [(3, city.house[1])]

    def nearest(layer, kinds, fx):
        pool = [b for b in city.layers[layer].buildings if b.roof in kinds]
        return min(pool or city.layers[layer].buildings, key=lambda b: abs(b.apex[0] / REF_W - fx))
    city.sleepers = [(3, nearest(3, ("gable", "mansard"), 0.30)), (2, nearest(2, ("gable",), 0.50)),
                     (3, nearest(3, ("gable", "mansard"), 0.90 - 0.45)), (2, nearest(2, ("gable",), 0.72))]
    city.kite_roof = nearest(2, ("gable", "mansard", "flat"), 0.34)
    city.smokers = [(i, c) for i in (2, 3, 4) for c in city.layers[i].chimneys
                    if 0.05 <= c[0] / REF_W <= 0.95]


# ── who is awake ─────────────────────────────────────────────────────────────

def home_on(who, t, dow):
    a, b, c, d = who

    def evening(day, tt):                 # an evening belongs to the day it began on
        late = day in (4, 5)              # Friday and Saturday nights
        if a >= (0.30 if late else 0.23 if day == 6 else 0.26):
            return False
        back = 14.0 + 5.5 * b if day >= 5 else 16.4 + 3.4 * b
        bed = 22.6 + 4.6 * d ** 1.4 if late else 21.4 + 3.0 * d ** 1.6
        if a < 0.012:
            bed = 28.5                    # the night owls
        return back <= tt < bed
    if evening(dow, t) or (t < 6 and evening((dow - 1) % 7, t + 24)):
        return True
    early = (a * 7.31 + c * 3.17) % 1.0
    if early >= (0.34, 0.26, 0.26, 0.26, 0.24, 0.09, 0.045)[dow]:
        return False
    wake = (5.3, 5.7, 5.7, 5.7, 5.8, 7.6, 8.6)[dow] + (1.7, 2.0, 2.0, 2.0, 2.0, 2.6, 2.6)[dow] * c
    return wake <= t < wake + 0.7 + 1.3 * d


def office_on(who, t, dow):
    a, b, c, d = who
    if dow < 5:
        if c < 0.62:
            close = 17.3 + 3.2 * b ** 1.5 + (0.8 if dow == 0 else -0.9 if dow == 4 else 0.0)
            if 7.0 + 1.6 * a <= t < close:
                return True
        return c > 0.985 and (t >= 21 or t < 5)       # the cleaners
    return c < 0.03 and 9.5 <= t < 16.5               # someone catching up


def window_state(w, sc):
    """None, or the colour this window glows right now."""
    a, b, c, d = w.who
    if w.office:
        if office_on(w.who, sc.t, sc.dow) and sc.el_eff < 3.0 - 2.0 * a:
            return COOL[w.tone]
        return None
    if home_on(w.who, sc.t, sc.dow) and sc.el_eff < 1.5 - 7.5 * ((b * 5.77 + d * 2.3) % 1.0):
        return SCREEN if a < 0.012 and (sc.t >= 23.5 or sc.t < 6) else WARM[w.tone]
    return None
