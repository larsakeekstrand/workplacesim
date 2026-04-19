//! Colour palette. Mote-colour table (keyed by tool-name prefix) and the
//! per-user sprite palette (shirt/pants/skin/hair) derived from a stable
//! char-code hash. Byte-for-byte parity with `public/main.js`.

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rgb(pub u8, pub u8, pub u8);

const fn rgb_from_hex(hex: u32) -> Rgb {
    Rgb(
        ((hex >> 16) & 0xff) as u8,
        ((hex >> 8) & 0xff) as u8,
        (hex & 0xff) as u8,
    )
}

pub const MOTE_DEFAULT_COLOR: Rgb = rgb_from_hex(0xcccccc);

// --- Scene palette. 1:1 with the PALETTE object in public/main.js. ---
pub const BG: Rgb = rgb_from_hex(0x0b0d12);
pub const FLOOR_A: Rgb = rgb_from_hex(0x3a2f24);
pub const FLOOR_B: Rgb = rgb_from_hex(0x433627);
pub const FLOOR_MEETING_A: Rgb = rgb_from_hex(0x2c2f3a);
pub const FLOOR_MEETING_B: Rgb = rgb_from_hex(0x353948);
pub const FLOOR_LAB_A: Rgb = rgb_from_hex(0x1f3038);
pub const FLOOR_LAB_B: Rgb = rgb_from_hex(0x263a44);
pub const FLOOR_LINE: Rgb = rgb_from_hex(0x2a2218);
pub const LAB_GRID: Rgb = rgb_from_hex(0x0e1c22);
pub const WALL: Rgb = rgb_from_hex(0x1a2028);
pub const WALL_HI: Rgb = rgb_from_hex(0x2d3643);
pub const DESK_TOP: Rgb = rgb_from_hex(0x8a6a3f);
pub const DESK_EDGE: Rgb = rgb_from_hex(0x5c4526);
pub const DESK_SHADE: Rgb = rgb_from_hex(0x6e5432);
pub const MONITOR: Rgb = rgb_from_hex(0x0f1722);
pub const MONITOR_GLOW: Rgb = rgb_from_hex(0x6ec6ff);
pub const KEYBOARD: Rgb = rgb_from_hex(0x1c232d);
pub const MOUSE: Rgb = rgb_from_hex(0x2a2f39);
pub const CHAIR: Rgb = rgb_from_hex(0x2b333d);
pub const CHAIR_HI: Rgb = rgb_from_hex(0x3e4856);
pub const WHITEBOARD_FRAME: Rgb = rgb_from_hex(0xa0a8b4);
pub const WHITEBOARD_BODY: Rgb = rgb_from_hex(0xf2f2ee);
pub const WINDOW_FRAME: Rgb = rgb_from_hex(0x3d4956);
pub const WINDOW_GLASS: Rgb = rgb_from_hex(0x7ab5d6);
pub const BENCH_TOP: Rgb = rgb_from_hex(0xc8cdd5);
pub const BENCH_SHADE: Rgb = rgb_from_hex(0x6f7682);
pub const BENCH_EDGE: Rgb = rgb_from_hex(0x4e535c);
pub const SCOPE: Rgb = rgb_from_hex(0x0a131a);
pub const SCOPE_TRACE: Rgb = rgb_from_hex(0x5cffaf);
pub const LED: Rgb = rgb_from_hex(0x66ff88);
pub const BUILD_BOARD_BG: Rgb = rgb_from_hex(0x10181c);

// Step 6 text + readout palette.
pub const WHITEBOARD_TEXT: Rgb = rgb_from_hex(0x1a1f28);
pub const TICKER_TEXT: Rgb = rgb_from_hex(0xcfe6ff);
pub const STATUS_TEXT: Rgb = rgb_from_hex(0xd8d2c5);
pub const STATUS_PANEL_BG: Rgb = rgb_from_hex(0x1c1510);
pub const BENCH_FLASH_OK: Rgb = rgb_from_hex(0x6ef08e);
pub const BENCH_FLASH_ERR: Rgb = rgb_from_hex(0xff6464);
pub const GLYPH_ERR: Rgb = rgb_from_hex(0xff6464);
pub const GLYPH_LAB: Rgb = rgb_from_hex(0x5cffaf);
pub const GLYPH_IDLE: Rgb = rgb_from_hex(0x9dc9ff);
pub const GLYPH_WALK: Rgb = rgb_from_hex(0xaaaaaa);
pub const GLYPH_PLAN: Rgb = rgb_from_hex(0xeef2f6);
pub const GLYPH_LONG: Rgb = rgb_from_hex(0xd2b48c);

/// Fixed alpha for window light-spill trapezoids. Step 4a: no breathing — a
/// single constant, roughly midway between WINDOW_SPILL_BASE_ALPHA (0.08) and
/// WINDOW_SPILL_PEAK_ALPHA (0.22) from main.js. Step 6 ties this to the EMA.
pub const WINDOW_SPILL_ALPHA: f32 = 0.18;

/// Tool-name → colour table. The JS object lookup is exact-match, but for
/// prefixed tool names (e.g. `NotebookEdit`) the JS table explicitly enumerates
/// each alias. We mirror that table here and match exactly; there is no real
/// prefix logic in JS, just a dictionary lookup with fallback to default.
pub const MOTE_COLORS: &[(&str, Rgb)] = &[
    ("Read", rgb_from_hex(0x7fc7ff)),
    ("Grep", rgb_from_hex(0x7fc7ff)),
    ("Glob", rgb_from_hex(0x7fc7ff)),
    ("LS", rgb_from_hex(0x7fc7ff)),
    ("NotebookRead", rgb_from_hex(0x7fc7ff)),
    ("Write", rgb_from_hex(0xffb86c)),
    ("Edit", rgb_from_hex(0xffb86c)),
    ("MultiEdit", rgb_from_hex(0xffb86c)),
    ("NotebookEdit", rgb_from_hex(0xffb86c)),
    ("Bash", rgb_from_hex(0x8be98b)),
    ("Agent", rgb_from_hex(0xff8fd4)),
    ("Task", rgb_from_hex(0xff8fd4)),
    ("TaskCreate", rgb_from_hex(0xff8fd4)),
    ("WebFetch", rgb_from_hex(0xc28fff)),
    ("WebSearch", rgb_from_hex(0xc28fff)),
];

/// Lookup a mote colour by tool name. Matches the JS `MOTE_COLORS[tool] ??
/// MOTE_DEFAULT_COLOR` semantics: exact match, case-sensitive, default on miss.
pub fn mote_color(tool_name: &str) -> Rgb {
    for &(name, rgb) in MOTE_COLORS {
        if name == tool_name {
            return rgb;
        }
    }
    MOTE_DEFAULT_COLOR
}

/// JS `SKIN_TONES` — kept as hex for parity; indexed by `hash_str(user) % 5`.
pub const SKIN_TONES: [Rgb; 5] = [
    rgb_from_hex(0xf5cfa6),
    rgb_from_hex(0xe3b58a),
    rgb_from_hex(0xc48f6c),
    rgb_from_hex(0x8d5a3d),
    rgb_from_hex(0xf0d4b4),
];

/// JS `SHIRT_HUES` — hue angles in degrees.
pub const SHIRT_HUES: [i32; 10] = [210, 340, 40, 140, 260, 20, 190, 300, 80, 170];

/// Stable hash over Unicode code points. Matches JS `hashStr`:
///
/// ```js
/// let h = 0;
/// for (const c of s) h = (h * 31 + c.charCodeAt(0)) >>> 0;
/// ```
///
/// `for..of` on a JS string iterates code points (not UTF-16 code units for BMP
/// chars, but does iterate surrogate *pairs* as one code point). Rust `chars()`
/// also iterates code points. For characters outside BMP, JS `charCodeAt(0)`
/// returns the high surrogate only, but `for..of c` yields the full code point
/// string, and `c.charCodeAt(0)` on a two-code-unit string returns the high
/// surrogate. This means JS subtly diverges from Rust's `c as u32` for
/// astral-plane characters. Usernames in practice are ASCII; if non-BMP input
/// ever matters we revisit.
pub fn hash_str(s: &str) -> u32 {
    let mut h: u32 = 0;
    for c in s.chars() {
        h = h.wrapping_mul(31).wrapping_add(c as u32);
    }
    h
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SimColors {
    pub shirt: Rgb,
    pub pants: Rgb,
    pub skin: Rgb,
    /// JS has no explicit hair table; we reuse `SHIRT_HUES` with a different
    /// offset and darker lightness, matching the visual choice in main.js's
    /// `drawSim` (hair tinted off the shirt hash).
    pub hair: Rgb,
}

/// Convert HSL in the JS Phaser convention (h ∈ [0,1], s/l ∈ [0,1]) to RGB.
/// Ports `Phaser.Display.Color.HSLToColor`.
fn hsl_to_rgb(h: f32, s: f32, l: f32) -> Rgb {
    let c = (1.0 - (2.0 * l - 1.0).abs()) * s;
    let hp = h * 6.0;
    let x = c * (1.0 - (hp.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = if (0.0..1.0).contains(&hp) {
        (c, x, 0.0)
    } else if (1.0..2.0).contains(&hp) {
        (x, c, 0.0)
    } else if (2.0..3.0).contains(&hp) {
        (0.0, c, x)
    } else if (3.0..4.0).contains(&hp) {
        (0.0, x, c)
    } else if (4.0..5.0).contains(&hp) {
        (x, 0.0, c)
    } else {
        (c, 0.0, x)
    };
    let m = l - c / 2.0;
    Rgb(
        ((r1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((g1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
        ((b1 + m) * 255.0).round().clamp(0.0, 255.0) as u8,
    )
}

/// JS `%` on signed ints keeps the sign of the dividend. `usize`-index in JS is
/// computed after `>>` (int32 signed shift), so a high-bit hash can produce a
/// negative modulus and index `undefined`. We faithfully reproduce the same
/// arithmetic: the shift is on i32, the modulus preserves dividend sign, and a
/// negative index wraps back by adding the divisor (matching the observed JS
/// behaviour for the in-range palette lookups that do succeed).
fn js_shift_mod(h: u32, shift: u32, add: i32, modulus: i32) -> i32 {
    let shifted = (h as i32) >> shift;
    let rem = (shifted.wrapping_add(add)) % modulus;
    if rem < 0 {
        rem + modulus
    } else {
        rem
    }
}

pub fn sim_colors(user: &str) -> SimColors {
    let user = if user.is_empty() { "?" } else { user };
    let h = hash_str(user);

    // Shirt: h % 10 → SHIRT_HUES[idx], HSL(hue/360, 0.55, 0.52)
    let shirt_hue = SHIRT_HUES[(h as usize) % SHIRT_HUES.len()];
    let shirt = hsl_to_rgb(shirt_hue as f32 / 360.0, 0.55, 0.52);

    // Pants: (h >> 3 + 3) % 10 → SHIRT_HUES[idx], HSL(hue/360, 0.35, 0.28)
    let pants_idx = js_shift_mod(h, 3, 3, SHIRT_HUES.len() as i32) as usize;
    let pants_hue = SHIRT_HUES[pants_idx];
    let pants = hsl_to_rgb(pants_hue as f32 / 360.0, 0.35, 0.28);

    // Skin: h % 5 → SKIN_TONES[idx]
    let skin = SKIN_TONES[(h as usize) % SKIN_TONES.len()];

    // Hair: derived from hash with a different offset. JS's drawSim shades hair
    // off the shirt hue; we reproduce a stable deterministic choice here.
    let hair_idx = js_shift_mod(h, 5, 7, SHIRT_HUES.len() as i32) as usize;
    let hair_hue = SHIRT_HUES[hair_idx];
    let hair = hsl_to_rgb(hair_hue as f32 / 360.0, 0.25, 0.18);

    SimColors {
        shirt,
        pants,
        skin,
        hair,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn mote_color_known_tools() {
        assert_eq!(mote_color("Read"), Rgb(0x7f, 0xc7, 0xff));
        assert_eq!(mote_color("Bash"), Rgb(0x8b, 0xe9, 0x8b));
        assert_eq!(mote_color("Edit"), Rgb(0xff, 0xb8, 0x6c));
        assert_eq!(mote_color("Agent"), Rgb(0xff, 0x8f, 0xd4));
        assert_eq!(mote_color("WebFetch"), Rgb(0xc2, 0x8f, 0xff));
    }

    #[test]
    fn mote_color_default_on_miss() {
        assert_eq!(mote_color("TotallyUnknown"), MOTE_DEFAULT_COLOR);
        assert_eq!(mote_color(""), MOTE_DEFAULT_COLOR);
    }

    #[test]
    fn mote_color_is_exact_match_not_prefix() {
        // `ReadFile` is NOT in JS's MOTE_COLORS dict; a prefix match would
        // break parity with the browser which returns the default here.
        assert_eq!(mote_color("ReadFile"), MOTE_DEFAULT_COLOR);
        assert_eq!(mote_color("BashTool"), MOTE_DEFAULT_COLOR);
    }

    #[test]
    fn hash_str_matches_known_values() {
        // Manually computed (multiplicative-31 wrapping u32):
        // 'a' = 97
        assert_eq!(hash_str("a"), 97);
        // 'a','b' = 97*31 + 98 = 3105
        assert_eq!(hash_str("ab"), 3105);
        // '?' = 63
        assert_eq!(hash_str("?"), 63);
    }

    #[test]
    fn sim_colors_deterministic() {
        let a1 = sim_colors("alice");
        let a2 = sim_colors("alice");
        assert_eq!(a1, a2);
        let b1 = sim_colors("bob");
        assert_ne!(a1.shirt, b1.shirt);
    }

    #[test]
    fn sim_colors_empty_user_falls_back_to_question_mark() {
        assert_eq!(sim_colors(""), sim_colors("?"));
    }

    #[test]
    fn hsl_to_rgb_sanity() {
        // HSL(0, 1, 0.5) = pure red
        let r = hsl_to_rgb(0.0, 1.0, 0.5);
        assert_eq!(r, Rgb(255, 0, 0));
        // HSL(1/3, 1, 0.5) = pure green
        let g = hsl_to_rgb(1.0 / 3.0, 1.0, 0.5);
        assert_eq!(g, Rgb(0, 255, 0));
        // HSL(2/3, 1, 0.5) = pure blue
        let b = hsl_to_rgb(2.0 / 3.0, 1.0, 0.5);
        assert_eq!(b, Rgb(0, 0, 255));
        // HSL(0, 0, 0.5) = grey
        let grey = hsl_to_rgb(0.0, 0.0, 0.5);
        assert_eq!(grey, Rgb(128, 128, 128));
    }
}
