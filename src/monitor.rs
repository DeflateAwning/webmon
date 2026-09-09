use crate::config::{Config, TargetConfig};
use crate::ntfy;
use anyhow::{Context, Result};
use regex::Regex;
use reqwest::blocking::Client;
use similar::{ChangeTag, TextDiff};
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Duration;

pub fn build_client(cfg: &Config) -> Result<Client> {
    Client::builder()
        .user_agent(cfg.general.user_agent.clone())
        .timeout(Duration::from_secs(cfg.general.timeout_secs))
        .build()
        .context("failed to build HTTP client")
}

/// Turns a target name into a filesystem-safe file stem.
fn sanitize_name(name: &str) -> String {
    name.chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

fn snapshot_path(storage_dir: &Path, target_name: &str) -> PathBuf {
    storage_dir.join(format!("{}.snapshot", sanitize_name(target_name)))
}

/// Applies the target's include/exclude regexes to raw page content to
/// produce the text that's actually compared run-to-run. With neither regex
/// set, the whole page is monitored.
fn extract_monitored(content: &str, include: Option<&Regex>, exclude: Option<&Regex>) -> String {
    let mut text = match include {
        Some(re) => re
            .find_iter(content)
            .map(|m| m.as_str())
            .collect::<Vec<_>>()
            .join("\n"),
        None => content.to_string(),
    };

    if let Some(re) = exclude {
        text = re.replace_all(&text, "").into_owned();
    }

    text
}

/// Produces a plain-text unified-style diff between two strings.
fn make_diff(old: &str, new: &str) -> String {
    let diff = TextDiff::from_lines(old, new);
    let mut out = String::new();
    for group in diff.grouped_ops(3) {
        for op in group {
            for change in diff.iter_changes(&op) {
                let sign = match change.tag() {
                    ChangeTag::Delete => "-",
                    ChangeTag::Insert => "+",
                    ChangeTag::Equal => " ",
                };
                out.push_str(sign);
                out.push_str(change.value());
                if change.missing_newline() {
                    out.push('\n');
                }
            }
        }
    }
    out
}

pub struct RunOutcome {
    pub changed: bool,
    pub notified: bool,
}

/// Fetches a target, compares it against its stored snapshot (through the
/// configured include/exclude regex lens), notifies ntfy.sh on a change,
/// and updates the on-disk snapshot with the freshly fetched content.
pub fn process_target(
    client: &Client,
    cfg: &Config,
    target: &TargetConfig,
) -> Result<RunOutcome> {
    let include_re = target
        .include_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .context("invalid include_regex")?;
    let exclude_re = target
        .exclude_regex
        .as_deref()
        .map(Regex::new)
        .transpose()
        .context("invalid exclude_regex")?;

    fs::create_dir_all(&cfg.general.storage_dir).with_context(|| {
        format!(
            "failed to create storage dir {}",
            cfg.general.storage_dir.display()
        )
    })?;
    let path = snapshot_path(&cfg.general.storage_dir, &target.name);

    log::debug!("fetching '{}' ({})", target.name, target.url);
    let resp = client
        .get(&target.url)
        .send()
        .with_context(|| format!("request to {} failed", target.url))?
        .error_for_status()
        .with_context(|| format!("{} returned an error status", target.url))?;
    let new_raw = resp
        .text()
        .with_context(|| format!("failed to read response body from {}", target.url))?;

    let old_raw = match fs::read_to_string(&path) {
        Ok(s) => Some(s),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(e).context(format!("failed to read snapshot {}", path.display())),
    };

    let new_monitored = extract_monitored(&new_raw, include_re.as_ref(), exclude_re.as_ref());

    let mut outcome = RunOutcome {
        changed: false,
        notified: false,
    };

    match &old_raw {
        None => {
            log::info!(
                "target '{}': no prior snapshot, saving baseline (no notification)",
                target.name
            );
        }
        Some(old) => {
            let old_monitored = extract_monitored(old, include_re.as_ref(), exclude_re.as_ref());
            if old_monitored != new_monitored {
                outcome.changed = true;
                let diff_text = make_diff(&old_monitored, &new_monitored);
                log::info!("target '{}': change detected", target.name);
                match ntfy::notify_change(client, &cfg.general, target, &diff_text) {
                    Ok(()) => {
                        if target.ntfy_topic.is_some() {
                            outcome.notified = true;
                            log::info!("target '{}': notification sent", target.name);
                        }
                    }
                    Err(e) => {
                        log::error!(
                            "target '{}': change detected but notification failed: {:#}",
                            target.name,
                            e
                        );
                    }
                }
            } else {
                log::debug!("target '{}': no change", target.name);
            }
        }
    }

    fs::write(&path, &new_raw)
        .with_context(|| format!("failed to write snapshot {}", path.display()))?;

    Ok(outcome)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn whole_page_by_default() {
        let out = extract_monitored("hello world", None, None);
        assert_eq!(out, "hello world");
    }

    #[test]
    fn include_regex_selects_parts() {
        let re = Regex::new(r"Price: \$\d+").unwrap();
        let content = "Widget\nPrice: $10\nIn stock\nPrice: $20 (bulk)";
        // note: the second "Price: $20" match stops at the digits, "(bulk)" isn't included
        let out = extract_monitored(content, Some(&re), None);
        assert_eq!(out, "Price: $10\nPrice: $20");
    }

    #[test]
    fn exclude_regex_strips_parts() {
        let re = Regex::new(r"(?m)^Last updated:.*$").unwrap();
        let content = "Title\nLast updated: 2026-01-01\nBody text";
        let out = extract_monitored(content, None, Some(&re));
        assert_eq!(out, "Title\n\nBody text");
    }

    #[test]
    fn diff_shows_added_and_removed_lines() {
        let diff = make_diff("a\nb\nc\n", "a\nx\nc\n");
        assert!(diff.contains("-b"));
        assert!(diff.contains("+x"));
    }

    #[test]
    fn sanitize_replaces_unsafe_chars() {
        assert_eq!(sanitize_name("My Page: v2/beta"), "My_Page__v2_beta");
    }
}
