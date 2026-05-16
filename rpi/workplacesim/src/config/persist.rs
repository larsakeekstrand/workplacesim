//! Config persistence: path resolution + atomic load/save.
//!
//! Path resolution order (first match wins):
//!   1. `$WORKPLACESIM_CONFIG_PATH` if set and non-empty.
//!   2. `dirs::config_dir()` joined with `workplacesim/config.json`
//!      (XDG-style; on macOS this is `~/Library/Application Support`,
//!      on Linux `$XDG_CONFIG_HOME` or `~/.config`).
//!   3. `./workplacesim-config.json` — last-ditch fallback for odd envs.
//!
//! Load is fail-safe: a missing file yields defaults, a corrupt file yields
//! defaults *and* logs a warning. No code path panics.
//!
//! Save is atomic: write to `<path>.json.tmp` then `rename` over `<path>`,
//! so a crash mid-write can never leave a half-written file.

use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::Config;

/// Where `Config` came from on this run. Surfaced on `/api/status` so the UI
/// can warn the user when a corrupt file was replaced by defaults.
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub enum ConfigSource {
    #[serde(rename = "loaded")]
    Loaded,
    #[serde(rename = "missing-used-defaults")]
    MissingUsedDefaults,
    #[serde(rename = "corrupt-used-defaults")]
    CorruptUsedDefaults,
}

/// Resolve the path to read/write the config file at. See module docs for
/// the search order. Never returns an empty path.
pub fn resolve_path() -> PathBuf {
    if let Ok(env_path) = std::env::var("WORKPLACESIM_CONFIG_PATH") {
        if !env_path.is_empty() {
            return PathBuf::from(env_path);
        }
    }
    if let Some(dir) = dirs::config_dir() {
        return dir.join("workplacesim").join("config.json");
    }
    PathBuf::from("./workplacesim-config.json")
}

/// Read `path`, returning a `Config` plus a label describing where it came
/// from. Never panics; missing and corrupt files both yield `Config::default()`
/// with the appropriate `ConfigSource` tag.
pub fn load_or_default(path: &Path) -> (Config, ConfigSource) {
    match fs::read_to_string(path) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            (Config::default(), ConfigSource::MissingUsedDefaults)
        }
        Err(err) => {
            tracing::warn!(
                "workplacesim: failed to read config at {}: {err}; using defaults",
                path.display()
            );
            (Config::default(), ConfigSource::CorruptUsedDefaults)
        }
        Ok(body) => match serde_json::from_str::<Config>(&body) {
            Ok(mut cfg) => {
                cfg.clamp();
                (cfg, ConfigSource::Loaded)
            }
            Err(err) => {
                tracing::warn!(
                    "workplacesim: failed to parse config at {}: {err}; using defaults",
                    path.display()
                );
                (Config::default(), ConfigSource::CorruptUsedDefaults)
            }
        },
    }
}

/// Write `cfg` to `path` atomically. Creates parent directories as needed.
/// Uses a `*.json.tmp` sibling + `fs::rename` so a crash mid-write can never
/// leave the destination half-written.
pub fn save(path: &Path, cfg: &Config) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)?;
        }
    }

    let body = serde_json::to_string_pretty(cfg)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;

    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, body)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn load_missing_file_returns_defaults() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("does-not-exist.json");
        let (cfg, source) = load_or_default(&path);
        assert_eq!(cfg, Config::default());
        assert_eq!(source, ConfigSource::MissingUsedDefaults);
    }

    #[test]
    fn save_then_load_roundtrips() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("nested").join("config.json");

        let cfg = Config {
            walk_speed_px_per_sec: 123.5,
            mote_cap: 17,
            fullscreen: true,
            ..Config::default()
        };

        save(&path, &cfg).expect("save");

        let (loaded, source) = load_or_default(&path);
        assert_eq!(source, ConfigSource::Loaded);
        assert_eq!(loaded, cfg);
    }

    #[test]
    fn corrupt_file_returns_defaults_and_tag() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("corrupt.json");
        fs::write(&path, "{not valid json").expect("write corrupt");

        let (cfg, source) = load_or_default(&path);
        assert_eq!(cfg, Config::default());
        assert_eq!(source, ConfigSource::CorruptUsedDefaults);
    }

    #[test]
    fn save_leaves_no_tmp_file_behind() {
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        save(&path, &Config::default()).expect("save");

        let tmp = path.with_extension("json.tmp");
        assert!(
            !tmp.exists(),
            "tmp file should have been renamed away: {}",
            tmp.display()
        );
        assert!(path.exists(), "final file should exist");
    }

    #[test]
    fn load_clamps_out_of_range_values_from_disk() {
        // A file hand-edited with silly values must not propagate them.
        let dir = tempdir().expect("tempdir");
        let path = dir.path().join("config.json");
        fs::write(
            &path,
            r#"{"walk_speed_px_per_sec": 9999.0, "mote_cap": 99999}"#,
        )
        .expect("write");

        let (cfg, source) = load_or_default(&path);
        assert_eq!(source, ConfigSource::Loaded);
        assert_eq!(cfg.walk_speed_px_per_sec, 500.0);
        assert_eq!(cfg.mote_cap, 500);
    }
}
