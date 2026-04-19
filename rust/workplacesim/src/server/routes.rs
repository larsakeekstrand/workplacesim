//! Handlers for the six hook POST endpoints + `/api/agents`, plus Task #3's
//! `/api/config*` and `/api/status` surfaces. Each handler takes the write
//! lock only long enough to mutate state, then drops it; the state methods
//! themselves own event emission.

use std::collections::BTreeMap;

use axum::extract::State as AxumState;
use axum::http::StatusCode;
use axum::Json;
use serde::Serialize;
use serde_json::{json, Value};

use super::{AppState, BuildInfo, Shared};
use crate::config::{persist, Config, ConfigBounds, SharedConfig};
use crate::render::classify::{classify, Room};
use crate::state::{
    clock, Agent, BufferDescription, Lifecycle, Pretool, StartAgent, StopAgent, ToolEvent,
    VisitRoom,
};

pub async fn pretool(AxumState(state): AxumState<Shared>, Json(body): Json<Pretool>) -> StatusCode {
    // Gate exactly as `server/index.js`: only buffer when the PreToolUse is
    // for the `Agent` tool. Other tool invocations reach this route but are
    // no-ops. `buffer_description` itself drops payloads with no
    // `subagent_type`, so we don't duplicate that check here.
    if body.tool_name.as_deref() == Some("Agent") {
        let now = clock::now_ms();
        let mut guard = state.write();
        guard.buffer_description(
            BufferDescription {
                session_id: body.session_id,
                subagent_type: body.tool_input.subagent_type,
                description: body.tool_input.description,
                tool_use_id: body.tool_use_id,
            },
            now,
        );
    }
    StatusCode::NO_CONTENT
}

pub async fn subagent_start(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<StartAgent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write();
    guard.start_agent(body, now);
    StatusCode::NO_CONTENT
}

pub async fn subagent_stop(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<StopAgent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write();
    guard.stop_agent(body, now);
    StatusCode::NO_CONTENT
}

pub async fn lab_visit(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<VisitRoom>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write();
    guard.visit_room(body, now);
    StatusCode::NO_CONTENT
}

pub async fn tool_event(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<ToolEvent>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write();
    guard.tool_event(body, now);
    StatusCode::NO_CONTENT
}

pub async fn lifecycle(
    AxumState(state): AxumState<Shared>,
    Json(body): Json<Lifecycle>,
) -> StatusCode {
    let now = clock::now_ms();
    let mut guard = state.write();
    guard.handle_lifecycle(body, now);
    StatusCode::NO_CONTENT
}

pub async fn list_agents(AxumState(state): AxumState<Shared>) -> Json<Value> {
    let guard = state.read();
    Json(json!({ "agents": guard.list_active() }))
}

// -----------------------------------------------------------------------------
// Task #3 — config + status surface
// -----------------------------------------------------------------------------

/// Body returned by `POST /api/config` and `/api/config/reset`. `save_error`
/// is set (and the HTTP status still 200) when the in-memory update succeeded
/// but the atomic write to disk failed — the UI surfaces this as a banner.
#[derive(Serialize)]
pub struct PostConfigResponse {
    pub config: Config,
    /// `None` on successful save, `Some` with a human-readable explanation
    /// when the write failed. The client should warn the user that their
    /// change is live but will revert on restart.
    pub save_error: Option<String>,
}

/// Shape used by `POST /api/config` when deserialization fails. 400 only.
#[derive(Serialize)]
pub struct ErrorBody {
    pub error: String,
    pub details: String,
}

/// `GET /api/config` — return the live config snapshot.
pub async fn get_config(AxumState(cfg): AxumState<SharedConfig>) -> Json<Config> {
    Json(cfg.read().clone())
}

/// `GET /api/config/bounds` — per-field min/max/default. UI uses this to
/// populate slider ranges.
pub async fn get_config_bounds() -> Json<ConfigBounds> {
    Json(Config::bounds())
}

/// `POST /api/config` — shallow-merge `patch` onto the live config, clamp,
/// then persist atomically. Returns the resulting config.
///
/// Merge semantics: `patch` is a JSON object; each known Config field in the
/// patch overrides the live value. Fields absent from the patch stay as they
/// are. Unknown fields are ignored rather than rejected, so a UI that
/// includes extras (e.g. a tentative future field) doesn't wedge on deploy.
pub async fn post_config(
    AxumState(app): AxumState<AppState>,
    Json(patch): Json<Value>,
) -> Result<Json<PostConfigResponse>, (StatusCode, Json<ErrorBody>)> {
    // Patch must be a JSON object — arrays/strings/numbers are meaningless
    // for a flat-record merge.
    let patch_obj = match patch {
        Value::Object(map) => map,
        other => {
            return Err((
                StatusCode::BAD_REQUEST,
                Json(ErrorBody {
                    error: "invalid patch".into(),
                    details: format!("expected a JSON object, got {}", value_kind(&other)),
                }),
            ));
        }
    };

    let merged = {
        let cfg = app.config.read().clone();
        let mut current = serde_json::to_value(&cfg).expect("Config serializes");
        if let Value::Object(ref mut current_map) = current {
            for (k, v) in patch_obj {
                current_map.insert(k, v);
            }
        }
        // Try to deserialize back. On failure, leave in-memory config alone.
        match serde_json::from_value::<Config>(current) {
            Ok(mut c) => {
                c.clamp();
                c
            }
            Err(e) => {
                return Err((
                    StatusCode::BAD_REQUEST,
                    Json(ErrorBody {
                        error: "invalid config".into(),
                        details: e.to_string(),
                    }),
                ));
            }
        }
    };

    // Commit under the write lock so concurrent reads see a consistent view.
    {
        let mut w = app.config.write();
        *w = merged.clone();
    }

    let save_error = match persist::save(&app.config_path, &merged) {
        Ok(()) => None,
        Err(e) => {
            tracing::warn!(
                "workplacesim: failed to persist config to {}: {e}",
                app.config_path.display()
            );
            Some(format!("{e}"))
        }
    };

    Ok(Json(PostConfigResponse {
        config: merged,
        save_error,
    }))
}

/// `POST /api/restart` — bounce the service so restart-required config
/// changes (window size, fullscreen) actually take effect.
///
/// Gated on the `INVOCATION_ID` env var, which systemd sets on each unit
/// invocation. When running under systemd we respond `202 Accepted` with a
/// tiny JSON ack, then spawn a thread that sleeps 300 ms and calls
/// `std::process::exit(0)` — the sleep gives axum time to flush the response
/// to the client, and `Restart=always` on the unit brings the service right
/// back. In dev (no `INVOCATION_ID`) we refuse with `409 Conflict` and never
/// exit — the only way to restart a manually-launched process is manually.
pub async fn post_restart() -> Result<(StatusCode, Json<Value>), (StatusCode, Json<ErrorBody>)> {
    if std::env::var_os("INVOCATION_ID").is_some() {
        // Spawn the exit thread now. 300 ms is comfortably longer than the
        // axum response flush window but short enough that the UI's restart
        // watcher (polling /api/status every 500 ms with a 20 s budget)
        // doesn't miss the dip-and-return.
        std::thread::spawn(|| {
            std::thread::sleep(std::time::Duration::from_millis(300));
            std::process::exit(0);
        });
        Ok((
            StatusCode::ACCEPTED,
            Json(json!({ "restarting": true, "delay_ms": 300 })),
        ))
    } else {
        Err((
            StatusCode::CONFLICT,
            Json(ErrorBody {
                error: "not running under systemd".into(),
                details: "Restart endpoint only works when launched as a systemd unit. \
                     Stop and re-run the process manually."
                    .into(),
            }),
        ))
    }
}

/// `POST /api/config/reset` — overwrite with `Config::default()`, save, return
/// the same body shape as POST /api/config.
pub async fn reset_config(AxumState(app): AxumState<AppState>) -> Json<PostConfigResponse> {
    let defaults = Config::default();
    {
        let mut w = app.config.write();
        *w = defaults.clone();
    }

    let save_error = match persist::save(&app.config_path, &defaults) {
        Ok(()) => None,
        Err(e) => {
            tracing::warn!(
                "workplacesim: failed to persist reset config to {}: {e}",
                app.config_path.display()
            );
            Some(format!("{e}"))
        }
    };

    Json(PostConfigResponse {
        config: defaults,
        save_error,
    })
}

/// Aggregated scene stats surfaced on `/api/status`. `by_type` counts agents
/// by `agent_type`; `by_room` maps each agent to `desk`/`meeting`/`lab`/
/// `transit` using the same `classify` logic the renderer uses.
#[derive(Serialize, Default)]
pub struct AgentStats {
    pub total: usize,
    /// Count per `agent_type`. BTreeMap for deterministic key ordering in
    /// the JSON output.
    pub by_type: BTreeMap<String, usize>,
    pub by_room: RoomCounts,
}

#[derive(Serialize, Default)]
pub struct RoomCounts {
    pub desk: usize,
    pub meeting: usize,
    pub lab: usize,
    /// "Currently walking between rooms" — has no active `visit` and isn't
    /// seated at the classifier's room. For MVP we use the same classifier
    /// result but mark an agent as transit while it has a `visit.until` in
    /// the future and the room from `classify` matches.
    /// Simpler definition below: `transit = 0` here, since the Rust port has
    /// no per-sim "seated" flag in `State`. The renderer computes transit
    /// locally. See the module comment at `get_status` for why.
    pub transit: usize,
}

/// Framebuffer runtime info — populated only when the `fb` backend is live.
/// Task #3 reserves the struct but leaves it unconditionally `None`; Task #5
/// threads real values in from `FbBackend` at the time the fb is opened.
#[derive(Serialize, Clone, Copy, Debug)]
pub struct FbInfo {
    pub panel_w: u32,
    pub panel_h: u32,
    pub bpp: u8,
    pub scaled_w: u32,
    pub scaled_h: u32,
    pub letterbox_x: u32,
    pub letterbox_y: u32,
}

/// `GET /api/status` body. Mixes build + runtime + scene snapshots. The UI
/// polls this on a timer to render the "system" panel.
#[derive(Serialize)]
pub struct StatusBody {
    pub uptime_ms: u64,
    pub agents: AgentStats,
    pub events_total: u64,
    pub events_per_min: f64,
    pub build: BuildInfo,
    /// Display path for the config file, with `$HOME` collapsed to `~` for
    /// readability. The in-memory `PathBuf` stays canonical so file IO
    /// doesn't depend on shell expansion.
    pub config_path: String,
    pub config_source: crate::config::persist::ConfigSource,
    pub fb_info: Option<FbInfo>,
}

/// `GET /api/status` — bag of everything the UI's system panel needs.
///
/// "transit" is always 0 in this view. The authoritative "is this sim
/// walking?" state lives only in the renderer (see `SimStore`), not in the
/// server's `State`. Once Task #5 ships and the renderer exposes a tiny
/// "is-walking" bool back to the server, this field can become nonzero. For
/// now it stays in the response body so the UI can already lay out the four
/// counters.
pub async fn get_status(AxumState(app): AxumState<AppState>) -> Json<StatusBody> {
    let now_ms = clock::now_ms();
    let uptime_ms = now_ms.saturating_sub(app.started_at_ms);

    let (agents, events_total, events_per_min) = {
        let guard = app.state.read();
        (
            guard.list_active(),
            guard.events_total(),
            guard.events_per_min(),
        )
    };

    let agents_stats = compute_agent_stats(&agents);

    // Short read-lock, immediate release. `FbInfo` is `Copy` so we take it
    // out of the Option by value rather than clone-by-reference.
    let fb_info = *app.fb_info.read();

    Json(StatusBody {
        uptime_ms,
        agents: agents_stats,
        events_total,
        events_per_min,
        build: app.build,
        config_path: display_config_path(&app.config_path),
        config_source: app.config_source,
        fb_info,
    })
}

fn compute_agent_stats(agents: &[Agent]) -> AgentStats {
    let mut by_type: BTreeMap<String, usize> = BTreeMap::new();
    let mut by_room = RoomCounts::default();
    for a in agents {
        *by_type.entry(a.agent_type.clone()).or_insert(0) += 1;
        match classify(&a.agent_type, &a.description, &a.permission_mode) {
            Room::Desk => by_room.desk += 1,
            Room::Meeting => by_room.meeting += 1,
            Room::Lab => by_room.lab += 1,
        }
    }
    AgentStats {
        total: agents.len(),
        by_type,
        by_room,
    }
}

/// Collapse `$HOME/…` to `~/…` for display. The canonical `PathBuf` kept on
/// `AppState` stays untouched — only this string representation hides the
/// user's home directory.
fn display_config_path(path: &std::path::Path) -> String {
    if let Some(home) = dirs::home_dir() {
        if let Ok(rest) = path.strip_prefix(&home) {
            return format!("~/{}", rest.display());
        }
    }
    path.display().to_string()
}

fn value_kind(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "boolean",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}
