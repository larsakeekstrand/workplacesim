//! Aspect-preserving nearest-neighbour fit math.
//!
//! Pure module — no syscalls, no mmap, no minifb — so it compiles on every
//! host regardless of which renderer feature is active. Used by:
//!
//! - `render::fb` (Linux only) to map the 640x360 render frame onto whatever
//!   resolution `/dev/fb0` reports.
//! - `render::desktop` (macOS/Linux) to map the same render frame into an
//!   arbitrarily sized minifb window. Task #5 (live window recreation)
//!   introduced this as a shared dependency so the desktop blit stops
//!   hardcoding 2x.
//!
//! Computed once per window-size change — the col/row lookup tables allocate
//! two `Vec<u16>` sized to the scaled output, so it's worth caching.

/// Geometry describing where the scaled render frame lands inside the
/// destination buffer. `dst_x`/`dst_y` centre the visible image with black
/// pillarbox/letterbox on any target whose aspect differs from the source.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letterbox {
    pub dst_x: i32,
    pub dst_y: i32,
    pub visible_w: i32,
    pub visible_h: i32,
}

/// Aspect-preserving nearest-neighbour fit. `letterbox` is where the scaled
/// image sits in the dst buffer; `col_src`/`row_src` are per-output-pixel
/// lookup tables into the source frame. Computed once when the target size
/// changes; the hot blit loop is a single table lookup + pack per output
/// pixel with no division.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ScaleFit {
    pub letterbox: Letterbox,
    pub col_src: Vec<u16>,
    pub row_src: Vec<u16>,
}

/// Pick the largest `scaled_w x scaled_h` where both axes preserve the
/// source aspect and fit within the target. Integer 2x / 3x fall out
/// exactly when the target is an integer multiple of the source in both
/// axes; non-integer panels (e.g. 1024x768) get `src_w * 1.6` horizontal
/// with a vertical letterbox.
pub fn compute_scale_fit(src_w: i32, src_h: i32, target_w: i32, target_h: i32) -> ScaleFit {
    let (scaled_w, scaled_h) = if src_w > 0 && src_h > 0 && target_w > 0 && target_h > 0 {
        // Try height-limited first: max vertical fit, then derive width. If
        // the derived width exceeds target_w, switch to width-limited.
        let try_w = (src_w as i64 * target_h as i64) / src_h as i64;
        if try_w <= target_w as i64 {
            (try_w as i32, target_h)
        } else {
            let try_h = (src_h as i64 * target_w as i64) / src_w as i64;
            (target_w, try_h as i32)
        }
    } else {
        (0, 0)
    };
    let scaled_w = scaled_w.max(0);
    let scaled_h = scaled_h.max(0);
    let dst_x = ((target_w - scaled_w) / 2).max(0);
    let dst_y = ((target_h - scaled_h) / 2).max(0);
    let col_src: Vec<u16> = (0..scaled_w)
        .map(|dx| {
            if scaled_w > 0 {
                ((dx as i64 * src_w as i64) / scaled_w as i64) as u16
            } else {
                0
            }
        })
        .collect();
    let row_src: Vec<u16> = (0..scaled_h)
        .map(|dy| {
            if scaled_h > 0 {
                ((dy as i64 * src_h as i64) / scaled_h as i64) as u16
            } else {
                0
            }
        })
        .collect();
    ScaleFit {
        letterbox: Letterbox {
            dst_x,
            dst_y,
            visible_w: scaled_w,
            visible_h: scaled_h,
        },
        col_src,
        row_src,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scale_fit_exact_2x_1280x720() {
        let fit = compute_scale_fit(640, 360, 1280, 720);
        assert_eq!(
            fit.letterbox,
            Letterbox {
                dst_x: 0,
                dst_y: 0,
                visible_w: 1280,
                visible_h: 720
            }
        );
        // Integer 2x: col_src[0]=0, col_src[1]=0, col_src[2]=1, col_src[3]=1, ...
        assert_eq!(fit.col_src[0], 0);
        assert_eq!(fit.col_src[1], 0);
        assert_eq!(fit.col_src[2], 1);
        assert_eq!(fit.col_src[3], 1);
        assert_eq!(*fit.col_src.last().unwrap(), 639);
        assert_eq!(*fit.row_src.last().unwrap(), 359);
    }

    #[test]
    fn scale_fit_exact_3x_1920x1080() {
        let fit = compute_scale_fit(640, 360, 1920, 1080);
        assert_eq!(
            fit.letterbox,
            Letterbox {
                dst_x: 0,
                dst_y: 0,
                visible_w: 1920,
                visible_h: 1080
            }
        );
    }

    #[test]
    fn scale_fit_1024x768_is_pillarbox_free_and_vertically_letterboxed() {
        let fit = compute_scale_fit(640, 360, 1024, 768);
        assert_eq!(
            fit.letterbox,
            Letterbox {
                dst_x: 0,
                dst_y: 96,
                visible_w: 1024,
                visible_h: 576
            }
        );
        assert_eq!(fit.col_src[0], 0);
        assert_eq!(*fit.col_src.last().unwrap(), 639);
        assert_eq!(fit.col_src.len(), 1024);
        assert_eq!(fit.row_src.len(), 576);
    }

    #[test]
    fn scale_fit_clamps_both_axes_on_tiny_fb() {
        let fit = compute_scale_fit(640, 360, 640, 480);
        assert_eq!(
            fit.letterbox,
            Letterbox {
                dst_x: 0,
                dst_y: 60,
                visible_w: 640,
                visible_h: 360
            }
        );
    }

    #[test]
    fn scale_fit_zero_dimensions_safe() {
        let fit = compute_scale_fit(640, 360, 0, 720);
        assert_eq!(fit.letterbox.visible_w, 0);
        assert_eq!(fit.letterbox.visible_h, 0);
        assert!(fit.col_src.is_empty());
        assert!(fit.row_src.is_empty());
    }
}
