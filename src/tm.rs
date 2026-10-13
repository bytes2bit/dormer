//! Local time, by way of the C library so the system's time zone rules apply.

use std::mem::MaybeUninit;

#[derive(Clone, Copy, Debug)]
pub struct Local {
    pub year: i32,
    /// 1..=12
    pub month: i32,
    pub mday: i32,
    pub hour: i32,
    pub min: i32,
    pub sec: i32,
    /// Monday is 0
    pub dow: usize,
    /// 1..=366
    pub yday: i32,
}

pub fn now() -> f64 {
    let mut ts = MaybeUninit::<libc::timespec>::uninit();
    unsafe {
        libc::clock_gettime(libc::CLOCK_REALTIME, ts.as_mut_ptr());
        let ts = ts.assume_init();
        ts.tv_sec as f64 + ts.tv_nsec as f64 * 1e-9
    }
}

/// Seconds that only ever go forwards, for pacing animation.
pub fn mono() -> f64 {
    let mut ts = MaybeUninit::<libc::timespec>::uninit();
    unsafe {
        libc::clock_gettime(libc::CLOCK_MONOTONIC, ts.as_mut_ptr());
        let ts = ts.assume_init();
        ts.tv_sec as f64 + ts.tv_nsec as f64 * 1e-9
    }
}

pub fn local(ts: f64) -> Local {
    let t = ts.floor() as libc::time_t;
    let mut tm = MaybeUninit::<libc::tm>::zeroed();
    let tm = unsafe {
        libc::localtime_r(&t, tm.as_mut_ptr());
        tm.assume_init()
    };
    Local {
        year: tm.tm_year + 1900,
        month: tm.tm_mon + 1,
        mday: tm.tm_mday,
        hour: tm.tm_hour,
        min: tm.tm_min,
        sec: tm.tm_sec,
        dow: ((tm.tm_wday + 6) % 7) as usize,
        yday: tm.tm_yday + 1,
    }
}

/// The timestamp of a local wall-clock time. Out-of-range fields carry over, as mktime does.
pub fn stamp(year: i32, month: i32, mday: i32, hour: i32, min: i32, sec: i32) -> f64 {
    let mut tm: libc::tm = unsafe { MaybeUninit::zeroed().assume_init() };
    tm.tm_year = year - 1900;
    tm.tm_mon = month - 1;
    tm.tm_mday = mday;
    tm.tm_hour = hour;
    tm.tm_min = min;
    tm.tm_sec = sec;
    tm.tm_isdst = -1;
    unsafe { libc::mktime(&mut tm) as f64 }
}

/// Midnight at the start of the local day that holds `ts`.
pub fn midnight(ts: f64) -> f64 {
    let l = local(ts);
    stamp(l.year, l.month, l.mday, 0, 0, 0)
}

/// "HH:MM" in local time.
pub fn clock(ts: f64) -> String {
    let l = local(ts);
    format!("{:02}:{:02}", l.hour, l.min)
}

pub const DAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
pub const MONTHS: [&str; 12] = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];

/// 'YYYY-MM-DD HH:MM', 'YYYY-MM-DDTHH:MM' or 'HH:MM' (today).
pub fn parse(text: &str) -> Option<f64> {
    let text = text.trim().replace('T', " ");
    let nums = |s: &str, sep: char| -> Option<Vec<i32>> { s.split(sep).map(|p| p.trim().parse().ok()).collect() };
    let (date, time) = match text.split_once(' ') {
        Some((d, t)) => (Some(d.to_string()), t.to_string()),
        None => (None, text.clone()),
    };
    let hm = nums(&time, ':')?;
    if hm.len() < 2 || !(0..=24).contains(&hm[0]) || !(0..60).contains(&hm[1]) {
        return None;
    }
    let (y, m, d) = match date {
        Some(d) => {
            let p = nums(&d, '-')?;
            if p.len() != 3 || !(1..=12).contains(&p[1]) || !(1..=31).contains(&p[2]) {
                return None;
            }
            (p[0], p[1], p[2])
        }
        None => {
            let l = local(now());
            (l.year, l.month, l.mday)
        }
    };
    Some(stamp(y, m, d, hm[0], hm[1], 0))
}
