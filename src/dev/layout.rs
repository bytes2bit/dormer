//! Where everything goes on the terminal's grid, worked out once and used for
//! both drawing and mouse hits, so what is drawn is what is clicked.
//!
//! The rules: the header and the action buttons are always there; the list of
//! controls takes what is left and scrolls; nothing moves while a control is
//! being dragged (a layout depends on the size and the model, never on where
//! the pointer is).

use super::model::{Category, Choice, Field, Model, Shape};
use crate::views;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct CellRect {
    pub x: u16,
    pub y: u16,
    pub w: u16,
    pub h: u16,
}

impl CellRect {
    pub fn new(x: u16, y: u16, w: u16, h: u16) -> CellRect {
        CellRect { x, y, w, h }
    }

    pub fn contains(&self, x: u16, y: u16) -> bool {
        x >= self.x && x < self.x + self.w && y >= self.y && y < self.y + self.h
    }

    pub fn right(&self) -> u16 {
        self.x + self.w
    }

    pub fn bottom(&self) -> u16 {
        self.y + self.h
    }

    fn within(&self, size: (u16, u16)) -> bool {
        self.right() <= size.0 && self.bottom() <= size.1
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Button {
    Save,
    Discard,
    Reset,
    Preview,
    Help,
    Quit,
}

impl Button {
    pub const ALL: [Button; 6] = [Button::Save, Button::Discard, Button::Reset, Button::Preview, Button::Help, Button::Quit];

    pub fn label(self) -> &'static str {
        match self {
            Button::Save => "Save",
            Button::Discard => "Discard",
            Button::Reset => "Reset",
            Button::Preview => "Preview",
            Button::Help => "Help",
            Button::Quit => "Quit",
        }
    }

    pub fn key(self) -> char {
        match self {
            Button::Save => 's',
            Button::Discard => 'd',
            Button::Reset => 'r',
            Button::Preview => 'p',
            Button::Help => '?',
            Button::Quit => 'q',
        }
    }
}

/// Everything on the screen that answers to the mouse.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ControlId {
    ViewTab(usize),
    Category(Category),
    /// the label and value of a row: selects it
    Row(Field),
    /// the bar of a slider: press and drag
    Slider(Field),
    /// a switch
    Toggle(Field),
    /// the arrows of a choice
    Step(Field, i8),
    /// the Auto / Fixed tag
    Source(Field),
    Button(Button),
    Choice(Choice),
    ScrollUp,
    ScrollDown,
}

/// One control as it is placed.
#[derive(Clone, Debug)]
pub struct Row {
    pub field: Field,
    pub y: u16,
    pub label: CellRect,
    pub bar: Option<CellRect>,
    pub value: CellRect,
    pub source: Option<CellRect>,
    pub shape: Shape,
}

#[derive(Clone, Debug)]
pub struct Layout {
    pub size: (u16, u16),
    /// categories down the left side, rather than as tabs
    pub wide: bool,
    pub header: CellRect,
    pub tabs: Vec<(CellRect, usize)>,
    pub cats: Vec<(CellRect, Category)>,
    pub rows: Vec<Row>,
    pub list: CellRect,
    pub first: usize,
    pub total: usize,
    pub detail: Option<CellRect>,
    pub status: CellRect,
    pub buttons: Vec<(CellRect, Button)>,
    pub dialog: Option<(CellRect, Vec<(CellRect, Choice)>)>,
    targets: Vec<(CellRect, ControlId)>,
}

/// Put `items` (widths) in rows no wider than `width`, `gap` apart. Each is placed whole;
/// an item wider than the row sits alone and is cut by the renderer.
fn wrap(items: &[u16], x: u16, y: u16, width: u16, gap: u16) -> Vec<CellRect> {
    let mut out = Vec::new();
    let (mut cx, mut cy) = (x, y);
    for &w in items {
        if cx > x && cx + w > x + width {
            cx = x;
            cy += 1;
        }
        out.push(CellRect::new(cx, cy, w.min(width.max(1)), 1));
        cx += w + gap;
    }
    out
}

fn text_w(s: &str) -> u16 {
    s.chars().count() as u16
}

impl Layout {
    pub fn build(size: (u16, u16), model: &Model) -> Layout {
        let (cols, rows_n) = (size.0.max(1), size.1.max(1));
        let wide = cols >= 88 && rows_n >= 20;
        let margin = if cols >= 30 { 2 } else { 0 };
        let inner = cols.saturating_sub(margin * 2).max(1);
        let mut targets: Vec<(CellRect, ControlId)> = Vec::new();

        let header = CellRect::new(0, 0, cols, 1);
        let mut y = 1u16;

        // the buttons and the status are fixed to the bottom, whatever else gives way
        let labels: Vec<String> = Button::ALL.iter().map(|b| format!("[{}]", b.label())).collect();
        let widths: Vec<u16> = labels.iter().map(|l| text_w(l)).collect();
        let mut buttons = wrap(&widths, margin, 0, inner, 1);
        let button_rows = buttons.last().map_or(1, |r| r.y + 1);
        let button_top = rows_n.saturating_sub(button_rows);
        for r in buttons.iter_mut() {
            r.y += button_top;
        }
        let buttons: Vec<(CellRect, Button)> = buttons.into_iter().zip(Button::ALL).map(|(r, b)| (r, b)).collect();
        let status_y = button_top.saturating_sub(1).max(1);
        let status = CellRect::new(margin, status_y, inner, 1);
        let bottom = status_y; // the list ends above the status line

        // views: a row of names that wraps
        let mut tabs = Vec::new();
        if rows_n >= 10 {
            let names: Vec<u16> = views::ALL.iter().map(|v| text_w(v.id()) + 2).collect();
            let placed = wrap(&names, margin, y, inner, 1);
            y = placed.last().map_or(y, |r| r.y + 1);
            tabs = placed.into_iter().enumerate().map(|(i, r)| (r, i)).collect();
        }

        // categories: down the left side, or as tabs
        let cats_list = model.categories();
        let mut cats = Vec::new();
        let (list_x, list_w);
        let mut list_top;
        if wide {
            list_top = y + 1;
            let col = 19u16;
            for (i, c) in cats_list.iter().enumerate() {
                cats.push((CellRect::new(margin, list_top + i as u16, col, 1), *c));
            }
            list_x = margin + col + 2;
            list_w = cols.saturating_sub(list_x + margin).max(1);
        } else {
            let names: Vec<u16> = cats_list.iter().map(|c| text_w(if cols >= 56 { c.label() } else { c.short() }) + 2).collect();
            let placed = wrap(&names, margin, y, inner, 1);
            list_top = placed.last().map_or(y, |r| r.y + 1);
            cats = placed.into_iter().zip(cats_list.iter().copied()).collect();
            list_x = margin;
            list_w = inner;
            if rows_n >= 20 {
                list_top += 1; // a blank line under the tabs when there is room
            }
        }

        // a line of help for the focused control, when there is room for it
        let want_detail = rows_n >= 14;
        let detail_y = bottom.saturating_sub(1);
        let (list_bottom, detail) = if want_detail && detail_y > list_top + 2 {
            (detail_y, Some(CellRect::new(margin, detail_y, inner, 1)))
        } else {
            (bottom, None)
        };
        let capacity = list_bottom.saturating_sub(list_top).max(1) as usize;
        let list = CellRect::new(list_x, list_top, list_w, capacity as u16);

        // rows
        let fields = model.fields(model.category);
        let total = fields.len();
        let at = fields.iter().position(|f| *f == model.focus).unwrap_or(0);
        let mut first = model.scroll.min(total.saturating_sub(capacity));
        if model.follow {
            if at < first {
                first = at;
            } else if at >= first + capacity {
                first = at + 1 - capacity;
            }
        }
        let (label_w, value_w, tag_w): (u16, u16, u16) = if list_w >= 70 {
            (18, 20, 7)
        } else if list_w >= 54 {
            (16, 16, 7)
        } else {
            (12, 11, 6)
        };
        // the last column of the list belongs to the scroll arrows
        let edge = (list_x + list_w).saturating_sub(1);
        let mut rows = Vec::new();
        for (n, field) in fields.iter().enumerate().skip(first).take(capacity) {
            let info = model.info(*field);
            let ry = list_top + (n - first) as u16;
            let label = CellRect::new(list_x + 2, ry, label_w.min(list_w.saturating_sub(3)), 1);
            let source = info.sourced.then(|| CellRect::new(edge.saturating_sub(tag_w), ry, tag_w, 1));
            let tag_space = if source.is_some() { tag_w + 1 } else { 0 };
            let value = CellRect::new(edge.saturating_sub(tag_space + value_w), ry, value_w, 1);
            let bar_x = label.right() + 1;
            let bar_w = value.x.saturating_sub(bar_x + 1);
            let bar = (info.shape == Shape::Slider && bar_w >= 6).then(|| CellRect::new(bar_x, ry, bar_w, 1));
            rows.push(Row { field: *field, y: ry, label, bar, value, source, shape: info.shape });
        }

        // what answers to the mouse, far to near
        let peeking = model.peeking();
        let asking = model.dialog.is_some();
        let editing = model.entry.is_some();
        let mut dialog = None;
        if asking {
            let d = model.dialog.as_ref().unwrap();
            let choices: Vec<&str> = d.choices().iter().map(|c| choice_label(*c)).collect();
            let widths: Vec<u16> = choices.iter().map(|c| text_w(c) + 2).collect();
            let box_w = (widths.iter().sum::<u16>() + widths.len() as u16 + 4).max(text_w(d.question()) + 4).min(cols);
            let box_x = cols.saturating_sub(box_w) / 2;
            let box_y = (rows_n / 2).saturating_sub(2).max(1);
            let placed = wrap(&widths, box_x + 2, box_y + 2, box_w.saturating_sub(4).max(1), 1);
            let area = CellRect::new(box_x, box_y, box_w, placed.last().map_or(3, |r| r.y + 2 - box_y));
            let list: Vec<(CellRect, Choice)> = placed.into_iter().zip(d.choices().iter().copied()).collect();
            for (r, c) in &list {
                targets.push((*r, ControlId::Choice(*c)));
            }
            dialog = Some((area, list));
        } else if !editing {
            if !peeking {
                for (r, i) in &tabs {
                    targets.push((*r, ControlId::ViewTab(*i)));
                }
                for (r, c) in &cats {
                    targets.push((*r, ControlId::Category(*c)));
                }
                for (r, b) in &buttons {
                    targets.push((*r, ControlId::Button(*b)));
                }
                if first > 0 {
                    targets.push((CellRect::new(list_x + list_w.saturating_sub(1), list_top, 1, 1), ControlId::ScrollUp));
                }
                if first + capacity < total {
                    targets.push((CellRect::new(list_x + list_w.saturating_sub(1), list_bottom.saturating_sub(1), 1, 1), ControlId::ScrollDown));
                }
            }
            for row in &rows {
                // while a control is held, only that control exists
                if peeking && row.field != model.focus {
                    continue;
                }
                let line = CellRect::new(list_x, row.y, list_w.saturating_sub(1).max(1), 1);
                targets.push((line, ControlId::Row(row.field)));
                match row.shape {
                    Shape::Slider => {
                        if let Some(bar) = row.bar {
                            targets.push((bar, ControlId::Slider(row.field)));
                        }
                    }
                    Shape::Toggle => targets.push((row.value, ControlId::Toggle(row.field)),
                    ),
                    Shape::Choice => {
                        let v = row.value;
                        targets.push((CellRect::new(v.x, v.y, 2, 1), ControlId::Step(row.field, -1)));
                        targets.push((CellRect::new(v.right().saturating_sub(2), v.y, 2, 1), ControlId::Step(row.field, 1)));
                    }
                }
                if let Some(tag) = row.source {
                    targets.push((tag, ControlId::Source(row.field)));
                }
            }
        }
        // a rectangle that does not fit the terminal is not a target: it cannot be seen
        targets.retain(|(r, _)| r.within(size));

        Layout { size, wide, header, tabs, cats, rows, list, first, total, detail, status, buttons, dialog, targets }
    }

    /// The control at a cell, nearest first.
    pub fn hit(&self, x: u16, y: u16) -> Option<ControlId> {
        self.targets.iter().rev().find(|(r, _)| r.contains(x, y)).map(|(_, id)| *id)
    }

    /// Where a slider's cells are, for drag capture.
    pub fn span(&self, field: Field) -> Option<(u16, u16)> {
        self.rows.iter().find(|r| r.field == field).and_then(|r| r.bar).map(|b| (b.x, b.w))
    }

    pub fn row(&self, field: Field) -> Option<&Row> {
        self.rows.iter().find(|r| r.field == field)
    }

    /// Every target, for tests and the renderer's bounds checks.
    pub fn targets(&self) -> &[(CellRect, ControlId)] {
        &self.targets
    }
}

pub fn choice_label(c: Choice) -> &'static str {
    match c {
        Choice::Save => "Save",
        Choice::Discard => "Discard",
        Choice::Cancel => "Cancel",
        Choice::Reload => "Reload",
        Choice::Overwrite => "Overwrite",
    }
}

#[cfg(test)]
mod tests {
    use super::super::model::{test_model, Action};
    use super::*;

    #[test]
    fn actions_remain_reachable_at_supported_sizes() {
        for size in [(100, 30), (70, 24), (40, 12), (30, 8)] {
            let mut m = test_model();
            for category in m.categories() {
                m.dispatch(Action::Category(category));
                let l = Layout::build(size, &m);
                for (r, id) in l.targets() {
                    assert!(r.within(size), "{:?} {:?} falls outside {:?}", id, r, size);
                    assert!(r.w > 0 && r.h > 0);
                }
                for b in Button::ALL {
                    assert!(l.targets().iter().any(|(_, id)| *id == ControlId::Button(b)), "{:?} missing at {:?}", b, size);
                }
                // each button's centre hits that button, so the map used to draw is the one clicked
                for (r, b) in &l.buttons {
                    assert_eq!(l.hit(r.x + r.w / 2, r.y), Some(ControlId::Button(*b)));
                }
                // the focused control is in view
                assert!(l.row(m.focus).is_some(), "focus {:?} not visible at {:?} in {:?}", m.focus, size, category);
                assert!(l.rows.iter().all(|r| r.y >= l.list.y && r.y < l.list.bottom()));
            }
        }
    }

    #[test]
    fn view_names_wrap_instead_of_clipping() {
        let m = test_model();
        let narrow = Layout::build((40, 14), &m);
        let rows: std::collections::BTreeSet<u16> = narrow.tabs.iter().map(|(r, _)| r.y).collect();
        assert!(rows.len() >= 2, "five view names do not fit one 40 column row");
        assert!(narrow.tabs.iter().all(|(r, _)| r.within((40, 14))));
        let wide = Layout::build((100, 30), &m);
        assert_eq!(wide.tabs.iter().map(|(r, _)| r.y).collect::<std::collections::BTreeSet<_>>().len(), 1);
        assert_eq!(wide.tabs.len(), views::ALL.len());
        assert!(wide.wide && !narrow.wide);
    }

    #[test]
    fn the_list_scrolls_and_keeps_the_focus_in_view() {
        let mut m = test_model();
        let size = (70, 14);
        let l = Layout::build(size, &m);
        assert!(l.total > l.list.h as usize, "sky has more controls than rows here");
        m.viewed(l.first);
        // walk to the end: the list follows
        for _ in 0..m.fields(Category::Sky).len() - 1 {
            m.dispatch(Action::FocusStep(1));
        }
        let l = Layout::build(size, &m);
        assert!(l.row(m.focus).is_some() && l.first > 0);
        assert!(l.hit(l.list.right() - 1, l.list.y) == Some(ControlId::ScrollUp));
        m.viewed(l.first);
        // the wheel moves the view of the list but not the focus, and is not undone
        let focus = m.focus;
        m.dispatch(Action::Scroll(-2));
        let l2 = Layout::build(size, &m);
        assert_eq!(m.focus, focus);
        assert_eq!(l2.first, l.first - 2);
        // and never past the ends
        m.dispatch(Action::Scroll(100));
        assert!(Layout::build(size, &m).first + (l.list.h as usize) <= l.total);
    }

    #[test]
    fn while_a_control_is_held_only_it_answers_and_nothing_moves() {
        let mut m = test_model();
        let size = (70, 24);
        let before = Layout::build(size, &m);
        let field = Field::Channel(crate::settings::Channel::Fog);
        m.dispatch(Action::Focus(field));
        let at = Layout::build(size, &m);
        let bar = at.row(field).unwrap().bar.unwrap();
        m.dispatch(Action::BeginDrag { field, span: (bar.x, bar.w), x: bar.x + 3 });
        assert!(m.peeking());
        let held = Layout::build(size, &m);
        // the same cell, the same bar
        assert_eq!(held.row(field).unwrap().bar, Some(bar));
        assert_eq!(held.row(field).unwrap().y, at.row(field).unwrap().y);
        assert_eq!(before.rows.len(), held.rows.len());
        // other rows, tabs and buttons do not exist as targets
        let kinds: Vec<ControlId> = held.targets().iter().map(|t| t.1).collect();
        assert!(kinds.iter().all(|k| matches!(k, ControlId::Row(f) | ControlId::Slider(f) | ControlId::Source(f) | ControlId::Toggle(f) if *f == field)), "{:?}", kinds);
        // dragging above the list does not hit a tab
        assert!(!matches!(held.hit(5, 1), Some(ControlId::ViewTab(_))));
    }

    #[test]
    fn a_dialog_is_the_only_target_and_is_centred() {
        let mut m = test_model();
        m.dispatch(Action::SetValue(Field::Fps, 10.0));
        m.dispatch(Action::Quit);
        for size in [(100, 30), (40, 12)] {
            let l = Layout::build(size, &m);
            let (area, choices) = l.dialog.as_ref().unwrap();
            assert!(area.within(size));
            assert_eq!(choices.len(), 3);
            assert!(l.targets().iter().all(|(_, id)| matches!(id, ControlId::Choice(_))));
            for (r, c) in choices {
                assert!(r.within(size));
                assert_eq!(l.hit(r.x, r.y), Some(ControlId::Choice(*c)));
            }
        }
    }

    #[test]
    fn sliders_have_a_bar_and_toggles_a_switch() {
        let m = test_model();
        let l = Layout::build((100, 30), &m);
        let time = l.row(Field::Time).unwrap();
        let bar = time.bar.unwrap();
        assert_eq!(l.hit(bar.x + 1, bar.y), Some(ControlId::Slider(Field::Time)));
        assert_eq!(l.hit(time.label.x, time.y), Some(ControlId::Row(Field::Time)));
        assert!(time.source.is_some(), "time has an Auto / Fixed tag");
        let mut m = test_model();
        m.dispatch(Action::Category(Category::Machine));
        let l = Layout::build((100, 30), &m);
        let music = l.row(Field::Channel(crate::settings::Channel::Music)).unwrap();
        assert!(music.bar.is_none());
        assert_eq!(l.hit(music.value.x, music.y), Some(ControlId::Toggle(Field::Channel(crate::settings::Channel::Music))));
    }
}
