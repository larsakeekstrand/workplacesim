//! Golden-frame regression for the static scene background. Guards against
//! accidental drift in the procedural-pixel-art port of public/main.js.

use workplacesim::render::{scene, RenderFrame, RENDER_H, RENDER_W};

const GOLDEN_PATH: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/golden/static-bg.raw");

#[test]
fn static_background_matches_golden() {
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame);
    let actual: Vec<u8> = frame.rgb_bytes().to_vec();

    if std::env::var_os("REGEN").is_some() {
        std::fs::write(GOLDEN_PATH, &actual).expect("write golden");
        eprintln!("wrote {GOLDEN_PATH} ({} bytes)", actual.len());
        return;
    }

    let expected = std::fs::read(GOLDEN_PATH).unwrap_or_else(|e| {
        panic!(
            "golden file missing: {GOLDEN_PATH} ({e}). run with REGEN=1 to create."
        )
    });
    assert_eq!(
        actual.len(),
        expected.len(),
        "static background size drift ({} vs {})",
        actual.len(),
        expected.len()
    );
    if actual != expected {
        // Surface the first diverging pixel for quick triage.
        for (i, (a, e)) in actual.iter().zip(expected.iter()).enumerate() {
            if a != e {
                let px = i / 3;
                let x = px as u32 % RENDER_W;
                let y = px as u32 / RENDER_W;
                panic!(
                    "static background drift at pixel ({x},{y}) byte {i}: actual={a} expected={e}. run with REGEN=1 to update."
                );
            }
        }
        unreachable!("lengths equal but contents differ")
    }
}
