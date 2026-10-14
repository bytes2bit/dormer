//! Wayland seat and cursor resources. No polling or keyboard acquisition.

use std::io::Write;
use std::os::fd::{AsFd, FromRawFd};
use wayland_client::protocol::{wl_buffer, wl_compositor, wl_pointer, wl_seat, wl_shm, wl_surface};
use wayland_client::{Proxy, QueueHandle};
use wayland_protocols::wp::cursor_shape::v1::client::wp_cursor_shape_device_v1;
use crate::wl::App;

#[derive(Default)]
pub struct Focus {
    pub output: Option<u32>,
    point: (f64, f64),
}

impl Focus {
    pub fn enter(&mut self, output: u32, point: (f64, f64)) { self.output = Some(output); self.point = point; }
    pub fn motion(&mut self, point: (f64, f64)) { self.point = point; }
    pub fn on(&self, output: u32) -> Option<(f64, f64)> { (self.output == Some(output)).then_some(self.point) }
    pub fn leave(&mut self) { self.output = None; }
    pub fn clear_output(&mut self, output: u32) { if self.output == Some(output) { self.leave(); } }
}

pub struct Seat {
    pub id: u32,
    pub seat: wl_seat::WlSeat,
    pub pointer: Option<wl_pointer::WlPointer>,
    pub shape: Option<wp_cursor_shape_device_v1::WpCursorShapeDeviceV1>,
    pub cursor: Option<Cursor>,
    pub focus: Focus,
}

impl Seat {
    pub fn new(id: u32, seat: wl_seat::WlSeat) -> Self {
        Self { id, seat, pointer: None, shape: None, cursor: None, focus: Focus::default() }
    }

    pub fn release_pointer(&mut self) {
        if let Some(s) = self.shape.take() { s.destroy(); }
        self.cursor = None;
        if let Some(p) = self.pointer.take() { if p.version() >= 3 { p.release(); } }
        self.focus.leave();
    }
}

impl Drop for Seat {
    fn drop(&mut self) {
        self.release_pointer();
        if self.seat.version() >= 5 { self.seat.release(); }
    }
}

pub struct Cursor {
    surface: wl_surface::WlSurface,
    buffer: wl_buffer::WlBuffer,
    _file: std::fs::File,
    pub scale: i32,
}

impl Cursor {
    pub fn new(compositor: &wl_compositor::WlCompositor, shm: &wl_shm::WlShm, qh: &QueueHandle<App>, scale: i32) -> Option<Self> {
        let scale = scale.clamp(1, 8);
        let px = arrow_pixels(scale as u32);
        let mut file = unsafe {
            let fd = libc::memfd_create(c"dormer-cursor".as_ptr(), libc::MFD_CLOEXEC);
            if fd < 0 { return None; }
            std::fs::File::from_raw_fd(fd)
        };
        for p in &px { file.write_all(&p.to_ne_bytes()).ok()?; }
        let side = 32 * scale;
        let pool = shm.create_pool(file.as_fd(), (px.len() * 4) as i32, qh, ());
        let buffer = pool.create_buffer(0, side, side, side * 4, wl_shm::Format::Argb8888, qh, ());
        pool.destroy();
        let surface = compositor.create_surface(qh, ());
        surface.set_buffer_scale(scale);
        surface.attach(Some(&buffer), 0, 0);
        surface.damage_buffer(0, 0, side, side);
        surface.commit();
        Some(Self { surface, buffer, _file: file, scale })
    }

    pub fn set(&self, pointer: &wl_pointer::WlPointer, serial: u32) {
        pointer.set_cursor(serial, Some(&self.surface), 3, 2);
    }
}

impl Drop for Cursor {
    fn drop(&mut self) { self.surface.destroy(); self.buffer.destroy(); }
}

fn arrow_pixels(scale: u32) -> Vec<u32> {
    let scale = scale.clamp(1, 8);
    let side = 32 * scale as usize;
    let s = scale as f32;
    let mut pixels = vec![0; side * side];
    let mut sh = crate::gfx::Shaper::new();
    let outer = [(3.0, 2.0), (3.0, 23.0), (8.5, 18.0), (12.0, 27.0), (16.0, 25.5), (12.5, 17.0), (20.0, 17.0)];
    let inner = [(4.5, 5.0), (4.5, 19.5), (9.0, 15.8), (12.8, 25.0), (14.0, 24.5), (10.0, 15.5), (16.4, 15.5)];
    for (pts, white) in [(outer, false), (inner, true)] {
        sh.begin(0.0, 0.0, side as f32, side as f32, side, side);
        sh.poly(&pts.map(|p| (p.0 * s, p.1 * s)));
        let (y0, mask) = sh.mask(side);
        for (n, a) in mask.iter().enumerate() {
            let at = y0 * side + n;
            if at >= pixels.len() || *a == 0 { continue; }
            let alpha = *a as u32;
            if white {
                let old = pixels[at] >> 24;
                pixels[at] = (old.max(alpha) << 24) | (alpha << 16) | (alpha << 8) | alpha;
            } else { pixels[at] = alpha << 24; }
        }
    }
    pixels
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn focus_routes_only_current_output_and_disappears_on_leave() {
        let mut f = Focus::default();
        f.enter(4, (12.0, 30.0));
        f.motion((40.0, 50.0));
        assert_eq!(f.on(4), Some((40.0, 50.0)));
        assert_eq!(f.on(5), None);
        f.clear_output(5);
        assert!(f.on(4).is_some());
        f.clear_output(4);
        assert!(f.on(4).is_none());
        f.enter(5, (8.0, 9.0));
        assert!(f.on(4).is_none());
        f.leave();
        f.motion((1.0, 2.0));
        assert!(f.on(5).is_none());
    }

    #[test]
    fn software_cursor_has_visible_arrow_and_transparent_surroundings() {
        for scale in [1, 2, 3] {
            let px = arrow_pixels(scale);
            assert_eq!(px.len(), (32 * scale * 32 * scale) as usize);
            assert!(px.iter().any(|p| p >> 24 != 0));
            assert!(px.iter().any(|p| *p == 0xFFFF_FFFF));
            assert_eq!(px[px.len() - 1], 0);
            assert_eq!(px[0], 0);
        }
    }
}
