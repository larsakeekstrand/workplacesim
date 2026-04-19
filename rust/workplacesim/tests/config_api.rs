//! Integration coverage for Task #3's config + status HTTP surface.
//!
//! Uses `axum::body::Body` + `tower::ServiceExt::oneshot` so the tests run
//! without binding a port, matching the pattern in `tests/http.rs`. Config
//! POSTs target a `tempfile::NamedTempFile` path so persistence round-trips
//! without touching the user's real `$XDG_CONFIG_HOME/workplacesim/`.

use axum::body::{to_bytes, Body};
use axum::http::{Request, StatusCode};
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use workplacesim::config::{self, persist::ConfigSource, Config};
use workplacesim::server::{self, AppState};
use workplacesim::state;

fn json_post(uri: &str, body: &str) -> Request<Body> {
    Request::builder()
        .method("POST")
        .uri(uri)
        .header("content-type", "application/json")
        .body(Body::from(body.to_owned()))
        .unwrap()
}

fn get(uri: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(uri)
        .body(Body::empty())
        .unwrap()
}

/// Build an AppState whose config persists to a throwaway tempdir file.
/// Returns the dir too so the caller keeps ownership and the path doesn't
/// get unlinked mid-test.
fn test_app_with_tempdir() -> (AppState, TempDir) {
    let dir = TempDir::new().expect("tempdir");
    let path = dir.path().join("config.json");
    let cfg = config::shared(Config::default());
    let (shared, _rx) = state::new_state(cfg.clone());
    let app = AppState::new(shared, cfg, path, ConfigSource::MissingUsedDefaults);
    (app, dir)
}

async fn body_json(resp: axum::http::Response<Body>) -> Value {
    let body = to_bytes(resp.into_body(), usize::MAX).await.unwrap();
    serde_json::from_slice(&body).unwrap()
}

#[tokio::test]
async fn get_config_returns_defaults_on_fresh_state() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router.oneshot(get("/api/config")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["walk_speed_px_per_sec"].as_f64(), Some(90.0));
    assert_eq!(v["mote_cap"].as_u64(), Some(40));
    assert_eq!(v["target_fps"].as_u64(), Some(30));
}

#[tokio::test]
async fn post_config_merges_single_field() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router
        .oneshot(json_post(
            "/api/config",
            r#"{"walk_speed_px_per_sec": 200.0}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;

    // Top-level shape: { config: {...}, save_error: null }
    assert!(v["save_error"].is_null(), "expected save to succeed");
    assert_eq!(v["config"]["walk_speed_px_per_sec"].as_f64(), Some(200.0));
    // Other fields stay default.
    assert_eq!(v["config"]["mote_cap"].as_u64(), Some(40));
    assert_eq!(v["config"]["target_fps"].as_u64(), Some(30));
}

#[tokio::test]
async fn post_config_clamps_out_of_range_value() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router
        .oneshot(json_post(
            "/api/config",
            r#"{"walk_speed_px_per_sec": 99999.0}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    // Clamp bound is 500.0.
    assert_eq!(v["config"]["walk_speed_px_per_sec"].as_f64(), Some(500.0));
}

#[tokio::test]
async fn post_config_malformed_returns_400_and_leaves_config_alone() {
    let (app, _dir) = test_app_with_tempdir();
    let cfg_handle = app.config.clone();
    let router = server::build_router(app);

    // Wrong type for a numeric field — serde rejects.
    let resp = router
        .oneshot(json_post(
            "/api/config",
            r#"{"walk_speed_px_per_sec": "fast"}"#,
        ))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let v = body_json(resp).await;
    assert!(v["error"].is_string());
    assert!(v["details"].is_string());

    // Config untouched.
    assert_eq!(cfg_handle.read().walk_speed_px_per_sec, 90.0);
}

#[tokio::test]
async fn post_config_non_object_patch_returns_400() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router
        .oneshot(json_post("/api/config", r#"42"#))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    let v = body_json(resp).await;
    assert!(v["error"].as_str().unwrap_or("").contains("invalid patch"));
}

#[tokio::test]
async fn reset_config_returns_defaults() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    // First bump a field away from default.
    let _ = router
        .clone()
        .oneshot(json_post(
            "/api/config",
            r#"{"walk_speed_px_per_sec": 123.0}"#,
        ))
        .await
        .unwrap();

    // Now reset.
    let resp = router
        .oneshot(json_post("/api/config/reset", "{}"))
        .await
        .unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert!(v["save_error"].is_null());
    assert_eq!(v["config"]["walk_speed_px_per_sec"].as_f64(), Some(90.0));
    assert_eq!(v["config"]["mote_cap"].as_u64(), Some(40));
}

#[tokio::test]
async fn get_status_shape_on_empty_state() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router.oneshot(get("/api/status")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;

    assert!(v["uptime_ms"].is_u64(), "uptime_ms should be a u64");
    // `is_u64` already implies >= 0 for serde_json.
    assert_eq!(v["agents"]["total"].as_u64(), Some(0));
    assert!(v["agents"]["by_type"].is_object());
    assert!(v["agents"]["by_room"].is_object());
    assert_eq!(v["agents"]["by_room"]["desk"].as_u64(), Some(0));
    assert_eq!(v["agents"]["by_room"]["meeting"].as_u64(), Some(0));
    assert_eq!(v["agents"]["by_room"]["lab"].as_u64(), Some(0));
    assert_eq!(v["agents"]["by_room"]["transit"].as_u64(), Some(0));
    assert_eq!(v["events_total"].as_u64(), Some(0));
    assert!(v["events_per_min"].is_number());
    assert!(v["build"].is_object());
    assert!(v["build"]["version"].is_string());
    assert!(v["build"]["features"].is_string());
    assert!(v["config_path"].is_string());
    assert!(v["config_source"].is_string());
    assert!(v["fb_info"].is_null(), "fb_info starts null (Task #5)");
}

#[tokio::test]
async fn get_status_counts_agents_after_start() {
    let (app, _dir) = test_app_with_tempdir();
    let shared = app.state.clone();
    let router = server::build_router(app);

    // Start one normal agent and one "verifier" (lab keyword).
    let _ = router
        .clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a1","agent_type":"coder","session_id":"sess"}"#,
        ))
        .await
        .unwrap();
    let _ = router
        .clone()
        .oneshot(json_post(
            "/hooks/subagent-start",
            r#"{"agent_id":"a2","agent_type":"verifier","session_id":"sess"}"#,
        ))
        .await
        .unwrap();

    let resp = router.oneshot(get("/api/status")).await.unwrap();
    let v = body_json(resp).await;
    assert_eq!(v["agents"]["total"].as_u64(), Some(2));
    assert_eq!(v["agents"]["by_type"]["coder"].as_u64(), Some(1));
    assert_eq!(v["agents"]["by_type"]["verifier"].as_u64(), Some(1));
    assert_eq!(v["agents"]["by_room"]["desk"].as_u64(), Some(1));
    assert_eq!(v["agents"]["by_room"]["lab"].as_u64(), Some(1));

    // events_total is nonzero after the two starts (two Start events).
    assert!(v["events_total"].as_u64().unwrap_or(0) >= 2);

    // Sanity: make sure this wasn't hitting a different state somewhere.
    assert_eq!(shared.read().list_active().len(), 2);
}

#[tokio::test]
async fn get_config_bounds_returns_nonempty_map() {
    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    let resp = router.oneshot(get("/api/config/bounds")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert!(v.is_object());
    let obj = v.as_object().unwrap();
    assert!(!obj.is_empty(), "bounds map should not be empty");
    // Spot-check a few known fields.
    assert!(obj.contains_key("walk_speed_px_per_sec"));
    assert!(obj.contains_key("mote_cap"));
    assert!(obj.contains_key("fullscreen"));

    // Numeric bounds carry min/max/default.
    let walk = &v["walk_speed_px_per_sec"];
    assert_eq!(walk["kind"].as_str(), Some("f32"));
    assert_eq!(walk["min"].as_f64(), Some(10.0));
    assert_eq!(walk["max"].as_f64(), Some(500.0));
    assert_eq!(walk["default"].as_f64(), Some(90.0));

    // Boolean bounds carry just default.
    let fs = &v["fullscreen"];
    assert!(fs["default"].is_boolean());
}

/// `POST /api/restart` outside systemd must refuse with 409 and *never* exit
/// the process. Also spot-checks that `GET /api/config/bounds` surfaces the
/// new per-field `restart_required` flag: the display trio is true, a sample
/// motion field is false. We do NOT cover the 202 success branch — that one
/// exits the process, which would tear down the test runner.
#[tokio::test]
async fn post_restart_refuses_outside_systemd_and_bounds_mark_restart_required() {
    // Guard: if a user somehow ran the test suite under systemd, skip. The
    // 202 branch kills the process via `std::process::exit(0)`; we never
    // want that in a test run. CI runs tests directly, not via systemd, so
    // in practice this env var is unset.
    // Safety: setting/removing env vars in tests is racy with other threads
    // that read them. We only *read* in this test (via the handler), so
    // asserting it is unset at the start is enough. We do not mutate env.
    assert!(
        std::env::var_os("INVOCATION_ID").is_none(),
        "INVOCATION_ID must be unset for this test — refuses to run the 202 branch",
    );

    let (app, _dir) = test_app_with_tempdir();
    let router = server::build_router(app);

    // 409 Conflict with an ErrorBody shape. No body required — the handler
    // takes no extractors beyond the state-less default.
    let req = Request::builder()
        .method("POST")
        .uri("/api/restart")
        .body(Body::empty())
        .unwrap();
    let resp = router.clone().oneshot(req).await.unwrap();
    assert_eq!(resp.status(), StatusCode::CONFLICT);
    let v = body_json(resp).await;
    assert!(v["error"].is_string(), "expected error field, got {v}");
    assert!(
        v["error"]
            .as_str()
            .unwrap_or("")
            .contains("not running under systemd"),
        "unexpected error message: {v}",
    );

    // Bounds carry the new flag.
    let resp = router.oneshot(get("/api/config/bounds")).await.unwrap();
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert_eq!(v["window_w"]["restart_required"].as_bool(), Some(true));
    assert_eq!(v["window_h"]["restart_required"].as_bool(), Some(true));
    assert_eq!(v["fullscreen"]["restart_required"].as_bool(), Some(true));
    assert_eq!(
        v["walk_speed_px_per_sec"]["restart_required"].as_bool(),
        Some(false),
    );
}

#[tokio::test]
async fn post_config_with_save_error_still_returns_200_with_save_error_set() {
    // Force a save error by pointing the config_path at a directory that
    // doesn't exist and can't be created. We use the null device as the
    // "parent" so `fs::create_dir_all` on its parent (`/dev`) succeeds but
    // writing to `/dev/null/config.json.tmp` fails because `/dev/null` is
    // a character device, not a directory.
    let cfg = config::shared(Config::default());
    let (shared, _rx) = state::new_state(cfg.clone());
    let bad_path = std::path::PathBuf::from("/dev/null/config.json");
    let app = AppState::new(shared, cfg, bad_path, ConfigSource::MissingUsedDefaults);
    let router = server::build_router(app);

    let resp = router
        .oneshot(json_post(
            "/api/config",
            r#"{"walk_speed_px_per_sec": 150.0}"#,
        ))
        .await
        .unwrap();
    // 200 with a save_error field, NOT 5xx.
    assert_eq!(resp.status(), StatusCode::OK);
    let v = body_json(resp).await;
    assert!(
        v["save_error"].is_string(),
        "expected save_error string, got {v}"
    );
    // The in-memory update still stuck.
    assert_eq!(v["config"]["walk_speed_px_per_sec"].as_f64(), Some(150.0));
}
