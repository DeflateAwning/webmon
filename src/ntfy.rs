use crate::config::{GeneralConfig, TargetConfig};
use anyhow::{Context, Result};
use reqwest::blocking::Client;

/// Publishes a plain-text message (the diff) to the target's ntfy topic.
/// If the target has no `ntfy_topic` configured, this is a no-op.
pub fn notify_change(
    client: &Client,
    general: &GeneralConfig,
    target: &TargetConfig,
    diff_text: &str,
) -> Result<()> {
    let Some(topic) = &target.ntfy_topic else {
        log::debug!(
            "target '{}' changed but has no ntfy_topic configured; skipping notification",
            target.name
        );
        return Ok(());
    };

    let server = target
        .ntfy_server
        .as_deref()
        .unwrap_or(&general.ntfy_server);
    let url = format!("{}/{}", server.trim_end_matches('/'), topic);

    let title = target
        .ntfy_title
        .clone()
        .unwrap_or_else(|| format!("Change detected: {}", target.name));

    // ntfy caps message bodies; trim very large diffs so the request
    // doesn't get rejected, and say so at the end. The monitored URL goes on
    // the first line, so it stays visible even when the diff is truncated.
    const MAX_DIFF: usize = 3800;
    let body = if diff_text.len() > MAX_DIFF {
        // Don't slice mid-character: back off to the nearest char boundary.
        let mut cut = MAX_DIFF;
        while !diff_text.is_char_boundary(cut) {
            cut -= 1;
        }
        format!(
            "{}\n\n{}\n\n... [diff truncated, {} bytes total]",
            target.url,
            &diff_text[..cut],
            diff_text.len()
        )
    } else {
        format!("{}\n\n{}", target.url, diff_text)
    };

    let mut req = client
        .post(&url)
        .header("Title", title)
        .header("Content-Type", "text/plain; charset=utf-8")
        .body(body);

    if let Some(priority) = &target.ntfy_priority {
        req = req.header("Priority", priority.clone());
    }
    req = req.header("Tags", "warning");
    req = req.header("Click", target.url.clone());

    let resp = req
        .send()
        .with_context(|| format!("failed to POST notification to {}", url))?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().unwrap_or_default();
        anyhow::bail!("ntfy server returned {}: {}", status, body);
    }

    Ok(())
}
