"""The painter: one still frame of the scene, drawn with Pillow."""
import math
import os
import random

from PIL import Image, ImageChops, ImageDraw, ImageFilter

from .city import REF_W, build_city, window_state
from .palette import AMBER, BEACON, BLUE, CREAM, ROSE, SAGE, STOPS, WARM, mix, smooth
from .sources import CACHE

SS = 2                        # supersampling: Pillow shapes have no antialiasing of their own


def gradient(w, h, stops):
    strip = Image.new("RGB", (1, h))
    px = strip.load()
    for y in range(h):
        t = y / (h - 1)
        for (p0, c0), (p1, c1) in zip(stops, stops[1:]):
            if p0 <= t <= p1:
                f = (t - p0) / (p1 - p0) if p1 > p0 else 0
                px[0, y] = mix(c0, c1, f * f * (3 - 2 * f))
                break
    return strip.resize((w, h), Image.BILINEAR)


def soft(img, shapes, colour, blur, div=8, rect=False):
    """Blend blurred ellipses (x0, y0, x1, y1, alpha) of one colour into the picture."""
    shapes = [s for s in shapes if s[4] >= 1 and s[2] > s[0] and s[3] > s[1]]
    if not shapes:
        return
    W, H = img.size
    pad = blur * 3.6 + div
    x0 = max(0, int(min(s[0] for s in shapes) - pad)) // div * div
    y0 = max(0, int(min(s[1] for s in shapes) - pad)) // div * div
    x1 = min(W, int(max(s[2] for s in shapes) + pad))
    y1 = min(H, int(max(s[3] for s in shapes) + pad))
    if x1 - x0 < div or y1 - y0 < div:
        return
    w, h = (x1 - x0) // div, (y1 - y0) // div
    mask = Image.new("L", (w, h), 0)
    md = ImageDraw.Draw(mask)
    for a, b, c, e, al in shapes:
        box = [(a - x0) / div, (b - y0) / div, (c - x0) / div, (e - y0) / div]
        (md.rectangle if rect else md.ellipse)(box, fill=min(255, int(al)))
    mask = mask.filter(ImageFilter.GaussianBlur(max(0.4, blur / div)))
    img.paste(colour, (x0, y0), mask.resize((w * div, h * div), Image.BILINEAR))


def draw_stars(d, sc, W, H, u, seed):
    r, twinkle = random.Random(seed + 1), random.Random(0 if sc.live else sc.slot)
    for _ in range(280):
        x, y = r.uniform(0, W), r.uniform(0.03, 0.66) * H
        size, base, tint = r.choice((1, 1, 1, 1.35, 1.9)), r.uniform(70, 235), r.random()
        fade = max(0.0, 1 - (y / (0.66 * H)) ** 1.6)
        a = base * fade * sc.night * (0.72 + 0.28 * twinkle.random())
        if a < 6:
            continue
        col = mix(sc.P["star"], (255, 214, 170) if tint < 0.12 else (190, 214, 255) if tint > 0.9 else sc.P["star"], 0.6)
        rr = size * u * 0.62
        d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=col + (int(min(255, a)),))
        if a > 105 and len(sc.out["stars"]) < 60:
            sc.out["stars"].append([round(x / W, 4), round(y / H, 4), round(size, 2), round(min(255, a) / 255.0, 2)])
    if sc.night > 0.6 and sc.slot % 23 == 7 and not sc.live:                      # now and then, a shooting star
        x, y = twinkle.uniform(0.2, 0.8) * W, twinkle.uniform(0.10, 0.34) * H
        for k in range(12):
            f = k / 12.0
            d.line([(x + f * 90 * u, y + f * 34 * u), (x + (f + 0.09) * 90 * u, y + (f + 0.09) * 34 * u)],
                   fill=sc.P["star"] + (int(200 * f),), width=max(1, int(1.3 * u)))


def draw_moon(img, d, sc, W, H, u):
    m = sc.moon
    if m.el < -3:
        return
    P = sc.P
    x, y, r = m.x * W, m.y * H, 15 * u
    vis = (0.32 + 0.68 * (1 - sc.day)) * (1 - 0.75 * smooth(0.5, 0.95, sc.cover)) * (1 - 0.8 * sc.fog)
    if vis < 0.05:
        return
    soft(img, [(x - r * 5, y - r * 5, x + r * 5, y + r * 5, 62 * m.lit * vis)], P["moon"], r * 2.0, div=4)
    d.ellipse([x - r, y - r, x + r, y + r], fill=P["moon"] + (int(26 * vis),))        # earthshine
    side = 1 if m.waxing == (sc.place["lat"] >= 0) else -1
    k = m.lit
    pts = [(x + side * r * math.cos(a), y + r * math.sin(a))
           for a in (-math.pi / 2 + math.pi * i / 40 for i in range(41))]
    pts += [(x + side * r * math.cos(a) * (1 - 2 * k), y + r * math.sin(a))
            for a in (math.pi / 2 - math.pi * i / 40 for i in range(41))]
    if k > 0.02:
        d.polygon(pts, fill=P["moon"] + (int(255 * vis),))


def draw_sun(img, d, sc, W, H, s, u):
    sun, ga = sc.sun, sc.glow_alpha
    x, y, hy = sun.x * W, sun.y * H, 0.80 * H
    gx = max(0.02 * W, min(0.98 * W, x))
    low = 1 - 0.75 * smooth(4, 26, sun.el)           # the wash along the horizon fades as the sun climbs
    for rr, a in ((0.42, 0.45), (0.20, 0.8), (0.08, 1.0)):
        rad = rr * W
        soft(img, [(gx - rad, hy - rad * 0.55, gx + rad, hy + rad * 0.55, ga * a * low)], sc.glow, 220 * s)
    if sun.el > -1.5 and sc.sun_vis > 0.02:
        up, R = smooth(-1.5, 5, sun.el), 21 * u
        for m, a in ((8.0, 0.30), (3.4, 0.55)):
            soft(img, [(x - R * m, y - R * m, x + R * m, y + R * m, min(255, ga * a * up * 1.5 * sc.sun_vis))],
                 sc.glow, R * m * 0.42, div=4)
        d.ellipse([x - R, y - R, x + R, y + R], fill=sc.sun_col + (int(255 * sc.sun_vis),))


def draw_clouds(img, sc, city, W, H, s, u, seed):
    r = random.Random(seed + 5)
    bands = list(city.clouds)
    for _ in range(12):
        bands.append((r.uniform(-0.3, 0.9), r.uniform(0.12, 0.52), r.uniform(0.3, 0.6), r.uniform(0.035, 0.08)))
    n = max(0, min(len(bands), int(round(1 + sc.cover * 13))))
    near_sun = lambda fx: max(0.0, 1 - abs(fx - sc.sun.x) / 0.6)
    for k, (cx, cy, cw, ch) in enumerate(bands[:n]):
        x = ((cx + sc.drift * (0.75 + 0.06 * (k % 5)) + 0.6) % 1.7 - 0.6) * W
        box = (x, cy * H, x + cw * W, (cy + ch) * H)
        soft(img, [box + (sc.cloud_alpha,)], sc.cloud, 180 * s)
        lit = 0.55 * sc.twilight * near_sun(x / W + cw / 2) * sc.sun_vis
        if lit > 0.03:                               # the low sun catches their undersides
            soft(img, [(box[0] + cw * W * 0.12, box[1] + ch * H * 0.55, box[2] - cw * W * 0.12,
                        box[3] + ch * H * 0.25, sc.cloud_alpha * lit)], sc.glow, 120 * s)
    # fair-weather heaps: only when the sky is part open and the sun is up
    heaps = int(round(sc.cover * 9)) if 0.15 < sc.cover < 0.8 and sc.sun.el > -4 else 0
    tone = mix(sc.cloud, sc.sky[5], 0.25)
    for k in range(heaps):
        px = ((r.uniform(0, 1.5) + sc.drift * 1.25 + 0.25) % 1.5 - 0.25) * W
        py, pw = r.uniform(0.34, 0.62) * H, r.uniform(70, 150) * u
        puffs = []
        for j in range(6):
            f = j / 5.0
            rr = pw * (0.20 + 0.16 * math.sin(f * math.pi)) * r.uniform(0.85, 1.15)
            puffs.append((px + (f - 0.5) * pw * 1.5 - rr, py - rr * 1.25, px + (f - 0.5) * pw * 1.5 + rr, py + rr * 0.45,
                          sc.cloud_alpha * 0.95))
        soft(img, puffs, tone, 11 * u, div=4)


def draw_planes(d, sc, W, H, u):
    P = sc.P
    for fx, fy, way, slope in sc.planes:
        x, y = fx * W, fy * H
        if sc.day > 0.25:
            col = mix(P["trail"], sc.glow, 0.6 * sc.twilight)
            length, n = 0.26 * W, 18
            for k in range(n):
                f0, f1 = k / n, (k + 1) / n
                xa, xb = x - way * length * f0, x - way * length * f1
                ya, yb = y - slope * H * 0.26 * f0, y - slope * H * 0.26 * f1
                ha, hb = (0.9 + 2.6 * f0) * u, (0.9 + 2.6 * f1) * u
                d.polygon([(xa, ya - ha), (xb, yb - hb), (xb, yb + hb), (xa, ya + ha)],
                          fill=col + (int(150 * sc.day * (1 - f0) ** 1.5),))
            d.ellipse([x - 1.8 * u, y - 1.5 * u, x + 1.8 * u, y + 1.5 * u], fill=col + (235,))
        else:
            on = sc.slot % 2 == 0
            d.ellipse([x - 1.5 * u, y - 1.5 * u, x + 1.5 * u, y + 1.5 * u], fill=P["star"] + (240 if on else 150,))
            d.ellipse([x - way * 5 * u - 1.3 * u, y - 1.3 * u, x - way * 5 * u + 1.3 * u, y + 1.3 * u],
                      fill=BEACON + (120 if on else 235,))


def draw_birds(d, sc, W, H, u):
    for fx, fy, way in sc.birds:
        col = mix(sc.P["bird"], sc.sky[3], 0.25) + (210,)
        for k in range(-4, 4):
            bx = fx * W - way * abs(k) * 15 * u + (k % 2) * 3 * u
            by = fy * H + k * 8 * u
            dip = (2.2 + 2.2 * ((sc.slot + k) % 3) / 2.0) * u
            d.line([(bx - 6.5 * u, by - dip), (bx, by), (bx + 6.5 * u, by - dip)], fill=col,
                   width=max(1, int(1.5 * u)), joint="curve")


def draw_balloon(d, sc, W, H, u):
    f = (sc.t - 7.5) / 4.0
    x, y = (0.26 + 0.34 * f) * W, (0.56 - 0.10 * math.sin(f * math.pi) + 0.01 * math.sin(sc.ts / 500.0)) * H
    far = lambda c: mix(sc.sky[4], c, 0.80)
    bw, bh = 27 * u, 33 * u
    d.ellipse([x - bw / 2, y - bh / 2, x + bw / 2, y + bh / 2], fill=far(AMBER))
    d.polygon([(x - bw * 0.43, y + bh * 0.20), (x + bw * 0.43, y + bh * 0.20), (x + 3.5 * u, y + bh * 0.74),
               (x - 3.5 * u, y + bh * 0.74)], fill=far(AMBER))
    d.ellipse([x - bw * 0.19, y - bh / 2, x + bw * 0.19, y + bh * 0.52], fill=far(CREAM))
    d.polygon([(x - bw * 0.17, y + bh * 0.2), (x + bw * 0.17, y + bh * 0.2), (x + 1.5 * u, y + bh * 0.74),
               (x - 1.5 * u, y + bh * 0.74)], fill=far(CREAM))
    lw = max(1, int(u))
    d.line([(x - 3.5 * u, y + bh * 0.74), (x - 3 * u, y + bh * 0.90)], fill=far(sc.layers[3]), width=lw)
    d.line([(x + 3.5 * u, y + bh * 0.74), (x + 3 * u, y + bh * 0.90)], fill=far(sc.layers[3]), width=lw)
    d.rectangle([x - 3.5 * u, y + bh * 0.88, x + 3.5 * u, y + bh * 0.88 + 5 * u], fill=far(sc.layers[3]))


def draw_kite(d, sc, city, W, H, s, u):
    ax, ay = city.kite_roof.apex[0] * s, city.kite_roof.apex[1] * s
    lean = sc.wind_x if abs(sc.wind_x) > 0.2 else 0.4
    x = ax + lean * 110 * u + math.sin(sc.ts / 420.0) * 22 * u
    y = ay - (215 + 26 * math.sin(sc.ts / 700.0)) * u
    d.line([(ax, ay), ((ax + x) / 2 + lean * 14 * u, (ay + y) / 2 + 26 * u), (x, y + 13 * u)],
           fill=sc.P["ink"] + (110,), width=max(1, int(u)), joint="curve")
    col = mix(sc.sky[3], ROSE, 0.9)
    d.polygon([(x, y - 14 * u), (x + 9 * u, y), (x, y + 14 * u), (x - 9 * u, y)], fill=col)
    d.line([(x, y - 14 * u), (x, y + 14 * u)], fill=mix(col, (0, 0, 0), 0.25), width=max(1, int(u)))
    tail = [(x - lean * k * 6 * u + math.sin(k * 1.4 + sc.ts / 300.0) * 4 * u, y + 14 * u + k * 8 * u) for k in range(6)]
    d.line(tail, fill=mix(sc.sky[3], AMBER, 0.9), width=max(1, int(1.5 * u)), joint="curve")


def draw_fireworks(img, d, sc, W, H, u):
    r = random.Random(sc.slot)
    for col in (AMBER, ROSE, BLUE, SAGE)[:3 + sc.slot % 2]:
        cx, cy, rad = r.uniform(0.2, 0.8) * W, r.uniform(0.22, 0.5) * H, r.uniform(50, 95) * u
        soft(img, [(cx - rad, cy - rad, cx + rad, cy + rad, 60)], col, rad * 0.5, div=4)
        for k in range(22):
            a = k / 22.0 * 2 * math.pi + r.uniform(-0.08, 0.08)
            x1, y1 = cx + math.cos(a) * rad, cy + math.sin(a) * rad + rad * 0.12
            d.line([(cx + math.cos(a) * rad * 0.35, cy + math.sin(a) * rad * 0.35), (x1, y1)],
                   fill=col + (170,), width=max(1, int(1.2 * u)))
            d.ellipse([x1 - 1.8 * u, y1 - 1.8 * u, x1 + 1.8 * u, y1 + 1.8 * u], fill=mix(col, (255, 255, 255), 0.5) + (240,))


def draw_lightning(img, d, sc, W, H, u):
    r = random.Random(sc.slot * 3 + 1)
    if r.random() < 0.35:
        return
    x, y = r.uniform(0.15, 0.85) * W, 0.20 * H
    soft(img, [(x - 0.25 * W, 0.05 * H, x + 0.25 * W, 0.6 * H, 70)], (214, 224, 240), 200 * u)
    pts = [(x, y)]
    while y < 0.80 * H:
        x, y = x + r.uniform(-34, 34) * u, y + r.uniform(22, 60) * u
        pts.append((x, y))
    d.line(pts, fill=(236, 240, 250, 235), width=max(1, int(2.2 * u)), joint="curve")
    fork = pts[len(pts) // 2:len(pts) // 2 + 4]
    d.line([(px + (i * 16) * u, py) for i, (px, py) in enumerate(fork)], fill=(236, 240, 250, 150), width=max(1, int(1.2 * u)))


def draw_crane(img, d, sc, W, H, u, col):
    x, y, top = 0.795 * W, 0.87 * H, 0.655 * H
    lw, mw = max(1, int(1.5 * u)), 4.5 * u
    for sx in (-mw, mw):
        d.line([(x + sx, y), (x + sx, top)], fill=col, width=lw)
    yy, flip = y, 1
    while yy > top:
        d.line([(x - mw * flip, yy), (x + mw * flip, max(top, yy - 13 * u))], fill=col, width=max(1, lw - 1))
        yy, flip = yy - 13 * u, -flip
    d.rectangle([x - 7 * u, top - 9 * u, x + 7 * u, top], fill=col)
    apex = (x, top - 30 * u)
    # The jib turns on working days; seen from here that shortens and lengthens it.
    if sc.crane_works:
        r = random.Random(sc.slot // 2)
        turn, reach, drop = math.cos(r.uniform(0, math.pi)), r.uniform(0.35, 0.95), r.uniform(30, 150)
    else:
        turn, reach, drop = -1.0, 0.25, 14
    if abs(turn) < 0.18:
        turn = 0.18 if turn >= 0 else -0.18
    jl, cj, jy = 128 * u * turn, -40 * u * turn, top - 9 * u
    for off in (0, 4 * u):
        d.line([(x + cj, jy - off), (x + jl, jy - off)], fill=col, width=lw)
    d.line([(x, top), apex], fill=col, width=lw)
    d.line([apex, (x + jl * 0.72, jy - 4 * u)], fill=col, width=max(1, lw - 1))
    d.line([apex, (x + cj, jy - 4 * u)], fill=col, width=max(1, lw - 1))
    d.rectangle(sorted_box(x + cj, jy, x + cj * 0.62, jy + 9 * u), fill=col)
    hx = x + jl * reach
    d.line([(hx, jy), (hx, jy + drop * u)], fill=col, width=max(1, lw - 1))
    d.rectangle([hx - 2.5 * u, jy + drop * u, hx + 2.5 * u, jy + (drop + 5) * u], fill=col)
    if sc.dark > 0.3:
        soft(img, [(apex[0] - 9 * u, apex[1] - 9 * u, apex[0] + 9 * u, apex[1] + 9 * u, 150 * sc.dark)], BEACON, 4 * u, div=2)
        d.ellipse([apex[0] - 2.2 * u, apex[1] - 2.2 * u, apex[0] + 2.2 * u, apex[1] + 2.2 * u], fill=BEACON + (255,))


def sorted_box(x0, y0, x1, y1):
    return [min(x0, x1), min(y0, y1), max(x0, x1), max(y0, y1)]


def draw_smoke(img, sc, x, y, u, col, phase):
    lean, puffs = sc.wind_x, []
    for k in range(9):
        f = k / 8.0
        px = x + lean * (k ** 1.3) * 5.0 * u + math.sin(phase + k * 0.9) * 2.4 * u
        py = y - k * (11.5 - 3.5 * min(1.5, abs(lean))) * u
        r = (3.5 + k * 2.4) * u
        puffs.append((px - r, py - r * 0.8, px + r, py + r * 0.8, 115 * (1 - f) ** 1.2))
    soft(img, puffs, col, 5 * u, div=2)


def draw_sleep(d, sc, x, y, u):
    sway = math.sin(sc.ts / 400.0 + x) * 3 * u
    for size, dx, dy, a in ((7, 0, 0, 215), (10.5, 13, -17, 170), (15, 31, -40, 120)):
        zx, zy, z = x + dx * u + sway * (dy / -40.0), y + dy * u, size * u
        d.line([(zx, zy - z), (zx + z, zy - z), (zx, zy), (zx + z, zy)], fill=sc.P["ink"] + (a,),
               width=max(1, int(1.7 * u)), joint="curve")


def draw_laundry(d, sc, flat, s, u, col, k):
    x0, x1, y = flat[0] * s, flat[1] * s, flat[2] * s
    inset = (x1 - x0) * 0.10
    a, b, ph = x0 + inset, x1 - inset, 25 * u
    lw = max(1, int(1.3 * u))
    d.line([(a, y), (a, y - ph)], fill=col, width=lw)
    d.line([(b, y), (b, y - ph)], fill=col, width=lw)
    sag = lambda f: y - ph + 4 * 5 * u * f * (1 - f)
    d.line([(a + (b - a) * f / 12.0, sag(f / 12.0)) for f in range(13)], fill=col, width=max(1, lw - 1), joint="curve")
    r = random.Random(77 + k)
    cloths = (CREAM, SAGE, BLUE, AMBER, ROSE, CREAM)
    x = a + 6 * u
    while x < b - 16 * u:
        w, h = r.uniform(8, 15) * u, r.uniform(10, 19) * u
        if r.random() < 0.82:
            f0, f1 = (x - a) / (b - a), (x + w - a) / (b - a)
            lean = sc.wind_x * 5 * u * (0.6 + 0.4 * math.sin(sc.ts / 240.0 + x))
            tint = mix(col, r.choice(cloths), 0.30 + 0.62 * sc.day)
            d.polygon([(x, sag(f0)), (x + w, sag(f1)), (x + w + lean, sag(f1) + h), (x + lean, sag(f0) + h)], fill=tint)
        x += w + r.uniform(2, 7) * u


def draw_festoon(img, d, sc, flat, s, u, col):
    x0, x1, y = flat[0] * s, flat[1] * s, flat[2] * s
    posts = [x0 + (x1 - x0) * f for f in (0.06, 0.5, 0.94)]
    ph, lw = 30 * u, max(1, int(1.3 * u))
    for px in posts:
        d.line([(px, y), (px, y - ph)], fill=col, width=lw)
    glows, bulbs = [], []
    tints = (AMBER, ROSE, WARM[0], SAGE, WARM[1], BLUE)
    for a, b in zip(posts, posts[1:]):
        n = max(4, int((b - a) / (9 * u)))
        line = [(a + (b - a) * k / n, y - ph + 4 * 8 * u * (k / n) * (1 - k / n)) for k in range(n + 1)]
        d.line(line, fill=col, width=max(1, lw - 1), joint="curve")
        bulbs += [(px, py + 2 * u, tints[(k + int(a)) % len(tints)]) for k, (px, py) in enumerate(line[1:-1])]
    for px, py, tint in bulbs:
        glows.append((px - 5 * u, py - 5 * u, px + 5 * u, py + 5 * u, 120))
    soft(img, glows, WARM[1], 3.5 * u, div=2)
    for px, py, tint in bulbs:
        d.ellipse([px - 1.9 * u, py - 1.9 * u, px + 1.9 * u, py + 1.9 * u], fill=mix(tint, (255, 255, 255), 0.25) + (255,))


def draw_cat(d, x, y, u, col, asleep, face):
    if asleep:
        d.ellipse([x - 12 * u, y - 9 * u, x + 12 * u, y + 2 * u], fill=col)
        hx, hy = x + face * 10 * u, y - 7 * u
        d.ellipse([hx - 5 * u, hy - 4.5 * u, hx + 5 * u, hy + 4.5 * u], fill=col)
        for e in (-1, 1):
            d.polygon([(hx + e * 1.2 * u, hy - 3 * u), (hx + e * 4.6 * u, hy - 8.5 * u), (hx + e * 5 * u, hy - 1.5 * u)], fill=col)
        d.line([(x - face * 11 * u, y - 3 * u), (x - face * 15 * u, y - 0.5 * u), (x - face * 6 * u, y + 0.5 * u)],
               fill=col, width=max(1, int(2.6 * u)), joint="curve")
        return
    d.ellipse([x - 6.5 * u, y - 17 * u, x + 6.5 * u, y + 2 * u], fill=col)
    d.polygon([(x - 6.5 * u, y - 6 * u), (x + 6.5 * u, y - 6 * u), (x + 7.5 * u, y + 1 * u), (x - 7.5 * u, y + 1 * u)], fill=col)
    hx, hy = x + face * 2 * u, y - 20.5 * u
    d.ellipse([hx - 5.4 * u, hy - 4.8 * u, hx + 5.4 * u, hy + 4.8 * u], fill=col)
    for e in (-1, 1):
        d.polygon([(hx + e * 1.0 * u, hy - 3.2 * u), (hx + e * 4.8 * u, hy - 9.5 * u), (hx + e * 5.4 * u, hy - 1.5 * u)], fill=col)
    d.line([(x - face * 5 * u, y - 1 * u), (x - face * 13 * u, y - 1 * u), (x - face * 16 * u, y - 6 * u),
            (x - face * 14 * u, y - 10 * u)], fill=col, width=max(1, int(2.4 * u)), joint="curve")


def draw_house(img, d, sc, city, s, u, col):
    """The house that stands for this machine."""
    x0, x1, y = (v * s for v in city.house[1])
    cx = (x0 + x1) / 2
    bw, bh = 46 * u, 30 * u
    d.rectangle([cx - bw / 2, y - bh, cx + bw / 2, y + 1], fill=col)
    d.polygon([(cx - bw / 2 - 5 * u, y - bh), (cx, y - bh - 13 * u), (cx + bw / 2 + 5 * u, y - bh)], fill=col)
    wx0, wy0, wx1, wy1 = cx - 14 * u, y - bh + 7 * u, cx + 14 * u, y - 6 * u
    if sc.agents:                                     # someone is working in the roof room
        lamp = mix(AMBER, (255, 236, 190), 0.45)
        soft(img, [(wx0 - 16 * u, wy0 - 14 * u, wx1 + 16 * u, wy1 + 14 * u, 70 + 100 * sc.dark)], AMBER, 9 * u, div=2)
        d.rectangle([wx0, wy0, wx1, wy1], fill=lamp)
    else:
        d.rectangle([wx0, wy0, wx1, wy1], fill=mix(col, sc.sky[5], 0.10 + 0.35 * sc.day))
    d.line([(cx, wy0), (cx, wy1)], fill=col, width=max(1, int(1.6 * u)))
    d.line([(wx0, (wy0 + wy1) / 2), (wx1, (wy0 + wy1) / 2)], fill=col, width=max(1, int(1.2 * u)))
    # the pennant shows the wind
    mx, top = cx + bw / 2 - 5 * u, y - bh - 46 * u
    d.line([(mx, y - bh), (mx, top)], fill=col, width=max(1, int(1.7 * u)))
    lean = sc.wind_x if abs(sc.wind_x) > 0.12 else 0.0
    reach = 19 * u * (1 if lean >= 0 else -1) * min(1.0, 0.35 + abs(lean))
    droop = (1 - min(1.0, abs(lean) * 1.2)) * 9 * u + math.sin(sc.ts / 200.0) * 1.5 * u
    flag = mix(col, AMBER, 0.42 + 0.58 * sc.day)
    d.polygon([(mx, top), (mx + reach, top + 5 * u + droop), (mx, top + 10 * u)], fill=flag)
    # one blue window for each running container
    n = min(sc.containers, 8)
    if n:
        boxes = [(cx - (n - 1) * 7 * u + k * 14 * u - 4 * u, y + 10 * u, cx - (n - 1) * 7 * u + k * 14 * u + 4 * u, y + 22 * u)
                 for k in range(n)]
        soft(img, [(a - 4 * u, b - 4 * u, c + 4 * u, e + 4 * u, 60 + 90 * sc.dark) for a, b, c, e in boxes], BLUE, 4 * u, div=2)
        for box in boxes:
            d.rectangle(list(box), fill=mix(BLUE, (255, 255, 255), 0.18))
    return cx, y, bw


def draw_trees(d, sc, city, s, u):
    P = sc.P
    tints = P["seasons"][sc.season]
    light = P["tree_light"] * smooth(-8, 16, sc.sun.el) * (1 - 0.4 * sc.gloom)
    for cx, cy, r, n in city.trees:
        base = mix(P["tree"][1] if n % 5 == 0 else P["tree"][0], P["tree_night"], 0.6 * (1 - sc.day))
        col = mix(base, tints[n % 3], light)
        d.ellipse([(cx - r) * s, (cy - r * 0.9) * s, (cx + r) * s, (cy + r * 0.9) * s], fill=col)
        if sc.snow > 0.3:
            d.pieslice([(cx - r) * s, (cy - r * 0.9) * s, (cx + r) * s, (cy + r * 0.9) * s], 200, 340, fill=mix(col, P["snow"], 0.7))
        elif sc.season == 1 and n % 4 == 0 and light > 0.1:            # blossom
            bl = random.Random(n)
            for _ in range(7):
                bx, by = cx + bl.uniform(-0.7, 0.7) * r, cy + bl.uniform(-0.7, 0.3) * r
                d.ellipse([(bx * s - 2.2 * u), (by * s - 2.2 * u), (bx * s + 2.2 * u), (by * s + 2.2 * u)],
                          fill=mix(col, (232, 178, 192), 0.3 + light))


def draw_weather(d, sc, W, H, u):
    P = sc.P
    r = random.Random(sc.slot * 31 + 7)
    area = (W * H) / (3840.0 * 2400.0)
    if sc.rain > 0:
        lean = sc.wind_x * 0.45
        vis = 0.55 + 0.45 * sc.day
        for _ in range(int(sc.rain * 1700 * area)):
            x, y = r.uniform(-0.2, 1.2) * W, r.uniform(-0.05, 1.0) * H
            ln = r.uniform(16, 44) * u * (0.6 + 0.5 * sc.rain)
            d.line([(x, y), (x + lean * ln, y + ln)], fill=P["rain"] + (int(r.uniform(28, 82) * vis),),
                   width=max(1, int(1.15 * u)))
    if sc.snow > 0:
        for _ in range(int(sc.snow * 900 * area)):
            x, y = r.uniform(0, W), r.uniform(0, H)
            rr = r.choice((1.0, 1.3, 1.8, 2.6)) * u
            d.ellipse([x - rr, y - rr, x + rr, y + rr], fill=P["snow"] + (int(r.uniform(110, 230)),))


def render(sc, w, h, seed=12, city=None):
    city = city or build_city(seed)
    P = sc.P
    W, H = w * SS, h * SS
    s, u = W / REF_W, H / 1200.0        # city scale; and one pixel of a 1200-line screen, for the small things
    img = gradient(W, H, list(zip(STOPS, sc.sky)))
    d = ImageDraw.Draw(img, "RGBA")

    if sc.night > 0.02:
        draw_stars(d, sc, W, H, u, seed)
    draw_moon(img, d, sc, W, H, u)
    draw_sun(img, d, sc, W, H, s, u)
    draw_clouds(img, sc, city, W, H, s, u, seed)
    if not sc.live:
        draw_planes(d, sc, W, H, u)
    if sc.storm and not sc.live:
        draw_lightning(img, d, sc, W, H, u)
    if sc.fireworks:
        draw_fireworks(img, d, sc, W, H, u)
    if not sc.live:
        draw_birds(d, sc, W, H, u)
    if sc.balloon:
        draw_balloon(d, sc, W, H, u)
    if sc.kite:
        draw_kite(d, sc, city, W, H, s, u)

    pt = lambda p: (p[0] * s, p[1] * s)
    smoke_pick = random.Random(seed + 3)
    smoking = {c for i, c in city.smokers if smoke_pick.random() < sc.smoke}
    cat_r = random.Random(sc.day_id * 7 + int(sc.t // 1.5))
    cat_layer, cat_flat = cat_r.choice(city.cat_spots)
    cat_at, cat_face = cat_r.uniform(0.25, 0.75), cat_r.choice((-1, 1))
    lit_count = total = 0

    for i, L in enumerate(city.layers):
        col = sc.layers[i]
        top = (L.base - L.hi) * H
        if sc.haze_alpha[i] > 1:
            soft(img, [(-W / 4, top, W + W / 4, L.base * H + 0.10 * H, sc.haze_alpha[i])], sc.haze, 140 * s)
        if sc.fog > 0 and i < 4:
            soft(img, [(-W / 4, top - 0.10 * H, W + W / 4, H * 1.2, 120 * sc.fog)], mix(sc.sky[4], sc.haze, 0.5), 160 * s)
        if i == 4 and sc.traffic * sc.dark > 0.05:        # the street between the last two rows of roofs
            soft(img, [(-W / 6, 0.905 * H, W + W / 6, 1.02 * H, min(95, 85 * sc.traffic * sc.dark))], WARM[3], 60 * s)
        if sc.snow > 0.3:                                   # snow lies on every upward edge
            lift = (3 + i) * u
            d.polygon([(x * s, y * s - lift) for x, y in L.poly], fill=mix(col, P["snow"], 0.85))
            for c in L.chimneys:
                d.rectangle([c[0] * s, c[1] * s - lift, c[2] * s, c[3] * s], fill=mix(col, P["snow"], 0.85))
        d.polygon([pt(p) for p in L.poly], fill=col)
        for c in L.chimneys:
            d.rectangle([c[0] * s, c[1] * s, c[2] * s, c[3] * s], fill=col)
        if i == 1:
            draw_crane(img, d, sc, W, H, u, col)

        # windows
        ww, wh = L.ww * s, L.wh * s
        glass = mix(col, sc.sky[5], 0.6) + (int(235 * smooth(-5, 4, sc.el_eff)),)
        halos = []
        for win in L.windows:
            if i == 3 and win.b == city.house[0]:
                continue
            total += 1
            x, y = win.x * s, win.y * s
            glow = window_state(win, sc)
            if glow:
                lit_count += 1
                d.rectangle([x, y, x + ww, y + wh], fill=glow + (235,))
                if i >= 2 and not win.office and (win.who[2] * 13.7) % 1.0 < 0.07 and len(sc.out["tv"]) < 12:
                    sc.out["tv"].append([round(v, 4) for v in (x / W, y / H, ww / W, wh / H)])
                halos.append((x - ww * 0.4, y - ww * 0.4, x + ww * 1.4, y + wh + ww * 0.4, 255 * 0.55 * (0.45 + 0.55 * sc.dark)))
            elif win.glass and glass[3] > 8:
                d.rectangle([x, y, x + ww, y + wh], fill=glass)
        soft(img, halos, WARM[1], L.ww * 1.1 * s, div=2, rect=True)

        # the things that live on this row of roofs
        for c in L.chimneys:
            if c in smoking:
                tone = mix(col, sc.sky[4], 0.55)
                sc.out["smoke"].append([round((c[0] + c[2]) / 2 * s / W, 4), round(c[1] * s / H, 4), round(0.7 + 0.15 * i, 2), list(tone)])
                if not sc.live:
                    draw_smoke(img, sc, (c[0] + c[2]) / 2 * s, c[1] * s, u * (0.7 + 0.15 * i), tone, c[0])
        for k, (layer, b) in enumerate(city.sleepers[:sc.sleepers]):
            if layer == i:
                draw_sleep(d, sc, b.apex[0] * s + 6 * u, b.apex[1] * s - 12 * u, u)
        if i == 3:
            for k, flat in enumerate(city.laundry[:sc.laundry]):
                draw_laundry(d, sc, flat, s, u, col, k)
            if sc.festoon:
                draw_festoon(img, d, sc, city.festoon[0], s, u, col)
            hx, hy, hw = draw_house(img, d, sc, city, s, u, col)
            if sc.cat_out and sc.cat_asleep:              # it sleeps at home, beside the roof room
                draw_cat(d, hx - hw / 2 - 20 * u, hy, u, col, True, 1)
        if sc.cat_out and not sc.cat_asleep and cat_layer == i:
            cx = (cat_flat[0] + (cat_flat[1] - cat_flat[0]) * cat_at) * s
            draw_cat(d, cx, cat_flat[2] * s, u * (0.85 + 0.12 * i), col, False, cat_face)

    draw_trees(d, sc, city, s, u)
    if not sc.live:
        draw_weather(d, sc, W, H, u)
    sc.lit_share = lit_count / max(1, total)
    hx0, hx1, hy = (v * s for v in city.house[1])
    if sc.agents:
        sc.out["lamp"] = [round((hx0 + hx1) / 2 / W, 4), round((hy - 16.5 * u) / H, 4), round(28 * u / W, 4), round(17 * u / H, 4)]
    if sc.dark > 0.3:
        sc.out["beacon"] = [0.795, round(0.655 - 30 * u / H, 4)]

    out = img.resize((w, h), Image.LANCZOS)
    if P["grain"]:
        out = ImageChops.overlay(out, grain(w, h, P["grain"]))
    return out


def grain(w, h, amount):
    """The same film grain every time, so two frames differ only where the scene does."""
    path = os.path.join(CACHE, "grain-%dx%d-%d.png" % (w, h, amount))
    try:
        return Image.open(path).convert("RGB")
    except OSError:
        pass
    noise = Image.effect_noise((w, h), amount).convert("RGB")
    tile = Image.blend(Image.new("RGB", (w, h), (128, 128, 128)), noise, 0.35)
    try:
        os.makedirs(CACHE, exist_ok=True)
        tile.save(path)
    except OSError:
        pass
    return tile
