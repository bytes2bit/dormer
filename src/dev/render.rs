//! Drawing the panel: a model and its layout in, a string of terminal output
//! out. Everything is placed where `Layout` says, so what is drawn is what a
//! click lands on. State is always spelled out in words; colour only adds.

use super::layout::{choice_label, Button, CellRect, Layout, Row};
use super::model::{Category, Field, Model, Shape, Status};
use crate::views;

const ACCENT: &str = "\x1b[33m";
const DIM: &str = "\x1b[38;5;250m";
const BOLD: &str = "\x1b[1m";
const ACCENT_BOLD: &str = "\x1b[33;1m";
const WARN: &str = "\x1b[1;31m";
const REVERSE: &str = "\x1b[7m";
const OFF: &str = "\x1b[0m";

struct Screen {
    rows: Vec<Vec<(u16, String)>>,
    size: (u16, u16),
}

impl Screen {
    fn new(size: (u16, u16)) -> Screen {
        Screen { rows: vec![Vec::new(); size.1 as usize], size }
    }

    /// Write `text` at a cell, cut to the terminal's width.
    fn put(&mut self, x: u16, y: u16, text: &str, style: &str) {
        if y >= self.size.1 || x >= self.size.0 {
            return;
        }
        let room = (self.size.0 - x) as usize;
        let cut: String = text.chars().filter(|c| !c.is_control()).take(room).collect();
        if cut.is_empty() {
            return;
        }
        self.rows[y as usize].push((x, format!("{}{}{}", style, cut, OFF)));
    }

    /// Write `text` inside a rectangle: cut to it, never past it.
    fn put_in(&mut self, r: CellRect, text: &str, style: &str) {
        let cut: String = text.chars().take(r.w as usize).collect();
        self.put(r.x, r.y, &cut, style);
    }

    fn finish(self, cursor: Option<(u16, u16)>) -> String {
        let mut out = String::new();
        for (y, segments) in self.rows.into_iter().enumerate() {
            out.push_str(&format!("\x1b[{};1H\x1b[2K", y + 1));
            for (x, s) in segments {
                out.push_str(&format!("\x1b[{};{}H{}", y + 1, x + 1, s));
            }
        }
        match cursor {
            Some((x, y)) => out.push_str(&format!("\x1b[{};{}H\x1b[?25h", y + 1, x + 1)),
            None => out.push_str("\x1b[?25l"),
        }
        out
    }
}

/// `~/.config/...` rather than the whole home directory.
fn home_relative(path: &std::path::Path) -> String {
    let home = crate::sources::home();
    match path.strip_prefix(&home) {
        Ok(rest) => format!("~/{}", rest.display()),
        Err(_) => path.display().to_string(),
    }
}

/// What the header says about the state of the settings.
pub fn state_word(model: &Model) -> String {
    match &model.status {
        Status::Saving => "Saving…".into(),
        Status::Failed(_) if model.dirty() => format!("Not saved ({} unsaved)", model.dirty_count()),
        Status::Failed(_) => "Not saved".into(),
        _ if model.dirty() => format!("Unsaved changes ({})", model.dirty_count()),
        _ if model.pinned => "Preview only".into(),
        _ => "Saved".into(),
    }
}

fn bar(width: u16, at: f64, lo: f64, hi: f64) -> usize {
    if hi <= lo || width < 2 {
        return 0;
    }
    (((at - lo) / (hi - lo)) * (width - 1) as f64).round().clamp(0.0, (width - 1) as f64) as usize
}

fn draw_row(s: &mut Screen, model: &Model, row: &Row, list: CellRect) {
    let field = row.field;
    let info = model.info(field);
    let focused = field == model.focus;
    let fixed = model.fixed(field);
    let held = fixed == Some(true);
    let editing = model.entry.as_ref().filter(|e| e.field == field);

    s.put(list.x, row.y, if focused { ">" } else { " " }, ACCENT);
    s.put_in(row.label, &info.label, if focused { BOLD } else { "" });

    let value = model.value(field);
    match row.shape {
        Shape::Slider => {
            if let Some(b) = row.bar {
                let knob = bar(b.w, value, info.lo, info.hi);
                let w = b.w as usize;
                let style = if held { ACCENT } else { DIM };
                s.put(b.x, b.y, &"━".repeat(knob), style);
                s.put(b.x + knob as u16 + 1, b.y, &"─".repeat(w - knob - 1), DIM);
                if held {
                    // where reality is, when the draft has moved away from it
                    let real = match field {
                        Field::Time => Some(model.reality.time),
                        Field::Date => Some(0.0),
                        Field::Channel(c) => model.reality.channels.get(&c).copied(),
                        Field::Knob(k) => model.reality.knobs.get(k).copied(),
                        _ => None,
                    };
                    if let Some(r) = real {
                        let at = bar(b.w, r, info.lo, info.hi);
                        if at != knob {
                            s.put(b.x + at as u16, b.y, "┆", DIM);
                        }
                    }
                }
                s.put(b.x + knob as u16, b.y, "●", if held { ACCENT } else { BOLD });
            }
        }
        Shape::Toggle | Shape::Choice | Shape::Button => {}
    }

    // the value, or the text being typed
    if let Some(e) = editing {
        s.put_in(row.value, &format!("[{}]", e.text), REVERSE);
    } else {
        let text = match row.shape {
            Shape::Toggle => format!("[{}] {}", if value > 0.5 { "x" } else { " " }, model.show(field)),
            Shape::Button => format!("[ {} ]", model.show(field)),
            Shape::Choice => format!("‹ {} ›", model.show(field)),
            Shape::Slider => {
                let full = model.show(field);
                // in a narrow column the "today" after a date is the first thing to go
                if full.chars().count() > row.value.w as usize {
                    full.split("  ").next().unwrap_or(&full).to_string()
                } else {
                    full
                }
            }
        };
        s.put_in(row.value, &text, if held { ACCENT } else { BOLD });
    }
    if let Some(tag) = row.source {
        let wide = tag.w >= 7;
        let (text, style) = match (fixed, wide) {
            (Some(true), true) => ("[fixed]", ACCENT),
            (Some(true), false) => ("fixed", ACCENT),
            (_, true) => ("[auto]", DIM),
            (_, false) => ("auto", DIM),
        };
        s.put_in(tag, text, style);
    }
}

/// The text under the list: what the focused control is, and its limits.
fn detail_text(model: &Model) -> String {
    if let Some(e) = &model.entry {
        let hint = match e.field {
            Field::Time => "a time like 21:30, or auto",
            Field::Date => "a date like 2026-10-15, +3, or auto",
            Field::View => "a view name",
            Field::Mode => "auto, dark or light",
            f if model.info(f).shape == Shape::Toggle => "on or off",
            _ => "a number with or without its unit; auto gives it back to reality",
        };
        return match &e.error {
            Some(why) => format!("✗ {} — {}", why, hint),
            None => format!("Type a value: {} · Enter applies · Esc cancels", hint),
        };
    }
    let f = model.focus;
    let info = model.info(f);
    let mut out = info.help.clone();
    match f {
        Field::Time | Field::Date | Field::Channel(_) | Field::Knob(_) => {
            let live = match f {
                Field::Time => Some(format!("{:02}:{:02}", model.reality.time as i64, ((model.reality.time.fract() * 60.0).round() as i64) % 60)),
                Field::Date => None,
                Field::Channel(c) => model.reality.channels.get(&c).map(|v| format!("{}", v.round())),
                _ => None,
            };
            if model.fixed(f) == Some(true) {
                out.push_str(" · fixed");
                if let Some(l) = live {
                    out.push_str(&format!(", reality {}", l));
                }
                out.push_str(" · a = auto");
            } else {
                out.push_str(" · live; a change fixes it");
            }
        }
        _ => {}
    }
    if info.on_save {
        out.push_str(" · applies after Save");
    }
    out
}

pub fn render(model: &Model, layout: &Layout) -> String {
    let (cols, rows) = layout.size;
    let mut s = Screen::new(layout.size);
    let peeking = model.peeking() && model.dialog.is_none() && model.entry.is_none();

    // header: the name, the state in words, and (room permitting) what the sky is doing
    s.put(2.min(cols.saturating_sub(1)), 0, "dormer", ACCENT_BOLD);
    let word = state_word(model);
    let word_style = match (&model.status, model.dirty()) {
        (Status::Failed(_), _) => WARN,
        (_, true) => ACCENT,
        _ => DIM,
    };
    let word_w = word.chars().count() as u16;
    let word_x = cols.saturating_sub(word_w + 2).max(9);
    s.put(word_x, 0, &word, word_style);
    if peeking {
        s.put(9.min(cols), 0, "preview", DIM);
    }

    if peeking {
        // only the control in the hand, in the cell it always had; the rest is out of the way
        if let Some(row) = layout.row(model.focus) {
            draw_row(&mut s, model, row, layout.list);
        }
        let hint = if model.capture.is_some() { "Release to return" } else if model.pinned { "Preview pinned · p or Esc returns" } else { "Preview" };
        s.put(layout.status.x, layout.status.y, hint, DIM);
        return s.finish(None);
    }

    // views
    for (r, i) in &layout.tabs {
        let v = views::ALL[*i];
        let on = *i == model.view_index();
        let text = if on { format!("[{}]", v.id()) } else { format!(" {} ", v.id()) };
        s.put_in(*r, &text, if on { ACCENT_BOLD } else { DIM });
    }
    // categories
    for (r, c) in &layout.cats {
        let on = *c == model.category;
        let label = if layout.wide || cols >= 56 { c.label() } else { c.short() };
        if layout.wide {
            s.put_in(*r, &format!("{} {}", if on { ">" } else { " " }, label), if on { ACCENT_BOLD } else { DIM });
        } else {
            let text = if on { format!("[{}]", label) } else { format!(" {} ", label) };
            s.put_in(*r, &text, if on { ACCENT_BOLD } else { DIM });
        }
    }

    // the list, or help in its place
    if model.help {
        let lines = [
            "↑ ↓ / Tab      choose a control        ← → / h l   adjust (Shift: bigger)",
            "Enter          type a value            Space       switch on/off, or fix/auto",
            "a              give back to reality    n           time and date to now",
            "r              reset to saved          d           discard all changes",
            "s / Ctrl-S     save to config.toml     p           pin the preview",
            "1-5            jump to a section       q           quit (asks if unsaved)",
            "Esc            close this, or the preview",
            "mouse          click or drag a slider; wheel scrolls the list",
        ];
        for (n, l) in lines.iter().enumerate() {
            if (n as u16) < layout.list.h {
                s.put(layout.list.x, layout.list.y + n as u16, l, "");
            }
        }
    } else {
        for row in &layout.rows {
            draw_row(&mut s, model, row, layout.list);
        }
        let arrow_x = (layout.list.x + layout.list.w).saturating_sub(1);
        if layout.first > 0 {
            s.put(arrow_x, layout.list.y, "▲", DIM);
        }
        if layout.first + layout.rows.len() < layout.total {
            s.put(arrow_x, layout.list.bottom().saturating_sub(1), "▼", DIM);
        }
        if model.category == Category::View && layout.rows.is_empty() {
            s.put(layout.list.x, layout.list.y, "This view has no controls of its own.", DIM);
        }
    }

    // a failure is the one message that must be read whole: it takes the help line too
    let failure = match &model.status {
        Status::Failed(why) if model.dialog.is_none() => Some(why.clone()),
        _ => None,
    };
    if let (Some(d), Some(why)) = (layout.detail, &failure) {
        let width = d.w as usize;
        let cut = why.char_indices().nth(width).map_or(why.len(), |(i, _)| i);
        // break at a space when there is one near the end
        let at = why[..cut].rfind(' ').filter(|i| *i > width / 2 && cut < why.len()).unwrap_or(cut);
        s.put_in(d, &why[..at], WARN);
        s.put_in(layout.status, why[at..].trim_start(), WARN);
    } else if let Some(d) = layout.detail {
        s.put_in(d, &detail_text(model), DIM);
    }

    // status: what saving will do, or what just happened
    let status = match &model.status {
        Status::Failed(why) if layout.detail.is_some() && model.dialog.is_none() => {
            let _ = why;
            (String::new(), WARN)
        }
        Status::Failed(why) => (why.clone(), WARN),
        Status::Saved(what) => (what.clone(), ""),
        Status::Notice(what) => (what.clone(), ""),
        Status::Saving => ("Saving…".into(), ""),
        Status::Idle if model.dirty() => {
            let held = model.fixed_summary();
            let held = if held.is_empty() { String::new() } else { format!("; fixed: {}", held.join(", ")) };
            let file = model.config_path.file_name().map_or_else(|| "the config".to_string(), |n| n.to_string_lossy().into_owned());
            (format!("Save writes {} change{} to {}{}", model.dirty_count(), if model.dirty_count() == 1 { "" } else { "s" }, file, held), DIM)
        }
        Status::Idle => (format!("Settings file: {}", home_relative(&model.config_path)), DIM),
    };
    s.put_in(layout.status, &status.0, status.1);

    for (r, b) in &layout.buttons {
        let live = match b {
            Button::Save | Button::Discard | Button::Reset => model.dirty() || *b == Button::Save,
            _ => true,
        };
        let text = format!("[{}]", b.label());
        let on = (*b == Button::Preview && model.pinned) || (*b == Button::Help && model.help);
        s.put_in(*r, &text, if on { REVERSE } else if live { BOLD } else { DIM });
    }

    // a question asked of the user
    if let (Some(d), Some((area, choices))) = (&model.dialog, &layout.dialog) {
        for y in area.y..area.bottom() {
            s.put(area.x, y, &" ".repeat(area.w as usize), REVERSE);
        }
        s.put_in(CellRect::new(area.x + 2, area.y + 1, area.w.saturating_sub(4), 1), d.question(), &format!("{}{}", REVERSE, BOLD));
        for (r, c) in choices {
            s.put_in(*r, &format!(" {} ", choice_label(*c)), BOLD);
        }
    }

    // the text cursor, while a value is being typed
    let cursor = model.entry.as_ref().and_then(|e| {
        let row = layout.row(e.field)?;
        Some((row.value.x + 1 + e.text.chars().count().min(row.value.w.saturating_sub(2) as usize) as u16, row.y))
    });
    let _ = rows;
    s.finish(cursor)
}

#[cfg(test)]
mod tests {
    use super::super::layout::Layout;
    use super::super::model::{test_model, Action};
    use super::*;

    fn plain(out: &str) -> String {
        // drop the escape sequences, keeping text in the order it was written
        let mut text = String::new();
        let mut chars = out.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '\x1b' {
                for n in chars.by_ref() {
                    if n.is_ascii_alphabetic() {
                        break;
                    }
                }
                text.push(' ');
            } else {
                text.push(c);
            }
        }
        text
    }

    #[test]
    fn state_is_in_words_not_only_colour() {
        let mut m = test_model();
        assert_eq!(state_word(&m), "Saved");
        m.dispatch(Action::SetValue(Field::Fps, 12.0));
        assert_eq!(state_word(&m), "Unsaved changes (1)");
        m.fail("nope".into());
        assert_eq!(state_word(&m), "Not saved (1 unsaved)");
        m.now = 100.0; // the adjustment preview is over
        let text = plain(&render(&m, &Layout::build((70, 24), &m)));
        assert!(text.contains("Not saved") && text.contains("nope"));
        let mut m = test_model();
        m.dispatch(Action::Preview);
        assert_eq!(state_word(&m), "Preview only");
    }

    #[test]
    fn auto_and_fixed_are_labelled_and_values_have_units() {
        let mut m = test_model();
        let rain = Field::Channel(crate::settings::Channel::Rain);
        m.dispatch(Action::Focus(rain));
        let text = plain(&render(&m, &Layout::build((100, 30), &m)));
        assert!(text.contains("[auto]") && text.contains("30%"), "{}", text);
        m.dispatch(Action::Adjust(rain, 1.0));
        m.now = 100.0; // the adjustment preview has long expired
        let text = plain(&render(&m, &Layout::build((100, 30), &m)));
        assert!(text.contains("[fixed]") && text.contains("35%"), "{}", text);
        assert!(text.contains("Save writes 1 change to config.toml") && text.contains("fixed: rain 35%"), "{}", text);
    }

    #[test]
    fn nothing_is_written_outside_the_terminal() {
        for size in [(100u16, 30u16), (70, 24), (40, 12), (30, 8)] {
            let mut m = test_model();
            m.dispatch(Action::SetValue(Field::Channel(crate::settings::Channel::Rain), 50.0));
            for category in m.categories() {
                m.dispatch(Action::Category(category));
                let out = render(&m, &Layout::build(size, &m));
                // every positioned write starts inside the grid
                for part in out.split("\x1b[").skip(1) {
                    if let Some((pos, _)) = part.split_once('H') {
                        if let Some((y, x)) = pos.split_once(';') {
                            if let (Ok(y), Ok(x)) = (y.parse::<u16>(), x.parse::<u16>()) {
                                assert!(y >= 1 && y <= size.1 && x >= 1 && x <= size.0, "{:?} outside {:?}", (x, y), size);
                            }
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn peeking_draws_only_the_focused_control() {
        let mut m = test_model();
        let fog = Field::Channel(crate::settings::Channel::Fog);
        m.dispatch(Action::Focus(fog));
        let size = (70, 24);
        let normal = plain(&render(&m, &Layout::build(size, &m)));
        assert!(normal.contains("[Save]") && normal.contains("thunderstorm"));
        let l = Layout::build(size, &m);
        let bar = l.row(fog).unwrap().bar.unwrap();
        m.dispatch(Action::BeginDrag { field: fog, span: (bar.x, bar.w), x: bar.x + 5 });
        let held = plain(&render(&m, &Layout::build(size, &m)));
        assert!(held.contains("fog") && held.contains("preview"));
        assert!(!held.contains("[Save]") && !held.contains("thunderstorm"), "{}", held);
    }

    #[test]
    fn entry_shows_its_error_and_the_cursor() {
        let mut m = test_model();
        m.dispatch(Action::Focus(Field::Time));
        m.dispatch(Action::EditStart);
        for c in "25:99".chars() {
            m.dispatch(Action::EditChar(c));
        }
        m.dispatch(Action::EditCommit);
        let out = render(&m, &Layout::build((70, 24), &m));
        assert!(out.contains("\x1b[?25h"), "the cursor is shown while typing");
        assert!(plain(&out).contains("✗"));
    }
}
