Golden files for scene regression tests.

`static-bg.raw` is 640 * 360 * 3 = 691200 bytes of tight-packed RGB. Load it
into an image viewer as raw 640x360 RGB before committing any change.

Regenerate:

    REGEN=1 cargo test --features desktop --no-default-features static_background_matches_golden

Inspect visually via the dump_bg binary:

    cargo run --features desktop --no-default-features --bin dump_bg -- /tmp/wps.png
    open /tmp/wps.png
