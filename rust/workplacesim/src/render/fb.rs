//! /dev/fb0 framebuffer backend. Renders the same `RenderFrame` produced by
//! `scene::*` to whatever resolution the Pi's HDMI pipe reports, with a 2x
//! nearest-neighbour blit and black letterboxing.
//!
//! Only mmap + ioctl + VT code is Linux-gated. The pure pixel-format
//! conversions and letterbox math live outside the cfg so they can be unit
//! tested on any host.

use super::geometry::Rect;
use super::Framebuffer;
use super::RenderFrame;

/// Detected framebuffer pixel layout. The Pi legacy stack supports either
/// 16bpp RGB565 or 32bpp XRGB8888 via `framebuffer_depth=` in `/boot/config.txt`;
/// anything else is rejected with a pointer to that knob.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PixelFormat {
    /// 16bpp: `((r & 0xf8) << 8) | ((g & 0xfc) << 3) | (b >> 3)`, little-endian u16.
    Rgb565,
    /// 32bpp: byte order B, G, R, X in memory (matches `FBIOGET_VSCREENINFO`
    /// offsets red=16, green=8, blue=0 on the Pi). "X" is ignored-on-write.
    Xrgb8888,
}

impl PixelFormat {
    #[inline]
    pub fn bytes_per_pixel(self) -> usize {
        match self {
            PixelFormat::Rgb565 => 2,
            PixelFormat::Xrgb8888 => 4,
        }
    }
}

/// Geometry describing where the 1280x720 scaled render frame lands inside
/// the physical fb. With a 1280x720 fb the offsets are (0, 0); with 1920x1080
/// they're (320, 180) for centre-letterbox; smaller-than-scaled fbs are
/// clamped (we accept truncation rather than downscaling — the Pi fb is
/// always configured to at least 1280x720 in practice).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Letterbox {
    pub dst_x: i32,
    pub dst_y: i32,
    pub visible_w: i32,
    pub visible_h: i32,
}

/// Compute letterbox placement of a `src_w x src_h` source inside a
/// `target_w x target_h` fb. Source is assumed to be already-scaled (i.e.
/// post-2x). Centres along both axes; clamps to the fb if source exceeds
/// target.
pub fn compute_letterbox(src_w: i32, src_h: i32, target_w: i32, target_h: i32) -> Letterbox {
    let visible_w = src_w.min(target_w).max(0);
    let visible_h = src_h.min(target_h).max(0);
    let dst_x = ((target_w - visible_w) / 2).max(0);
    let dst_y = ((target_h - visible_h) / 2).max(0);
    Letterbox {
        dst_x,
        dst_y,
        visible_w,
        visible_h,
    }
}

/// Pack a single RGB triple into RGB565 little-endian.
/// Bit layout (MSB → LSB): `RRRRRGGG GGGBBBBB`; stored as two bytes in
/// little-endian so the low byte goes first.
#[inline]
pub fn pack_rgb565(r: u8, g: u8, b: u8) -> [u8; 2] {
    let v: u16 = (((r as u16) & 0xf8) << 8) | (((g as u16) & 0xfc) << 3) | ((b as u16) >> 3);
    v.to_le_bytes()
}

/// Pack a single RGB triple into XRGB8888 (alpha byte = 0xff). Byte order in
/// memory is B, G, R, X — matches the Pi's VSCREENINFO red=16/green=8/blue=0
/// offsets with little-endian u32 store.
#[inline]
pub fn pack_xrgb8888(r: u8, g: u8, b: u8) -> [u8; 4] {
    [b, g, r, 0xff]
}

/// 2x nearest-neighbour upscale + pixel-format conversion + letterbox blit
/// into an arbitrary backing buffer. `dst` is the whole fb; `stride` is its
/// bytes-per-row. Source is the `RenderFrame` (`RENDER_W x RENDER_H` packed
/// RGB). Each source pixel becomes a 2x2 block in dst.
///
/// If `letterbox` is `None`, blit from (0,0) assuming the fb exactly matches
/// `2 * RENDER_W x 2 * RENDER_H`.
pub fn blit_scale2x(
    src: &RenderFrame,
    dst: &mut [u8],
    stride: usize,
    format: PixelFormat,
    letterbox: Letterbox,
) {
    let sw = src.width() as usize;
    let sh = src.height() as usize;
    let bpp = format.bytes_per_pixel();
    let bytes = src.rgb_bytes();

    // Each source row writes 2 dst rows, each source column writes 2 dst cols.
    // visible_w / visible_h are already clipped to the target; we still clamp
    // the per-pixel loop defensively so a misconfigured letterbox can't write
    // past the fb buffer end.
    let max_x_pairs = ((letterbox.visible_w as usize) / 2).min(sw);
    let max_y_pairs = ((letterbox.visible_h as usize) / 2).min(sh);

    for sy in 0..max_y_pairs {
        let dy0 = letterbox.dst_y as usize + sy * 2;
        let dy1 = dy0 + 1;
        let row0_base = dy0 * stride + (letterbox.dst_x as usize) * bpp;
        let row1_base = dy1 * stride + (letterbox.dst_x as usize) * bpp;
        for sx in 0..max_x_pairs {
            let i = (sy * sw + sx) * 3;
            let r = bytes[i];
            let g = bytes[i + 1];
            let b = bytes[i + 2];
            let col_off = sx * 2 * bpp;
            let off0 = row0_base + col_off;
            let off1 = row1_base + col_off;
            match format {
                PixelFormat::Rgb565 => {
                    let p = pack_rgb565(r, g, b);
                    dst[off0] = p[0];
                    dst[off0 + 1] = p[1];
                    dst[off0 + 2] = p[0];
                    dst[off0 + 3] = p[1];
                    dst[off1] = p[0];
                    dst[off1 + 1] = p[1];
                    dst[off1 + 2] = p[0];
                    dst[off1 + 3] = p[1];
                }
                PixelFormat::Xrgb8888 => {
                    let p = pack_xrgb8888(r, g, b);
                    dst[off0..off0 + 4].copy_from_slice(&p);
                    dst[off0 + 4..off0 + 8].copy_from_slice(&p);
                    dst[off1..off1 + 4].copy_from_slice(&p);
                    dst[off1 + 4..off1 + 8].copy_from_slice(&p);
                }
            }
        }
    }
}

/// 2x scale + format-convert only the pixels inside `dirty_rect` (in source
/// coords). Destination coordinates are offset by `letterbox`. Used by the
/// Pi backend to avoid blitting 3.5 MB of unchanged pixels every frame.
pub fn blit_scale2x_rect(
    src: &RenderFrame,
    dst: &mut [u8],
    stride: usize,
    format: PixelFormat,
    letterbox: Letterbox,
    dirty_rect: Rect,
) {
    let sw = src.width() as i32;
    let sh = src.height() as i32;
    let bpp = format.bytes_per_pixel();
    let bytes = src.rgb_bytes();

    // Clip dirty_rect to the source frame bounds so we can't walk past the
    // source buffer end for any reason.
    let x0 = dirty_rect.x.max(0);
    let y0 = dirty_rect.y.max(0);
    let x1 = (dirty_rect.x + dirty_rect.w).min(sw);
    let y1 = (dirty_rect.y + dirty_rect.h).min(sh);
    if x0 >= x1 || y0 >= y1 {
        return;
    }

    for sy in y0..y1 {
        let dy0 = letterbox.dst_y + sy * 2;
        let dy1 = dy0 + 1;
        // If the dst row is outside the fb, skip.
        if dy1 < 0 || dy0 * (stride as i32) + (letterbox.dst_x * bpp as i32) < 0 {
            continue;
        }
        let row0_base = (dy0 as usize) * stride + (letterbox.dst_x as usize) * bpp;
        let row1_base = (dy1 as usize) * stride + (letterbox.dst_x as usize) * bpp;
        for sx in x0..x1 {
            let i = ((sy as usize) * (sw as usize) + sx as usize) * 3;
            let r = bytes[i];
            let g = bytes[i + 1];
            let b = bytes[i + 2];
            let col_off = (sx as usize) * 2 * bpp;
            let off0 = row0_base + col_off;
            let off1 = row1_base + col_off;
            match format {
                PixelFormat::Rgb565 => {
                    let p = pack_rgb565(r, g, b);
                    dst[off0] = p[0];
                    dst[off0 + 1] = p[1];
                    dst[off0 + 2] = p[0];
                    dst[off0 + 3] = p[1];
                    dst[off1] = p[0];
                    dst[off1 + 1] = p[1];
                    dst[off1 + 2] = p[0];
                    dst[off1 + 3] = p[1];
                }
                PixelFormat::Xrgb8888 => {
                    let p = pack_xrgb8888(r, g, b);
                    dst[off0..off0 + 4].copy_from_slice(&p);
                    dst[off0 + 4..off0 + 8].copy_from_slice(&p);
                    dst[off1..off1 + 4].copy_from_slice(&p);
                    dst[off1 + 4..off1 + 8].copy_from_slice(&p);
                }
            }
        }
    }
}

/// Fill the fb with black. Called once at startup to clear whatever console
/// text or prior rendering was there.
pub fn clear_fb(dst: &mut [u8], stride: usize, format: PixelFormat, target_w: i32, target_h: i32) {
    let bpp = format.bytes_per_pixel();
    for y in 0..target_h as usize {
        let base = y * stride;
        for x in 0..target_w as usize {
            let off = base + x * bpp;
            match format {
                PixelFormat::Rgb565 => {
                    dst[off] = 0;
                    dst[off + 1] = 0;
                }
                PixelFormat::Xrgb8888 => {
                    dst[off] = 0;
                    dst[off + 1] = 0;
                    dst[off + 2] = 0;
                    dst[off + 3] = 0xff;
                }
            }
        }
    }
}

// --- Linux-only hardware path -------------------------------------------------

#[cfg(all(feature = "fb", target_os = "linux"))]
mod linux_impl {
    use super::*;

    use std::fs::{File, OpenOptions};
    use std::os::fd::{AsRawFd, RawFd};
    use std::path::Path;
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use memmap2::{MmapMut, MmapOptions};
    use tokio::sync::broadcast;

    use crate::render::dirty::DirtyTracker;
    use crate::render::fx_store::FxStore;
    use crate::render::sim_store::SimStore;
    use crate::render::world::RenderWorld;
    use crate::render::{palette, scene, RenderFrame, RENDER_H, RENDER_W};
    use crate::server::Shared;
    use crate::state::{clock, Event};

    const TARGET_FPS: u64 = 30;
    const SCALED_W: i32 = (RENDER_W * 2) as i32;
    const SCALED_H: i32 = (RENDER_H * 2) as i32;

    // Linux ioctl request numbers — from <linux/fb.h> and <linux/kd.h>.
    // Hard-coded so we don't need bindgen: these are stable ABI numbers that
    // haven't changed in decades.
    const FBIOGET_VSCREENINFO: libc::c_ulong = 0x4600;
    const FBIOGET_FSCREENINFO: libc::c_ulong = 0x4602;
    const KDSETMODE: libc::c_ulong = 0x4B3A;
    const KD_TEXT: libc::c_ulong = 0x00;
    const KD_GRAPHICS: libc::c_ulong = 0x01;

    // `struct fb_var_screeninfo` from <linux/fb.h>. We only read the fields we
    // need; the full struct is 40 u32s long.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct FbBitfield {
        offset: u32,
        length: u32,
        msb_right: u32,
    }

    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct FbVarScreeninfo {
        xres: u32,
        yres: u32,
        xres_virtual: u32,
        yres_virtual: u32,
        xoffset: u32,
        yoffset: u32,
        bits_per_pixel: u32,
        grayscale: u32,
        red: FbBitfield,
        green: FbBitfield,
        blue: FbBitfield,
        transp: FbBitfield,
        nonstd: u32,
        activate: u32,
        height: u32,
        width: u32,
        accel_flags: u32,
        pixclock: u32,
        left_margin: u32,
        right_margin: u32,
        upper_margin: u32,
        lower_margin: u32,
        hsync_len: u32,
        vsync_len: u32,
        sync: u32,
        vmode: u32,
        rotate: u32,
        colorspace: u32,
        reserved: [u32; 4],
    }

    // `struct fb_fix_screeninfo`. We only need line_length + smem_len.
    #[repr(C)]
    #[derive(Clone, Copy, Debug, Default)]
    struct FbFixScreeninfo {
        id: [u8; 16],
        smem_start: libc::c_ulong,
        smem_len: u32,
        type_: u32,
        type_aux: u32,
        visual: u32,
        xpanstep: u16,
        ypanstep: u16,
        ywrapstep: u16,
        _pad: u16,
        line_length: u32,
        mmio_start: libc::c_ulong,
        mmio_len: u32,
        accel: u32,
        capabilities: u16,
        reserved: [u16; 2],
    }

    /// RAII guard that puts the tty in KD_GRAPHICS and restores KD_TEXT on
    /// drop. Also fires on SIGINT/SIGTERM via a shared atomic flag the run
    /// loop polls.
    pub struct VtGuard {
        fd: RawFd,
        _file: File,
    }

    impl VtGuard {
        pub fn enter() -> anyhow::Result<Self> {
            // Try /dev/tty0 first (current VT); fall back to /dev/tty1. On
            // systemd console=tty1 is standard but tty0 resolves to the active
            // vt which is friendlier when user SSHes in.
            let candidates = ["/dev/tty0", "/dev/tty1"];
            let mut last_err: Option<std::io::Error> = None;
            for path in candidates {
                match OpenOptions::new().read(true).write(true).open(path) {
                    Ok(f) => {
                        let fd = f.as_raw_fd();
                        // SAFETY: fd is owned by `f` which outlives this call,
                        // KDSETMODE takes an integer arg, and KD_GRAPHICS is a
                        // valid mode.
                        let rc = unsafe { libc::ioctl(fd, KDSETMODE, KD_GRAPHICS) };
                        if rc < 0 {
                            last_err = Some(std::io::Error::last_os_error());
                            continue;
                        }
                        return Ok(VtGuard { fd, _file: f });
                    }
                    Err(e) => {
                        last_err = Some(e);
                    }
                }
            }
            Err(anyhow::anyhow!(
                "VtGuard: unable to open any of /dev/tty0 /dev/tty1 in rw+graphics mode: {:?}",
                last_err
            ))
        }
    }

    impl Drop for VtGuard {
        fn drop(&mut self) {
            // SAFETY: fd is still owned by the guarded File; ioctl arg is a
            // valid mode constant.
            unsafe { libc::ioctl(self.fd, KDSETMODE, KD_TEXT) };
        }
    }

    /// Opened framebuffer: mmap'd memory, detected pixel format, resolution,
    /// and the letterbox that places our 1280x720 scaled frame inside the fb.
    pub struct FbBackend {
        pub mmap: MmapMut,
        pub format: PixelFormat,
        pub stride: usize,
        pub target_w: i32,
        pub target_h: i32,
        pub letterbox: Letterbox,
    }

    impl FbBackend {
        pub fn open(path: &Path) -> anyhow::Result<Self> {
            let file = OpenOptions::new()
                .read(true)
                .write(true)
                .open(path)
                .map_err(|e| anyhow::anyhow!("open {}: {e}", path.display()))?;
            let fd = file.as_raw_fd();

            // SAFETY: both ioctls take a pointer to a correctly-sized struct;
            // we zero-init before passing so the kernel only reads padding it
            // explicitly ignores. FbVarScreeninfo and FbFixScreeninfo have the
            // same repr as the kernel ABI.
            let mut vinfo: FbVarScreeninfo = FbVarScreeninfo::default();
            let mut finfo: FbFixScreeninfo = FbFixScreeninfo::default();
            let rc_v = unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO, &mut vinfo as *mut _) };
            if rc_v < 0 {
                return Err(anyhow::anyhow!(
                    "FBIOGET_VSCREENINFO: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let rc_f = unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO, &mut finfo as *mut _) };
            if rc_f < 0 {
                return Err(anyhow::anyhow!(
                    "FBIOGET_FSCREENINFO: {}",
                    std::io::Error::last_os_error()
                ));
            }

            let format = match (vinfo.bits_per_pixel, vinfo.red.offset, vinfo.green.offset, vinfo.blue.offset) {
                (16, 11, 5, 0) => PixelFormat::Rgb565,
                (32, 16, 8, 0) => PixelFormat::Xrgb8888,
                (32, 0, 8, 16) => PixelFormat::Xrgb8888,
                (bpp, ro, go, bo) => {
                    return Err(anyhow::anyhow!(
                        "unsupported fb format bpp={} r@{} g@{} b@{}. Edit /boot/config.txt and set \
                         `framebuffer_depth=16` (RGB565) or `framebuffer_depth=32` (XRGB8888).",
                        bpp, ro, go, bo
                    ))
                }
            };

            let target_w = vinfo.xres as i32;
            let target_h = vinfo.yres as i32;
            let stride = finfo.line_length as usize;
            let buf_len = stride * (vinfo.yres as usize);

            // SAFETY: we mmap exactly `smem_len`-bounded `buf_len` bytes that
            // the kernel guarantees are backed by the fb. The returned MmapMut
            // borrows the file which is kept alive via the File in this
            // struct's caller (we drop `file` at end of open, but memmap2
            // dup's the fd internally).
            let mmap = unsafe {
                MmapOptions::new()
                    .len(buf_len)
                    .map_mut(&file)
                    .map_err(|e| anyhow::anyhow!("mmap fb: {e}"))?
            };

            let letterbox = compute_letterbox(SCALED_W, SCALED_H, target_w, target_h);

            tracing::info!(
                "fb: {}x{} {:?} stride={} letterbox=({},{})+{}x{}",
                target_w, target_h, format, stride,
                letterbox.dst_x, letterbox.dst_y, letterbox.visible_w, letterbox.visible_h,
            );

            Ok(FbBackend {
                mmap,
                format,
                stride,
                target_w,
                target_h,
                letterbox,
            })
        }

        pub fn clear(&mut self) {
            clear_fb(&mut self.mmap[..], self.stride, self.format, self.target_w, self.target_h);
        }

        pub fn blit_full(&mut self, src: &RenderFrame) {
            blit_scale2x(src, &mut self.mmap[..], self.stride, self.format, self.letterbox);
        }

        pub fn blit_rect(&mut self, src: &RenderFrame, rect: Rect) {
            blit_scale2x_rect(src, &mut self.mmap[..], self.stride, self.format, self.letterbox, rect);
        }
    }

    /// Install SIGINT/SIGTERM handlers that flip a shared flag. The run loop
    /// polls the flag and exits gracefully, letting `VtGuard` drop and restore
    /// the tty. Uses signal-hook-style atomics — no async runtime involved.
    fn install_signals(flag: Arc<AtomicBool>) -> anyhow::Result<()> {
        use nix::sys::signal::{sigaction, SaFlags, SigAction, SigHandler, SigSet, Signal};

        // SAFETY: the handler only does relaxed atomic writes + no heap; safe
        // from signal context. Flag is a leaked Arc so the raw pointer stays
        // valid for process lifetime.
        static mut GLOBAL_FLAG: *const AtomicBool = std::ptr::null();
        unsafe {
            GLOBAL_FLAG = Arc::into_raw(flag);
        }

        extern "C" fn handler(_: libc::c_int) {
            // SAFETY: GLOBAL_FLAG is set once before sigaction is installed.
            unsafe {
                if !GLOBAL_FLAG.is_null() {
                    (*GLOBAL_FLAG).store(true, Ordering::Relaxed);
                }
            }
        }

        let action = SigAction::new(SigHandler::Handler(handler), SaFlags::empty(), SigSet::empty());
        // SAFETY: handler is async-signal-safe (see above).
        unsafe {
            sigaction(Signal::SIGINT, &action)?;
            sigaction(Signal::SIGTERM, &action)?;
        }
        Ok(())
    }

    /// Main /dev/fb0 render loop. Mirrors `desktop::run_desktop` but writes to
    /// the mmap'd fb instead of a minifb window, and only blits dirty rects.
    pub fn run_fb(state: Shared, mut rx: broadcast::Receiver<Event>) -> anyhow::Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));
        install_signals(shutdown.clone())?;

        // VT first — if this fails, bail before touching the fb.
        let _vt = VtGuard::enter().map_err(|e| anyhow::anyhow!("VT guard: {e}. Try running as root or granting the user tty access."))?;

        let mut fb = FbBackend::open(Path::new("/dev/fb0"))?;
        fb.clear();

        let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
        let mut store = SimStore::new();
        let mut fx = FxStore::new();
        let mut tracker = DirtyTracker::new();
        let mut last_tick = Instant::now();
        let frame_budget = Duration::from_millis(1000 / TARGET_FPS);
        let mut first_frame = true;

        while !shutdown.load(Ordering::Relaxed) {
            let t0 = Instant::now();
            let now_ms = clock::now_ms();

            let world = {
                let s = state.read();
                RenderWorld::from_state(&s, now_ms)
            };
            {
                let mut s = state.write();
                s.tick(now_ms);
            }

            store.reconcile(&world);
            let dt_ms = last_tick.elapsed().as_millis() as u64;
            last_tick = t0;
            store.tick(dt_ms, now_ms);
            fx.drain_events(&mut rx, &store, now_ms);
            fx.tick(now_ms, &mut store);

            // Redraw the whole frame CPU-side (cheap on Pi 1 — we only touch
            // ~900 KB here, the bandwidth win is in NOT writing 3.5 MB of
            // scaled bytes to the fb mmap every frame).
            frame.clear(palette::BG);
            scene::draw_static_background(&mut frame);
            scene::effects::draw_below(&mut frame, &fx, &store, now_ms);
            scene::sim::draw_sims(&mut frame, &store);
            scene::effects::draw_above(&mut frame, &fx, &store, now_ms);

            // First frame: full-frame blit so the static background appears.
            // Subsequent frames: only the dynamic AABBs + last-frame AABBs
            // (so we erase vacated sim positions back to background).
            if first_frame {
                fb.blit_full(&frame);
                // Still call step so tracker.last captures current AABBs.
                let _ = tracker.step(&store, &fx);
                first_frame = false;
            } else {
                let dirties = tracker.step(&store, &fx);
                for r in &dirties {
                    fb.blit_rect(&frame, *r);
                }
            }

            let elapsed = t0.elapsed();
            if elapsed < frame_budget {
                std::thread::sleep(frame_budget - elapsed);
            }
        }

        tracing::info!("fb render loop exiting");
        // VtGuard drops here → tty restored to text mode.
        Ok(())
    }
}

#[cfg(all(feature = "fb", target_os = "linux"))]
pub use linux_impl::{run_fb, FbBackend, VtGuard};

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb565_pure_colors() {
        // Pure red: 0xf800 in RGB565 → bytes [0x00, 0xf8] little-endian.
        assert_eq!(pack_rgb565(0xff, 0, 0), [0x00, 0xf8]);
        // Pure green: 0x07e0 → [0xe0, 0x07].
        assert_eq!(pack_rgb565(0, 0xff, 0), [0xe0, 0x07]);
        // Pure blue: 0x001f → [0x1f, 0x00].
        assert_eq!(pack_rgb565(0, 0, 0xff), [0x1f, 0x00]);
        // Black and white.
        assert_eq!(pack_rgb565(0, 0, 0), [0x00, 0x00]);
        assert_eq!(pack_rgb565(0xff, 0xff, 0xff), [0xff, 0xff]);
    }

    #[test]
    fn rgb565_truncates_low_bits() {
        // Per spec: r >>= 3, g >>= 2, b >>= 3. Low bits are dropped, not rounded.
        // r=0x07 (<0x08, 5-bit = 0) g=0x03 (<0x04, 6-bit = 0) b=0x07 (<0x08 = 0)
        assert_eq!(pack_rgb565(0x07, 0x03, 0x07), [0x00, 0x00]);
        // A mid-grey-ish value.
        assert_eq!(pack_rgb565(0x80, 0x80, 0x80), {
            let v: u16 = ((0x80 & 0xf8) << 8) | ((0x80 & 0xfc) << 3) | (0x80 >> 3);
            v.to_le_bytes()
        });
    }

    #[test]
    fn xrgb8888_byte_order() {
        // Matches the Pi VSCREENINFO red=16/green=8/blue=0 layout, little-endian
        // in memory → [B, G, R, X].
        assert_eq!(pack_xrgb8888(0x11, 0x22, 0x33), [0x33, 0x22, 0x11, 0xff]);
        assert_eq!(pack_xrgb8888(0, 0, 0), [0, 0, 0, 0xff]);
    }

    #[test]
    fn center_letterbox_1280x720_into_1280x720() {
        let lb = compute_letterbox(1280, 720, 1280, 720);
        assert_eq!(lb, Letterbox { dst_x: 0, dst_y: 0, visible_w: 1280, visible_h: 720 });
    }

    #[test]
    fn center_letterbox_1280x720_into_1920x1080() {
        // 1280 in 1920: offset 320. 720 in 1080: offset 180.
        let lb = compute_letterbox(1280, 720, 1920, 1080);
        assert_eq!(lb, Letterbox { dst_x: 320, dst_y: 180, visible_w: 1280, visible_h: 720 });
    }

    #[test]
    fn center_letterbox_clamps_to_smaller_target() {
        // If the fb is somehow 640x480, the source gets clipped to the fb.
        let lb = compute_letterbox(1280, 720, 640, 480);
        assert_eq!(lb, Letterbox { dst_x: 0, dst_y: 0, visible_w: 640, visible_h: 480 });
    }

    #[test]
    fn bytes_per_pixel_matches_format() {
        assert_eq!(PixelFormat::Rgb565.bytes_per_pixel(), 2);
        assert_eq!(PixelFormat::Xrgb8888.bytes_per_pixel(), 4);
    }

    #[test]
    fn blit_scale2x_writes_2x2_blocks_rgb565() {
        // 2x1 source: red, green. After 2x scale + letterbox at (0,0) in a
        // 4x2 rgb565 fb: first two columns red, next two columns green, two
        // rows identical.
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0xff, 0));
        let stride = 4 * 2; // 4 pixels wide * 2 bytes = 8
        let mut dst = vec![0u8; stride * 2];
        let lb = Letterbox { dst_x: 0, dst_y: 0, visible_w: 4, visible_h: 2 };
        blit_scale2x(&src, &mut dst, stride, PixelFormat::Rgb565, lb);

        let red = pack_rgb565(0xff, 0, 0);
        let green = pack_rgb565(0, 0xff, 0);
        // Row 0: red, red, green, green
        assert_eq!(&dst[0..2], &red);
        assert_eq!(&dst[2..4], &red);
        assert_eq!(&dst[4..6], &green);
        assert_eq!(&dst[6..8], &green);
        // Row 1 identical.
        assert_eq!(&dst[8..10], &red);
        assert_eq!(&dst[10..12], &red);
        assert_eq!(&dst[12..14], &green);
        assert_eq!(&dst[14..16], &green);
    }

    #[test]
    fn blit_scale2x_writes_2x2_blocks_xrgb8888() {
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0, 0xff));
        let stride = 4 * 4; // 4 pixels wide * 4 bytes = 16
        let mut dst = vec![0u8; stride * 2];
        let lb = Letterbox { dst_x: 0, dst_y: 0, visible_w: 4, visible_h: 2 };
        blit_scale2x(&src, &mut dst, stride, PixelFormat::Xrgb8888, lb);

        let red = pack_xrgb8888(0xff, 0, 0);
        let blue = pack_xrgb8888(0, 0, 0xff);
        assert_eq!(&dst[0..4], &red);
        assert_eq!(&dst[4..8], &red);
        assert_eq!(&dst[8..12], &blue);
        assert_eq!(&dst[12..16], &blue);
        // Row 1 identical.
        assert_eq!(&dst[16..20], &red);
        assert_eq!(&dst[20..24], &red);
        assert_eq!(&dst[24..28], &blue);
        assert_eq!(&dst[28..32], &blue);
    }

    #[test]
    fn blit_scale2x_respects_letterbox_offset() {
        // 1x1 red source into a 4x4 fb with offset (1,1) → only dst pixels
        // at (1,1),(2,1),(1,2),(2,2) painted red; the rest stays zero.
        let mut src = RenderFrame::new(1, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        let stride = 4 * 4; // 4 wide * 4 bpp
        let mut dst = vec![0u8; stride * 4];
        let lb = Letterbox { dst_x: 1, dst_y: 1, visible_w: 2, visible_h: 2 };
        blit_scale2x(&src, &mut dst, stride, PixelFormat::Xrgb8888, lb);

        let red = pack_xrgb8888(0xff, 0, 0);
        // Check (1,1),(2,1) in row 1.
        let row1 = stride; // y=1
        assert_eq!(&dst[row1 + 4..row1 + 8], &red);
        assert_eq!(&dst[row1 + 8..row1 + 12], &red);
        // (0,1) and (3,1) untouched.
        assert_eq!(&dst[row1..row1 + 4], &[0, 0, 0, 0]);
        assert_eq!(&dst[row1 + 12..row1 + 16], &[0, 0, 0, 0]);
        // Row 0 entirely zero.
        assert_eq!(&dst[0..stride], &[0u8; 16]);
    }

    #[test]
    fn blit_scale2x_rect_only_touches_dirty_region() {
        // 4x4 source, filled non-zero. Only blit source rect (1,1,2,2). The
        // destination should have a 4x4 stamped block at (letterbox + 2*1, 2*1)
        // and everything else zero.
        let mut src = RenderFrame::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                src.set_pixel(x, y, super::super::palette::Rgb(0xff, 0x80, 0x40));
            }
        }
        let stride = 8 * 4; // 8 wide * 4 bpp
        let mut dst = vec![0u8; stride * 8];
        let lb = Letterbox { dst_x: 0, dst_y: 0, visible_w: 8, visible_h: 8 };
        let dirty = Rect::new(1, 1, 2, 2);
        blit_scale2x_rect(&src, &mut dst, stride, PixelFormat::Xrgb8888, lb, dirty);

        let painted = pack_xrgb8888(0xff, 0x80, 0x40);
        let zero = [0u8; 4];

        // Rows 0..2 untouched.
        for y in 0..2 {
            let row = y * stride;
            for x in 0..8 {
                assert_eq!(&dst[row + x * 4..row + x * 4 + 4], &zero, "row {y} col {x}");
            }
        }
        // Rows 2..6, cols 2..6 painted.
        for y in 2..6 {
            let row = y * stride;
            for x in 0..8 {
                let off = row + x * 4;
                let expected = if (2..6).contains(&x) { painted } else { zero };
                assert_eq!(&dst[off..off + 4], &expected, "row {y} col {x}");
            }
        }
        // Rows 6..8 untouched.
        for y in 6..8 {
            let row = y * stride;
            for x in 0..8 {
                assert_eq!(&dst[row + x * 4..row + x * 4 + 4], &zero, "row {y} col {x}");
            }
        }
    }

    #[test]
    fn blit_scale2x_rect_with_out_of_bounds_source_rect_is_noop() {
        let src = RenderFrame::new(4, 4);
        let stride = 8 * 4;
        let mut dst = vec![0u8; stride * 8];
        let lb = Letterbox { dst_x: 0, dst_y: 0, visible_w: 8, visible_h: 8 };
        blit_scale2x_rect(&src, &mut dst, stride, PixelFormat::Xrgb8888, lb, Rect::new(100, 100, 10, 10));
        assert!(dst.iter().all(|&b| b == 0));
    }

    #[test]
    fn clear_fb_xrgb_writes_opaque_black() {
        let stride = 4 * 4;
        let mut dst = vec![0xcc; stride * 2];
        clear_fb(&mut dst, stride, PixelFormat::Xrgb8888, 4, 2);
        // All pixels: [0, 0, 0, 0xff].
        for chunk in dst.chunks_exact(4) {
            assert_eq!(chunk, &[0, 0, 0, 0xff]);
        }
    }

    #[test]
    fn clear_fb_rgb565_writes_zeroes() {
        let stride = 4 * 2;
        let mut dst = vec![0xcc; stride * 2];
        clear_fb(&mut dst, stride, PixelFormat::Rgb565, 4, 2);
        for chunk in dst.chunks_exact(2) {
            assert_eq!(chunk, &[0, 0]);
        }
    }
}
