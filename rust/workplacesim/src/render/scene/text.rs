//! Text surfaces — whiteboard, file-touch ticker, corner status readout, and
//! lab-bench monitor flash overlays. Text is rendered via embedded-graphics'
//! FONT_5X8 / FONT_6X10; all other panel art lives in scene::furniture or
//! scene::rooms.

use embedded_graphics::mono_font::{ascii, MonoTextStyle};
use embedded_graphics::pixelcolor::Rgb888;
use embedded_graphics::prelude::*;
use embedded_graphics::text::Text;

use super::super::fx_store::{FxStore, BENCH_FLASH_MS, FILE_TICK_MS};
use super::super::geometry::{Rect, LAB_STATION_XS, MEETING_ROOM, OPEN_ROOM};
use super::super::palette::{self, Rgb};
use super::super::sim_store::{SimState, SimStore};
use super::super::{blend, Framebuffer, RenderFrame};
use super::h;

/// Max chars on the whiteboard at FONT_5X8 (~5px/char, ~96 px panel).
const WB_MAX_CHARS: usize = 18;

/// Max chars per ticker line; open-plan interior width at FONT_5X8.
const TICKER_MAX_CHARS: usize = 50;

/// Pick the sim whose `session_prompt` populates the whiteboard. Prefer
/// `agent_type == "claude"` (the main session sim) when seated-in-meeting;
/// fall back to any seated meeting sim with a prompt.
fn pick_whiteboard_agent<'a>(
    sim_store: &SimStore,
    agents: &'a [&crate::state::Agent],
) -> Option<&'a crate::state::Agent> {
    let mut fallback: Option<&crate::state::Agent> = None;
    for sim in sim_store.iter() {
        if !matches!(sim.state, SimState::Seated) {
            continue;
        }
        if !matches!(sim.room, super::super::classify::Room::Meeting) {
            continue;
        }
        let Some(agent) = agents.iter().copied().find(|a| a.agent_id == sim.agent_id) else {
            continue;
        };
        let Some(prompt) = agent.session_prompt.as_deref() else {
            continue;
        };
        if prompt.is_empty() {
            continue;
        }
        if agent.agent_type == "claude" {
            return Some(agent);
        }
        if fallback.is_none() {
            fallback = Some(agent);
        }
    }
    fallback
}

pub fn draw_whiteboard(
    fb: &mut RenderFrame,
    sim_store: &SimStore,
    agents: &[&crate::state::Agent],
) {
    let Some(agent) = pick_whiteboard_agent(sim_store, agents) else {
        return;
    };
    let Some(prompt) = agent.session_prompt.as_deref() else {
        return;
    };
    let truncated = truncate(prompt, WB_MAX_CHARS);
    // Panel position — mirrors draw_meeting_room.
    let wb_w = h(200);
    let wb_x = h(MEETING_ROOM.x) + (h(MEETING_ROOM.w) - wb_w) / 2;
    let wb_y = h(MEETING_ROOM.y) + h(10);
    let style = MonoTextStyle::new(&ascii::FONT_5X8, to_rgb888(palette::WHITEBOARD_TEXT));
    let _ = Text::new(
        &truncated,
        Point::new(wb_x + 2, wb_y + 7),
        style,
    )
    .draw(fb);
}

pub fn draw_file_ticker(fb: &mut RenderFrame, fx: &FxStore, now_ms: u64) {
    if fx.file_ticks.is_empty() {
        return;
    }
    let tx = h(OPEN_ROOM.x + 10);
    let ty = h(OPEN_ROOM.y + 20);
    // JS shows newest on top; FxStore keeps oldest first, so reverse.
    for (i, tick) in fx.file_ticks.iter().rev().enumerate().take(3) {
        let age = now_ms.saturating_sub(tick.born_ms) as f32;
        let alpha = (1.0 - age / FILE_TICK_MS as f32).clamp(0.0, 1.0);
        if alpha <= 0.0 {
            continue;
        }
        let short = short_path(&tick.path);
        let text = truncate(&short, TICKER_MAX_CHARS);
        let y = ty + (i as i32) * 9;
        blend_text(fb, &text, tx, y, palette::TICKER_TEXT, alpha);
    }
}

/// "parent/basename" trim. Mirrors `shortPath` in public/main.js.
fn short_path(p: &str) -> String {
    if p.is_empty() {
        return String::new();
    }
    let cleaned = p.replace('\\', "/");
    let trimmed = cleaned.trim_end_matches('/');
    let parts: Vec<&str> = trimmed.split('/').filter(|s| !s.is_empty()).collect();
    match parts.len() {
        0 => String::new(),
        1 => parts[0].to_string(),
        _ => format!("{}/{}", parts[parts.len() - 2], parts[parts.len() - 1]),
    }
}

fn truncate(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        return s.to_string();
    }
    let mut out: String = s.chars().take(max.saturating_sub(1)).collect();
    out.push('~');
    out
}

/// Rendered 4-px-inset from OPEN_ROOM's bottom-right. Only invoked by callers
/// that already decided `idle_since_ms` exceeded the 2 s threshold.
pub fn draw_status_readout(
    fb: &mut RenderFrame,
    now_ms: u64,
    started_at_ms: u64,
    last_session_ended_ms: Option<u64>,
) {
    let lines = [
        format_local_time_hms(),
        format_uptime(now_ms.saturating_sub(started_at_ms)),
        format_last_session(last_session_ended_ms),
    ];
    let style = MonoTextStyle::new(&ascii::FONT_6X10, to_rgb888(palette::STATUS_TEXT));
    // Box to the panel. FONT_6X10 is 6w x 10h per glyph.
    let inner_w = lines.iter().map(|s| s.chars().count()).max().unwrap_or(0) as i32 * 6 + 6;
    let inner_h = lines.len() as i32 * 10 + 4;
    let panel_x = h(OPEN_ROOM.x + OPEN_ROOM.w) - inner_w - 4;
    let panel_y = h(OPEN_ROOM.y + OPEN_ROOM.h) - inner_h - 4;
    fb.fill_rect(
        Rect::new(panel_x, panel_y, inner_w, inner_h),
        palette::STATUS_PANEL_BG,
    );
    for (i, line) in lines.iter().enumerate() {
        let y = panel_y + 10 + (i as i32) * 10;
        let _ = Text::new(line, Point::new(panel_x + 3, y), style).draw(fb);
    }
}

pub fn format_uptime(ms: u64) -> String {
    let total_minutes = ms / 60_000;
    let h = total_minutes / 60;
    let m = total_minutes % 60;
    format!("up {h}h {m}m")
}

fn format_last_session(last: Option<u64>) -> String {
    match last {
        None => "last --".to_string(),
        Some(ms) => {
            let secs = (ms / 1000) as i64;
            let dt = chrono::DateTime::<chrono::Local>::from(
                std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs as u64),
            );
            format!("last {}", dt.format("%H:%M:%S"))
        }
    }
}

fn format_local_time_hms() -> String {
    chrono::Local::now().format("%H:%M:%S").to_string()
}

pub fn draw_bench_flashes(fb: &mut RenderFrame, fx: &FxStore, now_ms: u64) {
    for bf in &fx.bench_flashes {
        let age = now_ms.saturating_sub(bf.born_ms) as f32;
        let t = (age / BENCH_FLASH_MS as f32).clamp(0.0, 1.0);
        let alpha = 0.6 * (1.0 - t);
        if alpha <= 0.0 || bf.station_idx >= LAB_STATION_XS.len() {
            continue;
        }
        let color = if bf.ok {
            palette::BENCH_FLASH_OK
        } else {
            palette::BENCH_FLASH_ERR
        };
        let cx_js = LAB_STATION_XS[bf.station_idx];
        let mw = h(26);
        let mh = h(14).max(4);
        let mx = h(cx_js) - mw / 2;
        let my = h(super::super::geometry::BENCH.y) + 2;
        blend_rect(fb, Rect::new(mx, my, mw, mh), color, alpha);
    }
}

fn blend_rect(fb: &mut RenderFrame, r: Rect, c: Rgb, alpha: f32) {
    let x0 = r.x.max(0);
    let y0 = r.y.max(0);
    let x1 = (r.x + r.w).min(fb.width() as i32);
    let y1 = (r.y + r.h).min(fb.height() as i32);
    for y in y0..y1 {
        for x in x0..x1 {
            let base = fb.get_pixel(x, y);
            fb.set_pixel(x, y, blend(base, c, alpha));
        }
    }
}

fn blend_text(fb: &mut RenderFrame, s: &str, x: i32, y: i32, c: Rgb, alpha: f32) {
    // Render into a local 1-bit mask by comparing pre/post pixel against the
    // floor baseline: cheapest correct path is to paint on a scratch frame then
    // copy blended. For our target widths (<50 chars) this stays sub-ms.
    // Walk glyphs one-shot into fb, then iterate back with a simple blend pass.
    let style = MonoTextStyle::new(&ascii::FONT_5X8, to_rgb888(c));
    let _ = Text::new(s, Point::new(x, y), style).draw(&mut Tinted { fb, c, alpha });
}

// DrawTarget that routes writes through the blend helper. Used only by
// `blend_text` so ticker alpha doesn't punch through the wall.
struct Tinted<'a> {
    fb: &'a mut RenderFrame,
    c: Rgb,
    alpha: f32,
}

impl<'a> OriginDimensions for Tinted<'a> {
    fn size(&self) -> Size {
        self.fb.size()
    }
}

impl<'a> DrawTarget for Tinted<'a> {
    type Color = Rgb888;
    type Error = core::convert::Infallible;

    fn draw_iter<I>(&mut self, pixels: I) -> Result<(), Self::Error>
    where
        I: IntoIterator<Item = Pixel<Self::Color>>,
    {
        for Pixel(p, _) in pixels {
            if p.x < 0 || p.y < 0 || (p.x as u32) >= self.fb.width() || (p.y as u32) >= self.fb.height() {
                continue;
            }
            let base = self.fb.get_pixel(p.x, p.y);
            self.fb.set_pixel(p.x, p.y, blend(base, self.c, self.alpha));
        }
        Ok(())
    }
}

fn to_rgb888(c: Rgb) -> Rgb888 {
    Rgb888::new(c.0, c.1, c.2)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::classify::Room;
    use crate::render::geometry::Point as GPoint;
    use crate::render::sim_store::{SimAnim, SimState};
    use crate::render::{RenderFrame, RENDER_H, RENDER_W};
    use crate::state::Agent;

    fn sim(id: &str, room: Room, state: SimState) -> SimAnim {
        SimAnim {
            agent_id: id.into(),
            session_id: None,
            user: "u".into(),
            permission_mode: "default".into(),
            is_lab: false,
            x: 0.0,
            y: 0.0,
            path: vec![GPoint::new(10, 10)],
            seat: None,
            room,
            state,
            bob_phase: 0.0,
            spawned_at_ms: 0,
            seated_at_ms: Some(0),
            seated_since_ms: Some(0),
            overflow_hash: 0,
            last_footstep_ms: 0,
        }
    }

    #[test]
    fn format_uptime_golden() {
        assert_eq!(format_uptime(0), "up 0h 0m");
        assert_eq!(format_uptime(42 * 60_000), "up 0h 42m");
        assert_eq!(format_uptime(3_600_000 + 42 * 60_000), "up 1h 42m");
        assert_eq!(format_uptime(25 * 3_600_000), "up 25h 0m");
    }

    #[test]
    fn short_path_one_level() {
        assert_eq!(short_path("/Users/laek/source/workplacesim/src/main.rs"), "src/main.rs");
        assert_eq!(short_path("main.rs"), "main.rs");
        assert_eq!(short_path(""), "");
        assert_eq!(short_path("a/b"), "a/b");
        assert_eq!(short_path("foo\\bar\\baz.txt"), "bar/baz.txt");
    }

    #[test]
    fn truncate_with_ellipsis() {
        assert_eq!(truncate("hi", 5), "hi");
        assert_eq!(truncate("hello", 5), "hello");
        assert_eq!(truncate("hellothere", 5), "hell~");
    }

    #[test]
    fn pick_whiteboard_prefers_claude_agent() {
        let mut store = SimStore::new();
        store.anim.insert("sess".into(), sim("sess", Room::Meeting, SimState::Seated));
        store.anim.insert("sub".into(), sim("sub", Room::Meeting, SimState::Seated));
        let claude = Agent {
            agent_id: "sess".into(),
            agent_type: "claude".into(),
            session_prompt: Some("from claude".into()),
            ..Default::default()
        };
        let other = Agent {
            agent_id: "sub".into(),
            agent_type: "verifier".into(),
            session_prompt: Some("from sub".into()),
            ..Default::default()
        };
        let agents = [&other, &claude];
        let picked = pick_whiteboard_agent(&store, &agents).unwrap();
        assert_eq!(picked.agent_id, "sess");
    }

    #[test]
    fn pick_whiteboard_none_when_not_seated() {
        let mut store = SimStore::new();
        store
            .anim
            .insert("sub".into(), sim("sub", Room::Meeting, SimState::WalkingIn));
        let a = Agent {
            agent_id: "sub".into(),
            session_prompt: Some("p".into()),
            ..Default::default()
        };
        let agents = [&a];
        assert!(pick_whiteboard_agent(&store, &agents).is_none());
    }

    #[test]
    fn draw_whiteboard_is_noop_when_no_meeting_sim() {
        let store = SimStore::new();
        let mut fb = RenderFrame::new(RENDER_W, RENDER_H);
        fb.clear(palette::BG);
        let before = fb.rgb_bytes().to_vec();
        draw_whiteboard(&mut fb, &store, &[]);
        assert_eq!(fb.rgb_bytes(), before.as_slice());
    }

    /// The renderer gates the readout via an external `idle_since_ms` tracker
    /// (see `render_loop`s); here we verify the threshold math in isolation.
    #[test]
    fn idle_readout_gates_on_2_second_threshold() {
        // Simulate the gate the renderer uses.
        fn should_draw(now: u64, idle_since: Option<u64>) -> bool {
            match idle_since {
                None => false,
                Some(t) => now.saturating_sub(t) > 2_000,
            }
        }
        assert!(!should_draw(1_000, None));
        assert!(!should_draw(1_500, Some(0)));
        assert!(!should_draw(2_000, Some(0)));
        assert!(should_draw(2_001, Some(0)));
        // Cleared the moment we spawn a sim (idle_since reset to None).
        assert!(!should_draw(9_999, None));
    }

    #[test]
    fn status_readout_paints_panel() {
        let mut fb = RenderFrame::new(RENDER_W, RENDER_H);
        fb.clear(palette::BG);
        draw_status_readout(&mut fb, 3_600_000, 0, Some(0));
        // Confirm the bottom-right corner has non-BG pixels.
        let any_non_bg = (h(OPEN_ROOM.x + OPEN_ROOM.w) - 60..h(OPEN_ROOM.x + OPEN_ROOM.w))
            .any(|x| {
                (h(OPEN_ROOM.y + OPEN_ROOM.h) - 40..h(OPEN_ROOM.y + OPEN_ROOM.h))
                    .any(|y| fb.get_pixel(x, y) != palette::BG)
            });
        assert!(any_non_bg);
    }

    #[test]
    fn format_last_session_handles_none() {
        assert_eq!(format_last_session(None), "last --");
    }
}
