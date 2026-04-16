//! Serves the two shipping frontend files (`index.html` + `main.js`)
//! embedded into the binary via `include_bytes!`. We embed rather than
//! filesystem-serve so the Pi systemd unit doesn't need a paired `public/`
//! directory next to the binary — one file deploy, no runtime path surprises.
//!
//! `public/assets/` (Kenney 1-Bit) and `public/picker.html` are NOT embedded:
//! per `CLAUDE.md`, procedural pixel-art is the shipping path. The picker and
//! tile pack are scaffolding for a future swap.

use axum::http::{header, HeaderValue, StatusCode};
use axum::response::{IntoResponse, Response};

// Embedded at compile time from the repo's `public/` directory. CARGO_MANIFEST_DIR
// is `rust/workplacesim/`, so we step up twice.
const INDEX_HTML: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/index.html"));
const MAIN_JS: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/main.js"));

pub async fn index() -> Response {
    static_response(INDEX_HTML, "text/html; charset=utf-8")
}

pub async fn main_js() -> Response {
    static_response(MAIN_JS, "text/javascript; charset=utf-8")
}

fn static_response(bytes: &'static [u8], content_type: &'static str) -> Response {
    (
        StatusCode::OK,
        [
            (header::CONTENT_TYPE, HeaderValue::from_static(content_type)),
            (
                header::CACHE_CONTROL,
                HeaderValue::from_static("no-cache, no-transform"),
            ),
        ],
        bytes,
    )
        .into_response()
}

