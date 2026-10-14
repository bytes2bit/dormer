//! The same scene, said in words.

use crate::scene::Scene;
use crate::sky::star_at;
use crate::tm::{clock, DAYS};

pub fn compass(az: f64) -> &'static str {
    ["north", "north-east", "east", "south-east", "south", "south-west", "west", "north-west"][((az + 22.5).rem_euclid(360.0) / 45.0) as usize % 8]
}

pub fn phase_name(elong: f64) -> &'static str {
    for (limit, name) in [
        (12.0, "new"),
        (78.0, "waxing crescent"),
        (102.0, "first quarter"),
        (168.0, "waxing gibbous"),
        (192.0, "full"),
        (258.0, "waning gibbous"),
        (282.0, "last quarter"),
        (348.0, "waning crescent"),
    ] {
        if elong < limit {
            return name;
        }
    }
    "new"
}

fn list(items: &[String]) -> String {
    match items.len() {
        0 => String::new(),
        1 => items[0].clone(),
        n => format!("{} and {}", items[..n - 1].join(", "), items[n - 1]),
    }
}

/// What can be seen in the night sky right now, or None when it is day or overcast.
fn night_sky(sc: &Scene, stars: &[crate::sky::Star]) -> Option<String> {
    let el = sc.eph.sun.el;
    if el > -4.0 || sc.cover > 0.8 || sc.fog > 0.5 {
        return None;
    }
    let mut parts = Vec::new();
    let planets: Vec<String> = sc
        .eph
        .planets
        .iter()
        .filter(|p| p.body.el > 3.0 && p.elong > 9.0 && (el < -9.0 || p.mag < -1.0))
        .map(|p| format!("{} in the {}", p.name, compass(p.body.az)))
        .collect();
    if !planets.is_empty() {
        parts.push(list(&planets));
    }
    if el < -9.0 {
        let mut up: Vec<(&str, f32)> = stars
            .iter()
            .filter(|s| !s.name.is_empty() && s.mag < 1.7)
            .filter(|s| {
                let (x, _, height) = star_at(&sc.eph, s.ra, s.dec);
                height > 8.0 && (0.03..0.97).contains(&x)
            })
            .map(|s| (s.name, s.mag))
            .collect();
        up.sort_by(|a, b| a.1.partial_cmp(&b.1).unwrap());
        let names: Vec<String> = up.iter().take(4).map(|s| s.0.to_string()).collect();
        if !names.is_empty() {
            parts.push(format!("{} among the stars", list(&names)));
        }
    }
    if let Some((name, _)) = sc.shower {
        parts.push(format!("the {} are falling", name));
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("; "))
    }
}

pub fn explain(sc: &Scene, view: &dyn crate::views::View, stars: &[crate::sky::Star]) -> String {
    let (sun, moon, wx) = (&sc.eph.sun, &sc.eph.moon, &sc.wx);
    let mut out = Vec::new();
    let place = if sc.place.name.is_empty() { String::new() } else { format!("  ·  {}", sc.place.name) };
    out.push(format!("{} {}{}", DAYS[sc.dow], clock(sc.ts), place));
    // somewhere with no sky of ours, the weather is news from home and the view tells the rest
    let here = !sc.void;
    if !here {
    } else if sun.el > 0.0 {
        out.push(format!("sun    {}° up in the {}, sets {}", sun.el.round(), compass(sun.az), clock(sun.set)));
    } else if sun.ha > 0.0 {
        out.push(format!("sun    set at {}, now {}° below the horizon", clock(sun.set), (-sun.el).round()));
    } else {
        out.push(format!("sun    rises {}, still {}° below the horizon", clock(sun.rise), (-sun.el).round()));
    }
    let place = if moon.body.el > 0.0 {
        format!("{}° up in the {}", moon.body.el.round(), compass(moon.body.az))
    } else {
        "below the horizon".to_string()
    };
    if here {
        out.push(format!("moon   {}, {}% lit, {}", phase_name(moon.elong), (moon.lit * 100.0).round(), place));
        if let Some(line) = night_sky(sc, stars) {
            out.push(format!("stars  {}", line));
        }
    }
    let label = if here { "sky " } else { "home" };
    if wx.known {
        let note = match wx.note {
            "pretend" => "  (pretending)",
            "dev" => "  (dev panel)",
            _ => "",
        };
        let coming = match wx.rain_in {
            Some(h) if sc.front > 0.0 => {
                if h < 0.75 {
                    ", rain within the hour".to_string()
                } else {
                    format!(", rain expected in about {} h", h.round().max(1.0))
                }
            }
            _ => String::new(),
        };
        out.push(format!("{}   {}, {}°, wind {} km/h from the {}{}{}", label, wx.kind, wx.temp.round(), wx.wind.round(), compass(wx.wdir), coming, note));
    } else {
        out.push(format!("{}   no recent weather reading, so a fair sky is assumed", label));
    }
    for (label, text) in view.words(sc) {
        out.push(format!("{:<6} {}", label, text));
    }
    out.join("\n")
}
