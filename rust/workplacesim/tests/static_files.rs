//! `GET /` and `GET /main.js` serve bytes embedded at compile time.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use workplacesim::{server, state};

const INDEX_SRC: &str =
    include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/index.html"));
const MAIN_JS_SRC: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/main.js"));

#[tokio::test]
async fn index_serves_html() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = Request::builder().method("GET").uri("/").body(Body::empty()).unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.starts_with("text/html"), "got content-type {ct}");

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    let text = std::str::from_utf8(&body).unwrap();
    assert!(text.contains("<title>workplacesim</title>"));
    assert!(text.contains(r#"<script type="module" src="./main.js"></script>"#));
    assert_eq!(text, INDEX_SRC, "served index diverges from source");
}

#[tokio::test]
async fn main_js_serves_embedded_payload() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = Request::builder()
        .method("GET")
        .uri("/main.js")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let ct = resp
        .headers()
        .get("content-type")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("");
    assert!(ct.starts_with("text/javascript"), "got content-type {ct}");

    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    assert_eq!(body.len(), MAIN_JS_SRC.len());
    assert_eq!(body.as_ref(), MAIN_JS_SRC);
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    let req = Request::builder()
        .method("GET")
        .uri("/nothing")
        .body(Body::empty())
        .unwrap();
    let resp = app.oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::NOT_FOUND);
}

#[tokio::test]
async fn picker_html_not_served() {
    // CLAUDE.md: picker and the Kenney assets are scaffolding, not the
    // shipping path. They should never be reachable from the Rust server.
    let (state, _rx) = state::new_state();
    let app = server::build_router(state);

    for path in ["/picker.html", "/assets/kenney-1bit/0.png", "/assets/"] {
        let req = Request::builder()
            .method("GET")
            .uri(path)
            .body(Body::empty())
            .unwrap();
        let resp = app.clone().oneshot(req).await.unwrap();
        assert_eq!(
            resp.status(),
            StatusCode::NOT_FOUND,
            "expected 404 for {path}"
        );
    }
}
