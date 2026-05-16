//! /dev/fb0 framebuffer backend. Renders the same `RenderFrame` produced by
//! `scene::*` to whatever resolution the Pi's HDMI pipe reports, with an
//! aspect-preserving nearest-neighbour blit and black letterboxing. Integer
//! scale (2x, 3x, ...) produces crisp pixels; non-integer scales (e.g. 1.6x
//! for a 1024x768 panel) use a precomputed source-column/row lookup table so
//! the hot loop stays divide-free.
//!
//! Only mmap + ioctl + VT code is Linux-gated. The pure pixel-format
//! conversions and fit math live outside the cfg so they can be unit tested
//! on any host.

use super::fit::ScaleFit;
use super::geometry::Rect;
use super::Framebuffer;
use super::RenderFrame;

// Re-exported for external callers that used to import these from
// `render::fb` — the types moved to `render::fit` in Task #5 so the desktop
// backend (which doesn't compile `fb`) can use them too.
#[allow(unused_imports)]
pub use super::fit::compute_scale_fit;

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

#[inline]
fn write_pixel(dst: &mut [u8], off: usize, format: PixelFormat, r: u8, g: u8, b: u8) {
    match format {
        PixelFormat::Rgb565 => {
            let p = pack_rgb565(r, g, b);
            dst[off] = p[0];
            dst[off + 1] = p[1];
        }
        PixelFormat::Xrgb8888 => {
            let p = pack_xrgb8888(r, g, b);
            dst[off..off + 4].copy_from_slice(&p);
        }
    }
}

/// Nearest-neighbour scale + pixel-format conversion + letterbox blit into
/// an arbitrary backing buffer. `dst` is the whole fb; `stride` is its
/// bytes-per-row. `fit` carries the scaled dst size, centre offset, and the
/// per-pixel src-column/src-row lookup tables — precomputed once so the
/// inner loop is divide-free.
pub fn blit_scale_nn(
    src: &RenderFrame,
    dst: &mut [u8],
    stride: usize,
    format: PixelFormat,
    fit: &ScaleFit,
) {
    let sw = src.width() as usize;
    let bpp = format.bytes_per_pixel();
    let bytes = src.rgb_bytes();
    let lb = fit.letterbox;
    let vis_w = lb.visible_w as usize;
    let vis_h = lb.visible_h as usize;

    for dy in 0..vis_h {
        let sy = fit.row_src[dy] as usize;
        let row_base = (lb.dst_y as usize + dy) * stride + (lb.dst_x as usize) * bpp;
        let src_row_base = sy * sw * 3;
        for dx in 0..vis_w {
            let sx = fit.col_src[dx] as usize;
            let si = src_row_base + sx * 3;
            let r = bytes[si];
            let g = bytes[si + 1];
            let b = bytes[si + 2];
            write_pixel(dst, row_base + dx * bpp, format, r, g, b);
        }
    }
}

/// Nearest-neighbour scale + format-convert only the pixels that sample from
/// inside `dirty_src` (in source coords). Used by the Pi backend to avoid
/// rewriting unchanged pixels each frame.
pub fn blit_scale_nn_rect(
    src: &RenderFrame,
    dst: &mut [u8],
    stride: usize,
    format: PixelFormat,
    fit: &ScaleFit,
    dirty_src: Rect,
) {
    let sw = src.width() as i32;
    let sh = src.height() as i32;
    let bpp = format.bytes_per_pixel();
    let bytes = src.rgb_bytes();
    let lb = fit.letterbox;
    let scaled_w = lb.visible_w;
    let scaled_h = lb.visible_h;
    if scaled_w <= 0 || scaled_h <= 0 {
        return;
    }

    let sx0 = dirty_src.x.max(0);
    let sy0 = dirty_src.y.max(0);
    let sx1 = (dirty_src.x + dirty_src.w).min(sw);
    let sy1 = (dirty_src.y + dirty_src.h).min(sh);
    if sx0 >= sx1 || sy0 >= sy1 {
        return;
    }

    // Map src-rect [sx0,sx1) × [sy0,sy1) to dst-rect [dx_lo,dx_hi) × [dy_lo,dy_hi).
    // col_src[dx] = (dx * sw) / scaled_w (floor). Want all dx with sx0 <= col_src[dx] < sx1.
    //   dx * sw >= sx0 * scaled_w  ⇒  dx >= ceil(sx0 * scaled_w / sw)
    //   dx * sw <  sx1 * scaled_w  ⇒  dx <  ceil(sx1 * scaled_w / sw)  (exclusive)
    let ceil_div = |num: i64, den: i64| -> i32 { ((num + den - 1) / den) as i32 };
    let dx_lo = ceil_div((sx0 as i64) * (scaled_w as i64), sw as i64).max(0);
    let dx_hi = ceil_div((sx1 as i64) * (scaled_w as i64), sw as i64).min(scaled_w);
    let dy_lo = ceil_div((sy0 as i64) * (scaled_h as i64), sh as i64).max(0);
    let dy_hi = ceil_div((sy1 as i64) * (scaled_h as i64), sh as i64).min(scaled_h);
    if dx_lo >= dx_hi || dy_lo >= dy_hi {
        return;
    }

    let sw_usize = sw as usize;
    for dy in dy_lo..dy_hi {
        let sy = fit.row_src[dy as usize] as usize;
        let row_base = (lb.dst_y as usize + dy as usize) * stride + (lb.dst_x as usize) * bpp;
        let src_row_base = sy * sw_usize * 3;
        for dx in dx_lo..dx_hi {
            let sx = fit.col_src[dx as usize] as usize;
            let si = src_row_base + sx * 3;
            let r = bytes[si];
            let g = bytes[si + 1];
            let b = bytes[si + 2];
            write_pixel(dst, row_base + dx as usize * bpp, format, r, g, b);
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

    use crate::config::SharedConfig;
    use crate::render::dirty::DirtyTracker;
    use crate::render::fx_store::{FxLimits, FxStore};
    use crate::render::sim_store::SimStore;
    use crate::render::world::RenderWorld;
    use crate::render::{palette, scene, RenderFrame, RENDER_H, RENDER_W};
    use crate::server::Shared;
    use crate::state::{clock, Event};

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
                        let rc = unsafe { libc::ioctl(fd, KDSETMODE as _, KD_GRAPHICS) };
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
            unsafe { libc::ioctl(self.fd, KDSETMODE as _, KD_TEXT) };
        }
    }

    /// Opened framebuffer: mmap'd memory, detected pixel format, resolution,
    /// and the aspect-preserving scale+letterbox fit that maps our 640x360
    /// render frame to the fb.
    pub struct FbBackend {
        pub mmap: MmapMut,
        pub format: PixelFormat,
        pub stride: usize,
        pub target_w: i32,
        pub target_h: i32,
        pub fit: ScaleFit,
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
            let rc_v = unsafe { libc::ioctl(fd, FBIOGET_VSCREENINFO as _, &mut vinfo as *mut _) };
            if rc_v < 0 {
                return Err(anyhow::anyhow!(
                    "FBIOGET_VSCREENINFO: {}",
                    std::io::Error::last_os_error()
                ));
            }
            let rc_f = unsafe { libc::ioctl(fd, FBIOGET_FSCREENINFO as _, &mut finfo as *mut _) };
            if rc_f < 0 {
                return Err(anyhow::anyhow!(
                    "FBIOGET_FSCREENINFO: {}",
                    std::io::Error::last_os_error()
                ));
            }

            // rustfmt has an idempotency bug on this macro-in-match-arm:
            // running `cargo fmt` alternates between the inline form and the
            // block form. #[rustfmt::skip] pins a single stable shape so
            // `cargo fmt --check` stays clean.
            #[rustfmt::skip]
            let format = match (
                vinfo.bits_per_pixel,
                vinfo.red.offset,
                vinfo.green.offset,
                vinfo.blue.offset,
            ) {
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

            let fit = compute_scale_fit(RENDER_W as i32, RENDER_H as i32, target_w, target_h);

            tracing::info!(
                "fb: {}x{} {:?} stride={} fit={}x{}@({},{})",
                target_w,
                target_h,
                format,
                stride,
                fit.letterbox.visible_w,
                fit.letterbox.visible_h,
                fit.letterbox.dst_x,
                fit.letterbox.dst_y,
            );

            Ok(FbBackend {
                mmap,
                format,
                stride,
                target_w,
                target_h,
                fit,
            })
        }

        pub fn clear(&mut self) {
            clear_fb(
                &mut self.mmap[..],
                self.stride,
                self.format,
                self.target_w,
                self.target_h,
            );
        }

        pub fn blit_full(&mut self, src: &RenderFrame) {
            blit_scale_nn(src, &mut self.mmap[..], self.stride, self.format, &self.fit);
        }

        pub fn blit_rect(&mut self, src: &RenderFrame, rect: Rect) {
            blit_scale_nn_rect(
                src,
                &mut self.mmap[..],
                self.stride,
                self.format,
                &self.fit,
                rect,
            );
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

        let action = SigAction::new(
            SigHandler::Handler(handler),
            SaFlags::empty(),
            SigSet::empty(),
        );
        // SAFETY: handler is async-signal-safe (see above).
        unsafe {
            sigaction(Signal::SIGINT, &action)?;
            sigaction(Signal::SIGTERM, &action)?;
        }
        Ok(())
    }

    /// Main /dev/fb0 render loop. Mirrors `desktop::run_desktop` but writes to
    /// the mmap'd fb instead of a minifb window, and only blits dirty rects.
    pub fn run_fb(
        state: Shared,
        config: SharedConfig,
        rx: broadcast::Receiver<Event>,
    ) -> anyhow::Result<()> {
        run_fb_with_fb_info(state, config, rx, None)
    }

    /// Same as `run_fb`, plus `fb_info` gets populated with panel metrics
    /// after the fb is opened — used by `/api/status` (Task #5).
    pub fn run_fb_with_fb_info(
        state: Shared,
        config: SharedConfig,
        mut rx: broadcast::Receiver<Event>,
        fb_info: Option<std::sync::Arc<parking_lot::RwLock<Option<crate::server::routes::FbInfo>>>>,
    ) -> anyhow::Result<()> {
        let shutdown = Arc::new(AtomicBool::new(false));
        install_signals(shutdown.clone())?;

        // VT first — if this fails, bail before touching the fb.
        let _vt = VtGuard::enter().map_err(|e| {
            anyhow::anyhow!("VT guard: {e}. Try running as root or granting the user tty access.")
        })?;

        let mut fb = FbBackend::open(Path::new("/dev/fb0"))?;
        fb.clear();

        // Publish the detected panel metrics once the fb is up. `/api/status`
        // reads this under a short read-lock. `FbBackend::fit` already carries
        // the aspect-preserving scaled size and letterbox offsets, and
        // `PixelFormat::bytes_per_pixel` × 8 gives the bpp.
        if let Some(ref h) = fb_info {
            let bpp = (fb.format.bytes_per_pixel() * 8) as u8;
            *h.write() = Some(crate::server::routes::FbInfo {
                panel_w: fb.target_w.max(0) as u32,
                panel_h: fb.target_h.max(0) as u32,
                bpp,
                scaled_w: fb.fit.letterbox.visible_w.max(0) as u32,
                scaled_h: fb.fit.letterbox.visible_h.max(0) as u32,
                letterbox_x: fb.fit.letterbox.dst_x.max(0) as u32,
                letterbox_y: fb.fit.letterbox.dst_y.max(0) as u32,
            });
        }

        let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
        let mut store = SimStore::new();
        let mut fx = FxStore::new();
        let mut tracker = DirtyTracker::new();
        let mut last_tick = Instant::now();
        let mut first_frame = true;
        let started_at_ms = clock::now_ms();
        let mut idle_since_ms: Option<u64> = None;
        let mut last_session_ended_ms: Option<u64> = None;

        while !shutdown.load(Ordering::Relaxed) {
            let t0 = Instant::now();
            let now_ms = clock::now_ms();

            // Snapshot per frame under a single short read-lock. Matches the
            // desktop loop — see src/render/desktop.rs for the pattern.
            let (
                fx_limits,
                walk_speed_px_per_sec,
                bob_cycle_ms,
                error_glyph_ms,
                window_spill_alpha,
                target_fps,
            ) = {
                let c = config.read();
                (
                    FxLimits::from_config(&c),
                    c.walk_speed_px_per_sec,
                    c.bob_cycle_ms,
                    c.error_glyph_ms,
                    c.window_spill_alpha,
                    c.target_fps as u64,
                )
            };
            let frame_budget = Duration::from_millis(1000 / target_fps.max(1));

            let (world, agents) = {
                let s = state.read();
                (RenderWorld::from_state(&s, now_ms), s.list_active())
            };
            for a in &agents {
                if a.agent_type == "claude" {
                    if let Some(fa) = a.finished_at {
                        last_session_ended_ms = Some(last_session_ended_ms.unwrap_or(0).max(fa));
                    }
                }
            }
            if agents.is_empty() {
                idle_since_ms.get_or_insert(now_ms);
            } else {
                idle_since_ms = None;
            }
            {
                let mut s = state.write();
                s.tick(now_ms);
            }

            store.reconcile(&world);
            let dt_ms = last_tick.elapsed().as_millis() as u64;
            last_tick = t0;
            store.tick(dt_ms, now_ms, walk_speed_px_per_sec, bob_cycle_ms);
            fx.drain_events(&mut rx, &store, now_ms, &fx_limits);
            fx.tick(now_ms, &mut store, &fx_limits);

            // Redraw the whole frame CPU-side (cheap on Pi 1 — we only touch
            // ~900 KB here, the bandwidth win is in NOT writing 3.5 MB of
            // scaled bytes to the fb mmap every frame).
            frame.clear(palette::BG);
            scene::draw_static_background(&mut frame, window_spill_alpha);
            scene::effects::draw_below(&mut frame, &fx, &store, now_ms, &fx_limits);
            scene::sim::draw_sims(&mut frame, &store);
            let agent_refs: Vec<&crate::state::Agent> = agents.iter().collect();
            scene::glyph::draw_glyphs(&mut frame, &store, &agent_refs, &fx, now_ms, error_glyph_ms);
            scene::effects::draw_above(&mut frame, &fx, &store, now_ms, &fx_limits);
            scene::text::draw_whiteboard(&mut frame, &store, &agent_refs);
            scene::text::draw_file_ticker(&mut frame, &fx, now_ms, &fx_limits);
            scene::text::draw_bench_flashes(&mut frame, &fx, now_ms, &fx_limits);
            if let Some(since) = idle_since_ms {
                if now_ms.saturating_sub(since) > 2_000 {
                    scene::text::draw_status_readout(
                        &mut frame,
                        now_ms,
                        started_at_ms,
                        last_session_ended_ms,
                    );
                }
            }

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
pub use linux_impl::{run_fb, run_fb_with_fb_info, FbBackend, VtGuard};

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::fit::Letterbox;

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

    // scale_fit_* tests were moved to `src/render/fit.rs` along with the
    // ScaleFit/Letterbox/compute_scale_fit types in Task #5 (so the desktop
    // backend — which doesn't compile the fb module — can reuse the fit math).

    #[test]
    fn bytes_per_pixel_matches_format() {
        assert_eq!(PixelFormat::Rgb565.bytes_per_pixel(), 2);
        assert_eq!(PixelFormat::Xrgb8888.bytes_per_pixel(), 4);
    }

    #[test]
    fn blit_scale_nn_writes_2x2_blocks_rgb565() {
        // 2x1 source → 4x2 dst. compute_scale_fit gives exact 2x2 blocks.
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0xff, 0));
        let stride = 4 * 2;
        let mut dst = vec![0u8; stride * 2];
        let fit = compute_scale_fit(2, 1, 4, 2);
        blit_scale_nn(&src, &mut dst, stride, PixelFormat::Rgb565, &fit);

        let red = pack_rgb565(0xff, 0, 0);
        let green = pack_rgb565(0, 0xff, 0);
        assert_eq!(&dst[0..2], &red);
        assert_eq!(&dst[2..4], &red);
        assert_eq!(&dst[4..6], &green);
        assert_eq!(&dst[6..8], &green);
        assert_eq!(&dst[8..10], &red);
        assert_eq!(&dst[10..12], &red);
        assert_eq!(&dst[12..14], &green);
        assert_eq!(&dst[14..16], &green);
    }

    #[test]
    fn blit_scale_nn_writes_2x2_blocks_xrgb8888() {
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0, 0xff));
        let stride = 4 * 4;
        let mut dst = vec![0u8; stride * 2];
        let fit = compute_scale_fit(2, 1, 4, 2);
        blit_scale_nn(&src, &mut dst, stride, PixelFormat::Xrgb8888, &fit);

        let red = pack_xrgb8888(0xff, 0, 0);
        let blue = pack_xrgb8888(0, 0, 0xff);
        assert_eq!(&dst[0..4], &red);
        assert_eq!(&dst[4..8], &red);
        assert_eq!(&dst[8..12], &blue);
        assert_eq!(&dst[12..16], &blue);
        assert_eq!(&dst[16..20], &red);
        assert_eq!(&dst[20..24], &red);
        assert_eq!(&dst[24..28], &blue);
        assert_eq!(&dst[28..32], &blue);
    }

    #[test]
    fn blit_scale_nn_centres_in_letterboxed_fb() {
        // 2x1 source into 4x4 fb → scaled to 4x2, centred vertically at dst_y=1.
        let mut src = RenderFrame::new(2, 1);
        src.set_pixel(0, 0, super::super::palette::Rgb(0xff, 0, 0));
        src.set_pixel(1, 0, super::super::palette::Rgb(0, 0, 0xff));
        let stride = 4 * 4;
        let mut dst = vec![0u8; stride * 4];
        let fit = compute_scale_fit(2, 1, 4, 4);
        assert_eq!(
            fit.letterbox,
            Letterbox {
                dst_x: 0,
                dst_y: 1,
                visible_w: 4,
                visible_h: 2
            }
        );
        blit_scale_nn(&src, &mut dst, stride, PixelFormat::Xrgb8888, &fit);

        // Row 0 (y=0) is letterbox: all zero.
        assert_eq!(&dst[0..stride], &[0u8; 16]);
        // Rows 1 and 2 hold the content.
        let red = pack_xrgb8888(0xff, 0, 0);
        let blue = pack_xrgb8888(0, 0, 0xff);
        for row in [1, 2] {
            let row_off = row * stride;
            assert_eq!(&dst[row_off..row_off + 4], &red);
            assert_eq!(&dst[row_off + 4..row_off + 8], &red);
            assert_eq!(&dst[row_off + 8..row_off + 12], &blue);
            assert_eq!(&dst[row_off + 12..row_off + 16], &blue);
        }
        // Row 3 is letterbox: all zero.
        assert_eq!(&dst[3 * stride..4 * stride], &[0u8; 16]);
    }

    #[test]
    fn blit_scale_nn_1024x768_downscale_does_not_clip() {
        // The regression this feature fixes: 1280-wide content into a
        // 1024-wide fb used to truncate the right 128 px per row. After
        // the fit change, every source column maps into some dst column.
        let src = RenderFrame::new(640, 360);
        let fit = compute_scale_fit(640, 360, 1024, 768);
        // Every source column index appears in col_src[].
        let mut seen = [false; 640];
        for &sx in &fit.col_src {
            seen[sx as usize] = true;
        }
        assert!(seen.iter().all(|&b| b), "not every src column is sampled");
        // The fit doesn't dereference past src bounds. Also validate blit
        // won't panic by running it against a real backing buffer.
        let stride = 1024 * 2;
        let mut dst = vec![0u8; stride * 768];
        blit_scale_nn(&src, &mut dst, stride, PixelFormat::Rgb565, &fit);
    }

    #[test]
    fn blit_scale_nn_rect_only_touches_dirty_region() {
        // 4x4 source at exact 2x scale → 8x8 dst. Dirty src rect (1,1,2,2) covers
        // dst cols 2..6 and rows 2..6.
        let mut src = RenderFrame::new(4, 4);
        for y in 0..4 {
            for x in 0..4 {
                src.set_pixel(x, y, super::super::palette::Rgb(0xff, 0x80, 0x40));
            }
        }
        let stride = 8 * 4;
        let mut dst = vec![0u8; stride * 8];
        let fit = compute_scale_fit(4, 4, 8, 8);
        let dirty = Rect::new(1, 1, 2, 2);
        blit_scale_nn_rect(&src, &mut dst, stride, PixelFormat::Xrgb8888, &fit, dirty);

        let painted = pack_xrgb8888(0xff, 0x80, 0x40);
        let zero = [0u8; 4];
        for y in 0..2 {
            let row = y * stride;
            for x in 0..8 {
                assert_eq!(&dst[row + x * 4..row + x * 4 + 4], &zero, "row {y} col {x}");
            }
        }
        for y in 2..6 {
            let row = y * stride;
            for x in 0..8 {
                let off = row + x * 4;
                let expected = if (2..6).contains(&x) { painted } else { zero };
                assert_eq!(&dst[off..off + 4], &expected, "row {y} col {x}");
            }
        }
        for y in 6..8 {
            let row = y * stride;
            for x in 0..8 {
                assert_eq!(&dst[row + x * 4..row + x * 4 + 4], &zero, "row {y} col {x}");
            }
        }
    }

    #[test]
    fn blit_scale_nn_rect_with_out_of_bounds_source_rect_is_noop() {
        let src = RenderFrame::new(4, 4);
        let stride = 8 * 4;
        let mut dst = vec![0u8; stride * 8];
        let fit = compute_scale_fit(4, 4, 8, 8);
        blit_scale_nn_rect(
            &src,
            &mut dst,
            stride,
            PixelFormat::Xrgb8888,
            &fit,
            Rect::new(100, 100, 10, 10),
        );
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
