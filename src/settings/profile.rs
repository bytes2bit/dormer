//! Laying the saved profile over a moment, in system-local time.

use serde_json::{json, Map};

use super::{Channel, RuntimeSettings, SettingsError, Source};
use crate::{tm, Moment};

impl RuntimeSettings {
    /// Put the saved profile on top of the real inputs in `moment`: fixed sources replace what
    /// was read, `Auto` leaves the real reading alone. Dev-panel drafts and command-line
    /// arguments are laid over this afterwards.
    #[cfg(test)]
    pub fn apply(&self, moment: &mut Moment) -> Result<(), SettingsError> {
        self.apply_except(moment, &[])
    }

    /// The same, leaving out the named inputs (dev.json keys: `time`, `day`, a channel's
    /// `key()` or a knob's key) that a draft has handed back to reality.
    pub fn apply_except(&self, moment: &mut Moment, skip: &[String]) -> Result<(), SettingsError> {
        let skipped = |key: &str| skip.iter().any(|s| s == key);
        moment.rates = crate::scene::Rates { birds: self.rates.birds, meteors: self.rates.meteors, planes: self.rates.planes };
        let mut clock = self.clone();
        if skipped("time") {
            clock.scene.time = Source::Auto;
        }
        if skipped("day") {
            clock.scene.date = Source::Auto;
        }
        clock.apply_clock(moment)?;
        let mut held = Map::new();
        for channel in Channel::ALL {
            if let (Source::Fixed(v), false) = (self.scene.get(channel), skipped(channel.key())) {
                held.insert(channel.key().to_string(), json!(v));
            }
        }
        // the view's saved knobs, kept to what the view says they may be
        for (key, v) in self.knobs(moment.view.id()) {
            if let Some(k) = moment.view.knobs().iter().find(|k| k.key == key && !skipped(k.key)) {
                moment.knobs.retain(|(name, _)| name != key);
                moment.knobs.push((key.to_string(), v.clamp(k.lo, k.hi)));
            }
        }
        let weather = Channel::ALL.iter().any(|c| !c.is_machine() && self.scene.get(*c).is_fixed() && !skipped(c.key()));
        crate::with_overrides(&held, moment);
        if weather {
            moment.wx.note = "saved settings";
        }
        Ok(())
    }

    /// A fixed time is that wall-clock time on the calendar day in question, whatever the
    /// zone's daylight saving did that day; a fixed date keeps the real time of day.
    fn apply_clock(&self, moment: &mut Moment) -> Result<(), SettingsError> {
        let (time, date) = (self.scene.time, self.scene.date);
        if !time.is_fixed() && !date.is_fixed() {
            return Ok(());
        }
        let real = tm::local(moment.ts);
        let (y, mo, d) = match date {
            Source::Fixed(d) => (d.year, d.month as i32, d.day as i32),
            Source::Auto => (real.year, real.month, real.mday),
        };
        let (h, mi, s) = match time {
            Source::Fixed(c) => (c.hour(), c.min(), 0),
            Source::Auto => (real.hour, real.min, real.sec),
        };
        let ts = tm::stamp(y, mo, d, h, mi, s);
        if !ts.is_finite() || ts <= 0.0 {
            return Err(SettingsError::Invalid { key: "scene.date".into(), why: "is outside the range the clock can show".into() });
        }
        moment.ts = ts;
        moment.bent = true;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::super::*;
    use crate::scene::{Machine, Place, Rates};
    use crate::{tm, Moment};
    use crate::sky::from_wmo;

    fn real() -> Moment {
        Moment {
            ts: tm::stamp(2026, 6, 1, 9, 15, 20),
            wx: from_wmo(2),
            place: Place { lat: 35.78, lon: -5.79, name: String::new() },
            machine: Machine { agents: 1, containers: 2, load: 0.25, battery: Some(0.9), audio: false },
            mode: "dark".into(),
            rates: Rates::default(),
            bent: false,
            view: crate::views::ALL[0],
            knobs: Vec::new(),
        }
    }

    fn with(text: &str) -> RuntimeSettings {
        RuntimeSettings::parse(text).unwrap()
    }

    #[test]
    fn auto_changes_nothing_and_real_readings_survive() {
        let mut m = real();
        let (ts, agents, load, cover) = (m.ts, m.machine.agents, m.machine.load, m.wx.cover);
        with("[scene]\nrain = 30\n[scene.simulated_machine]\ncontainers = 5\n").apply(&mut m).unwrap();
        assert_eq!(m.ts, ts);
        assert!(!m.bent);
        // the unfixed ones keep reality; the fixed ones are what was saved
        assert_eq!((m.machine.agents, m.machine.load), (agents, load));
        assert_eq!(m.machine.containers, 5);
        assert_eq!(m.machine.battery, Some(0.9));
        assert!((m.wx.rain - 0.30).abs() < 1e-9);
        assert_eq!(m.wx.cover, cover.max(0.75 + 0.25 * 0.30));
    }

    #[test]
    fn fixed_clock_and_date_use_local_calendar() {
        let mut m = real();
        with("[scene]\ntime = \"21:30\"\ndate = \"2026-10-15\"\n").apply(&mut m).unwrap();
        let l = tm::local(m.ts);
        assert_eq!((l.year, l.month, l.mday, l.hour, l.min, l.sec), (2026, 10, 15, 21, 30, 0));
        assert!(m.bent);
        // time alone: today's date, that minute
        let mut m = real();
        with("[scene]\ntime = \"06:05\"\n").apply(&mut m).unwrap();
        let l = tm::local(m.ts);
        assert_eq!((l.year, l.month, l.mday, l.hour, l.min), (2026, 6, 1, 6, 5));
        // date alone: that day, the real time of day
        let mut m = real();
        with("[scene]\ndate = \"2027-01-02\"\n").apply(&mut m).unwrap();
        let l = tm::local(m.ts);
        assert_eq!((l.year, l.month, l.mday, l.hour, l.min, l.sec), (2027, 1, 2, 9, 15, 20));
    }

    #[test]
    fn dst_days_keep_the_wall_clock() {
        // runs in a child process so that the zone is fixed whatever the test runner has
        if std::env::var("DORMER_TZ_CHILD").is_ok() {
            for (date, time, want_hour) in [("2026-11-01", "21:30", 21), ("2026-03-08", "21:30", 21), ("2026-11-01", "12:00", 12), ("2026-03-08", "03:30", 3)] {
                let mut m = real();
                with(&format!("[scene]\ndate = \"{}\"\ntime = \"{}\"\n", date, time)).apply(&mut m).unwrap();
                let l = tm::local(m.ts);
                assert_eq!((l.hour, l.min), (want_hour, time[3..].parse().unwrap()), "{} {}", date, time);
                assert_eq!(format!("{:04}-{:02}-{:02}", l.year, l.month, l.mday), date);
            }
            return;
        }
        let exe = std::env::current_exe().unwrap();
        let out = std::process::Command::new(exe)
            .args(["--exact", "settings::profile::tests::dst_days_keep_the_wall_clock", "--nocapture"])
            .env("DORMER_TZ_CHILD", "1")
            .env("TZ", "America/New_York")
            .output()
            .unwrap();
        let said = String::from_utf8_lossy(&out.stdout).to_string();
        // a child that matched no test would also "succeed"
        assert!(out.status.success() && said.contains("1 passed"), "{}{}", said, String::from_utf8_lossy(&out.stderr));
    }

    #[test]
    fn saved_knobs_are_kept_to_the_views_range() {
        let street = crate::views::find("street").unwrap();
        let Some(knob) = street.knobs().first() else { return };
        let mut m = real();
        m.view = street;
        with(&format!("[views.street]\n{} = 100000\n", knob.key)).apply(&mut m).unwrap();
        assert_eq!(m.knobs, vec![(knob.key.to_string(), knob.hi)]);
        // a knob of another view means nothing here
        let mut m = real();
        m.view = street;
        with("[views.moon]\nzoom = 3\n").apply(&mut m).unwrap();
        assert!(m.knobs.is_empty());
    }

    #[test]
    fn rates_come_from_the_profile() {
        let mut m = real();
        with("[activity]\nbirds = 3\nplanes = 0\n").apply(&mut m).unwrap();
        assert_eq!(m.rates, Rates { birds: 3.0, meteors: 1.0, planes: 0.0 });
    }
}
