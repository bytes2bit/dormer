"""Colour, and the few small tools everything else leans on.

The tables run night, deep twilight, civil twilight, sun on the horizon,
golden hour, day; KEYS gives the sun elevation each row belongs to and
`keyed` blends between them. The first sky stop of every row is the colour
of your bar, so the bar never shows an edge.
"""
import math

STOPS = (0.00, 0.22, 0.46, 0.62, 0.72, 0.79, 1.00)
KEYS = (-16.0, -9.0, -3.0, 1.0, 8.0, 22.0)   # sun elevations the colour rows below belong to


class Bag:
    """A plain record."""


def rgb(h):
    h = h.lstrip("#")
    return tuple(int(h[i:i + 2], 16) for i in (0, 2, 4))


def row(s):
    return [rgb(c) for c in s.split()]


def mix(a, b, t):
    t = max(0.0, min(1.0, t))
    return tuple(int(round(a[i] + (b[i] - a[i]) * t)) for i in range(3))


def smooth(a, b, x):
    t = max(0.0, min(1.0, (x - a) / (b - a)))
    return t * t * (3 - 2 * t)


def gauss(x, mu, sig):
    return math.exp(-((x - mu) / sig) ** 2 / 2)


def keyed(rows, el):
    """Interpolate a row of the tables below at this sun elevation."""
    if el <= KEYS[0]:
        return rows[0]
    if el >= KEYS[-1]:
        return rows[-1]
    for i in range(len(KEYS) - 1):
        if el <= KEYS[i + 1]:
            f = (el - KEYS[i]) / (KEYS[i + 1] - KEYS[i])
            a, b = rows[i], rows[i + 1]
            if isinstance(a, (int, float)):
                return a + (b - a) * f
            if isinstance(a[0], int):
                return mix(a, b, f)
            return [mix(p, q, f) for p, q in zip(a, b)]


# Rows run night, deep twilight, civil twilight, sun on the horizon, golden hour, day.
# The first sky stop is always the bar's colour, so the bar never shows an edge.
PALETTES = {
    "dark": {
        "sky": [row("1A2027 1A212A 1B2531 1D2A38 20303F 243544 27394A"),
                row("1A2027 1C2531 213245 2D4159 4A4C63 7C5E64 9C7060"),
                row("1A2027 1E2833 293C4D 44525F 7D6862 C08A58 D6A062"),
                row("1A2027 1F2A35 2C4152 4A5A68 8A7468 D39A5C E3B06A"),
                row("1A2027 223141 335069 5A7B91 9FA39B DDB98A EBCB9C"),
                row("1A2027 24394E 2F5878 477EA3 639AB9 82B0C8 98C0D2")],
        "rose": rgb("C58A9E"),
        "layers": [row("2B3643 232D38 1C252E 151D24 0F151A"),
                   row("3C4856 2D3945 222C35 181F27 10171B"),
                   row("566472 3B4854 28333C 1A2329 11181C"),
                   row("5B6876 3F4C58 2B363F 1C252C 12191D"),
                   row("6F7F8C 4E5E6B 36444F 232F37 162027"),
                   row("86A0B2 67849A 4A667C 31495C 203240")],
        "haze": row("2A3A4A 4A4A5E C98F62 D9A05E D9B182 A9C8D6"),
        "haze_gain": (0.5, 0.7, 1.0, 1.0, 0.9, 0.8), "haze_alpha": (70, 55, 40, 0, 0),
        "cloud": row("242D38 1A2029 151B22 171D25 8E959C C6D5DD"),
        "cloud_alpha": (95, 105, 120, 120, 110, 150),
        "glow": row("2A3A4A B9785A E8A968 F4C987 F8DCAE FFF4DC"),
        "glow_gain": (0.0, 0.35, 0.85, 1.0, 0.8, 0.5), "glow_alpha": 150,
        "sun": (rgb("F6B458"), rgb("FFF0CC")),
        "grey": (rgb("1D232A"), rgb("5E6B76")),
        "storm": rgb("232A32"),
        "star": rgb("EBE8DC"), "moon": rgb("EDE7D6"), "ink": rgb("E6DFCF"),
        "trail": rgb("F2EEE3"), "rain": rgb("C9D6DE"), "snow": rgb("E9EEF0"),
        "bird": rgb("141A20"),
        "tree": (rgb("15221D"), rgb("1B2B25")), "tree_night": rgb("0F1714"), "tree_light": 0.34,
        "seasons": [row("22302C 283631 1E2B28"), row("2F5A40 38684A 2A5039"),
                    row("23492F 2B5637 1F4029"), row("6E4A2C 7C6632 2C4A35")],
        "grain": 10,
    },
    "light": {
        "sky": [row("ECEFEE DEE2EA C6CCDC AFB7CE A3ABC6 A0A6C0 A4A8C0"),
                row("ECEFEE DFE3EA C9CEDD B6BCD0 B8B3C8 C9AFBA D7B5AE"),
                row("ECEFEE E2E6EA D2DAE2 C9CED8 D9C6C6 EEC7A6 F4D0A6"),
                row("ECEFEE E4E9EB D8E1E6 D6D9DC E8D3C4 F5D2A8 F8DDB6"),
                row("ECEFEE E3EBEC DCE6E8 E2E3DC EFE0CC F6DFC2 F8E6CC"),
                row("ECEFEE DDEAF0 C9DFEB BAD6E7 BDD9E8 CBE1EA D7E8EC")],
        "rose": rgb("E9B9C0"),
        "layers": [row("A9B0C6 8C96AE 6F7B96 55617C 404B63"),
                   row("B4B6C8 959BAF 767F97 5A657E 444F66"),
                   row("C9BFC0 A9A3AC 86899A 646C80 4A5368"),
                   row("CFC3BE ADA6AA 898C99 666E80 4B5568"),
                   row("CDCBC8 ACB2B6 87949C 62717B 485862"),
                   row("C9D3D7 A9B8BF 8497A0 5F747E 465A63")],
        "haze": row("C9CDE0 D9C9D6 F6D9BC FBE3C3 FBEBD3 EEF4F5"),
        "haze_gain": (0.6, 0.7, 1.0, 1.0, 1.0, 0.9), "haze_alpha": (90, 70, 45, 0, 0),
        "cloud": row("D5D9E6 D9D6E2 F1E4E0 FFF1E6 FFFFFF FFFFFF"),
        "cloud_alpha": (110, 115, 125, 130, 130, 135),
        "glow": row("C9CDE0 E8B9A6 F7D2A4 FFE9C4 FFF3DC FFFBEF"),
        "glow_gain": (0.0, 0.4, 0.9, 1.0, 0.9, 0.6), "glow_alpha": 190,
        "sun": (rgb("F2B45A"), rgb("FBE7B0")),
        "grey": (rgb("B9BEC8"), rgb("D5DADD")),
        "storm": rgb("8E98A6"),
        "star": rgb("FFFFFF"), "moon": rgb("FFFFFF"), "ink": rgb("3A4656"),
        "trail": rgb("FFFFFF"), "rain": rgb("5F747E"), "snow": rgb("FFFFFF"),
        "bird": rgb("46525E"),
        "tree": (rgb("52705F"), rgb("5C7A68")), "tree_night": rgb("3D4A5C"), "tree_light": 0.85,
        "seasons": [row("6F7F7A 7B8A84 657570"), row("6FA77F 7DB58B 63997A"),
                    row("52705F 5C7A68 4A6857"), row("C08A52 C9A85A 6E8A6A")],
        "grain": 8,
    },
}

WARM = row("FFD98A F2C46D E8A94F F7B45A")        # windows at home
COOL = row("E9E4CF DDE3DA F1E6C4 E4E8E0")        # office strip lights
SCREEN = rgb("9FC3E0")                           # the ones still up at three, lit by a monitor
AMBER, ROSE, SAGE, BLUE, CREAM = rgb("E0A951"), rgb("E07A70"), rgb("8FB8A0"), rgb("86AED0"), rgb("E6DFCF")
BEACON = rgb("E5736B")
