//! Putting the living wallpaper on the screen.
//!
//! One Wayland layer surface per monitor, on the background layer, drawn in
//! software into shared memory. The still frame is painted afresh every few
//! seconds and cross-faded in; between those, only the rectangles that the
//! moving things touch are repainted and reported as damage, so a quiet
//! night costs the compositor a few hundred pixels a frame.
//!
//! It moves only while it can be seen. With a tiled or fullscreen window on
//! the workspace, or floating ones that fill the screen (Hyprland), after
//! `dormer hold on`, or while the configured hold file exists, it stands still
//! and costs nothing. The same goes for a screen that has stopped showing
//! frames (switched off, asleep): the compositor stops asking for the next one,
//! and nothing is drawn or worked out until it does. A hidden screen's still
//! frame is not kept up to date either; it is painted afresh as it comes back.

use std::collections::HashMap;
use std::io::Read;
use std::os::fd::{AsFd, AsRawFd, FromRawFd, OwnedFd};
use std::os::unix::net::UnixStream;
use std::path::PathBuf;

use wayland_client::globals::{registry_queue_init, GlobalListContents};
use wayland_client::protocol::{wl_buffer, wl_callback, wl_compositor, wl_output, wl_pointer, wl_seat, wl_region, wl_registry, wl_shm, wl_shm_pool, wl_surface};
use wayland_client::{delegate_noop, Connection, Dispatch, QueueHandle};
use wayland_protocols::wp::fractional_scale::v1::client::{wp_fractional_scale_manager_v1, wp_fractional_scale_v1};
use wayland_protocols::wp::viewporter::client::{wp_viewport, wp_viewporter};
use wayland_protocols::wp::cursor_shape::v1::client::{wp_cursor_shape_manager_v1, wp_cursor_shape_device_v1};
use wayland_protocols_wlr::layer_shell::v1::client::{zwlr_layer_shell_v1, zwlr_layer_surface_v1};

use crate::gfx::{Draw, Frame, Rect, Shaper};
use crate::live::{coalesce, Sources, World};
use crate::paint::{self, Cache};
use crate::scene::Scene;
use crate::sources::{self, cache_file, config};
use crate::views::{Anchors, View};
use crate::interaction::{Interaction, Target, pointer_position};
use crate::pointer::{Cursor, Seat};
use crate::{build, real_moment, tm, with_overrides, Moment, State};

/// Layers this one must stay above.
const WALLPAPERS: [&str; 4] = ["noctalia-wallpaper", "hyprpaper", "swww-daemon", "wallpaper"];
const FADE: f64 = 0.7;

struct Buf {
    buffer: wl_buffer::WlBuffer,
    /// where its pixels start in the pool
    at: usize,
    busy: bool,
    /// what was drawn over the still frame the last time this buffer was used
    dirty: Vec<Rect>,
    /// rows of the still frame that have changed since
    stale: Option<(usize, usize)>,
}

struct Pool {
    map: *mut u32,
    bytes: usize,
    pool: wl_shm_pool::WlShmPool,
    _fd: OwnedFd,
    bufs: Vec<Buf>,
    w: usize,
    h: usize,
}

impl Pool {
    fn new(app: &App, id: u32, w: usize, h: usize) -> Option<Pool> {
        let bytes = w * h * 4 * 2;
        unsafe {
            let fd = libc::memfd_create(c"dormer".as_ptr(), libc::MFD_CLOEXEC);
            if fd < 0 {
                return None;
            }
            let fd = OwnedFd::from_raw_fd(fd);
            if libc::ftruncate(fd.as_raw_fd(), bytes as libc::off_t) != 0 {
                return None;
            }
            let map = libc::mmap(std::ptr::null_mut(), bytes, libc::PROT_READ | libc::PROT_WRITE, libc::MAP_SHARED, fd.as_raw_fd(), 0);
            if map == libc::MAP_FAILED {
                return None;
            }
            let pool = app.shm.create_pool(fd.as_fd(), bytes as i32, &app.qh, ());
            let bufs = (0..2)
                .map(|k| Buf {
                    buffer: pool.create_buffer((k * w * h * 4) as i32, w as i32, h as i32, (w * 4) as i32, wl_shm::Format::Xrgb8888, &app.qh, (id, k)),
                    at: k * w * h,
                    busy: false,
                    dirty: Vec::new(),
                    stale: Some((0, h)),
                })
                .collect();
            Some(Pool { map: map as *mut u32, bytes, pool, _fd: fd, bufs, w, h })
        }
    }
}

impl Drop for Pool {
    fn drop(&mut self) {
        for b in &self.bufs {
            b.buffer.destroy();
        }
        self.pool.destroy();
        unsafe {
            libc::munmap(self.map as *mut libc::c_void, self.bytes);
        }
    }
}

struct Screen {
    id: u32,
    output: wl_output::WlOutput,
    name: String,
    whole_scale: i32,
    surface: wl_surface::WlSurface,
    layer: zwlr_layer_surface_v1::ZwlrLayerSurfaceV1,
    viewport: Option<wp_viewport::WpViewport>,
    fraction: Option<wp_fractional_scale_v1::WpFractionalScaleV1>,
    /// the scale the compositor asks for, in 120ths; 0 until it says
    scale120: u32,
    logical: (u32, u32),
    pool: Option<Pool>,
    cache: Option<Cache>,
    /// the still frame as it is being shown (mid-fade, a blend of `from` and `to`)
    shown: Vec<u32>,
    from: Vec<u32>,
    to: Vec<u32>,
    /// start, first row, one past the last row
    fade: Option<(f64, usize, usize)>,
    anchors: Anchors,
    waiting: bool,
    last_dirty: Vec<Rect>,
    /// can the wallpaper be seen on this monitor
    open: bool,
    /// when the last frame was handed over
    sent: f64,
    /// the compositor has not shown that frame for a while: the screen is off or asleep
    dark: bool,
    interaction: Interaction,
    targets: Vec<Target>,
    base_stale: bool,
}

impl Screen {
    fn release(&mut self) {
        self.pool = None;
        if let Some(v) = self.viewport.take() {
            v.destroy();
        }
        if let Some(f) = self.fraction.take() {
            f.destroy();
        }
        self.layer.destroy();
        self.surface.destroy();
    }
}

pub(crate) struct App {
    compositor: wl_compositor::WlCompositor,
    shm: wl_shm::WlShm,
    shell: zwlr_layer_shell_v1::ZwlrLayerShellV1,
    viewporter: Option<wp_viewporter::WpViewporter>,
    fractions: Option<wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1>,
    cursors: Option<wp_cursor_shape_manager_v1::WpCursorShapeManagerV1>,
    seats: Vec<Seat>,
    qh: QueueHandle<App>,
    screens: Vec<Screen>,
    view: &'static dyn View,
    scene: Option<Scene>,
    /// when the scene was worked out, on both clocks
    scene_at: (f64, f64),
    world: World,
    sources: Sources,
    shaper: Shaper,
    /// the weather and the machine, read at most every quarter minute
    real: Option<(Moment, f64)>,
    bent: bool,
    held: bool,
    saver: bool,
    /// a dark screen has just started showing frames again
    woke: bool,
    /// a monitor has said its name, so whether it is covered can be looked up
    named: bool,
    quit: bool,
    next_base: f64,
    last_tick: f64,
    /// DORMER_PROBE: keep moving even when covered, and report where the time goes
    probe: bool,
    spent: [f64; 4],
    frames: u32,
}

impl App {
    fn add_screen(&mut self, id: u32, output: wl_output::WlOutput) {
        let (surface, layer, viewport, fraction) = self.map_surface(id, &output);
        self.screens.push(Screen {
            id,
            output,
            name: String::new(),
            whole_scale: 1,
            surface,
            layer,
            viewport,
            fraction,
            scale120: 0,
            logical: (0, 0),
            pool: None,
            cache: None,
            shown: Vec::new(),
            from: Vec::new(),
            to: Vec::new(),
            fade: None,
            anchors: Anchors::default(),
            waiting: false,
            last_dirty: Vec::new(),
            open: true,
            sent: 0.0,
            dark: false,
            interaction: Interaction::new(),
            targets: Vec::new(),
            base_stale: true,
        });
    }

    #[allow(clippy::type_complexity)]
    fn map_surface(
        &self,
        id: u32,
        output: &wl_output::WlOutput,
    ) -> (wl_surface::WlSurface, zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, Option<wp_viewport::WpViewport>, Option<wp_fractional_scale_v1::WpFractionalScaleV1>) {
        use zwlr_layer_surface_v1::{Anchor, KeyboardInteractivity};
        let surface = self.compositor.create_surface(&self.qh, ());
        let layer = self.shell.get_layer_surface(&surface, Some(output), zwlr_layer_shell_v1::Layer::Background, "dormer".to_string(), &self.qh, id);
        layer.set_anchor(Anchor::Top | Anchor::Bottom | Anchor::Left | Anchor::Right);
        layer.set_exclusive_zone(-1);
        layer.set_keyboard_interactivity(KeyboardInteractivity::None);
        layer.set_size(0, 0);
        if !crate::settings::runtime().interactive {
            let region = self.compositor.create_region(&self.qh, ());
            surface.set_input_region(Some(&region));
            region.destroy();
        }
        let viewport = self.viewporter.as_ref().map(|v| v.get_viewport(&surface, &self.qh, ()));
        let fraction = self.fractions.as_ref().map(|f| f.get_fractional_scale(&surface, &self.qh, id));
        surface.commit();
        (surface, layer, viewport, fraction)
    }

    /// A new wallpaper surface from someone else would sit on top of ours: map ours again, above it.
    fn restack(&mut self) {
        for seat in &mut self.seats { seat.focus.leave(); }
        for i in 0..self.screens.len() {
            let (id, output) = (self.screens[i].id, self.screens[i].output.clone());
            self.screens[i].release();
            let (surface, layer, viewport, fraction) = self.map_surface(id, &output);
            let s = &mut self.screens[i];
            s.surface = surface;
            s.layer = layer;
            s.viewport = viewport;
            s.fraction = fraction;
            s.waiting = false;
            s.last_dirty.clear();
            s.interaction.reset();
        }
    }

    fn screen(&mut self, id: u32) -> Option<usize> {
        self.screens.iter().position(|s| s.id == id)
    }

    /// The compositor has said how big the surface is, or what scale it wants: get buffers to match.
    fn resize(&mut self, i: usize) {
        let s = &self.screens[i];
        if s.logical.0 == 0 || s.logical.1 == 0 {
            return;
        }
        let fractional = s.viewport.is_some() && s.scale120 > 0;
        let scale = if fractional { s.scale120 as f64 / 120.0 } else { s.whole_scale.max(1) as f64 };
        let (w, h) = ((s.logical.0 as f64 * scale).round() as usize, (s.logical.1 as f64 * scale).round() as usize);
        if let Some(v) = &s.viewport {
            v.set_destination(s.logical.0 as i32, s.logical.1 as i32);
        } else {
            s.surface.set_buffer_scale(s.whole_scale.max(1));
        }
        if s.pool.as_ref().map_or(false, |p| p.w == w && p.h == h) {
            return;
        }
        let pool = Pool::new(self, s.id, w, h);
        let keep = self.screens[i].cache.as_ref().map_or(false, |c| c.w == w && c.h == h && c.view.id() == self.view.id());
        let cache = if keep { self.screens[i].cache.take() } else { Some(Cache::new(self.view, config().seed, w, h)) };
        let s = &mut self.screens[i];
        s.pool = pool;
        s.cache = cache;
        s.waiting = false;
        s.fade = None;
        s.last_dirty.clear();
        s.interaction.reset();
        if !keep || s.shown.len() != w * h {
            s.shown.clear();
        }
        self.rebase(true);
        self.draw(i, tm::mono());
    }

    fn moving(&self, s: &Screen) -> bool {
        !self.held && !self.saver && ((s.open && !s.dark) || self.probe)
    }

    /// Could anyone see a change to this screen's still frame?
    fn seen(&self, s: &Screen) -> bool {
        self.probe || (s.open && !s.dark)
    }

    /// Work the scene out afresh and paint the still frame for every screen.
    fn rebase(&mut self, instant: bool) {
        let mono = tm::mono();
        if !self.screens.iter().any(|s| s.cache.is_some() && crate::buffer::should_paint_base(s.open, s.dark, self.probe, !s.shown.is_empty())) {
            for s in &mut self.screens { s.base_stale = true; s.fade = None; s.interaction.reset(); }
            self.next_base = mono + 3600.0;
            return;
        }
        let now = tm::now();
        if self.real.as_ref().map_or(true, |r| now - r.1 > 15.0) {
            let state = State::load();
            let mode = if state.mode == "dark" || state.mode == "light" { state.mode } else { sources::theme_mode() };
            self.real = Some((real_moment(Some(&mode), false), now));
        }
        let mut m: Moment = self.real.as_ref().unwrap().0.at(now);
        with_overrides(&sources::overrides(), &mut m);
        self.bent = m.bent || m.wx.note == "dev";
        // looking out somewhere else: lay the new view out for every screen, and fade over to it
        let moved = m.view.id() != self.view.id();
        if moved {
            self.view = m.view;
            for s in self.screens.iter_mut() {
                s.interaction.reset();
                s.targets.clear();
                s.base_stale |= !self.probe && (!s.open || s.dark);
            }
            self.world.leave();
        }
        let drift = State::load().drift_at(now, &m);
        let mut sc = build(&m, Some(drift as f32));
        sc.live = true;
        let mono = tm::mono();
        for s in self.screens.iter_mut() {
            if !crate::buffer::should_paint_base(s.open, s.dark, self.probe, !s.shown.is_empty()) {
                s.base_stale = true;
                s.fade = None;
                s.interaction.reset();
                continue;
            }
            if s.cache.as_ref().is_some_and(|c| c.view.id() != self.view.id()) {
                let c = s.cache.take().unwrap();
                s.cache = Some(Cache::new(self.view, config().seed, c.w, c.h));
            }
            let Some(cache) = &s.cache else { continue };
            let (cv, anchors) = paint::render(&mut sc, cache);
            let px = cv.finish(sc.p.grain);
            s.anchors = anchors;
            s.targets = cache.stage.targets(&sc, &s.anchors);
            let stale = std::mem::replace(&mut s.base_stale, false);
            if crate::buffer::replace_base(stale, instant, self.bent, moved, s.shown.len() == px.len()) {
                s.shown = px;
                s.from = Vec::new();
                s.to = Vec::new();
                s.fade = None;
                if let Some(p) = s.pool.as_mut() {
                    for b in p.bufs.iter_mut() {
                        b.stale = Some((0, cache.h));
                    }
                }
                continue;
            }
            // fade in only the rows that changed
            let w = cache.w;
            let differs = |y: usize| px[y * w..(y + 1) * w] != s.shown[y * w..(y + 1) * w];
            let Some(y0) = (0..cache.h).find(|&y| differs(y)) else { continue };
            let y1 = (0..cache.h).rev().find(|&y| differs(y)).unwrap() + 1;
            s.from = s.shown.clone();
            s.to = px;
            s.fade = Some((mono, y0, y1));
        }
        if let Some(s) = self.screens.iter().find(|s| self.seen(s) && !s.base_stale && s.cache.is_some()) {
            self.sources = Sources::from(&s.anchors, &s.cache.as_ref().unwrap().lay);
        }
        self.world.retune(&sc, self.screens.iter().map(|s| s.anchors.flicker.len()).max().unwrap_or(0));
        self.scene = Some(sc);
        self.scene_at = (now, mono);
        // a still frame, and now and then a whole view, has just come and gone: hand the memory back
        unsafe { libc::malloc_trim(0) };
        let any_moving = self.screens.iter().any(|s| self.moving(s));
        // the sky moves on every quarter hour under a still wallpaper; nobody needs it moved while it is hidden,
        // because it is painted afresh as it comes back into view
        let any_seen = self.screens.iter().any(|s| self.seen(s));
        self.next_base = mono + if any_moving { 20.0 } else if any_seen { 900.0 } else { 3600.0 };
    }

    /// Paint one frame for one screen and hand it to the compositor.
    fn draw(&mut self, i: usize, mono: f64) {
        if !self.seen(&self.screens[i]) && self.screens[i].sent > 0.0 { return; }
        let moving = self.moving(&self.screens[i]);
        let Some(sc) = self.scene.as_ref() else { return };
        let s = &mut self.screens[i];
        let (Some(pool), Some(cache)) = (s.pool.as_mut(), s.cache.as_ref()) else { return };
        if s.waiting || s.shown.len() != pool.w * pool.h {
            return;
        }
        let Some(k) = pool.bufs.iter().position(|b| !b.busy) else { return };
        let (w, h) = (pool.w, pool.h);

        // carry the cross-fade forward
        if let Some((start, y0, y1)) = s.fade {
            let f = ((mono - start) / FADE).clamp(0.0, 1.0);
            let a = (f * f * (3.0 - 2.0 * f) * 256.0) as u32;
            if f >= 1.0 {
                s.shown = std::mem::take(&mut s.to);
                s.from = Vec::new();
                s.fade = None;
            } else {
                for ((d, &p), &q) in s.shown[y0 * w..y1 * w].iter_mut().zip(&s.from[y0 * w..y1 * w]).zip(&s.to[y0 * w..y1 * w]) {
                    if p != q {
                        let rb = ((p & 0x00FF_00FF) * (256 - a) + (q & 0x00FF_00FF) * a) >> 8;
                        let g = ((p & 0x0000_FF00) * (256 - a) + (q & 0x0000_FF00) * a) >> 8;
                        *d = 0xFF00_0000 | (rb & 0x00FF_00FF) | (g & 0x0000_FF00);
                    }
                }
            }
            for b in pool.bufs.iter_mut() {
                b.stale = Some(match b.stale {
                    Some((a0, a1)) => (a0.min(y0), a1.max(y1)),
                    None => (y0, y1),
                });
            }
        }

        let buf = &mut pool.bufs[k];
        let px = unsafe { std::slice::from_raw_parts_mut(pool.map.add(buf.at), w * h) };
        let everywhere = moving && self.world.everywhere();
        let mut damage: Vec<Rect> = std::mem::take(&mut s.last_dirty);
        let stale = buf.stale.take();
        crate::buffer::restore_pixels(px, &s.shown, w, stale, &buf.dirty);
        if let Some((y0, y1)) = stale {
            damage.push(Rect { x0: 0, y0: y0 as i32, x1: w as i32, y1: y1 as i32 });
        }
        let mut dirty = std::mem::take(&mut buf.dirty);
        dirty.clear();
        if moving {
            let began = tm::mono();
            let now = self.scene_at.0 + (mono - self.scene_at.1);
            let mut frame = Frame { w, h, px, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None, full_damage: everywhere };
            let mut d = Draw::new(&mut frame, &mut self.shaper);
            self.world.draw(&mut d, sc, cache, &s.anchors, now);
            s.interaction.draw(&mut d, sc, &cache.lay);
            self.spent[3] += tm::mono() - began;
        }
        let dirty = if everywhere { vec![Rect { x0: 0, y0: 0, x1: w as i32, y1: h as i32 }] } else { coalesce(dirty, w as i32, h as i32) };
        damage.extend_from_slice(&dirty);
        buf.dirty = dirty.clone();
        s.last_dirty = dirty;
        if damage.is_empty() {
            return;
        }
        for r in coalesce(damage, w as i32, h as i32) {
            s.surface.damage_buffer(r.x0, r.y0, r.x1 - r.x0, r.y1 - r.y0);
        }
        s.surface.attach(Some(&buf.buffer), 0, 0);
        s.surface.frame(&self.qh, s.id);
        s.surface.commit();
        buf.busy = true;
        s.waiting = true;
        s.sent = mono;
    }

    /// Frames a second worth drawing right now; 0 when nothing on any screen should move.
    fn pace(&self) -> f64 {
        let Some(sc) = self.scene.as_ref() else { return 0.0 };
        if !self.screens.iter().any(|s| self.moving(s) && s.pool.is_some()) {
            return if self.screens.iter().any(|s| s.fade.is_some() && self.seen(s)) { 20.0 } else { 0.0 };
        }
        let fps = crate::settings::runtime().fps as f64;
        if self.world.everywhere() {
            // rain repaints the whole screen every frame, so it gets a film's frame rate; snow falls slower still
            fps.min(if self.world.raining() { 22.0 } else { 18.0 })
        } else if self.world.lively() || self.screens.iter().any(|s| self.moving(s) && (s.interaction.active() || s.cache.as_ref().map_or(false, |c| c.stage.lively(sc, self.clock(tm::mono()))))) {
            fps
        } else if self.screens.iter().any(|s| s.fade.is_some() && self.seen(s)) {
            fps.min(20.0)
        } else {
            (fps / 2.0).max(10.0)
        }
    }

    /// The time on the clock, carried on smoothly from when the scene was last worked out.
    fn clock(&self, mono: f64) -> f64 {
        self.scene_at.0 + (mono - self.scene_at.1)
    }

    fn tick(&mut self, mono: f64) {
        let dt = (mono - self.last_tick).clamp(0.0, 0.12) as f32;
        self.last_tick = mono;
        let began = tm::mono();
        let paused = self.held || self.saver;
        for s in &mut self.screens {
            if !paused && ((s.open && !s.dark) || self.probe) {
                s.interaction.step(dt, &s.targets);
            } else { s.interaction.reset(); }
        }
        if let Some(sc) = self.scene.as_ref() {
            if self.screens.iter().any(|s| self.moving(s)) {
                let now = self.scene_at.0 + (mono - self.scene_at.1);
                self.world.step(dt, sc, &self.sources, now);
            }
        }
        let stepped = tm::mono();
        for i in 0..self.screens.len() {
            self.draw(i, mono);
        }
        self.spent[0] += stepped - began;
        self.spent[1] += tm::mono() - stepped;
        self.frames += 1;
    }
}

impl App {
    fn forget_output(&mut self, id: u32) {
        for seat in &mut self.seats { seat.focus.clear_output(id); }
    }

    fn motion(&mut self, seat_id: u32, point: (f64, f64)) {
        let Some(seat) = self.seats.iter_mut().find(|s| s.id == seat_id) else { return };
        seat.focus.motion(point);
        let Some(id) = seat.focus.output else { return };
        let Some(i) = self.screen(id) else { return };
        if !self.moving(&self.screens[i]) { return; }
        let s = &mut self.screens[i];
        if let Some(cache) = &s.cache { s.interaction.motion(pointer_position(point, s.logical, &cache.lay)); }
    }

    fn leave(&mut self, seat_id: u32) {
        let Some(seat) = self.seats.iter_mut().find(|s| s.id == seat_id) else { return };
        let id = seat.focus.output;
        seat.focus.leave();
        if let Some(i) = id.and_then(|id| self.screen(id)) { self.screens[i].interaction.motion(None); }
    }
}

// ── what the compositor tells us ─────────────────────────────────────────────

impl Dispatch<wl_registry::WlRegistry, GlobalListContents> for App {
    fn event(app: &mut Self, registry: &wl_registry::WlRegistry, event: wl_registry::Event, _: &GlobalListContents, _: &Connection, qh: &QueueHandle<Self>) {
        match event {
            wl_registry::Event::Global { name, interface, version } if interface == "wl_output" => {
                let output = registry.bind::<wl_output::WlOutput, _, _>(name, version.min(4), qh, name);
                app.add_screen(name, output);
            }
            wl_registry::Event::Global { name, interface, version } if interface == "wl_seat" && crate::settings::runtime().interactive => {
                app.seats.push(Seat::new(name, registry.bind::<wl_seat::WlSeat, _, _>(name, version.min(7), qh, name)));
            }
            wl_registry::Event::GlobalRemove { name } => {
                if let Some(i) = app.screen(name) {
                    app.forget_output(name);
                    let mut s = app.screens.remove(i);
                    s.release();
                }
                if let Some(i) = app.seats.iter().position(|s| s.id == name) {
                    app.leave(name);
                    app.seats.remove(i);
                }
            }
            _ => {}
        }
    }
}

impl Dispatch<wl_output::WlOutput, u32> for App {
    fn event(app: &mut Self, _: &wl_output::WlOutput, event: wl_output::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        let Some(i) = app.screen(*id) else { return };
        match event {
            wl_output::Event::Scale { factor } => app.screens[i].whole_scale = factor,
            wl_output::Event::Name { name } => {
                app.screens[i].name = name;
                app.named = true;
            }
            wl_output::Event::Done => app.resize(i),
            _ => {}
        }
    }
}

impl Dispatch<zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, u32> for App {
    fn event(app: &mut Self, layer: &zwlr_layer_surface_v1::ZwlrLayerSurfaceV1, event: zwlr_layer_surface_v1::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        let Some(i) = app.screen(*id) else { return };
        match event {
            zwlr_layer_surface_v1::Event::Configure { serial, width, height } => {
                layer.ack_configure(serial);
                let s = &mut app.screens[i];
                if s.logical != (width, height) || s.pool.is_none() {
                    s.logical = (width, height);
                    s.pool = None;
                    app.resize(i);
                } else {
                    // the same size again: the surface was mapped anew, so show it something
                    if let Some(p) = s.pool.as_mut() {
                        for b in p.bufs.iter_mut() {
                            b.stale = Some((0, p.h));
                        }
                    }
                    s.waiting = false;
                    app.draw(i, tm::mono());
                }
            }
            zwlr_layer_surface_v1::Event::Closed => {
                app.forget_output(*id);
                let mut s = app.screens.remove(i);
                s.release();
            }
            _ => {}
        }
    }
}

impl Dispatch<wp_fractional_scale_v1::WpFractionalScaleV1, u32> for App {
    fn event(app: &mut Self, _: &wp_fractional_scale_v1::WpFractionalScaleV1, event: wp_fractional_scale_v1::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let (Some(i), wp_fractional_scale_v1::Event::PreferredScale { scale }) = (app.screen(*id), event) {
            if app.screens[i].scale120 != scale {
                app.screens[i].scale120 = scale;
                app.resize(i);
            }
        }
    }
}

impl Dispatch<wl_callback::WlCallback, u32> for App {
    fn event(app: &mut Self, _: &wl_callback::WlCallback, event: wl_callback::Event, id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        if let (Some(i), wl_callback::Event::Done { .. }) = (app.screen(*id), event) {
            let s = &mut app.screens[i];
            s.waiting = false;
            if s.dark {
                s.dark = false;
                app.woke = true;
            }
        }
    }
}

impl Dispatch<wl_buffer::WlBuffer, (u32, usize)> for App {
    fn event(app: &mut Self, _: &wl_buffer::WlBuffer, event: wl_buffer::Event, key: &(u32, usize), _: &Connection, _: &QueueHandle<Self>) {
        if let (Some(i), wl_buffer::Event::Release) = (app.screen(key.0), event) {
            if let Some(b) = app.screens[i].pool.as_mut().and_then(|p| p.bufs.get_mut(key.1)) {
                b.busy = false;
            }
        }
    }
}

delegate_noop!(App: wl_compositor::WlCompositor);
delegate_noop!(App: ignore wl_shm::WlShm);
delegate_noop!(App: wl_shm_pool::WlShmPool);
delegate_noop!(App: ignore wl_surface::WlSurface);
delegate_noop!(App: wl_region::WlRegion);
delegate_noop!(App: zwlr_layer_shell_v1::ZwlrLayerShellV1);
delegate_noop!(App: wp_viewporter::WpViewporter);
delegate_noop!(App: wp_viewport::WpViewport);
delegate_noop!(App: wp_fractional_scale_manager_v1::WpFractionalScaleManagerV1);
delegate_noop!(App: wp_cursor_shape_manager_v1::WpCursorShapeManagerV1);
delegate_noop!(App: wp_cursor_shape_device_v1::WpCursorShapeDeviceV1);
delegate_noop!(App: ignore wl_buffer::WlBuffer);

impl Dispatch<wl_seat::WlSeat, u32> for App {
    fn event(app: &mut Self, _: &wl_seat::WlSeat, event: wl_seat::Event, id: &u32, _: &Connection, qh: &QueueHandle<Self>) {
        if let wl_seat::Event::Capabilities { capabilities } = event {
            let pointer = capabilities.into_result().is_ok_and(|c| c.contains(wl_seat::Capability::Pointer));
            if !pointer { app.leave(*id); }
            let Some(seat) = app.seats.iter_mut().find(|s| s.id == *id) else { return };
            if pointer && seat.pointer.is_none() {
                let p = seat.seat.get_pointer(qh, *id);
                seat.shape = app.cursors.as_ref().map(|m| m.get_pointer(&p, qh, ()));
                seat.pointer = Some(p);
            } else if !pointer { seat.release_pointer(); }
        }
    }
}

impl Dispatch<wl_pointer::WlPointer, u32> for App {
    fn event(app: &mut Self, pointer: &wl_pointer::WlPointer, event: wl_pointer::Event, seat_id: &u32, _: &Connection, _: &QueueHandle<Self>) {
        match event {
            wl_pointer::Event::Enter { serial, surface, surface_x, surface_y } => {
                app.leave(*seat_id);
                let Some(i) = app.screens.iter().position(|s| s.surface == surface) else { return };
                let id = app.screens[i].id;
                let scale = if app.screens[i].scale120 > 0 { app.screens[i].scale120.div_ceil(120) as i32 } else { app.screens[i].whole_scale.max(1) };
                let Some(seat) = app.seats.iter_mut().find(|s| s.id == *seat_id) else { return };
                seat.focus.enter(id, (surface_x, surface_y));
                if let Some(shape) = &seat.shape {
                    shape.set_shape(serial, wp_cursor_shape_device_v1::Shape::Default);
                } else {
                    if seat.cursor.as_ref().is_none_or(|c| c.scale != scale) { seat.cursor = Cursor::new(&app.compositor, &app.shm, &app.qh, scale); }
                    if let Some(cursor) = &seat.cursor { cursor.set(pointer, serial); }
                }
                app.motion(*seat_id, (surface_x, surface_y));
            }
            wl_pointer::Event::Motion { surface_x, surface_y, .. } => app.motion(*seat_id, (surface_x, surface_y)),
            wl_pointer::Event::Leave { .. } => app.leave(*seat_id),
            wl_pointer::Event::Button { button, state, .. } if button == 0x110 && matches!(state.into_result(), Ok(wl_pointer::ButtonState::Pressed)) => {
                let Some(seat) = app.seats.iter().find(|s| s.id == *seat_id) else { return };
                let Some(id) = seat.focus.output else { return };
                let Some(point) = seat.focus.on(id) else { return };
                let Some(i) = app.screen(id) else { return };
                if !app.moving(&app.screens[i]) { return; }
                let s = &mut app.screens[i];
                if let Some(p) = s.cache.as_ref().and_then(|c| pointer_position(point, s.logical, &c.lay)) {
                    s.interaction.click(p, tm::mono(), &s.targets);
                }
            }
            _ => {}
        }
    }
}

// ── asking Hyprland whether anyone can see the wallpaper ─────────────────────

struct Hypr {
    dir: Option<PathBuf>,
    events: Option<UnixStream>,
}

impl Hypr {
    fn new() -> Hypr {
        let dir = std::env::var("HYPRLAND_INSTANCE_SIGNATURE").ok().filter(|s| !s.is_empty()).map(|sig| sources::runtime().join("hypr").join(sig));
        let events = dir.as_ref().and_then(|d| UnixStream::connect(d.join(".socket2.sock")).ok());
        if let Some(e) = &events {
            let _ = e.set_nonblocking(true);
        }
        Hypr { dir, events }
    }

    /// For each monitor: is the wallpaper in view? Not under a tiled or fullscreen window, nor under
    /// floating ones that between them leave next to nothing of it showing.
    fn look(&self) -> Option<HashMap<String, bool>> {
        self.dir.as_ref()?;
        let (monitors, clients) = (sources::hyprland("j/monitors")?, sources::hyprland("j/clients")?);
        let mut open = HashMap::new();
        for m in monitors.as_array()? {
            let mut shown = vec![m["activeWorkspace"]["id"].as_i64().unwrap_or(i64::MIN)];
            match m["specialWorkspace"]["id"].as_i64() {
                Some(id) if id != 0 => shown.push(id),
                _ => {}
            }
            let here: Vec<&serde_json::Value> = clients
                .as_array()?
                .iter()
                .filter(|c| shown.contains(&c["workspace"]["id"].as_i64().unwrap_or(i64::MAX)) && c["mapped"].as_bool().unwrap_or(true) && !c["hidden"].as_bool().unwrap_or(false))
                .collect();
            let fullscreen = |c: &serde_json::Value| c["fullscreen"].as_i64().unwrap_or(0) != 0 || c["fullscreen"].as_bool().unwrap_or(false);
            let covered = here.iter().any(|c| !c["floating"].as_bool().unwrap_or(false) || fullscreen(c)) || buried(m, &here);
            open.insert(m["name"].as_str().unwrap_or("").to_string(), !covered);
        }
        Some(open)
    }
}

/// Do these floating windows hide nearly all of this monitor, outside its bars? Measured on a coarse grid.
fn buried(m: &serde_json::Value, windows: &[&serde_json::Value]) -> bool {
    const N: usize = 40;
    let num = |v: &serde_json::Value| v.as_f64().unwrap_or(0.0);
    let scale = num(&m["scale"]).max(0.1);
    // the monitor in the same layout coordinates as the windows; a quarter turn swaps its sides
    let (mut w, mut h) = (num(&m["width"]) / scale, num(&m["height"]) / scale);
    if m["transform"].as_i64().unwrap_or(0) % 2 == 1 {
        std::mem::swap(&mut w, &mut h);
    }
    // leave out what the bars keep for themselves: they hide the wallpaper anyway
    let gap = |k: usize| num(&m["reserved"][k]);
    let (x0, y0) = (num(&m["x"]) + gap(0), num(&m["y"]) + gap(1));
    let (w, h) = (w - gap(0) - gap(2), h - gap(1) - gap(3));
    if w < 1.0 || h < 1.0 || windows.is_empty() {
        return false;
    }
    let mut hidden = 0;
    for gy in 0..N {
        for gx in 0..N {
            let (x, y) = (x0 + (gx as f64 + 0.5) * w / N as f64, y0 + (gy as f64 + 0.5) * h / N as f64);
            if windows.iter().any(|c| {
                let (cx, cy, cw, ch) = (num(&c["at"][0]), num(&c["at"][1]), num(&c["size"][0]), num(&c["size"][1]));
                cx <= x && x < cx + cw && cy <= y && y < cy + ch
            }) {
                hidden += 1;
            }
        }
    }
    hidden * 100 >= N * N * 96
}

fn readable(fds: &[libc::pollfd], k: usize) -> bool {
    fds[k].revents & (libc::POLLIN | libc::POLLHUP | libc::POLLERR) != 0
}

/// Run the living wallpaper until told to stop.
pub fn run() -> i32 {
    // Each still frame needs tens of megabytes for a few milliseconds. Ask for such blocks to be
    // mapped and unmapped rather than kept on the heap, so the memory goes back between frames.
    unsafe { libc::mallopt(libc::M_MMAP_THRESHOLD, 1 << 20) };
    let conn = match Connection::connect_to_env() {
        Ok(c) => c,
        Err(e) => {
            eprintln!("dormer: no Wayland compositor to talk to ({})", e);
            return 1;
        }
    };
    let (globals, mut queue) = match registry_queue_init::<App>(&conn) {
        Ok(g) => g,
        Err(e) => {
            eprintln!("dormer: {}", e);
            return 1;
        }
    };
    let qh = queue.handle();
    let (Ok(compositor), Ok(shm)) = (globals.bind::<wl_compositor::WlCompositor, _, _>(&qh, 4..=6, ()), globals.bind::<wl_shm::WlShm, _, _>(&qh, 1..=1, ())) else {
        eprintln!("dormer: this compositor is missing wl_compositor or wl_shm");
        return 1;
    };
    let Ok(shell) = globals.bind::<zwlr_layer_shell_v1::ZwlrLayerShellV1, _, _>(&qh, 1..=4, ()) else {
        eprintln!("dormer: this compositor has no layer shell, so there is nowhere to put a wallpaper");
        return 1;
    };
    let mut app = App {
        compositor,
        shm,
        shell,
        viewporter: globals.bind(&qh, 1..=1, ()).ok(),
        fractions: globals.bind(&qh, 1..=1, ()).ok(),
        cursors: globals.bind(&qh, 1..=1, ()).ok(),
        seats: Vec::new(),
        qh: qh.clone(),
        screens: Vec::new(),
        view: sources::view(),
        scene: None,
        scene_at: (0.0, 0.0),
        world: World::new(tm::now() as u64),
        sources: Sources::default(),
        shaper: Shaper::new(),
        real: None,
        bent: false,
        held: false,
        saver: sources::held(),
        woke: false,
        named: false,
        quit: false,
        next_base: 0.0,
        last_tick: tm::mono(),
        probe: std::env::var("DORMER_PROBE").is_ok(),
        spent: [0.0; 4],
        frames: 0,
    };
    let outputs: Vec<(u32, u32)> = globals.contents().with_list(|list| list.iter().filter(|g| g.interface == "wl_output").map(|g| (g.name, g.version)).collect());
    for (name, version) in outputs {
        let output = globals.registry().bind::<wl_output::WlOutput, _, _>(name, version.min(4), &qh, name);
        app.add_screen(name, output);
    }
    if crate::settings::runtime().interactive {
        let seats: Vec<(u32, u32)> = globals.contents().with_list(|list| list.iter().filter(|g| g.interface == "wl_seat").map(|g| (g.name, g.version)).collect());
        for (name, version) in seats {
            app.seats.push(Seat::new(name, globals.registry().bind::<wl_seat::WlSeat, _, _>(name, version.min(7), &qh, name)));
        }
    }

    let _ = std::fs::create_dir_all(sources::cache());
    let _ = std::fs::write(cache_file("live.pid"), std::process::id().to_string());
    let hypr = Hypr::new();

    // signals and file changes arrive as readable descriptors, alongside the compositor's
    let (signals, watch) = unsafe {
        let mut set: libc::sigset_t = std::mem::zeroed();
        libc::sigemptyset(&mut set);
        for sig in [libc::SIGUSR1, libc::SIGUSR2, libc::SIGTERM, libc::SIGINT] {
            libc::sigaddset(&mut set, sig);
        }
        libc::sigprocmask(libc::SIG_BLOCK, &set, std::ptr::null_mut());
        let signals = libc::signalfd(-1, &set, libc::SFD_NONBLOCK | libc::SFD_CLOEXEC);
        let watch = libc::inotify_init1(libc::IN_NONBLOCK | libc::IN_CLOEXEC);
        let _ = std::fs::create_dir_all(sources::state());
        for dir in [sources::cache(), sources::state()] {
            if let Ok(dir) = std::ffi::CString::new(dir.to_string_lossy().as_bytes()) {
                libc::inotify_add_watch(watch, dir.as_ptr(), libc::IN_CLOSE_WRITE | libc::IN_MOVED_TO | libc::IN_DELETE);
            }
        }
        (signals, watch)
    };

    let (mut look_due, mut restack_due, mut rebase_due) = (Some(tm::mono()), None::<f64>, None::<f64>);
    let mut saver_due = tm::mono() + 5.0;
    let mut probe_due = tm::mono() + 5.0;
    while !app.quit {
        if let Err(e) = queue.dispatch_pending(&mut app) {
            eprintln!("dormer: {}", e);
            break;
        }
        let mono = tm::mono();
        if std::mem::take(&mut app.named) {
            look_due = Some(mono);
        }
        if look_due.map_or(false, |t| mono >= t) {
            look_due = None;
            if let Some(open) = hypr.look() {
                let mut uncovered = false;
                for s in app.screens.iter_mut() {
                    let now_open = *open.get(&s.name).unwrap_or(&true);
                    uncovered |= now_open && !s.open;
                    s.open = now_open;
                }
                if uncovered {
                    app.rebase(true); // back in view: show this minute, not the one it was hidden in
                }
            }
        }
        if restack_due.map_or(false, |t| mono >= t) {
            restack_due = None;
            app.restack();
        }
        // a frame handed over a second ago and still not shown: this screen is off, so stop until it asks again
        for s in app.screens.iter_mut() {
            s.dark = s.waiting && mono - s.sent > 1.0;
        }
        if crate::buffer::rebase_on_wake(std::mem::take(&mut app.woke), app.screens.iter().any(|s| app.seen(s) && s.base_stale), mono - app.scene_at.1) {
            app.rebase(true); // awake again: show this minute, not the one it went dark in
        }
        if mono >= saver_due {
            saver_due = mono + 5.0;
            let saver = sources::held();
            if saver != app.saver {
                app.saver = saver;
                app.next_base = mono;
            }
        }
        let before = tm::mono();
        if rebase_due.map_or(false, |t| mono >= t) {
            rebase_due = None;
            app.real = None;
            app.rebase(false);
        } else if mono >= app.next_base && app.screens.iter().any(|s| s.cache.is_some()) {
            app.rebase(false);
        }
        app.spent[2] += tm::mono() - before;
        if app.probe && mono >= probe_due {
            eprintln!(
                "5s: {} frames; step {:.1} ms, draw {:.1} ms (the moving things {:.1} ms), still frame {:.1} ms; pace {}",
                app.frames, app.spent[0] * 1e3, app.spent[1] * 1e3, app.spent[3] * 1e3, app.spent[2] * 1e3, app.pace()
            );
            app.spent = [0.0; 4];
            app.frames = 0;
            probe_due = mono + 5.0;
        }
        let pace = app.pace();
        if pace > 0.0 && mono - app.last_tick >= 1.0 / pace - 0.002 {
            app.tick(mono);
        }
        if conn.flush().is_err() {
            break;
        }

        let Some(guard) = queue.prepare_read() else { continue };
        let mono = tm::mono();
        let mut wake = app.next_base.max(mono + 0.05).min(saver_due);
        if pace > 0.0 {
            wake = wake.min(app.last_tick + 1.0 / pace);
        }
        for due in [look_due, restack_due, rebase_due].into_iter().flatten() {
            wake = wake.min(due);
        }
        let timeout = ((wake - mono).max(0.0) * 1000.0).ceil() as i32;
        let mut fds = [
            libc::pollfd { fd: guard.connection_fd().as_raw_fd(), events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: signals, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: watch, events: libc::POLLIN, revents: 0 },
            libc::pollfd { fd: hypr.events.as_ref().map_or(-1, |e| e.as_raw_fd()), events: libc::POLLIN, revents: 0 },
        ];
        unsafe { libc::poll(fds.as_mut_ptr(), fds.len() as libc::nfds_t, timeout) };
        if readable(&fds, 0) {
            if let Err(wayland_client::backend::WaylandError::Io(e)) = guard.read() {
                if e.kind() != std::io::ErrorKind::WouldBlock {
                    eprintln!("dormer: the compositor went away ({})", e);
                    break;
                }
            }
        } else {
            drop(guard);
        }
        let mono = tm::mono();
        if readable(&fds, 1) {
            let mut info: libc::signalfd_siginfo = unsafe { std::mem::zeroed() };
            while unsafe { libc::read(signals, &mut info as *mut _ as *mut libc::c_void, std::mem::size_of::<libc::signalfd_siginfo>()) } > 0 {
                match info.ssi_signo as i32 {
                    libc::SIGUSR1 => app.held = true,
                    libc::SIGUSR2 => {
                        app.held = false;
                        rebase_due = Some(mono);
                    }
                    _ => app.quit = true,
                }
            }
        }
        if readable(&fds, 2) {
            // the dev panel moved a slider, `dormer pretend` changed the weather, `dormer view` chose
            // another view, or a draw left new state
            let mut raw = [0u8; 4096];
            let mut mine = false;
            loop {
                let n = unsafe { libc::read(watch, raw.as_mut_ptr() as *mut libc::c_void, raw.len()) };
                if n <= 0 {
                    break;
                }
                let text = String::from_utf8_lossy(&raw[..n as usize]);
                mine |= ["dev.json", "pretend.json", "state.json", "view"].iter().any(|name| text.contains(name));
            }
            if mine {
                rebase_due = Some(mono);
            }
        }
        if readable(&fds, 3) {
            let mut text = String::new();
            if let Some(mut e) = hypr.events.as_ref() {
                let mut raw = [0u8; 8192];
                while let Ok(n) = e.read(&mut raw) {
                    if n == 0 {
                        break;
                    }
                    text.push_str(&String::from_utf8_lossy(&raw[..n]));
                }
            }
            if WALLPAPERS.iter().any(|name| text.contains(&format!("openlayer>>{}", name))) {
                restack_due = Some(mono + 0.4);
            }
            // only what can change what covers the wallpaper; a window's title or focus cannot
            let covering = text.lines().filter_map(|l| l.split(">>").next()).any(|event| {
                !matches!(event, "windowtitle" | "windowtitlev2" | "activewindow" | "activewindowv2" | "urgent" | "activelayout" | "submap" | "screencast" | "bell")
                    && ["workspace", "window", "fullscreen", "floating", "special", "monitor", "minimized"].iter().any(|key| event.contains(key))
            });
            if covering && look_due.is_none() {
                look_due = Some(mono + 0.14);
            }
        }
    }
    let _ = std::fs::remove_file(cache_file("live.pid"));
    0
}

/// Run the world for a while without a screen and save what it would be showing.
pub fn one_frame(path: &str, seconds: f64, m: &Moment, size: (usize, usize), seed: u64, pointer: Option<crate::gfx::P>, click: Option<crate::gfx::P>) -> Result<(), String> {
    let cache = Cache::new(m.view, seed, size.0, size.1);
    let mut sc = build(m, None);
    sc.live = true;
    let (cv, anchors) = paint::render(&mut sc, &cache);
    let targets = cache.stage.targets(&sc, &anchors);
    let mut interaction = crate::interaction::Interaction::new();
    interaction.motion(pointer);
    if let Some(p) = click { interaction.click(p, 0.0, &targets); }
    let mut px = cv.finish(sc.p.grain);
    let mut world = World::new(7);
    world.retune(&sc, anchors.flicker.len());
    let src = Sources::from(&anchors, &cache.lay);
    if sc.eph.sun.el > -3.0 {
        world.spawn_flock(Some(520.0));
        world.spawn_flock(Some(1050.0));
    }
    let steps = (seconds * 30.0) as usize;
    for k in 0..steps {
        if k == steps.saturating_sub(14) {
            world.spawn_meteor();
            if let Some(b) = src.perch {
                if m.ts as i64 % 3600 < 60 {
                    world.scatter(b);
                }
            }
        }
        world.step(1.0 / 30.0, &sc, &src, m.ts + k as f64 / 30.0);
        interaction.step(1.0 / 30.0, &targets);
    }
    let mut dirty = Vec::new();
    let mut shaper = Shaper::new();
    let mut frame = Frame { w: cache.w, h: cache.h, px: &mut px, dirty: &mut dirty, depth: Some(cache.stage.depth()), behind: None, full_damage: false };
    let mut d = Draw::new(&mut frame, &mut shaper);
    world.draw(&mut d, &sc, &cache, &anchors, m.ts + seconds);
    interaction.draw(&mut d, &sc, &cache.lay);
    drop(d);
    // how long a frame of this takes to draw
    let began = std::time::Instant::now();
    for _ in 0..50 {
        let (mut scratch, mut marks) = (px.clone(), Vec::new());
        let mut frame = Frame { w: cache.w, h: cache.h, px: &mut scratch, dirty: &mut marks, depth: Some(cache.stage.depth()), behind: None, full_damage: world.everywhere() };
        let mut d = Draw::new(&mut frame, &mut shaper);
        world.draw(&mut d, &sc, &cache, &anchors, m.ts + seconds);
        interaction.draw(&mut d, &sc, &cache.lay);
    }
    let copy = std::time::Instant::now();
    for _ in 0..50 {
        std::hint::black_box(px.clone());
    }
    let per = (began.elapsed().as_secs_f64() - 2.0 * copy.elapsed().as_secs_f64()) / 50.0 * 1e3;
    let rects = coalesce(dirty, cache.w as i32, cache.h as i32);
    let area: i64 = rects.iter().map(|r| r.area()).sum();
    crate::gfx::save_png(path, cache.w, cache.h, &px)?;
    println!("{}  damage: {} rectangles, {:.1}% of the screen; {:.2} ms to draw", path, rects.len(), area as f64 / (cache.w * cache.h) as f64 * 100.0, per);
    Ok(())
}
