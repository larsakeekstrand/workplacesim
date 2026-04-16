//! Render the static scene background once and write a PNG for visual QA.
//! Used by the step 4a verification script; not part of the shipped binary.

use workplacesim::render::{scene, RenderFrame, RENDER_H, RENDER_W};

fn main() -> anyhow::Result<()> {
    let mut frame = RenderFrame::new(RENDER_W, RENDER_H);
    scene::draw_static_background(&mut frame);

    let path = std::env::args()
        .nth(1)
        .unwrap_or_else(|| "/tmp/workplacesim-bg.png".to_string());

    let buf = image::RgbImage::from_raw(RENDER_W, RENDER_H, frame.rgb_bytes().to_vec())
        .ok_or_else(|| anyhow::anyhow!("RgbImage::from_raw — buffer size mismatch"))?;
    buf.save(&path)?;
    println!("wrote {path}");
    Ok(())
}
