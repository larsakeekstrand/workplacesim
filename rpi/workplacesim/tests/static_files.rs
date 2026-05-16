//! `GET /` and `GET /main.js` serve bytes embedded at compile time.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use tower::ServiceExt;
use workplacesim::config::{self, Config, SharedConfig};
use workplacesim::server::AppState;
use workplacesim::{server, state};

fn test_config() -> SharedConfig {
    config::shared(Config::default())
}

fn test_app(state: server::Shared) -> AppState {
    AppState::for_tests(state, test_config())
}

const INDEX_SRC: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../public/index.html"
));
const MAIN_JS_SRC: &[u8] =
    include_bytes!(concat!(env!("CARGO_MANIFEST_DIR"), "/../../public/main.js"));

#[tokio::test]
async fn index_serves_html() {
    let (state, _rx) = state::new_state(test_config());
    let app = server::build_router(test_app(state));

    let req = Request::builder()
        .method("GET")
        .uri("/")
        .body(Body::empty())
        .unwrap();
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
    let (state, _rx) = state::new_state(test_config());
    let app = server::build_router(test_app(state));

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
async fn config_html_serves_embedded_page() {
    // Task #4: `GET /config` should serve the embedded live-tuning HTML page
    // (not the placeholder stub). We look for the marker comment baked into
    // `rust/workplacesim/public/config.html` and for each group heading, so a
    // regression that drops the template or a section fails here instead of
    // on visual inspection.
    let (state, _rx) = state::new_state(test_config());
    let app = server::build_router(test_app(state));

    let req = Request::builder()
        .method("GET")
        .uri("/config")
        .body(Body::empty())
        .unwrap();
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

    // Stable marker — cheap smoke that the real page (not the stub) is wired.
    assert!(
        text.contains("<!-- workplacesim-config-page -->"),
        "missing marker comment in /config response"
    );
    // Each group heading name shows up in both the GROUPS map and the summary
    // text. Spot-checking them catches a regression on the accordion template
    // without snapshotting the whole file.
    for group in ["Motion", "Effects", "Ticker", "Lifecycle", "Display"] {
        assert!(
            text.contains(group),
            "config page missing group heading: {group}"
        );
    }
}

#[tokio::test]
async fn unknown_path_returns_404() {
    let (state, _rx) = state::new_state(test_config());
    let app = server::build_router(test_app(state));

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
    let (state, _rx) = state::new_state(test_config());
    let app = server::build_router(test_app(state));

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
