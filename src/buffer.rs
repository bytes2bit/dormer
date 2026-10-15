//! Restoring reusable screen buffers without copying the same stale rows twice.

use crate::gfx::Rect;

pub fn should_paint_base(open: bool, dark: bool, probe: bool, has_base: bool) -> bool {
    !has_base || probe || (open && !dark)
}

pub fn replace_base(stale: bool, instant: bool, bent: bool, moved: bool, same_size: bool) -> bool {
    stale || !same_size || ((instant || bent) && !moved)
}

pub fn rebase_on_wake(woke: bool, visible_stale: bool, scene_age: f64) -> bool {
    woke && (visible_stale || scene_age > 2.0)
}

/// Put just these rectangles of `new` into `base`: a part of the picture was repainted and the
/// rest is what it was. Returns the number of pixels written.
pub fn install_regions(base: &mut [u32], new: &[u32], w: usize, regions: &[Rect]) -> usize {
    if w == 0 || base.len() != new.len() {
        return 0;
    }
    let h = base.len() / w;
    let mut written = 0;
    for r in regions {
        let (x0, x1) = (r.x0.clamp(0, w as i32) as usize, r.x1.clamp(0, w as i32) as usize);
        let (y0, y1) = (r.y0.clamp(0, h as i32) as usize, r.y1.clamp(0, h as i32) as usize);
        if x1 <= x0 {
            continue;
        }
        for y in y0..y1 {
            base[y * w + x0..y * w + x1].copy_from_slice(&new[y * w + x0..y * w + x1]);
            written += x1 - x0;
        }
    }
    written
}

/// The rows a set of rectangles touches, as one stale range for a buffer: (first, one past last).
pub fn rows_of(regions: &[Rect], h: usize) -> Option<(usize, usize)> {
    let mut rows: Option<(usize, usize)> = None;
    for r in regions {
        let (a, b) = (r.y0.clamp(0, h as i32) as usize, r.y1.clamp(0, h as i32) as usize);
        if b > a {
            rows = Some(rows.map_or((a, b), |(x, y)| (x.min(a), y.max(b))));
        }
    }
    rows
}

/// Returns the number of pixels copied, useful for checking redundant work.
pub fn restore_pixels(dst: &mut [u32], base: &[u32], w: usize, stale: Option<(usize, usize)>, dirty: &[Rect]) -> usize {
    if w == 0 { return 0; }
    let h = dst.len().min(base.len()) / w;
    let stale = stale.map(|(y0, y1)| (y0.min(h), y1.min(h)));
    let full = stale.is_some_and(|(y0, y1)| y0 == 0 && y1 == h)
        || dirty.iter().any(|r| r.x0 <= 0 && r.y0 <= 0 && r.x1 >= w as i32 && r.y1 >= h as i32);
    if full {
        dst[..w * h].copy_from_slice(&base[..w * h]);
        return w * h;
    }
    let mut copied = 0;
    if let Some((y0, y1)) = stale.filter(|(y0, y1)| y1 > y0) {
        dst[y0 * w..y1 * w].copy_from_slice(&base[y0 * w..y1 * w]);
        copied += (y1 - y0) * w;
    }
    for r in dirty {
        let (x0, x1) = (r.x0.clamp(0, w as i32) as usize, r.x1.clamp(0, w as i32) as usize);
        let (y0, y1) = (r.y0.clamp(0, h as i32) as usize, r.y1.clamp(0, h as i32) as usize);
        if x1 <= x0 { continue; }
        for y in y0..y1 {
            if stale.is_some_and(|(a, b)| a <= y && y < b) { continue; }
            dst[y * w + x0..y * w + x1].copy_from_slice(&base[y * w + x0..y * w + x1]);
            copied += x1 - x0;
        }
    }
    copied
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::gfx::Rect;

    #[test]
    fn full_old_damage_restores_once_even_after_rain_stops() {
        let base = (10..22).collect::<Vec<u32>>();
        let mut dst = vec![99; 12];
        let old = [Rect { x0: 0, y0: 0, x1: 4, y1: 3 }];
        assert_eq!(restore_pixels(&mut dst, &base, 4, Some((1, 2)), &old), 12);
        assert_eq!(dst, base);
    }

    #[test]
    fn clipped_sparse_damage_omits_already_restored_stale_rows() {
        let base = (10..22).collect::<Vec<u32>>();
        let mut dst = vec![99; 12];
        let marks = [Rect { x0: -5, y0: -2, x1: 2, y1: 5 }];
        assert_eq!(restore_pixels(&mut dst, &base, 4, Some((1, 2)), &marks), 8);
        assert_eq!(dst, [10, 11, 99, 99, 14, 15, 16, 17, 18, 19, 99, 99]);
        let before = dst.clone();
        assert_eq!(restore_pixels(&mut dst, &base, 4, None, &[Rect { x0: 9, y0: 0, x1: 12, y1: 4 }]), 0);
        assert_eq!(dst, before);
    }

    #[test]
    fn only_the_asked_for_regions_are_installed() {
        let mut base = vec![1u32; 6 * 4];
        let new: Vec<u32> = (100..124).collect();
        let n = install_regions(&mut base, &new, 6, &[Rect { x0: 1, y0: 1, x1: 3, y1: 3 }, Rect { x0: -4, y0: 3, x1: 2, y1: 99 }, Rect { x0: 4, y0: 0, x1: 4, y1: 4 }]);
        assert_eq!(n, 4 + 2);
        for (i, v) in base.iter().enumerate() {
            let (x, y) = (i % 6, i / 6);
            let inside = ((1..3).contains(&x) && (1..3).contains(&y)) || (x < 2 && y == 3);
            assert_eq!(*v, if inside { new[i] } else { 1 }, "pixel {},{}", x, y);
        }
        assert_eq!(rows_of(&[Rect { x0: 0, y0: 2, x1: 1, y1: 3 }, Rect { x0: 0, y0: 5, x1: 1, y1: 9 }], 8), Some((2, 8)));
        assert_eq!(rows_of(&[], 8), None);
        // a different size is not a picture of the same screen: nothing is written
        assert_eq!(install_regions(&mut base, &new[..12], 6, &[Rect { x0: 0, y0: 0, x1: 6, y1: 2 }]), 0);
    }

    #[test]
    fn hidden_outputs_skip_work_but_initial_mapping_and_resume_paint() {
        assert!(!should_paint_base(false, false, false, true));
        assert!(!should_paint_base(true, true, false, true));
        assert!(should_paint_base(false, false, false, false));
        assert!(should_paint_base(true, false, false, true));
        assert!(should_paint_base(false, true, true, true));
    }

    #[test]
    fn visible_view_changes_fade_and_stale_outputs_resume_immediately() {
        assert!(!replace_base(false, true, false, true, true));
        assert!(replace_base(true, false, false, true, true));
        assert!(rebase_on_wake(true, true, 0.1));
        assert!(!rebase_on_wake(false, true, 20.0));
    }
}
