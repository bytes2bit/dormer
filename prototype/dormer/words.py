"""The same scene, said in words."""
import time

DAYS = ("Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday")


def clock(ts):
    return time.strftime("%H:%M", time.localtime(ts))


def compass(az):
    return ("north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west")[int((az + 22.5) % 360 // 45)]


def phase_name(elong):
    for limit, name in ((12, "new"), (78, "waxing crescent"), (102, "first quarter"), (168, "waxing gibbous"),
                        (192, "full"), (258, "waning gibbous"), (282, "last quarter"), (348, "waning crescent")):
        if elong < limit:
            return name
    return "new"


def explain(sc):
    sun, moon, wx = sc.sun, sc.moon, sc.wx
    out = ["%s %s%s" % (DAYS[sc.dow], clock(sc.ts), "  ·  " + sc.place["name"] if sc.place.get("name") else "")]
    if sun.el > 0:
        out.append("sun    %d° up in the %s, sets %s" % (round(sun.el), compass(sun.az), clock(sun.set)))
    elif sun.ha > 0:
        out.append("sun    set at %s, now %d° below the horizon" % (clock(sun.set), round(-sun.el)))
    else:
        out.append("sun    rises %s, still %d° below the horizon" % (clock(sun.rise), round(-sun.el)))
    where = "%d° up in the %s" % (round(moon.el), compass(moon.az)) if moon.el > 0 else "below the horizon"
    out.append("moon   %s, %d%% lit, %s" % (phase_name(moon.elong), round(moon.lit * 100), where))
    if wx.get("known", True):
        note = "  (pretending)" if wx.get("pretend") else "  (dev panel)" if wx.get("dev") else ""
        out.append("sky    %s, %d°, wind %d km/h from the %s%s" % (wx["kind"], round(wx["temp"]), round(wx["wind"]), compass(wx["wdir"]), note))
    else:
        out.append("sky    no recent weather reading, so a fair sky is assumed")
    week = ("the city starts early: more lamps before sunrise, more planes, the crane at work",
            "an ordinary working day", "an ordinary working day", "an ordinary working day",
            "offices empty early; string lights go up on a roof after dark",
            "no office lights; washing out by day, string lights and late windows at night",
            "the city sleeps in, hangs the washing out, and goes to bed early")[sc.dow]
    out.append("week   " + week)
    doing = []
    if hasattr(sc, "lit_share"):
        doing.append("%d%% of windows lit" % round(sc.lit_share * 100))
    for flag, text in ((sc.sleepers, "people still asleep"), (sc.laundry, "washing on the line"), (sc.kite, "a kite up"),
                       (sc.balloon, "a balloon drifting over"), (sc.festoon, "string lights on"),
                       (sc.crane_works, "the crane working"), (sc.planes, "a plane crossing"),
                       (sc.birds, "birds crossing"), (sc.smoke > 0.05, "chimneys smoking"),
                       (sc.traffic * sc.dark > 0.3, "rush-hour glow in the street")):
        if flag:
            doing.append(text)
    if sc.cat_out:
        doing.append("the cat asleep at home" if sc.cat_asleep else "the cat out on a roof")
    out.append("city   " + ", ".join(doing))
    out.append("yours  lamp %s (%d agent session%s), %d container window%s" % (
        "on" if sc.agents else "off", sc.agents, "" if sc.agents == 1 else "s",
        min(sc.containers, 8), "" if sc.containers == 1 else "s"))
    return "\n".join(out)
