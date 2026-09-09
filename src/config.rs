use anyhow::{Context, Result};
use serde::Deserialize;
use std::path::{Path, PathBuf};

fn default_log_file() -> PathBuf {
    PathBuf::from("webmon.log")
}
fn default_state_file() -> PathBuf {
    PathBuf::from("webmon.state")
}
fn default_ntfy_server() -> String {
    "https://ntfy.sh".to_string()
}
fn default_interval_secs() -> u64 {
    3600
}
fn default_user_agent() -> String {
    "webmon/0.1 (+https://github.com/)".to_string()
}
fn default_timeout_secs() -> u64 {
    30
}

#[derive(Debug, Deserialize, Clone)]
pub struct GeneralConfig {
    /// Directory where fetched page snapshots are stored.
    pub storage_dir: PathBuf,

    /// Plain-text log file. Relative paths are resolved relative to the CWD.
    #[serde(default = "default_log_file")]
    pub log_file: PathBuf,

    /// Plain-text state file recording, per target, the last time it was checked.
    #[serde(default = "default_state_file")]
    pub state_file: PathBuf,

    /// Default ntfy server, used unless a target overrides it.
    #[serde(default = "default_ntfy_server")]
    pub ntfy_server: String,

    /// Default polling interval for targets that don't specify their own.
    #[serde(default = "default_interval_secs")]
    pub default_interval_secs: u64,

    #[serde(default = "default_user_agent")]
    pub user_agent: String,

    #[serde(default = "default_timeout_secs")]
    pub timeout_secs: u64,
}

#[derive(Debug, Deserialize, Clone)]
pub struct TargetConfig {
    /// Unique, human-readable name. Used as the snapshot filename and state key.
    pub name: String,

    pub url: String,

    /// Overrides `general.default_interval_secs` for this target.
    pub interval_secs: Option<u64>,

    /// ntfy.sh topic to publish change notifications to. If omitted, this
    /// target is monitored and snapshotted but no notification is sent.
    pub ntfy_topic: Option<String>,

    /// Overrides `general.ntfy_server` for this target.
    pub ntfy_server: Option<String>,

    /// If set, ONLY the text matched by this regex is considered when
    /// looking for changes (all matches are concatenated, in order,
    /// separated by newlines). Mutually composable with `exclude_regex`:
    /// inclusion is applied first, then exclusion is applied to what's left.
    /// Use inline flags like `(?s)` or `(?m)` in the pattern if you need
    /// dot-matches-newline or multiline behavior.
    pub include_regex: Option<String>,

    /// If set, any text matched by this regex is stripped out before
    /// comparing. Useful for ignoring things like timestamps, view counters,
    /// CSRF tokens, etc. that change on every load but aren't interesting.
    pub exclude_regex: Option<String>,

    /// Optional ntfy priority (1-5, or "min","low","default","high","max").
    pub ntfy_priority: Option<String>,

    /// Optional notification title override. Defaults to the target name.
    pub ntfy_title: Option<String>,
}

#[derive(Debug, Deserialize, Clone)]
pub struct Config {
    pub general: GeneralConfig,

    #[serde(rename = "target", default)]
    pub targets: Vec<TargetConfig>,
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read config file at {}", path.display()))?;
        let mut cfg: Config = toml::from_str(&raw)
            .with_context(|| format!("failed to parse TOML config at {}", path.display()))?;

        // Relative paths in the config (storage_dir, log_file, state_file)
        // are resolved relative to the config file's own directory, not the
        // process's current directory. This matters because this tool is
        // meant to be invoked from cron with an arbitrary CWD.
        let base = path
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map(Path::to_path_buf)
            .unwrap_or_else(|| PathBuf::from("."));
        let resolve = |p: &Path| -> PathBuf {
            if p.is_absolute() {
                p.to_path_buf()
            } else {
                base.join(p)
            }
        };
        cfg.general.storage_dir = resolve(&cfg.general.storage_dir);
        cfg.general.log_file = resolve(&cfg.general.log_file);
        cfg.general.state_file = resolve(&cfg.general.state_file);

        if cfg.targets.is_empty() {
            anyhow::bail!("config has no [[target]] entries; nothing to monitor");
        }

        let mut seen = std::collections::HashSet::new();
        for t in &cfg.targets {
            if t.name.trim().is_empty() {
                anyhow::bail!("a target has an empty name");
            }
            if !seen.insert(t.name.clone()) {
                anyhow::bail!("duplicate target name '{}': names must be unique", t.name);
            }
            // Validate regexes up front so config errors surface immediately
            // rather than mid-run.
            if let Some(re) = &t.include_regex {
                regex::Regex::new(re).with_context(|| {
                    format!("target '{}': invalid include_regex '{}'", t.name, re)
                })?;
            }
            if let Some(re) = &t.exclude_regex {
                regex::Regex::new(re).with_context(|| {
                    format!("target '{}': invalid exclude_regex '{}'", t.name, re)
                })?;
            }
        }

        Ok(cfg)
    }

    pub fn interval_for(&self, target: &TargetConfig) -> u64 {
        target.interval_secs.unwrap_or(self.general.default_interval_secs)
    }
}
