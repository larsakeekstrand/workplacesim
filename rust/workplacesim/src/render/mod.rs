//! Pure layout / routing / classification logic and the framebuffer primitives
//! that draw it. The `classify`, `geometry`, `palette`, `routing` modules are
//! 1:1 ports of `public/main.js`; the new `scene` + backend modules here let a
//! headless `RenderFrame` — or the minifb desktop window — draw the same world.

pub mod classify;
pub mod dirty;
pub mod fx_store;
pub mod geometry;
pub mod palette;
pub mod routing;
pub mod scene;
pub mod sim_store;
pub mod world;

#[cfg(feature = "desktop")]
pub mod desktop;

// Framebuffer backend: pure blit functions build on any host so they can be
// unit tested from macOS; hardware path (mmap, ioctls, VT) is Linux-only.
#[cfg(feature = "fb")]
pub mod fb;

pub use classify::{classify, Room, LAB_KEYWORDS};
pub use geometry::{
    desk_seats, lab_stations, meeting_seats, DeskSeat, DoorV, LabStation, MeetingSeat,
    MeetingSide, Point, Rect, Spill, Table, WindowRec, APPROACH_OFFSET_Y, BENCH, CORRIDOR_YS,
    DESK_COLS, DESK_H, DESK_ROWS, DESK_W, DOOR, HALLWAY_LEFT_X, HALLWAY_RIGHT_X, LAB_DOOR,
    LAB_QUEUE_SPOTS, LAB_ROOM, LAB_STATION_XS, MEETING_DOOR, MEETING_QUEUE_SPOTS, MEETING_ROOM,
    NORTH_CORRIDOR_Y, OPEN_ROOM, OUTSIDE_X, QUEUE_SPOTS, SEAT_OFFSET_Y, TABLE, TABLE_NORTH_Y,
    TABLE_SOUTH_Y, WALL_THICKNESS, WINDOWS, WINDOW_H, WINDOW_W, WORLD_H, WORLD_W,
};
pub use palette::{
    hash_str, mote_color, sim_colors, Rgb, SimColors, MOTE_COLORS, MOTE_DEFAULT_COLOR, SHIRT_HUES,
    SKIN_TONES,
};
pub use routing::{
    compute_route, nearest_corridor_y, on_corridor, path_between_hall_nodes, path_from_door_to,
    path_to_door_from, staging_for_target, target_approach_waypoints, Target,
};

/// Internal render resolution. The physical window (and /dev/fb0 in step 5)
/// is `2 * RENDER_W x 2 * RENDER_H`; each pixel in the render frame becomes a
/// 2x2 block on screen. This halves the 1280x640 JS world but keeps parity in
/// layout since `WORLD_W = 2 * RENDER_W`; scene code draws into the half-size
/// frame by halving incoming coordinates.
pub const RENDER_W: u32 = 640;
pub const RENDER_H: u32 = 360;

/// Framebuffer primitives. `scene::*` only touches pixels through this trait,
/// so the same draw code feeds both `RenderFrame` (CPU buffer) and any future
/// backend (e.g. direct /dev/fb0 writes with a different stride / byte order).
pub trait Framebuffer {
    fn width(&self) -> u32;
    fn height(&self) -> u32;
    fn set_pixel(&mut self, x: i32, y: i32, c: Rgb);
    fn fill_rect(&mut self, r: Rect, c: Rgb);
    fn stroke_rect(&mut self, r: Rect, c: Rgb);
    fn draw_hline(&mut self, x0: i32, x1: i32, y: i32, c: Rgb);
    fn draw_vline(&mut self, x: i32, y0: i32, y1: i32, c: Rgb);
    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, c: Rgb);
    fn draw_line(&mut self, a: Point, b: Point, c: Rgb);
}

/// Alpha blend `over` onto `base` with `alpha` in [0,1]. Used to fake the
/// trapezoidal window light-spill: the renderer is tight-packed RGB (no alpha
/// channel), so scene code reads the floor tile, blends, writes back.
pub fn blend(base: Rgb, over: Rgb, alpha: f32) -> Rgb {
    let a = alpha.clamp(0.0, 1.0);
    let inv = 1.0 - a;
    Rgb(
        (base.0 as f32 * inv + over.0 as f32 * a).round().clamp(0.0, 255.0) as u8,
        (base.1 as f32 * inv + over.1 as f32 * a).round().clamp(0.0, 255.0) as u8,
        (base.2 as f32 * inv + over.2 as f32 * a).round().clamp(0.0, 255.0) as u8,
    )
}

/// In-memory RGB framebuffer, tight-packed (3 bytes per pixel, row-major).
pub struct RenderFrame {
    width: u32,
    height: u32,
    buf: Vec<u8>,
}

impl RenderFrame {
    pub fn new(width: u32, height: u32) -> Self {
        let len = (width as usize) * (height as usize) * 3;
        Self {
            width,
            height,
            buf: vec![0; len],
        }
    }

    pub fn rgb_bytes(&self) -> &[u8] {
        &self.buf
    }

    pub fn clear(&mut self, c: Rgb) {
        for chunk in self.buf.chunks_exact_mut(3) {
            chunk[0] = c.0;
            chunk[1] = c.1;
            chunk[2] = c.2;
        }
    }

    pub fn get_pixel(&self, x: i32, y: i32) -> Rgb {
        if x < 0 || y < 0 || (x as u32) >= self.width || (y as u32) >= self.height {
            return Rgb(0, 0, 0);
        }
        let i = ((y as usize) * self.width as usize + x as usize) * 3;
        Rgb(self.buf[i], self.buf[i + 1], self.buf[i + 2])
    }

    #[inline]
    fn idx(&self, x: u32, y: u32) -> usize {
        ((y * self.width) + x) as usize * 3
    }
}

impl Framebuffer for RenderFrame {
    fn width(&self) -> u32 {
        self.width
    }
    fn height(&self) -> u32 {
        self.height
    }

    fn set_pixel(&mut self, x: i32, y: i32, c: Rgb) {
        if x < 0 || y < 0 {
            return;
        }
        let (xu, yu) = (x as u32, y as u32);
        if xu >= self.width || yu >= self.height {
            return;
        }
        let i = self.idx(xu, yu);
        self.buf[i] = c.0;
        self.buf[i + 1] = c.1;
        self.buf[i + 2] = c.2;
    }

    fn fill_rect(&mut self, r: Rect, c: Rgb) {
        let x0 = r.x.max(0);
        let y0 = r.y.max(0);
        let x1 = (r.x + r.w).min(self.width as i32);
        let y1 = (r.y + r.h).min(self.height as i32);
        if x0 >= x1 || y0 >= y1 {
            return;
        }
        for y in y0..y1 {
            for x in x0..x1 {
                let i = self.idx(x as u32, y as u32);
                self.buf[i] = c.0;
                self.buf[i + 1] = c.1;
                self.buf[i + 2] = c.2;
            }
        }
    }

    fn stroke_rect(&mut self, r: Rect, c: Rgb) {
        if r.w <= 0 || r.h <= 0 {
            return;
        }
        self.draw_hline(r.x, r.x + r.w - 1, r.y, c);
        self.draw_hline(r.x, r.x + r.w - 1, r.y + r.h - 1, c);
        self.draw_vline(r.x, r.y, r.y + r.h - 1, c);
        self.draw_vline(r.x + r.w - 1, r.y, r.y + r.h - 1, c);
    }

    fn draw_hline(&mut self, x0: i32, x1: i32, y: i32, c: Rgb) {
        let (xa, xb) = if x0 <= x1 { (x0, x1) } else { (x1, x0) };
        for x in xa..=xb {
            self.set_pixel(x, y, c);
        }
    }

    fn draw_vline(&mut self, x: i32, y0: i32, y1: i32, c: Rgb) {
        let (ya, yb) = if y0 <= y1 { (y0, y1) } else { (y1, y0) };
        for y in ya..=yb {
            self.set_pixel(x, y, c);
        }
    }

    fn fill_circle(&mut self, cx: i32, cy: i32, r: i32, c: Rgb) {
        if r <= 0 {
            return;
        }
        let r2 = r * r;
        for dy in -r..=r {
            let dy2 = dy * dy;
            for dx in -r..=r {
                if dx * dx + dy2 <= r2 {
                    self.set_pixel(cx + dx, cy + dy, c);
                }
            }
        }
    }

    fn draw_line(&mut self, a: Point, b: Point, c: Rgb) {
        // Standard Bresenham.
        let mut x0 = a.x;
        let mut y0 = a.y;
        let x1 = b.x;
        let y1 = b.y;
        let dx = (x1 - x0).abs();
        let sx = if x0 < x1 { 1 } else { -1 };
        let dy = -(y1 - y0).abs();
        let sy = if y0 < y1 { 1 } else { -1 };
        let mut err = dx + dy;
        loop {
            self.set_pixel(x0, y0, c);
            if x0 == x1 && y0 == y1 {
                break;
            }
            let e2 = 2 * err;
            if e2 >= dy {
                err += dy;
                x0 += sx;
            }
            if e2 <= dx {
                err += dx;
                y0 += sy;
            }
        }
    }
}

// embedded-graphics adapter. Lets text + sprite draws target `RenderFrame`
// directly; draws routed through `set_pixel` so clipping + byte layout match
// the rest of the scene code.
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;

impl OriginDimensions for RenderFrame {
    fn size(&self) -> Size {
        Size::new(self.width, self.height)
    }
}

impl DrawTarget for RenderFrame {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(point, color) in pixels {
            self.set_pixel(
                point.x,
                point.y,
                Rgb(color.r(), color.g(), color.b()),
            );
        }
        Ok(())
    }
}

/// Nearest-neighbour 2x upscale. Writes a 1280x720 (or generally `2*src_w` x
/// `2*src_h`) ARGB buffer packed as `(0xff << 24) | (r << 16) | (g << 8) | b`
/// — the encoding minifb expects.
pub fn nearest_neighbour_blit_2x(src: &RenderFrame, dst: &mut [u32]) {
    let sw = src.width as usize;
    let sh = src.height as usize;
    let dw = sw * 2;
    debug_assert_eq!(dst.len(), dw * sh * 2);
    let bytes = src.rgb_bytes();
    for y in 0..sh {
        for x in 0..sw {
            let i = (y * sw + x) * 3;
            let pixel = 0xff00_0000
                | ((bytes[i] as u32) << 16)
                | ((bytes[i + 1] as u32) << 8)
                | (bytes[i + 2] as u32);
            let row0 = y * 2 * dw + x * 2;
            let row1 = row0 + dw;
            dst[row0] = pixel;
            dst[row0 + 1] = pixel;
            dst[row1] = pixel;
            dst[row1 + 1] = pixel;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn render_frame_set_and_get_pixel() {
        let mut f = RenderFrame::new(4, 4);
        f.set_pixel(1, 2, Rgb(10, 20, 30));
        assert_eq!(f.get_pixel(1, 2), Rgb(10, 20, 30));
        assert_eq!(f.get_pixel(0, 0), Rgb(0, 0, 0));
        // OOB is silently ignored.
        f.set_pixel(-1, 0, Rgb(1, 2, 3));
        f.set_pixel(10, 0, Rgb(1, 2, 3));
        assert_eq!(f.get_pixel(0, 0), Rgb(0, 0, 0));
    }

    #[test]
    fn fill_rect_clips() {
        let mut f = RenderFrame::new(4, 4);
        f.fill_rect(Rect::new(-2, -2, 4, 4), Rgb(7, 7, 7));
        assert_eq!(f.get_pixel(0, 0), Rgb(7, 7, 7));
        assert_eq!(f.get_pixel(1, 1), Rgb(7, 7, 7));
        assert_eq!(f.get_pixel(2, 2), Rgb(0, 0, 0));
    }

    #[test]
    fn stroke_rect_is_four_lines() {
        let mut f = RenderFrame::new(5, 5);
        f.stroke_rect(Rect::new(0, 0, 5, 5), Rgb(9, 9, 9));
        // Interior untouched.
        assert_eq!(f.get_pixel(2, 2), Rgb(0, 0, 0));
        // Corners + edges painted.
        assert_eq!(f.get_pixel(0, 0), Rgb(9, 9, 9));
        assert_eq!(f.get_pixel(4, 4), Rgb(9, 9, 9));
        assert_eq!(f.get_pixel(2, 0), Rgb(9, 9, 9));
    }

    #[test]
    fn fill_circle_radius_1() {
        let mut f = RenderFrame::new(5, 5);
        f.fill_circle(2, 2, 1, Rgb(4, 4, 4));
        assert_eq!(f.get_pixel(2, 2), Rgb(4, 4, 4));
        assert_eq!(f.get_pixel(1, 2), Rgb(4, 4, 4));
        assert_eq!(f.get_pixel(2, 1), Rgb(4, 4, 4));
        assert_eq!(f.get_pixel(0, 0), Rgb(0, 0, 0));
    }

    #[test]
    fn blend_linear_midpoint() {
        assert_eq!(blend(Rgb(0, 0, 0), Rgb(100, 100, 100), 0.5), Rgb(50, 50, 50));
        assert_eq!(blend(Rgb(200, 200, 200), Rgb(0, 0, 0), 0.0), Rgb(200, 200, 200));
        assert_eq!(blend(Rgb(10, 10, 10), Rgb(20, 20, 20), 1.0), Rgb(20, 20, 20));
    }

    #[test]
    fn blit_2x_doubles_each_pixel() {
        let mut src = RenderFrame::new(2, 2);
        src.set_pixel(0, 0, Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, Rgb(0, 0xff, 0));
        src.set_pixel(0, 1, Rgb(0, 0, 0xff));
        src.set_pixel(1, 1, Rgb(0xff, 0xff, 0));
        let mut dst = vec![0u32; 4 * 4];
        nearest_neighbour_blit_2x(&src, &mut dst);
        // Top-left block all red.
        assert_eq!(dst[0], 0xffff_0000);
        assert_eq!(dst[1], 0xffff_0000);
        assert_eq!(dst[4], 0xffff_0000);
        assert_eq!(dst[5], 0xffff_0000);
        // Top-right green.
        assert_eq!(dst[2], 0xff00_ff00);
        // Bottom-left blue.
        assert_eq!(dst[8], 0xff00_00ff);
        // Bottom-right yellow.
        assert_eq!(dst[10], 0xffff_ff00);
    }
}
