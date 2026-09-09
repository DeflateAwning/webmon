use anyhow::{Context, Result};
use std::collections::HashMap;
use std::path::Path;

/// Plain-text state: one `name<TAB>unix_timestamp` line per target.
/// Kept deliberately simple and human-editable (e.g. delete a line to force
/// a re-check, or hand-edit a timestamp) — no embedded database.
#[derive(Debug, Default)]
pub struct State {
    last_run: HashMap<String, i64>,
}

impl State {
    pub fn load(path: &Path) -> Result<State> {
        if !path.exists() {
            return Ok(State::default());
        }
        let raw = std::fs::read_to_string(path)
            .with_context(|| format!("failed to read state file at {}", path.display()))?;

        let mut last_run = HashMap::new();
        for (lineno, line) in raw.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let mut parts = line.splitn(2, '\t');
            let name = parts.next().unwrap_or_default();
            let ts = parts.next().unwrap_or_default();
            match ts.trim().parse::<i64>() {
                Ok(v) => {
                    last_run.insert(name.to_string(), v);
                }
                Err(_) => {
                    log::warn!(
                        "state file {}: skipping malformed line {}: {:?}",
                        path.display(),
                        lineno + 1,
                        line
                    );
                }
            }
        }
        Ok(State { last_run })
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            if !parent.as_os_str().is_empty() {
                std::fs::create_dir_all(parent)?;
            }
        }
        let mut out = String::from("# webmon state file — plain text, safe to hand-edit.\n");
        out.push_str("# format: <target-name>\\t<unix-timestamp-of-last-check>\n");
        let mut names: Vec<&String> = self.last_run.keys().collect();
        names.sort();
        for name in names {
            out.push_str(&format!("{}\t{}\n", name, self.last_run[name]));
        }
        std::fs::write(path, out)
            .with_context(|| format!("failed to write state file at {}", path.display()))?;
        Ok(())
    }

    pub fn last_run(&self, name: &str) -> Option<i64> {
        self.last_run.get(name).copied()
    }

    pub fn mark_run(&mut self, name: &str, ts: i64) {
        self.last_run.insert(name.to_string(), ts);
    }

    /// Whether `interval_secs` have elapsed since the target was last checked
    /// (targets never seen before are always due).
    pub fn is_due(&self, name: &str, interval_secs: u64, now: i64) -> bool {
        match self.last_run(name) {
            Some(last) => now.saturating_sub(last) >= interval_secs as i64,
            None => true,
        }
    }
}
