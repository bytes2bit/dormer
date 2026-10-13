"""Sun, moon and weather codes: what the sky is doing over one place."""
import math

from .palette import Bag

HORIZON = 0.770               # where the sun meets the far roofs, as a fraction of the height
ARC = 0.30                    # how far above that the highest summer sun climbs


def ephemeris(ts, lat_deg, lon_deg):
    """Low-precision sun and moon: good to a fraction of a degree, which is a few pixels here."""
    rad, sin, cos = math.radians, math.sin, math.cos
    d = ts / 86400.0 - 10957.5                      # days since J2000.0
    g = rad((357.529 + 0.98560028 * d) % 360)
    lam_s = (280.459 + 0.98564736 * d + 1.915 * sin(g) + 0.020 * sin(2 * g)) % 360
    eps = rad(23.439 - 0.00000036 * d)
    ls = rad(lam_s)
    s_ra, s_dec = math.atan2(cos(eps) * sin(ls), cos(ls)), math.asin(sin(eps) * sin(ls))
    T = d / 36525.0
    sn = lambda a: sin(rad(a))
    lam_m = (218.32 + 481267.881 * T + 6.29 * sn(135.0 + 477198.87 * T) - 1.27 * sn(259.3 - 413335.36 * T)
             + 0.66 * sn(235.7 + 890534.22 * T) + 0.21 * sn(269.9 + 954397.74 * T)
             - 0.19 * sn(357.5 + 35999.05 * T) - 0.11 * sn(186.5 + 966404.03 * T)) % 360
    beta = rad(5.13 * sn(93.3 + 483202.02 * T) + 0.28 * sn(228.2 + 960400.89 * T)
               - 0.28 * sn(318.3 + 6003.15 * T) - 0.17 * sn(217.6 - 407332.21 * T))
    lm = rad(lam_m)
    m_ra = math.atan2(cos(eps) * cos(beta) * sin(lm) - sin(eps) * sin(beta), cos(beta) * cos(lm))
    m_dec = math.asin(sin(eps) * cos(beta) * sin(lm) + cos(eps) * sin(beta))
    lst = rad((280.46061837 + 360.98564736629 * d) % 360) + rad(lon_deg)
    lat = rad(lat_deg)

    def body(ra, dec, dip):
        ha = (lst - ra + math.pi) % (2 * math.pi) - math.pi
        el = math.asin(sin(lat) * sin(dec) + cos(lat) * cos(dec) * cos(ha))
        az = math.atan2(-sin(ha), math.tan(dec) * cos(lat) - sin(lat) * cos(ha)) % (2 * math.pi)
        c = (sin(rad(dip)) - sin(lat) * sin(dec)) / (cos(lat) * cos(dec))
        h0 = max(0.35, math.acos(max(-1.0, min(1.0, c))))      # hour angle at which it sets
        ref = sin(rad(min(89.0, 90 - abs(lat_deg) + 23.44)))
        o = Bag()
        o.el, o.az, o.ha, o.h0 = math.degrees(el), math.degrees(az), ha, h0
        side = 1 if lat_deg >= 0 else -1                       # south of the equator the sun crosses the north
        o.x = 0.5 + side * (ha / h0) * 0.36
        o.y = HORIZON - ARC * sin(el) / ref
        o.rise, o.set = ts + (-h0 - ha) / (2 * math.pi) * 86400, ts + (h0 - ha) / (2 * math.pi) * 86400
        return o
    sun, moon = body(s_ra, s_dec, -0.833), body(m_ra, m_dec, 0.125)
    elong = (lam_m - lam_s) % 360
    moon.lit, moon.waxing, moon.elong = (1 - cos(rad(elong))) / 2, elong < 180, elong
    return sun, moon


def from_wmo(code):
    """Open-Meteo's weather code as the handful of things this picture can show."""
    w = dict(kind="clear", cover=0.05, rain=0.0, snow=0.0, fog=0.0, storm=False)
    if code == 1:
        w.update(kind="mostly clear", cover=0.22)
    elif code == 2:
        w.update(kind="partly cloudy", cover=0.5)
    elif code == 3:
        w.update(kind="overcast", cover=0.95)
    elif code in (45, 48):
        w.update(kind="fog", cover=0.75, fog=1.0)
    elif code in (51, 53, 55, 56, 57):
        w.update(kind="drizzle", cover=0.92, rain={51: 0.2, 53: 0.3}.get(code, 0.4))
    elif code in (61, 66, 80):
        w.update(kind="light rain", cover=1.0, rain=0.55)
    elif code in (63, 67, 81):
        w.update(kind="rain", cover=1.0, rain=0.75)
    elif code in (65, 82):
        w.update(kind="heavy rain", cover=1.0, rain=1.0)
    elif code in (71, 73, 75, 77, 85, 86):
        w.update(kind="snow", cover=0.95, snow={71: 0.4, 77: 0.3, 85: 0.5, 73: 0.7}.get(code, 1.0))
    elif code in (95, 96, 99):
        w.update(kind="thunderstorm", cover=1.0, rain=0.9, storm=True)
    return w


PRESETS = {"clear": 0, "fair": 1, "partly": 2, "overcast": 3, "fog": 45, "drizzle": 53,
           "rain": 63, "heavy": 65, "snow": 73, "storm": 95}


def preset(name, like):
    """A named kind of weather, keeping the wind and temperature of `like`."""
    w = from_wmo(PRESETS[name])
    w.update(temp=like["temp"], wind=like["wind"], wdir=like["wdir"], known=True)
    return w


def describe(w):
    """A name for weather given as numbers (the dev panel's sliders)."""
    if w.get("storm"):
        return "thunderstorm"
    if w.get("snow", 0) > 0.05:
        return "snow"
    if w.get("rain", 0) > 0.05:
        return "drizzle" if w["rain"] < 0.45 else "rain" if w["rain"] < 0.9 else "heavy rain"
    if w.get("fog", 0) > 0.3:
        return "fog"
    cover = w.get("cover", 0)
    return "clear" if cover < 0.15 else "mostly clear" if cover < 0.38 else "partly cloudy" if cover < 0.75 else "overcast"
